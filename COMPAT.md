# Migrating from chess.js

This document is the reference for engineers porting a chess.js codebase to `ultrachess`. Every row is a behavior that changes or a guarantee we keep; every claim is backed by a specific file in this repository.

Nothing here is a judgement of chess.js. Where we compare behavior, we describe **our** behavior precisely and leave the chess.js side to its own documentation.

## API differences

| chess.js                                     | ultrachess                                                               | Reason                                                                                           |
|----------------------------------------------|--------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| `new Chess(fen?)` (synchronous)              | `await Chess.create(fen?)` / `Chess.createSync(fen?)` (inline entry)     | WebAssembly must be instantiated before any position exists. The `ultrachess/inline` entry embeds the `.wasm` as base64 and exposes a purely-synchronous path. |
| `chess.move('e4')`                           | `chess.move('e4')`                                                       | Identical SAN acceptance path.                                                                   |
| `chess.move({ from, to, promotion })`        | not supported — build a `Move` via `parseSan(...)` or the packed helpers | `Move` is a branded packed `u16` (`moveFrom`, `moveTo`, `moveKind`, `movePromotion`). Object-form `{from, to}` would require an extra legality search per call. |
| `moves({ verbose: true })` → flag-string + 9 fields | same call, `VerboseMove[]` with `kind: MoveKind` + explicit `captured?` / `promotion?` / `uci` | Discriminated, typed enum beats string flags for TS consumers. Every field is populated from the packed `Move` on demand, not stored. |
| `chess.ascii()` → boxed grid with borders    | 8 lines × 8 chars, rank 8 first, `.` for empty, no separators            | Round-trips through snapshot tests; trivially diffable. See `rust/core/src/position.rs:748`.     |
| `history({ verbose: true })` includes `before` / `after` FEN | `VerboseMove[]` only; no embedded FEN                        | `before` / `after` are derivable by walking `history()` or by cloning and replaying. Emitting them by default would run FEN serialization per move (95 ns native but a full boundary crossing in WASM). |
| `chess.pgn({ newline, maxWidth })`           | `chess.pgn()` — seven-tag roster first, remaining headers in insertion order, 80-column wrap | PGN §8.1 (STR: seven-tag roster) and §8.2.6 (80-column wrap). Wrapping is not configurable today. See `src/chess.ts:593`. |
| `chess.put({ type, color }, sq)` → `boolean` | `chess.put({ color, type }, sq)` → `Piece \| null`; **throws** on invalid | Field order in a TS object literal is irrelevant. The important change: we return the replaced piece (or `null`) and throw `RangeError` on an unplaceable piece. See `src/chess.ts:371`. |
| `chess.remove(sq)` → `Piece \| false`        | `chess.remove(sq)` → `Piece \| null`                                     | `Piece \| null` is the typed return. `false` cannot carry a piece, so the union was inconsistent. |
| `chess.board()` → 8×8 array                  | not exposed                                                              | Today: iterate `pieceAt(0)` … `pieceAt(63)`. If you need the array form for a renderer, open an issue. |
| `chess.validate_fen(fen)` (static)           | not exposed                                                              | `Chess.create(fen)` throws `InvalidFenError` on bad input; catch to validate. |
| error returns (`null` / `false`)             | typed exceptions (`IllegalMoveError`, `InvalidFenError`, `InvalidPgnError`, `DisposedError`, `AbiVersionMismatchError`) | Exceptions carry call-site context; return-sentinels do not. |

## Behavior we keep aligned

Reachable chess positions should produce the same primary classifications under both libraries. We enforce this on our side via:

- Perft matches reference node counts at every standard position (`rust/core/tests/perft.rs`).
- Hash / make-unmake / FEN / SAN round-trip identity over thousands of random games (`rust/core/tests/` property suite).

What "aligned" covers:

- The set of legal moves at every reachable position.
- The FEN string after each move, including halfmove and fullmove clocks.
- `inCheck()`, `isCheckmate()`, `isStalemate()` classification at every ply.
- `isDraw()` = insufficient material OR 50-move rule OR threefold repetition OR stalemate (`rust/core/src/position.rs:698`).
- SAN disambiguation: emits the minimum prefix that resolves (file, then rank, then full square). See `rust/core/src/san.rs`.
- Castling rights update when the king or a corner-rook moves or is captured (`castling_clear_table` in `rust/core/src/position.rs:764`).
- En-passant rejection when the capture would expose the moving side's king to a horizontal discovered check (the `"8/…KPp…/…k…"` class).

