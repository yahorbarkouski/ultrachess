//! The high-level `Chess` class — the library's primary entry point.
//!
//! Wraps an opaque WASM handle. Mutable (make/undo, put/remove), clonable
//! (cheap WASM clone), and disposable via `using` (TC39 Explicit Resource
//! Management) or explicit `.dispose()`.
//!
//! Construction requires an awaited `init()` to have been called at least
//! once. `Chess.create()` encapsulates the idempotent init-then-construct
//! pattern.

import {
  getAbi,
  init,
  readStringScratch,
  readU64,
  type UltrachessAbi,
  writeStringToScratch,
} from "./loader.js";
import {
  type Color,
  decodePiece,
  encodePiece,
  type Move,
  MoveKind,
  moveFrom as moveFromHelper,
  moveKind as moveKindHelper,
  movePromotion as movePromotionHelper,
  moveTo as moveToHelper,
  moveToUci,
  type Piece,
  PieceType,
  parseSquare,
  squareName,
  type VerboseMove,
} from "./move.js";

export class DisposedError extends Error {
  constructor() {
    super("Chess instance has been disposed");
    this.name = "DisposedError";
  }
}

export class IllegalMoveError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "IllegalMoveError";
  }
}

export class InvalidFenError extends Error {
  constructor(fen: string) {
    super(`invalid FEN: ${fen}`);
    this.name = "InvalidFenError";
  }
}

export class InvalidPgnError extends Error {
  constructor(public readonly detail: string) {
    super(`invalid PGN: ${detail}`);
    this.name = "InvalidPgnError";
  }
}

export const STARTING_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

// Lazily populated by `Chess.create` / `Chess.fromFen` / `Chess.loadPgn`.
let abi: UltrachessAbi | null = null;

async function ensureAbi(): Promise<UltrachessAbi> {
  if (!abi) {
    abi = await init();
  }
  return abi;
}

// WASM i32 returns come to JS as signed numbers; -1 and 0xFFFFFFFF are the
// same bit pattern but present differently.
function isInvalidHandle(h: number): boolean {
  return h === 0xffffffff || h === -1;
}

// Sentinels returned by `ultrachess_put` / `ultrachess_remove`:
const PIECE_EMPTY = 255;
const PIECE_INVALID_ARGS = 254;

/**
 * Chess position with a full chess.js-compatible API — move generation,
 * make/undo, FEN/SAN/PGN I/O, draw/mate detection, attacks, position edits.
 *
 * Instances own a handle into the WASM slab allocator. Always dispose them
 * (via `using`, `[Symbol.dispose]()`, or `.dispose()`) to free the slot.
 */
export class Chess {
  private handle: number;
  private readonly abi: UltrachessAbi;
  /** Packed moves played through this instance, in order. Needed because
   *  the WASM `undo_with_move` ABI is passed the move to reverse. */
  private readonly moveStack: number[] = [];
  /** PGN headers (case-preserving insertion order). Populated by
   *  `loadPgn` and `setHeader`. */
  private readonly headerMap = new Map<string, string>();
  private disposed = false;

  private constructor(abi: UltrachessAbi, handle: number) {
    this.abi = abi;
    this.handle = handle;
  }

  /** Construct a Chess at the starting position (or custom FEN).
   *  Ensures WASM is initialised. */
  static async create(fen?: string): Promise<Chess> {
    const a = await ensureAbi();
    let handle: number;
    if (fen === undefined || fen === STARTING_FEN) {
      handle = a.ultrachess_new_startpos();
    } else {
      const { ptr, len } = writeStringToScratch(a, fen);
      handle = a.ultrachess_new_from_fen(ptr, len);
      if (isInvalidHandle(handle)) {
        throw new InvalidFenError(fen);
      }
    }
    return new Chess(a, handle);
  }

  /** Alias for `create(fen)`. */
  static fromFen(fen: string): Promise<Chess> {
    return Chess.create(fen);
  }

