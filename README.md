# ultrachess

A Rust chess engine compiled to WebAssembly behind a typed TypeScript API with zero runtime dependencies. Legal move generation, FEN / SAN / PGN, perft, and Zobrist hashing at native-Rust speed

- Fully legal move generation via pin masks + check masks
- Magic bitboards for sliders; O(1) `inCheck()` and `hash()`
- Zero-copy move enumeration through a shared `Uint32Array`
- Full PGN: headers, NAGs, comments; parse -> replay -> emit round-trip
- ~55x the perft throughput of chess.js on Node, ~95x on Bun, in a 45 KB brotli WASM + ~10 KB TS bundle

---

## Performance

Apple M4 Max, single-threaded, min of N trials. Every benchmark gate-runs the full Rust test suite first and refuses to publish numbers from a broken tree. Reproduce with `just bench`. Full methodology in [BENCH.md](./BENCH.md).

### Through the WASM boundary (what a `npm install` user gets)

Startpos perft depth 6 = 119,060,324 nodes. Measured via `await Chess.create()` + `chess.perft(6)`:

| Runtime           | Min ms  | Mnps  | vs native Rust |
|-------------------|--------:|------:|---------------:|
| **Bun 1.3.10**    |   204.9 | 581.0 |          0.70x |
| **Node 25.7.0**   |   353.7 | 336.7 |          0.40x |
| chess.js (pure JS, cited)       | — | 5–7   | — |
| chessops (pure TS, cited)       | — | 2–3   | — |

Geomean across the six standard perft positions: **770 Mnps (Bun)**, **465 Mnps (Node)**

### Native Rust perft — context for the number above

The WASM number is a fraction of what the Rust core produces on the same host. Peer comparison, startpos d6 / kiwipete d5:

| Library              | Startpos d6 | Kiwipete d5 | Geomean (6 positions) |
|----------------------|------------:|------------:|----------------------:|
| **ultrachess**       |  **836 Mnps** | **1,562 Mnps** |             **1.00x** |
| cozy-chess 0.3.4     |   600 Mnps  |  1,085 Mnps |                 0.81x |
| shakmaty 0.30.0      |   345 Mnps  |    369 Mnps |                 0.27x |
| chess 3.2.0 (jb)     |   519 Mnps  |    840 Mnps |                 0.52x |


### Native Rust micro-benchmarks, ns per op (lower is better)

| Operation                           | ultrachess | cozy-chess | shakmaty |
|-------------------------------------|-----------:|-----------:|---------:|
| FEN write (startpos)                |    **88**  |        453 |      220 |
| FEN parse (startpos)                |       144  |        151 |  **125** |
| Movegen one-shot (startpos)         |        25  |     **19** |       41 |
| Make + Unmake (48-move cycle)       |       503  |    **353** |    1,292 |
| `isCheck` (not in check)            |  **0.32**  |   **0.32** |     2.04 |
| `isCheck` (in check)                |  **0.33**  |          — |        — |
| Cached Zobrist `hash()`             |  **0.34**  |          — |        — |
| SAN write (48 moves, kiwipete)      | **1.43 µs**|          — |  2.08 µs |
| `clone` (startpos)                  |       3.3  |    **1.7** |      2.5 |

---

## Install

```
npm  install ultrachess
pnpm add     ultrachess
bun  add     ultrachess
deno add     npm:ultrachess
```

Zero runtime dependencies, the `.wasm` ships inside the package and is loaded by whichever entry you import

---

## Entry points

| Import                    | Init                           | Bundle cost                                      | Use when                                                                             |
|---------------------------|--------------------------------|--------------------------------------------------|--------------------------------------------------------------------------------------|
| `ultrachess`            | async (`await Chess.create()`)   | 10 KB TS + fetches 45 KB brotli WASM             | default; anywhere WASM can be fetched.                                               |
| `ultrachess/inline`     | sync (`Chess.createSync()`)      | 10 KB TS + ~60 KB inlined base64 WASM (33% larger) | no top-level await; edge runtimes without fetch / FS; CSP without `wasm-unsafe-eval` |
| `ultrachess/low-level`  | manual `await init()`            | raw ABI, no `Chess` class                        | building a search engine on top; direct linear-memory access.                        |

```ts
// Default — WASM loads on first Chess.create(); no explicit init() needed
import { Chess } from "ultrachess";
const chess = await Chess.create();
```

```ts
// Inline — synchronous; no network, no FS
import { Chess } from "ultrachess/inline";
const chess = Chess.createSync();
```