## Behaviors that are explicitly ours

### 1. `put` enforces the one-king invariant

Placing a king when one of the same color already sits on a different square returns an invalid-args code from the ABI (`rust/wasm/src/lib.rs:312`), which the TS shim turns into a `RangeError`. This is not a validator pass applied after the fact — every move-generation, attack-query, and serialization path in the core assumes exactly one king per side. A second king would silently miscompute `king_sq`.

Workaround: if you need to swap a king's square, `remove` the old square first, then `put` on the new one.

### 2. `Chess.loadPgn` replays strictly

Every SAN in the mainline is fed through `move()` and must be legal at the reached position. The first rejection throws `InvalidPgnError` with the offending ply index and SAN (`src/chess.ts:217`). Over-disambiguated SAN (e.g., `Nbd2` when `Nd2` is unambiguous) is accepted by the SAN parser so long as the constraint is consistent; nonsense tokens are not.

If a real-world PGN that you believe is well-formed is rejected, please file an issue with the exact PGN.

### 3. `history()` is what was played through this instance

`Chess.loadPgn` replays the parsed mainline through `move()`, so the PGN's moves end up in `history()`. A fresh `Chess.create()` followed by `put` / `remove` clears the history (`src/chess.ts:383`) — editing invalidates undo, because the pre-edit state is no longer reachable.

### 4. `clone()` copies TS-side state

The Rust side's `clone` drops the undo stack by design (it's a snapshot of the board, not the game). The TS wrapper layers on top: the clone receives a copy of `moveStack` and the headers map (`src/chess.ts:654`). In practice that means `undo()` works on a clone back to where the parent was, and `pgn()` on a clone emits the same headers.

### 5. `hash()` is a stable Zobrist key

793 keys seeded deterministically via SplitMix64 from `INITIAL_SEED: u64 = 0xC3A5_C85C_97CB_3127` (`rust/core/src/zobrist.rs:17`). Layout:

- 12 × 64 piece-square keys (6 types × 2 colors × 64 squares)
- 16 castling-state keys (all subsets of KQkq)
- 8 en-passant file keys (one per file; only set when an EP capture is actually possible)
- 1 side-to-move key

Positions that are equivalent for FIDE repetition purposes — same pieces, same side to move, same castling rights, same en-passant option — therefore collide on `hash()`. If the seed ever changes, downstream transposition tables break. Treat `INITIAL_SEED` as a public contract.

### 6. Threefold repetition is scoped by the halfmove counter

`isThreefoldRepetition()` counts occurrences of the current hash by walking back exactly `halfmove` positions (`rust/core/src/position.rs:686`). That is FIDE 9.2 / 9.3 in code form: pawn moves and captures reset the 50-move counter, so they also bound the repetition window — positions that straddle such a move cannot have been "the same position" in the FIDE sense anyway.

Castling and a king-move-that-loses-castling-rights do **not** reset the halfmove counter, but they do change the castling-rights portion of the Zobrist key. So two positions that share piece placement but differ in castling rights are different positions, which is the FIDE answer regardless of the lookback rule.

### 7. The WASM handle is explicit

`chess.dispose()` frees the underlying WASM slot. On TypeScript ≥5.2 / Node ≥22, `using` invokes it automatically at scope end (`chess[Symbol.dispose]()` at `src/chess.ts:665`). Calling any method on a disposed instance throws `DisposedError`.

## Not exposed today

These aren't differences with chess.js per se — they're things we simply don't ship yet. All are acceptable PRs if you need them:

- `board()` 8×8 array accessor.
- Chess960 / Fischer random castling.
- Variants (atomic, antichess, crazyhouse, three-check, king-of-the-hill).
- Configurable PGN wrap width or newline style.
- Move notation other than SAN / UCI (e.g., ICCF numeric).

If you're porting chess.js code that relies on any of these, the port will need structural changes, not just a rename.
