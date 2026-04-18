# Migrating from chess.js

Reference for engineers porting a chess.js codebase to `ultrachess`. Each
row either calls out a behavior that changes, a shim we added for
migration ergonomics, or an alignment we went out of our way to keep.

Nothing here is a judgement of chess.js. Where behavior is compared, we
describe **our** behavior precisely and leave the chess.js side to its own
documentation. Symbol references (e.g. `Chess.put` in `src/chess.ts`) are
stable; line numbers drift, so we don't cite them.

## API differences

| chess.js | ultrachess | What changed |
|---|---|---|
| `new Chess(fen?)` (sync) | `await Chess.create(fen?)` — or `Chess.createSync(fen?)` from `ultrachess/inline` | WebAssembly must be instantiated before a position can exist. The inline entry embeds the `.wasm` as base64 and exposes a purely-synchronous constructor. |
| `chess.move('e4')` | `chess.move('e4')` | Unchanged. Identical SAN acceptance path. |
| `chess.move({ from, to, promotion? })` | same — convenience shim (`resolveMoveInput` in `src/chess.ts`) | Shim scans the legal-move list for a from/to/promotion match and resolves to a packed `Move`. The packed `Move` path is still the documented fast path — the object form is for migration ergonomics, not inner loops. |
| — (chess.js has no equivalent) | `chess.moveFromInput({ from, to, promotion? })` | Resolves without playing. |
| `moves({ verbose: true })` → flag-string + 9 fields | same call, `VerboseMove[]` with `kind: MoveKind` + explicit `captured?` / `promotion?` / `uci` | Discriminated, typed enum beats string flags for TS consumers. Every field is populated from the packed `Move` on demand, not stored. |
| `moves({ square, piece })` | same — client-side filter | No extra WASM boundary crossing: the filter runs over the already-materialised packed moves. |
| `chess.ascii()` → boxed grid with borders | 8 lines × 8 chars, rank 8 first, `.` for empty, no separators (see `Position::ascii` in `rust/core/src/position.rs`) | Round-trips through snapshot tests; trivially diffable. |
| `chess.board()` → 8×8 array of `{ square, type, color } \| null` | `chess.board()` → 8×8 array of `BoardSquare \| null` (row 0 = rank 8, col 0 = file a) | Returned cells include our enum-typed `type` / `color` plus `square` and `index`, matching chess.js's row/col orientation. |
| `history({ verbose: true })` — `before` / `after` FEN populated by default | opt in via `history({ verbose: true, before: true, after: true })` | FEN serialisation per ply is cheap natively but a full boundary crossing in WASM. Default stays lean; callers that want FENs pay for them explicitly. |
| `chess.pgn({ newline, maxWidth })` | same signature — defaults `"\n"` and 80-column wrap; pass `maxWidth: 0` to disable wrapping (see `Chess.pgn` in `src/chess.ts`) | PGN §8.1 (STR: seven-tag roster) and §8.2.6 (80-column wrap). |
| `chess.reset()` / `chess.load(fen)` / `chess.clear()` | `chess.reset()` / `chess.load(fen)` — no `clear()` yet | `reset()` and `load(fen)` recycle the underlying WASM slab slot in place and clear history, headers, and position-keyed comments. `clear()` is tracked — it needs a new `ultrachess_new_empty()` export. |
| `chess.put({ type, color }, sq)` → `boolean` | `chess.put({ color, type }, sq)` → `Piece \| null`; **throws `RangeError`** on invalid (see `Chess.put` in `src/chess.ts`) | Field order in a TS object literal is irrelevant. The real change: we return the replaced piece (or `null`) instead of a success boolean, and throw on an unplaceable piece (see [one-king invariant](#1-put-enforces-the-one-king-invariant)). |
| `chess.remove(sq)` → `Piece \| false` | `chess.remove(sq)` → `Piece \| null` | `Piece \| null` is the typed return. `false` cannot carry a piece, so the original union was inconsistent. |
| `chess.header(k, v, ...)` (variadic) | `chess.header(k)` (getter) / `chess.setHeader(k, v)` / `chess.setHeaders({ ... })` | Our getter keeps a clean `string \| undefined` return type; bulk-setting is via a record. |
| `chess.moveNumber()` / `chess.fullmove()` | both names accepted — aliases | Same underlying field; two names smooth migration and reduce churn in ported code. |
| `chess.squareColor('a1')` | exported as a module helper `squareColor(sq)` | Pure TS helper returning `"light" \| "dark" \| null`. Accepts algebraic or 0..63 index. |
| `chess.getComment()` / `setComment(c)` / `getComments()` / `removeComment()` / `removeComments()` | same names — keyed by Zobrist `hash()` | Comments live in a TS-side `Map<bigint, string>`; the Rust core stays unaware. `getComments()` walks the played history and emits the comments at each reached position in order. |
| `chess.validate_fen(fen)` (static) | not exposed | `Chess.create(fen)` / `chess.load(fen)` throw `InvalidFenError` on bad input; catch to validate. |
| sentinel returns (`null` / `false`) | typed exceptions (`IllegalMoveError`, `InvalidFenError`, `InvalidPgnError`, `DisposedError`, `AbiVersionMismatchError`) | Exceptions carry call-site context; return-sentinels do not. |
| — | `chess.dispose()` / `using chess = ...` | The WASM handle is explicit. See [§7 below](#7-the-wasm-handle-is-explicit). |

Every shim listed above sits on top of the existing WASM ABI — none of
them perturb the movegen / perft / hash hot paths. The shim for
`move({ from, to, promotion? })` is exercised indirectly by the
[differential fuzzer](test/differential/vs-chess-js.test.ts): ultrachess
emits legal moves, both libraries replay the SAN, and a divergence on
either side fails the test.

## Semantic alignment with chess.js

Reachable positions produce the same primary classifications under both
libraries. Aligned on our side via the reference-perft gate
(`rust/core/tests/perft.rs`) and the make-unmake / FEN / SAN / hash
round-trip property suite in `rust/core/tests/`.

What "aligned" covers:

- The set of legal moves at every reachable position.
- The FEN string after each move, including halfmove and fullmove clocks.
- `inCheck()`, `isCheckmate()`, `isStalemate()` at every ply.
- `isDraw()` = insufficient material OR 50-move rule OR threefold repetition OR stalemate (`Position::is_draw` in `rust/core/src/position.rs`).
- SAN disambiguation: emits the minimum prefix that resolves (file, then rank, then full square). See `rust/core/src/san.rs`.
- Castling rights update when the king or a corner-rook moves or is captured (`castling_clear_table` in `rust/core/src/position.rs`).
- En-passant rejection when the capture would expose the moving side's king to a horizontal discovered check (the `"8/…KPp…/…k…"` class).

## Behaviors that are explicitly ours

### 1. `put` enforces the one-king invariant

Placing a king when one of the same color already sits on a different
square returns an invalid-args code from the ABI (`ultrachess_put` in
`rust/wasm/src/lib.rs`), which the TS shim turns into a `RangeError`.
This is not a validator pass applied after the fact — every
move-generation, attack-query, and serialization path in the core assumes
exactly one king per side. A second king would silently miscompute
`king_sq`.

Workaround: if you need to relocate a king, `remove` the old square
first, then `put` on the new one.

### 2. `Chess.loadPgn` replays strictly

Every SAN in the mainline is fed through `move()` and must be legal at
the reached position. The first rejection throws `InvalidPgnError` with
the offending ply index and SAN (see `Chess.loadPgn` in `src/chess.ts`).
Over-disambiguated SAN (e.g. `Nbd2` when `Nd2` is unambiguous) is
accepted by the SAN parser so long as the constraint is consistent;
nonsense tokens are not.

If a real-world PGN you believe is well-formed is rejected, please file
an issue with the exact PGN.

### 3. `history()` is what was played through this instance

`Chess.loadPgn` replays the parsed mainline through `move()`, so PGN
moves end up in `history()`. A fresh `Chess.create()` followed by `put`
/ `remove` clears the history — editing invalidates undo, because the
pre-edit state is no longer reachable. Same for `reset()` and
`load(fen)`: they replace the position, so the old history no longer
applies.

### 4. `clone()` copies TS-side state but drops history

The Rust core's `clone` drops the undo stack by design (it's a snapshot
of the board, not the game). The TS wrapper layers on top: the clone
receives a copy of the headers map and the position-keyed comment map
(see `Chess.clone` in `src/chess.ts`). The returned instance has **no**
move history — a clone is "the same board state, ready for a fresh
game," equivalent to `await Chess.create(original.fen())`. `pgn()` on a
clone emits the same headers; comments attached to positions that
survive in the clone's eventual play are still recalled by Zobrist hash.

### 5. `hash()` is a stable Zobrist key

793 keys seeded deterministically via SplitMix64 from
`INITIAL_SEED: u64 = 0xC3A5_C85C_97CB_3127` (see `rust/core/src/zobrist.rs`).
Layout:

- 12 × 64 piece-square keys (6 types × 2 colors × 64 squares) = 768
- 16 castling-state keys (all subsets of KQkq)
- 8 en-passant file keys (one per file; only set when an EP capture is actually possible)
- 1 side-to-move key

Positions that are equivalent for FIDE repetition purposes — same
pieces, same side to move, same castling rights, same en-passant option
— therefore collide on `hash()`. If the seed ever changes, downstream
transposition tables break. Treat `INITIAL_SEED` as a public contract.

### 6. Threefold repetition is scoped by the halfmove counter

`isThreefoldRepetition()` counts occurrences of the current hash by
walking back exactly `halfmove` positions
(`Position::is_threefold_repetition` in `rust/core/src/position.rs`).
That is FIDE 9.2 / 9.3 in code form: pawn moves and captures reset the
50-move counter, so they also bound the repetition window — positions
that straddle such a move cannot have been "the same position" in the
FIDE sense anyway.

Castling and a king-move-that-loses-castling-rights do **not** reset the
halfmove counter, but they do change the castling-rights portion of the
Zobrist key. So two positions that share piece placement but differ in
castling rights are different positions — which is the FIDE answer
regardless of the lookback rule.

### 7. The WASM handle is explicit

`chess.dispose()` frees the underlying WASM slot. On TypeScript ≥5.2 /
Node ≥22, `using` invokes it automatically at scope end via
`chess[Symbol.dispose]()` (see `src/chess.ts`). Calling any method on a
disposed instance throws `DisposedError`.

## Not exposed today

Not differences with chess.js per se — things we simply don't ship yet.
All are acceptable PRs if you need them:

- `clear()` — needs a new `ultrachess_new_empty()` ABI export. In the
  interim: `chess.load("8/8/8/8/8/4k3/8/4K3 w - - 0 1")` or equivalent
  with kings on any two squares (the one-king-per-side invariant still
  applies).
- `getCastlingRights(color)` / `setCastlingRights(...)` — readable via
  `fen()` today, but a dedicated accessor needs a new ABI call.
- `getEnPassantSquare()` / `setEnPassantSquare(...)` — same story.
- PGN annotation exposure — the Rust core already parses `{...}`
  comments, `;...` line comments, `$N` NAGs, and `(...)` variations
  into a structured tree. The WASM ABI doesn't surface these accessors
  yet, so `Chess.loadPgn` currently drops them when replaying the
  mainline.
- Lenient PGN parsing (`{ strict: false }` mode).
- Chess960 / Fischer random castling.
- Variants (atomic, antichess, crazyhouse, three-check, king-of-the-hill).
- Move notation other than SAN / UCI (e.g. ICCF numeric).

If you're porting chess.js code that relies on any of these, the port
will need structural changes, not just a rename.