```ts
// Low-level — raw WASM ABI
import { init, type UltrachessAbi } from "ultrachess/low-level";
const abi: UltrachessAbi = await init();
const handle = abi.ultrachess_new_startpos();
```

SSR tip: `Chess.create()` initialises WASM lazily on first call, so no explicit init is required. If you want to warm the WASM at module load (so the first request doesn't pay the compile cost), import `init` and `await init()` once at boot — it's idempotent. Do not call it per request.

---

## Quick tour

One runnable script covering every core capability.

```ts
import {
  Chess,
  Color,
  MoveKind,
  moveFrom,
  moveTo,
  moveKind,
  moveToUci,
} from "ultrachess";

const chess = await Chess.create();        // WASM loads on first call — no init() needed

// --- Play moves (SAN) ---
chess.move("e4");
chess.move("e5");
chess.move("Nf3");
chess.move("Nc6");

chess.fen();        // "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3"
chess.ascii();      // 8-line grid, rank 8 first
chess.turn();       // Color.White === 0
chess.halfmove();   // 2
chess.fullmove();   // 3

// --- Generate legal moves three ways ---
chess.moves();                    // ["a3", "a4", ..., "Nxe5"] — SAN strings
chess.moves({ verbose: true });   // VerboseMove[] — from, to, piece, captured, san, uci, ...
const packed = chess.moves({ raw: true });  // Move[] — branded u16, zero allocation

// --- Work with a raw Move ---
const m = packed[0];
moveFrom(m);        // square index 0–63
moveTo(m);
moveKind(m);        // MoveKind.Normal | Promotion | EnPassant | Castle
moveToUci(m);       // "e2e4" / "e7e8q"
chess.san(m);       // canonical SAN for this position

// --- Game-end checks ---
chess.inCheck();           // false        — O(1), cached checkers bitboard
chess.isCheckmate();       // false
chess.isStalemate();       // false
chess.isDraw();            // false        — threefold | 50-move | insufficient | stalemate
chess.isGameOver();        // false
chess.hash();              // 0x9d39247e33776d41n — bigint, cached Zobrist

// --- Migrating from chess.js? Object-form move and 8×8 board both work ---
chess.move({ from: "d2", to: "d4" });       // convenience shim; see COMPAT.md
const grid = chess.board();                   // 8×8, row 0 = rank 8, col 0 = file a

// --- Reuse a single instance ---
chess.reset();                                // back to startpos, history cleared
chess.load("8/P7/8/8/8/8/4k3/7K w - - 0 1");  // swap in any FEN

// --- Clone (independent handle) ---
const fork = chess.clone();
fork.move("Bc4");
chess.fen() === fork.fen();  // false

// --- Attacks ---
chess.isAttacked("e5", Color.White);   // true (Nf3 attacks e5)
chess.attackers("e5", Color.White);    // ["f3"]
chess.findPiece({ color: Color.White, type: 1 /* Knight */ });  // ["f3", "b1"]

// --- Perft (one WASM boundary crossing, regardless of depth) ---
chess.perft(4);     // 197281n — bigint

// --- PGN: parse, replay, emit ---
const src = `[Event "Evergreen"]\n[White "Anderssen"]\n\n1. e4 e5 2. Nf3 Nc6 *`;
const game = await Chess.loadPgn(src);
game.header("White");     // "Anderssen"
game.pgn();               // full emitted PGN with seven-tag roster first, 80-col wrap

// --- Dispose (free the WASM handle) ---
chess.dispose();
fork.dispose();
game.dispose();

// Or with TS 5.2+ / Node 22+: automatic disposal at scope end
{
  using game2 = await Chess.create();
  game2.move("e4");
}   // game2.dispose() invoked here, even on throw
```

---

## API

Signatures are TypeScript. The `Chess` class is the primary surface; everything below is exported from `ultrachess`.

### Construction & lifecycle

| Signature                                                | Notes                                           |
|----------------------------------------------------------|-------------------------------------------------|
| `Chess.create(fen?: string): Promise<Chess>`             | Default async construction. Starting position if `fen` is omitted. |
| `Chess.fromFen(fen: string): Promise<Chess>`             | Alias for `create(fen)`.                        |
| `Chess.createSync(fen?: string): Chess`                  | Inline entry only. The inline module auto-calls `initSync()` at import time, so `Chess.createSync()` works with no further setup. |
| `Chess.loadPgn(pgn: string): Promise<Chess>`             | Parse PGN + replay mainline into a fresh instance. |
| `chess.clone(): Chess`                                   | New handle, same position. History is reset. Headers and position-keyed comments travel with the clone. |
| `chess.reset(): void`                                    | Swap back to the starting position in place; clears history, headers, and comments. |
| `chess.load(fen: string): void`                          | Replace the position with `fen` in place; clears history, headers, and comments. Throws `InvalidFenError` on bad input (current position preserved). |
| `chess.dispose(): void`                                  | Free the WASM handle. Required unless using `using`. |
| `chess[Symbol.dispose](): void`                          | TC39 Explicit Resource Management.              |

### Position state

| Signature                                                | Notes                                           |
|----------------------------------------------------------|-------------------------------------------------|
| `fen(): string`                                          | Full FEN including clocks + castling rights.    |
| `hash(): bigint`                                         | Cached 64-bit Zobrist key. O(1).                |
| `turn(): Color`                                          | `Color.White` (0) or `Color.Black` (1).         |
| `halfmove(): number`                                     | 50-move rule counter (0–100).                   |
| `fullmove(): number`                                     | Move number, starts at 1.                       |
| `moveNumber(): number`                                   | Alias for `fullmove()` — chess.js-compatible name. |
| `pieceAt(sq: number \| string): Piece \| null`           | Accepts `0..63` or `"e4"`.                      |
| `ascii(): string`                                        | 8-line grid, rank 8 first, `.` for empty.       |
| `board(): Array<Array<BoardSquare \| null>>`             | 8×8 view, row 0 = rank 8, col 0 = file a. Each cell is `{ square, index, type, color }` or `null`. |

### Game-end checks (all return `boolean`)

| Signature                        | Notes                                            |
|----------------------------------|--------------------------------------------------|
| `inCheck()`                      | O(1); reads the cached checkers bitboard.        |
| `isCheckmate()`                  | `inCheck() && !hasLegalMoves()`.                 |
| `isStalemate()`                  | `!inCheck() && !hasLegalMoves()`.                |
| `isInsufficientMaterial()`       | FIDE-conformant material draw.                   |
| `isThreefoldRepetition()`        | Since last irreversible move (FIDE 9.2 / 9.3).   |
| `isFiftyMoveRule()`              | Halfmove counter ≥ 100.                          |
| `isDraw()`                       | Any of the above four plus stalemate.            |
| `isGameOver()`                   | Checkmate, stalemate, or draw.                   |

### Attacks & queries

| Signature                                                         | Notes                                           |
|-------------------------------------------------------------------|-------------------------------------------------|
| `isAttacked(sq: number \| string, by: Color): boolean`            | Magic-bitboard attack lookup.                   |
| `attackers(sq: number \| string, by: Color): string[]`            | Algebraic squares, ordered by piece iteration.  |
| `findPiece(piece: Piece): string[]`                               | All squares with the given piece.               |

### Move generation & play

| Signature                                                | Notes                                           |
|----------------------------------------------------------|-------------------------------------------------|
| `moves(opts?: { square?, piece? }): string[]`            | SAN for every legal move. Optional `square` / `piece` filters run client-side. |
| `moves(opts: { raw: true, square?, piece? }): Move[]`    | Packed u16, zero-allocation scratch buffer.     |
| `moves(opts: { verbose: true, square?, piece? }): VerboseMove[]` | Full objects with `san`, `uci`, `captured`, ... |
| `legalMoves(): Move[]`                                   | Packed moves, same as `moves({ raw: true })`.   |
| `legalMoveCount(): number`                               | Count only; cheaper than materialising.         |
| `san(move: Move): string`                                | SAN for a packed move in the current position.  |
| `parseSan(san: string): Move`                            | Throws `IllegalMoveError` on invalid SAN.       |
| `verboseMove(move: Move): VerboseMove`                   | Derive a verbose descriptor.                    |
| `move(input: string \| Move \| MoveInput): Move`         | Plays the move. `MoveInput = { from, to, promotion? }` is the chess.js-compatible object form (convenience — the packed `Move` path is still the fast path). |
| `moveFromInput(input: MoveInput): Move`                  | Resolve `{ from, to, promotion? }` to a packed legal move without playing it. |
| `undo(): Move \| null`                                   | Reverse the last move, or `null` if empty.      |
| `history(): Move[]`                                      | Moves played through this instance.             |
| `history(opts: { verbose: true, before?, after? }): VerboseMove[]` | Verbose records; opt-in `before` / `after` FEN capture per ply. |

### Editing (clears history + repetition stack)

| Signature                                                | Notes                                           |
|----------------------------------------------------------|-------------------------------------------------|
| `put(piece: Piece, sq: number \| string): Piece \| null` | Throws on two-king or invalid-piece states.     |
| `remove(sq: number \| string): Piece \| null`            | Returns the removed piece, or `null` if empty.  |

### PGN

| Signature                                                | Notes                                           |
|----------------------------------------------------------|-------------------------------------------------|
| `headers(): Record<string, string>`                      | Insertion order; seven-tag roster first.        |
| `header(key: string): string \| undefined`               | Single-key lookup.                              |
| `setHeader(key: string, value?: string): void`           | Omit `value` to delete.                         |
| `setHeaders(record: Record<string, string \| undefined>): void` | Bulk-set; `undefined` deletes that key.    |
| `pgn(opts?: { newline?, maxWidth? }): string`            | Defaults to `"\n"` / 80-column wrap. Pass `maxWidth: 0` to disable wrapping. |
| `Chess.loadPgn(pgn: string): Promise<Chess>`             | Parse headers + replay the mainline.            |
| `getComment(): string \| undefined`                      | Comment attached to the current position (keyed by Zobrist hash). |
| `setComment(comment: string \| undefined): void`         | Attach a comment to the current position; `undefined` removes it. |
| `removeComment(): string \| undefined`                   | Remove and return the comment at the current position. |
| `getComments(): Array<{ fen, comment }>`                 | Every position-keyed comment reached while walking history, in play order. |
| `removeComments(): void`                                 | Drop every position-keyed comment on this instance. |

### Perft

| Signature                                                | Notes                                           |
|----------------------------------------------------------|-------------------------------------------------|
| `perft(depth: number): bigint`                           | Runs entirely inside WASM; one boundary crossing regardless of depth. |

### Types, enums, helpers

Exported from `ultrachess`:

- `Move` — branded `number`; packed u16 layout (6-bit from, 6-bit to, 2-bit promo, 2-bit kind).
- `moveFrom(m)`, `moveTo(m)`, `moveKind(m)`, `movePromotion(m)`, `moveToUci(m)`.
- `enum Color { White = 0, Black = 1 }`.
- `enum PieceType { Pawn = 0, Knight, Bishop, Rook, Queen, King }`.
- `enum MoveKind { Normal = 0, Promotion, EnPassant, Castle }`.
- `interface Piece { color: Color; type: PieceType }`.
- `interface MoveInput { from: number | string; to: number | string; promotion?: PieceType }`.
- `interface BoardSquare { square: string; index: number; type: PieceType; color: Color }`.
- `interface VerboseMove { fromIndex, toIndex, from, to, piece, color, captured?, promotion?, kind, san, uci, before?, after? }`.
- `squareName(sq: number): string`, `parseSquare(name: string): number | null`, `squareColor(sq): "light" | "dark" | null`.
- `decodePiece(code: number): Piece | null`, `encodePiece(p: Piece): number`, `pieceChar(p: Piece): string`.
- `STARTING_FEN` — the usual constant.

### Errors

Every error extends `Error` and is also exported.

| Error                       | Thrown when                                                     |
|-----------------------------|-----------------------------------------------------------------|
| `DisposedError`             | A method is called on a disposed `Chess` instance.              |
| `IllegalMoveError`          | `move()` or `parseSan()` reject the input.                      |
| `InvalidFenError`           | FEN parse fails or the FEN describes an illegal position.       |
| `InvalidPgnError`           | PGN parse fails, or a mainline SAN is illegal at its position.  |
| `AbiVersionMismatchError`   | The loaded `.wasm` ABI version does not match the TS loader.    |

---

## How it works

**Legal move generation (pin + check masks).** Before emitting moves, compute the enemy checkers bitboard and a `king_danger` mask (squares attacked by the opponent with our king removed). Emit king moves first; on double-check, return. Otherwise derive a `check_mask` (full board when not in check, the checker square plus between-squares for a slider, or just the checker square for a knight or pawn). Split pinned pieces into horizontal-vertical and diagonal sets, each constrained to its pin ray. No pseudo-legal-then-filter pass; every emitted move is legal exactly once. This is what lets one-shot movegen cost 26 ns at startpos.

**Magic bitboards.** Slider attacks (bishop, rook, queen) resolve to one multiply, one shift, one indexed load. Magic tables are generated at module init from a seeded RNG search (~1 ms, cached inside the WASM instance). Bishop ~42 KiB, rook ~800 KiB — both counted in the 157 KB WASM artifact.

**Cached state.** The Zobrist hash and a `checkers: Bitboard` are updated incrementally on every `make_move` / `unmake_move`. `hash()` is a pointer load (0.4 ns native); `inCheck()` is `checkers != 0` (0.38 ns native). This is what makes `isCheckmate()`, `isStalemate()`, and `isDraw()` cheap enough to call inside move-generation loops.

**Zero-copy move enumeration.** A single `Uint32Array` view over WASM linear memory holds the generator's output scratch (512 move slots). `moves({ raw: true })` returns a slice of that view without per-call allocation or GC pressure. The same machinery backs perft's leaf-count fast path (`MoveCounter`: popcount target bitboards without materializing any move).

**WASM ABI.** A fixed, versioned `extern "C"` interface; no `wasm-bindgen` runtime, no `wee_alloc`. An `AbiVersionMismatchError` is thrown when the loader and the `.wasm` disagree on `EXPECTED_ABI_VERSION`. A generational-handle allocator (`slab.rs`) ensures `clone()` yields an independent handle that can outlive its parent.

---

## Correctness

- **Perft.** All six standard positions (startpos, Kiwipete, positions 3–6) match reference node counts to depth 6 (depth 7 for position 3, the en-passant discovered-check catcher). Deep perft (119M–193M node trials per position) runs as a separate CI tier: `just test-deep`.
- **Property tests.** 1.2M+ assertions over 4,000 random games: make/unmake round-trip identity, hash consistency, FEN round-trip, SAN parse/emit round-trip.
- **PGN corpus.** 100% parse + replay + re-emit on the embedded classics (Evergreen, Immortal, Opera, Scholar's Mate, Fool's Mate, Kasparov–Topalov Linares 1999).
- **Runtime sanity gate.** The WASM perft harness refuses to publish numbers unless every leaf count matches the reference.

---

## Runtime support & bundle size

| Runtime                                          | Default entry | `/inline` entry |
|--------------------------------------------------|---------------|-----------------|
| Node ≥18 (tested 18, 20, 22)                     | ✅            | ✅              |
| Bun ≥1.2                                         | ✅            | ✅              |
| Deno ≥1.40                                       | ✅            | ✅              |
| Chromium / Firefox / Safari (evergreen)          | ✅            | ✅              |
| Cloudflare Workers                               | ✅ (bundler must emit the `.wasm`) | ✅ (zero fetch) |
| Vercel Edge                                      | ✅            | ✅              |
| Strict CSP without `wasm-unsafe-eval`            | —             | — (ship the `.wasm` via an allowed URL) |

**Bundle:**

- WASM: 157 KB uncompressed · 56 KB gzip · **45 KB brotli**
- TS bundle (`dist/index.*`): ~10 KB pre-minify, tree-shakes further
- Inline entry: adds ~60 KB (base64 WASM payload)

`sideEffects: false` — unused exports are pruned by Rollup, esbuild, and webpack ≥5.

---

## Limitations

- **No engine.** This is a chess *rules* core. No evaluation, no search, no opening book. Pair with a UCI engine (Stockfish, Lc0) for play.
- **Standard chess only.** No Chess960, atomic, antichess, crazyhouse, or three-check. For variants, `chessops` is excellent.

---

## Coming from chess.js

The API surface is different — construction is async by default, moves are packed 16-bit integers, and legal-move generation never emits an illegal move in the first place (no pseudo-legal pass, no filter step). Full side-by-side differences and the fuzz-gated semantic equivalences are in [`COMPAT.md`](./COMPAT.md).

---

## Build & contribute

```
just build        # cargo + wasm-opt + tsup
just test         # Rust (147 tests) + TS (135 tests)
just test-deep    # billion-node perft (ignored by default)
just bench        # native perft NPS + criterion micro + WASM perft
just coverage     # Rust (cargo-llvm-cov) + TS (vitest v8)
```

Toolchain: Rust ≥1.95 with `wasm32-unknown-unknown`, Node ≥18, `wasm-opt` optional (~30% size reduction).

Pull requests that touch move generation must include a perft diff for all six standard positions. The benchmark recipes refuse to publish numbers from a tree whose tests fail.

---

## License & credits

MIT.

Credits: Peter Ellis Jones for the pin/check-mask legal move-generation algorithm; Pradyumna Kannan and Volker Annuss for the fancy magic bitboard construction; the Stockfish, cozy-chess, and shakmaty projects for prior-art techniques and reference perft tables; Steven Edwards for the PGN specification (1994).