  /** Synchronous construction. Requires `init()` or `initSync()` to have
   *  run first — `ultrachess/inline` does this automatically at import time,
   *  so `Chess.createSync()` is the one-liner sync path there.
   *
   *  Throws if the ABI has not yet been initialised, or if `fen` is invalid. */
  static createSync(fen?: string): Chess {
    const a = getAbi();
    if (!a) {
      throw new Error(
        "ultrachess: Chess.createSync() requires init() or initSync() to have been called first. " +
          "Use `ultrachess/inline` for sync access in environments without top-level await.",
      );
    }
    let handle: number;
    if (fen === undefined || fen === STARTING_FEN) {
      handle = a.ultrachess_new_startpos();
    } else {
      const { ptr, len } = writeStringToScratch(a, fen);
      handle = a.ultrachess_new_from_fen(ptr, len);
      if (isInvalidHandle(handle)) {
        throw new InvalidFenError(fen);
      }
    }
    return new Chess(a, handle);
  }

  /** Parse a PGN, construct a Chess at its starting position, and replay
   *  the mainline. The resulting instance is positioned at the final
   *  reached position, with `history()` returning every move played and
   *  `headers()` returning the parsed tags. */
  static async loadPgn(pgn: string): Promise<Chess> {
    const a = await ensureAbi();
    const { ptr, len } = writeStringToScratch(a, pgn);
    const pgnHandle = a.ultrachess_pgn_parse(ptr, len);
    if (isInvalidHandle(pgnHandle)) {
      throw new InvalidPgnError("parse failed");
    }
    try {
      // Pull headers + moves out of the parsed-PGN slab.
      const headerCount = a.ultrachess_pgn_header_count(pgnHandle);
      const headers = new Map<string, string>();
      for (let i = 0; i < headerCount; i++) {
        const keyLen = a.ultrachess_pgn_header_key(
          pgnHandle,
          i,
          0,
          a.ultrachess_string_scratch_cap(),
        );
        const key = readStringScratch(a, keyLen);
        const valLen = a.ultrachess_pgn_header_value(
          pgnHandle,
          i,
          0,
          a.ultrachess_string_scratch_cap(),
        );
        const value = readStringScratch(a, valLen);
        headers.set(key, value);
      }

      const mainlineLen = a.ultrachess_pgn_mainline_len(pgnHandle);
      const sans: string[] = new Array(mainlineLen);
      for (let i = 0; i < mainlineLen; i++) {
        const sanLen = a.ultrachess_pgn_mainline_san(
          pgnHandle,
          i,
          0,
          a.ultrachess_string_scratch_cap(),
        );
        sans[i] = readStringScratch(a, sanLen);
      }

      // Pick the starting position — respect a FEN/SetUp header if present.
      const startFen = headers.get("FEN");
      const chess = startFen !== undefined ? await Chess.create(startFen) : await Chess.create();

      // Copy parsed headers onto the Chess instance.
      for (const [k, v] of headers) {
        chess.headerMap.set(k, v);
      }

      // Replay moves — each must be legal in the position reached so far.
      for (let i = 0; i < sans.length; i++) {
        const san = sans[i]!;
        try {
          chess.move(san);
        } catch (err) {
          chess.dispose();
          throw new InvalidPgnError(`illegal SAN "${san}" at ply ${i}: ${(err as Error).message}`);
        }
      }
      return chess;
    } finally {
      a.ultrachess_pgn_free(pgnHandle);
    }
  }

  private requireAlive(): void {
    if (this.disposed) throw new DisposedError();
  }

  // --------------------------------------------------------------------
  // Position state
  // --------------------------------------------------------------------

  /** Zobrist hash (64-bit as BigInt). */
  hash(): bigint {
    this.requireAlive();
    const lo = this.abi.ultrachess_hash(this.handle);
    return readU64(this.abi, lo);
  }

  /** FEN string for the current position. */
  fen(): string {
    this.requireAlive();
    const cap = this.abi.ultrachess_string_scratch_cap();
    const len = this.abi.ultrachess_fen_write(this.handle, 0, cap);
    return readStringScratch(this.abi, len);
  }

