# Compatibility notes

ultrachessjs is **API-inspired** by [chess.js](https://github.com/jhlywa/chess.js)
— it aims at full *feature* parity, but the public surface is redesigned around
the WASM-first architecture. This document is the living reference for places
where we deliberately diverge from chess.js, and known rough edges to watch
during the phase 8 differential-fuzz run.

## Deliberate API differences

| chess.js                 | ultrachessjs                                      | Why |
|--------------------------|---------------------------------------------------|-----|
| `new Chess(fen)` (sync)  | `await Chess.create(fen)` / `Chess.createSync()` | WASM must be instantiated first. The `inline` entry makes sync construction trivial. |
| `chess.move('e4')`       | same                                              | — |
| `chess.move({ from, to })` | `chess.move(chess.parseSan(...))` or construct a `Move` via helpers | We expose packed `Move` (u16) as the canonical form; verbose objects are derived on demand. |
| `chess.moves({ verbose: true })` emits `Move` objects as first-class | same, but with a narrower and more precisely typed `VerboseMove` | Discriminated-ish flags field replaced by a typed `kind: MoveKind` + explicit `captured`/`promotion`. |
| `chess.ascii()` returns a boxed grid with borders | plain 8-line grid (rank 8 first, `.` for empty) | Easier to diff in snapshot tests and align with standard FEN presentation. |
| `chess.history({ verbose: true })` returns full SAN + before/after FEN | returns `VerboseMove[]` (derived by walking history) | `before`/`after` FEN are computable from the Chess instance; omitted by default to avoid per-move FEN work. |
| `chess.pgn()` supports header order control | 7-tag-roster emitted first, remaining headers in insertion order | Matches PGN §8.1 (seven-tag-roster). |
| `chess.put({ type, color }, sq)` | `chess.put({ color, type }, sq)` | Field order is documentation-only; both orders are valid TS object literals. The important difference: `put` **throws** on invalid piece rather than returning `false`. |
| `chess.remove(sq)` returns `false` when empty | returns `null` when empty | `Piece \| null` is the typed return. |
| `chess.isGameOver()` incl. draw rules | same behaviour | — |
| `chess.board()` (8×8 array) | not yet exposed | Planned for phase 6 follow-up if users need it; `pieceAt` + iteration is the current path. |

## Semantic equivalences (must not diverge)

These are hard gates for phase 8's differential fuzz:

- Set of legal moves at every reachable position.
- FEN string after each move (including halfmove / fullmove clocks).
- `isCheck()`, `isCheckmate()`, `isStalemate()` classification at every ply.
- `isDraw()` = insufficient material OR 50-move OR threefold OR stalemate.
- SAN disambiguation: chess.js emits minimum-distinguishing prefix (file, then
  rank, then full square). We match that.
- Castling rights update when king or corner-rook moves/captures.
- En-passant discovered-check rejection (the `8/...KPp4r.../...k.../` class
  of positions).

## Known differences (documented — not bugs)

### 1. `Chess.put` enforces the one-king invariant strictly

chess.js permits transient states with two kings of the same colour during
position editing; we reject. Rationale: every other operation assumes the
invariant holds, and silent violation would miscompute `king_sq` during move
generation.

### 2. `Chess.loadPgn` replay is strict

We require every SAN in the mainline to be legal at its position. chess.js's
permissive parser accepts some over-disambiguated or garbage forms; our
parser is slightly stricter. If you hit a real-world PGN we reject, file it
— it's interesting.

### 3. `history()` is the moves played through this instance, not the whole
game encoded by a loaded PGN

`Chess.loadPgn` replays the PGN through `move()`, so the mainline ends up in
`history()` — matching the chess.js behaviour. If you `Chess.create` then
`put`/`remove`, history is cleared (editing invalidates undo).

### 4. `Chess.hash()` is a **stable** Zobrist key

The 793 random keys are deterministically seeded from a constant (SplitMix64
from `0xC3A5_C85C_97CB_3127`). If this seed ever changes, downstream
transposition tables break. Treat the seed as a public contract.

### 5. Threefold repetition scope

We track repetition *since the last irreversible move* (FIDE 9.2 / 9.3).
Two positions that appear identical but straddle an irreversible move
(capture, pawn push, castling, castling-right change) do **not** count as
repetitions. chess.js does the same.

## Runtime support

| Runtime            | Default entry | `/inline` entry |
|--------------------|---------------|-----------------|
| Node ≥ 18          | ✅ (via `fs`) | ✅               |
| Node < 18          | ❌            | ❌ (needs `atob`) |
| Bun ≥ 1.2          | ✅            | ✅               |
| Deno ≥ 1.40        | ✅            | ✅               |
| Chromium / Firefox / Safari (evergreen) | ✅ | ✅ (>4 KB sync compile emits a dev-tools warning but works) |
| Cloudflare Workers | ✅ (bundler must emit the WASM alongside) | ✅ (zero-fetch) |
| Vercel Edge        | ✅            | ✅               |

Strict CSP environments that set `script-src 'self'` without
`wasm-unsafe-eval`: neither entry will work. Ship the raw `.wasm` and load
it from a URL allowed by CSP.
