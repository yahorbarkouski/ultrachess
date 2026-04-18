//! WASM loader + typed ABI surface.
//!
//! Loads `assets/ultrachess.wasm` once per process and exposes the exported
//! functions. The shim in `chess.ts` layers an ergonomic `Chess` class on
//! top; `low-level.ts` re-exports this module for power users.

export interface UltrachessAbi {
  memory: WebAssembly.Memory;

  // Smoke + version.
  ultrachess_abi_check(a: number, b: number): number;
  ultrachess_abi_version(): number;

  // Scratch areas (pointers + capacities).
  ultrachess_move_scratch_ptr(): number;
  ultrachess_move_scratch_cap(): number;
  ultrachess_string_scratch_ptr(): number;
  ultrachess_string_scratch_cap(): number;
  ultrachess_last_u64_hi(): number;

  // Lifecycle (positions).
  ultrachess_new_startpos(): number;
  ultrachess_new_from_fen(fen_ptr: number, fen_len: number): number;
  ultrachess_free(handle: number): number;
  ultrachess_clone(handle: number): number;

  // State queries.
  ultrachess_hash(handle: number): number; // low 32 bits
  ultrachess_side_to_move(handle: number): number;
  ultrachess_halfmove(handle: number): number;
  ultrachess_fullmove(handle: number): number;
  ultrachess_in_check(handle: number): number;
  ultrachess_is_checkmate(handle: number): number;
  ultrachess_is_stalemate(handle: number): number;
  ultrachess_is_insufficient_material(handle: number): number;
  ultrachess_is_threefold_repetition(handle: number): number;
  ultrachess_is_fifty_move_rule(handle: number): number;
  ultrachess_is_draw(handle: number): number;
  ultrachess_is_game_over(handle: number): number;
  ultrachess_piece_at(handle: number, sq: number): number;

  // Attack queries.
  ultrachess_is_attacked(handle: number, sq: number, by_color: number): number;
  ultrachess_attackers(
    handle: number,
    sq: number,
    by_color: number,
    out_ptr: number,
    cap: number,
  ): number;
  ultrachess_find_piece(
    handle: number,
    color: number,
    piece_type: number,
    out_ptr: number,
    cap: number,
  ): number;

  // Position edits.
  ultrachess_put(handle: number, piece_code: number, sq: number): number;
  ultrachess_remove(handle: number, sq: number): number;

  // Move generation / make / unmake.
  ultrachess_generate_moves(handle: number, out_ptr: number, cap: number): number;
  ultrachess_make_move(handle: number, packed: number): number;
  ultrachess_undo_with_move(handle: number, packed: number): number;

  // Perft.
  ultrachess_perft(handle: number, depth: number): number;

  // Strings.
  ultrachess_fen_write(handle: number, out_ptr: number, cap: number): number;
  ultrachess_san_write(handle: number, packed: number, out_ptr: number, cap: number): number;
  ultrachess_san_parse(handle: number, utf8_ptr: number, utf8_len: number): number;
  ultrachess_ascii_write(handle: number, out_ptr: number, cap: number): number;

  // PGN parsing (separate slab).
  ultrachess_pgn_parse(pgn_ptr: number, pgn_len: number): number;
  ultrachess_pgn_free(handle: number): number;
  ultrachess_pgn_header_count(handle: number): number;
  ultrachess_pgn_header_key(handle: number, idx: number, out_ptr: number, cap: number): number;
  ultrachess_pgn_header_value(handle: number, idx: number, out_ptr: number, cap: number): number;
  ultrachess_pgn_mainline_len(handle: number): number;
  ultrachess_pgn_mainline_san(handle: number, idx: number, out_ptr: number, cap: number): number;
  ultrachess_pgn_termination(handle: number): number;

  // Move introspection.
  ultrachess_move_from(packed: number): number;
  ultrachess_move_to(packed: number): number;
  ultrachess_move_kind(packed: number): number;
  ultrachess_move_promotion(packed: number): number;
}

let cached: Promise<UltrachessAbi> | null = null;
/** Resolved ABI for synchronous access once init has completed. */
let cachedAbi: UltrachessAbi | null = null;

/** Must match `ABI_VERSION` in `rust/wasm/src/lib.rs`. */
export const EXPECTED_ABI_VERSION = 2;

export class AbiVersionMismatchError extends Error {
  constructor(
    public readonly expected: number,
    public readonly actual: number,
  ) {
    super(
      `ultrachess: WASM ABI version mismatch (expected ${expected}, got ${actual}). ` +
        `The bundled .wasm is out of sync with the TS loader.`,
    );
    this.name = "AbiVersionMismatchError";
  }
}