  /** Side to move. */
  turn(): Color {
    this.requireAlive();
    return this.abi.ultrachess_side_to_move(this.handle) as Color;
  }

  /** Halfmove clock (50-move rule counter; 100 = draw). */
  halfmove(): number {
    this.requireAlive();
    return this.abi.ultrachess_halfmove(this.handle) >>> 0;
  }

  /** Fullmove number (starts at 1, increments after Black's move). */
  fullmove(): number {
    this.requireAlive();
    return this.abi.ultrachess_fullmove(this.handle) >>> 0;
  }

  /** Piece at `square`, either as algebraic (`"e4"`) or numeric index. */
  pieceAt(square: number | string): Piece | null {
    this.requireAlive();
    const idx = typeof square === "string" ? parseSquare(square) : square;
    if (idx === null || idx < 0 || idx >= 64) return null;
    return decodePiece(this.abi.ultrachess_piece_at(this.handle, idx));
  }

  /** 8×8 ASCII rendering (ranks 8→1, files a→h). Empty squares are '.'. */
  ascii(): string {
    this.requireAlive();
    const cap = this.abi.ultrachess_string_scratch_cap();
    const len = this.abi.ultrachess_ascii_write(this.handle, 0, cap);
    return readStringScratch(this.abi, len);
  }

  // --------------------------------------------------------------------
  // Game-end checks
  // --------------------------------------------------------------------

  inCheck(): boolean {
    this.requireAlive();
    return this.abi.ultrachess_in_check(this.handle) !== 0;
  }
  isCheckmate(): boolean {
    this.requireAlive();
    return this.abi.ultrachess_is_checkmate(this.handle) !== 0;
  }
  isStalemate(): boolean {
    this.requireAlive();
    return this.abi.ultrachess_is_stalemate(this.handle) !== 0;
  }
  isInsufficientMaterial(): boolean {
    this.requireAlive();
    return this.abi.ultrachess_is_insufficient_material(this.handle) !== 0;
  }
  isThreefoldRepetition(): boolean {
    this.requireAlive();
    return this.abi.ultrachess_is_threefold_repetition(this.handle) !== 0;
  }
  isFiftyMoveRule(): boolean {
    this.requireAlive();
    return this.abi.ultrachess_is_fifty_move_rule(this.handle) !== 0;
  }
  isDraw(): boolean {
    this.requireAlive();
    return this.abi.ultrachess_is_draw(this.handle) !== 0;
  }
  isGameOver(): boolean {
    this.requireAlive();
    return this.abi.ultrachess_is_game_over(this.handle) !== 0;
  }

  // --------------------------------------------------------------------
  // Attack queries
  // --------------------------------------------------------------------

  /** True if `square` is attacked by any piece of `by`. */
  isAttacked(square: number | string, by: Color): boolean {
    this.requireAlive();
    const idx = typeof square === "string" ? parseSquare(square) : square;
    if (idx === null || idx < 0 || idx >= 64) return false;
    return this.abi.ultrachess_is_attacked(this.handle, idx, by) !== 0;
  }

  /** Squares of every piece of `by` that attacks `square`. Returns square
   *  names (`"e5"`). Empty array if the square is not attacked. */
  attackers(square: number | string, by: Color): string[] {
    this.requireAlive();
    const idx = typeof square === "string" ? parseSquare(square) : square;
    if (idx === null || idx < 0 || idx >= 64) return [];
    const scratchPtr = this.abi.ultrachess_string_scratch_ptr();
    // Max 16 attackers in practice; leave ample headroom.
    const cap = 32;
    const count = this.abi.ultrachess_attackers(this.handle, idx, by, 0, cap);
    const view = new Uint8Array(this.abi.memory.buffer, scratchPtr, count);
    const out = new Array<string>(count);
    for (let i = 0; i < count; i++) out[i] = squareName(view[i]!);
    return out;
  }

