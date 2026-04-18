# ultrachessjs

Ultra-fast, WASM-first chess library — Rust core compiled to WebAssembly, with
a thin TypeScript shim providing a modern API.

- **Fully legal move generation** (pin masks + check masks; never emits an
  illegal move).
- **Magic bitboards** for slider attacks — one multiply, one shift, one load.
- **Zero-copy move enumeration** via a shared Uint32 buffer in WASM linear memory.
- **Incremental Zobrist** hashing, threefold repetition, 50-move rule, insufficient
  material, checkmate, stalemate.
- **Full PGN** (parse + replay + emit; headers, NAGs, comments, variations).
- **Runs everywhere**: Node, Bun, Deno, browsers, Cloudflare Workers.

> Status: **under active development**. Phases 0–7 of the build plan are
> complete (see `plan/humming-watching-hamming.md`). Phases 8–9 (benchmarks,
> differential fuzzing vs chess.js, 1.0.0) remain.

---

## Quick start

```ts
import { Chess, init } from "ultrachessjs";

await init(); // instantiate WASM once at app start

const chess = await Chess.create();
chess.move("e4");
chess.move("e5");
chess.move("Nf3");

console.log(chess.fen());          // ...after 1.e4 e5 2.Nf3
console.log(chess.moves());        // SAN strings
console.log(chess.perft(4));       // bigint
console.log(chess.ascii());        // 8×8 grid

chess.dispose();                    // free the WASM handle
```

### Sync entry (no top-level await)

The `ultrachess/inline` entry embeds the WASM as a base64 string and
instantiates it synchronously at import time. Use this when top-level
await is inconvenient:

```ts
import { Chess } from "ultrachessjs/inline";

const chess = Chess.createSync();
chess.move("e4");
```

### Explicit Resource Management (`using`)

If your target supports TC39 ERM (TS 5.2+ / Node ≥22), `using` disposes the
WASM handle at scope end — no manual `.dispose()` needed:

```ts
using chess = await Chess.create();
chess.move("e4");
// chess.dispose() is invoked automatically here
```

---

## API

### Construction

| Method                       | Returns         | Notes |
|------------------------------|-----------------|-------|
| `Chess.create(fen?)`         | `Promise<Chess>`| Requires `await init()` at least once globally. |
| `Chess.fromFen(fen)`         | `Promise<Chess>`| Alias for `create`. |
| `Chess.createSync(fen?)`     | `Chess`         | Available after `initSync()` — i.e. via the `ultrachess/inline` entry. |
| `Chess.loadPgn(pgn)`         | `Promise<Chess>`| Parses + replays. Headers accessible via `header()` / `headers()`. |

### State

`fen()`, `hash(): bigint`, `turn()`, `halfmove()`, `fullmove()`, `pieceAt(sq)`,
`ascii()`.

### Game-end checks

`inCheck()`, `isCheckmate()`, `isStalemate()`, `isInsufficientMaterial()`,
`isThreefoldRepetition()`, `isFiftyMoveRule()`, `isDraw()`, `isGameOver()`.

### Attacks

`isAttacked(sq, byColor)`, `attackers(sq, byColor)` → `string[]`,
`findPiece({ color, type })` → `string[]`.

### Edits (invalidate move history)

`put(piece, sq)`, `remove(sq)`.

### Move generation / play

`moves()` → SAN strings; `moves({ verbose: true })` → `VerboseMove[]`;
`moves({ raw: true })` → packed `Move[]`. Also `legalMoves()`,
`legalMoveCount()`, `san(move)`, `parseSan(san)`, `verboseMove(move)`.

`move(san | Move)` plays a move; `undo()` reverses the last one.
`history()` / `history({ verbose: true })` returns moves played via this
instance.

### PGN

`header(key)`, `setHeader(key, value)`, `headers()`, `pgn()` (emit),
`Chess.loadPgn(str)` (parse + replay).

### Perft + clone

`perft(depth): bigint` — runs entirely inside WASM (one boundary crossing
regardless of node count). `clone()` — independent handle + copied history.

---

## Entry points

| Import                          | Purpose |
|---------------------------------|---------|
| `ultrachessjs`                  | Default. Async init. Smallest JS bundle. Recommended. |
| `ultrachessjs/inline`           | Sync init. WASM embedded as base64 (bundle is ~33% larger, but zero network/FS). |
| `ultrachessjs/low-level`        | Raw `UltrachessAbi` + memory helpers for people building search engines on top. |

Exports are declared via `package.json#exports` so bundlers tree-shake
cleanly. Zero runtime dependencies.

---

## Build & test

```
just build       # cargo + wasm-opt + tsup
just test        # Rust unit + integration + Vitest TS
just test-deep   # deep perft (billion-plus nodes)
```

Targets:
- `wasm-opt` is optional but strongly recommended (~30% size reduction).
- Rust ≥ 1.75 with `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`).
- Node ≥ 18. Also tested on Bun ≥ 1.2.

---

## Performance notes

- **Rust release perft**: 762M nodes across the six standard Perft positions
  in ~0.6 s (on Apple M-series, parallelised by `cargo test`).
- **WASM size**: 112 KB uncompressed, **39 KB brotli**, **46 KB gzip**.
- **TS bundle**: `dist/index.*` ≈ 10 KB pre-minify (tree-shakes further).

All comparisons to chess.js will land in phase 8.

For a level-by-level walkthrough of *how* move generation got this fast —
starting from a naive mailbox board and working up through magics, sinks,
and cached checkers — see [docs/movegen-ladder.md](docs/movegen-ladder.md).

---

## Correctness

- **Perft** — matches reference node counts to depth 6 (or 7 for position 3,
  the en-passant discovered-check catcher) for every standard test position.
- **Randomized property tests** — 1.2M+ assertions green over 4k random
  games: hash consistency, make/unmake identity, FEN round-trip.
- **PGN corpus** — 100% parse + replay on the embedded classics (Evergreen,
  Immortal, Opera, Scholar's Mate, Fool's Mate, Kasparov-Topalov…).

See `COMPAT.md` for known differences from chess.js.