/** Load + instantiate the WASM module. Idempotent. */
export async function init(override?: BufferSource): Promise<UltrachessAbi> {
  if (cached) return cached;
  cached = instantiate(override).catch((err) => {
    cached = null;
    throw err;
  });
  return cached;
}

/** Synchronously instantiate from caller-supplied WASM bytes (used by the
 *  `ultrachess/inline` entry). Primes the async `init()` cache so subsequent
 *  `await init()` calls resolve with the same instance. Throws on version mismatch. */
export function initSync(bytes: BufferSource): UltrachessAbi {
  if (cachedAbi) return cachedAbi;
  const module = new WebAssembly.Module(bytes);
  const instance = new WebAssembly.Instance(module, {});
  const exports = instance.exports as unknown as UltrachessAbi;
  const actualVersion = exports.ultrachess_abi_version();
  if (actualVersion !== EXPECTED_ABI_VERSION) {
    throw new AbiVersionMismatchError(EXPECTED_ABI_VERSION, actualVersion);
  }
  cachedAbi = exports;
  cached = Promise.resolve(exports);
  return exports;
}

/** Resolved ABI if initialised, else `null`. */
export function getAbi(): UltrachessAbi | null {
  return cachedAbi;
}

async function instantiate(override?: BufferSource): Promise<UltrachessAbi> {
  const bytes = override ?? (await fetchWasmBytes());
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const exports = instance.exports as unknown as UltrachessAbi;
  const actualVersion = exports.ultrachess_abi_version();
  if (actualVersion !== EXPECTED_ABI_VERSION) {
    throw new AbiVersionMismatchError(EXPECTED_ABI_VERSION, actualVersion);
  }
  cachedAbi = exports;
  return exports;
}

async function fetchWasmBytes(): Promise<ArrayBuffer> {
  const wasmUrl = new URL("../assets/ultrachess.wasm", import.meta.url);
  if (wasmUrl.protocol === "file:") {
    const { readFile } = await import("node:fs/promises");
    const { fileURLToPath } = await import("node:url");
    const path = fileURLToPath(wasmUrl);
    const buf = await readFile(path);
    return buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength);
  }
  const response = await fetch(wasmUrl);
  if (!response.ok) {
    throw new Error(
      `ultrachess: failed to fetch WASM at ${wasmUrl.href}: ${response.status} ${response.statusText}`,
    );
  }
  return await response.arrayBuffer();
}

export function __resetLoaderForTests(): void {
  cached = null;
  cachedAbi = null;
}

// --------------------------------------------------------------------------
// Linear-memory helpers used by `chess.ts`.
// --------------------------------------------------------------------------

const encoder = /* @__PURE__ */ new TextEncoder();
const decoder = /* @__PURE__ */ new TextDecoder("utf-8");

/** Encode `s` into WASM linear memory at the string-scratch area. Returns
 *  `[ptr, len]`. The scratch buffer is reused — do NOT hold the pointer
 *  across further calls that also write into scratch. */
export function writeStringToScratch(abi: UltrachessAbi, s: string): { ptr: number; len: number } {
  const bytes = encoder.encode(s);
  const ptr = abi.ultrachess_string_scratch_ptr();
  const cap = abi.ultrachess_string_scratch_cap();
  if (bytes.length > cap) {
    throw new RangeError(
      `ultrachess: string (${bytes.length} bytes) exceeds scratch capacity (${cap})`,
    );
  }
  new Uint8Array(abi.memory.buffer, ptr, bytes.length).set(bytes);
  return { ptr, len: bytes.length };
}

/** Read `len` bytes at `ptr` as a UTF-8 string. `len` is clamped to the
 *  available buffer — WASM may return the untruncated full length so the
 *  caller can detect and retry. */
export function readStringFromMemory(abi: UltrachessAbi, ptr: number, len: number): string {
  const buf = abi.memory.buffer;
  const safe = Math.max(0, Math.min(len, buf.byteLength - ptr));
  return decoder.decode(new Uint8Array(buf, ptr, safe));
}

/** Read `len` bytes from the string scratch. Convenience for ABI calls
 *  that write into scratch when `out_ptr == 0`. */
export function readStringScratch(abi: UltrachessAbi, len: number): string {
  return readStringFromMemory(
    abi,
    abi.ultrachess_string_scratch_ptr(),
    Math.min(len, abi.ultrachess_string_scratch_cap()),
  );
}

/** Combine the low-32 return and `ultrachess_last_u64_hi()` into a BigInt. */
export function readU64(abi: UltrachessAbi, lo: number): bigint {
  const hi = abi.ultrachess_last_u64_hi() >>> 0;
  return (BigInt(hi) << 32n) | BigInt(lo >>> 0);
}