  /** Squares currently holding `piece`. */
  findPiece(piece: Piece): string[] {
    this.requireAlive();
    const scratchPtr = this.abi.ultrachess_string_scratch_ptr();
    const count = this.abi.ultrachess_find_piece(this.handle, piece.color, piece.type, 0, 64);
    const view = new Uint8Array(this.abi.memory.buffer, scratchPtr, count);
    const out = new Array<string>(count);
    for (let i = 0; i < count; i++) out[i] = squareName(view[i]!);
    return out;
  }

  // --------------------------------------------------------------------
  // Position edits (invalidate history)
  // --------------------------------------------------------------------

  /** Place a piece on `square`, replacing whatever was there. Invalidates
   *  move history (undo no longer makes sense after an arbitrary edit).
   *  Returns the replaced piece (or `null` if the square was empty).
   *
   *  Throws if the piece / square is invalid, or if placing a king when
   *  one of the same color already exists elsewhere. */
  put(piece: Piece, square: number | string): Piece | null {
    this.requireAlive();
    const idx = typeof square === "string" ? parseSquare(square) : square;
    if (idx === null || idx < 0 || idx >= 64) {
      throw new RangeError(`invalid square: ${String(square)}`);
    }
    const code = encodePiece(piece);
    const result = this.abi.ultrachess_put(this.handle, code, idx);
    if (result === PIECE_INVALID_ARGS) {
      throw new RangeError(`invalid put(${JSON.stringify(piece)}, ${String(square)})`);
    }
    // Edits wipe history; clear our own stack so `undo()` returns null.
    this.moveStack.length = 0;
    return result === PIECE_EMPTY ? null : decodePiece(result);
  }

  /** Remove the piece at `square`, returning it (or `null` if empty). */
  remove(square: number | string): Piece | null {
    this.requireAlive();
    const idx = typeof square === "string" ? parseSquare(square) : square;
    if (idx === null || idx < 0 || idx >= 64) {
      throw new RangeError(`invalid square: ${String(square)}`);
    }
    const result = this.abi.ultrachess_remove(this.handle, idx);
    if (result === PIECE_INVALID_ARGS) {
      throw new RangeError(`invalid remove(${String(square)})`);
    }
    this.moveStack.length = 0;
    return result === PIECE_EMPTY ? null : decodePiece(result);
  }

  // --------------------------------------------------------------------
  // Move generation
  // --------------------------------------------------------------------

  /** Legal-move enumeration.
   *
   *  - `moves()` — SAN strings.
   *  - `moves({ raw: true })` — packed `Move` integers (fastest).
   *  - `moves({ verbose: true })` — `VerboseMove` objects (most informative).
   */
  moves(): string[];
  moves(options: { raw: true }): Move[];
  moves(options: { verbose: true }): VerboseMove[];
  moves(options?: { raw?: boolean; verbose?: boolean }): string[] | Move[] | VerboseMove[] {
    this.requireAlive();
    const packed = this.legalMoves();
    if (options?.raw) return packed;
    if (options?.verbose) return packed.map((m) => this.verboseMove(m));
    return packed.map((m) => this.san(m));
  }

  /** All legal moves as packed integer IDs. */
  legalMoves(): Move[] {
    this.requireAlive();
    const cap = this.abi.ultrachess_move_scratch_cap();
    const n = this.abi.ultrachess_generate_moves(this.handle, 0, cap);
    const scratchPtr = this.abi.ultrachess_move_scratch_ptr();
    const view = new Uint32Array(this.abi.memory.buffer, scratchPtr, n);
    const out = new Array<Move>(n);
    for (let i = 0; i < n; i++) out[i] = view[i]! as Move;
    return out;
  }

  legalMoveCount(): number {
    this.requireAlive();
    return this.abi.ultrachess_generate_moves(
      this.handle,
      0,
      this.abi.ultrachess_move_scratch_cap(),
    );
  }

  /** SAN string of a legal move in the current position. */
  san(m: Move): string {
    this.requireAlive();
    const cap = this.abi.ultrachess_string_scratch_cap();
    const len = this.abi.ultrachess_san_write(this.handle, m as number, 0, cap);
    return readStringScratch(this.abi, len);
  }

  /** Parse SAN into a legal move. Throws `IllegalMoveError` on failure. */
  parseSan(san: string): Move {
    this.requireAlive();
    const { ptr, len } = writeStringToScratch(this.abi, san);
    const packed = this.abi.ultrachess_san_parse(this.handle, ptr, len);
    if (isInvalidHandle(packed)) {
      throw new IllegalMoveError(`illegal / unparseable SAN: ${san}`);
    }
    return ((packed >>> 0) & 0xffff) as Move;
  }

  /** Build a `VerboseMove` object from a packed move in the CURRENT position.
   *  The move does not need to have been played — it's inspected as it would
   *  apply from the current position. */
  verboseMove(m: Move): VerboseMove {
    this.requireAlive();
    const fromIndex = moveFromHelper(m);
    const toIndex = moveToHelper(m);
    const kind = moveKindHelper(m);
    const movingPiece = this.pieceAt(fromIndex);
    if (!movingPiece) {
      throw new IllegalMoveError(
        `verboseMove called for a move whose source square (${squareName(fromIndex)}) is empty`,
      );
    }

    let captured: PieceType | undefined;
    if (kind === MoveKind.EnPassant) {
      // EP always captures an enemy pawn.
      captured = PieceType.Pawn;
    } else {
      const targetPiece = this.pieceAt(toIndex);
      if (targetPiece && targetPiece.color !== movingPiece.color) {
        captured = targetPiece.type;
      }
    }

    const verbose: VerboseMove = {
      fromIndex,
      toIndex,
      from: squareName(fromIndex),
      to: squareName(toIndex),
      piece: movingPiece.type,
      color: movingPiece.color,
      kind,
      san: this.san(m),
      uci: moveToUci(m),
    };
    if (captured !== undefined) verbose.captured = captured;
    if (kind === MoveKind.Promotion) verbose.promotion = movePromotionHelper(m);
    return verbose;
  }

  // --------------------------------------------------------------------
  // Make / undo
  // --------------------------------------------------------------------

  /** Play a move (SAN string OR packed `Move`). Returns the packed move. */
  move(input: string | Move): Move {
    this.requireAlive();
    const packed: number =
      typeof input === "string" ? (this.parseSan(input) as number) : (input as number);
    const status = this.abi.ultrachess_make_move(this.handle, packed);
    if (status === 1) throw new DisposedError();
    if (status === 2) throw new IllegalMoveError(`illegal move: ${String(input)}`);
    this.moveStack.push(packed);
    return packed as Move;
  }

  /** Undo the most recently made move. Returns the move that was undone,
   *  or `null` if no moves have been made through this object. */
  undo(): Move | null {
    this.requireAlive();
    const packed = this.moveStack.pop();
    if (packed === undefined) return null;
    const status = this.abi.ultrachess_undo_with_move(this.handle, packed);
    if (status !== 0) throw new Error(`undo failed with status ${status}`);
    return packed as Move;
  }

  /** Move history played through this object (packed or verbose). */
  history(): Move[];
  history(options: { verbose: true }): VerboseMove[];
  history(options?: { verbose?: boolean }): Move[] | VerboseMove[] {
    this.requireAlive();
    if (options?.verbose) {
      // Rebuild verbose records by walking backward through the history,
      // reconstructing each position by undoing moves, taking the verbose
      // snapshot, then redoing. Runs in O(n) make/unmake pairs.
      const n = this.moveStack.length;
      if (n === 0) return [];
      // Undo all.
      const popped: number[] = [];
      for (let i = 0; i < n; i++) {
        const last = this.moveStack[this.moveStack.length - 1]!;
        this.abi.ultrachess_undo_with_move(this.handle, last);
        popped.push(this.moveStack.pop()!);
      }
      // Replay and record.
      const verbose = new Array<VerboseMove>(n);
      for (let i = 0; i < n; i++) {
        const m = popped[popped.length - 1 - i]! as Move;
        verbose[i] = this.verboseMove(m);
        this.abi.ultrachess_make_move(this.handle, m as number);
        this.moveStack.push(m as number);
      }
      return verbose;
    }
    return this.moveStack.slice() as Move[];
  }

  // --------------------------------------------------------------------
  // PGN
  // --------------------------------------------------------------------

  /** PGN headers (insertion-ordered). */
  headers(): Record<string, string> {
    this.requireAlive();
    return Object.fromEntries(this.headerMap);
  }

  /** Get a header, or `undefined` if unset. */
  header(key: string): string | undefined {
    this.requireAlive();
    return this.headerMap.get(key);
  }

  /** Set (or replace) a header. Pass `undefined` as value to delete. */
  setHeader(key: string, value: string | undefined): void {
    this.requireAlive();
    if (value === undefined) {
      this.headerMap.delete(key);
    } else {
      this.headerMap.set(key, value);
    }
  }

  /** Emit a PGN string for the current game (headers + mainline).
   *
   *  Implementation: walks the history (via make/unmake) to compute the SAN
   *  in each position, then serialises with 80-column wrapping per PGN §8.2.6. */
  pgn(): string {
    this.requireAlive();
    const verboseHistory = this.history({ verbose: true });
    const out: string[] = [];

    // 7-tag roster first, then everything else in insertion order.
    const sevenTags = ["Event", "Site", "Date", "Round", "White", "Black", "Result"];
    const emitted = new Set<string>();
    for (const tag of sevenTags) {
      const v = this.headerMap.get(tag);
      if (v !== undefined) {
        out.push(`[${tag} "${escapeHeader(v)}"]`);
        emitted.add(tag);
      }
    }
    for (const [k, v] of this.headerMap) {
      if (!emitted.has(k)) out.push(`[${k} "${escapeHeader(v)}"]`);
    }
    if (out.length > 0) out.push("");

    // Movetext, wrapped at ~80 columns.
    let line = "";
    const push = (tok: string) => {
      if (line.length > 0 && line.length + 1 + tok.length > 80) {
        out.push(line);
        line = "";
      }
      line += line.length > 0 ? ` ${tok}` : tok;
    };

    for (let i = 0; i < verboseHistory.length; i++) {
      const node = verboseHistory[i]!;
      const moveNumber = Math.floor(i / 2) + 1;
      if (i % 2 === 0) push(`${moveNumber}.`);
      push(node.san);
    }
    const result = this.headerMap.get("Result") ?? "*";
    push(result);
    if (line.length > 0) out.push(line);

    return `${out.join("\n")}\n`;
  }

  // --------------------------------------------------------------------
  // Perft + clone + lifecycle
  // --------------------------------------------------------------------

  /** Count leaf nodes at `depth`. Runs entirely inside WASM — one boundary
   *  crossing, no per-node JS overhead. */
  perft(depth: number): bigint {
    this.requireAlive();
    const lo = this.abi.ultrachess_perft(this.handle, depth >>> 0);
    return readU64(this.abi, lo);
  }

  /** Clone the current position as an independent, **fresh** Chess.
   *
   *  Semantics mirror `Position::clone` in the Rust core: the returned
   *  instance represents the same board state but with an empty move
   *  history — `undo()` on the clone returns `null` until it has played
   *  moves of its own. This matches the common "snapshot this state"
   *  use case (equivalent to `new Chess(original.fen())`) and avoids a
   *  heap copy of the undo stack on every clone.
   *
   *  Headers are copied — they're metadata about the *game*, not the
   *  undo stack, and users expect them to travel with a clone. */
  clone(): Chess {
    this.requireAlive();
    const handle = this.abi.ultrachess_clone(this.handle);
    if (isInvalidHandle(handle)) throw new DisposedError();
    const copy = new Chess(this.abi, handle);
    for (const [k, v] of this.headerMap) copy.headerMap.set(k, v);
    return copy;
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.abi.ultrachess_free(this.handle);
  }

  [Symbol.dispose](): void {
    this.dispose();
  }
}

function escapeHeader(v: string): string {
  return v.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}
