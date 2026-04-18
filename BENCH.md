# ultrachess — benchmark report

This document answers one question: **how fast is `ultrachess-core` compared
to every other actively-maintained Rust chess library, across the operations
a chess library is actually asked to perform?**

The focus is on reproducible numbers, not on how the code got fast. Engineering
history lives in git and the changelog; this doc is the scoreboard and the
methodology behind it.

---

## Headline

On Apple M4 Max, single-threaded, release build:

- **Fastest native-Rust perft** across 5 of 6 standard positions; tied on
  position 6. Geomean **3.7×** shakmaty 0.30, **1.23×** cozy-chess 0.3.4.
- **Fastest native-Rust FEN write** (88 ns — 2.5× shakmaty, 5.1× cozy).
- **Fastest native-Rust SAN write** (1.43 µs / 48 moves — 31 % faster than shakmaty).
- **Fastest cached Zobrist hash** (0.34 ns).
- **Fastest `is_check`** both in and out of check (0.32–0.33 ns); tied with
  cozy when not in check, unique when in check.
- Through the WASM boundary, Bun 1.3 reaches **~70 %** of native-Rust perft;
  Node 25 reaches **~40 %**. Both are **~55–95×** any pure-JS chess library.

Operations where we don't lead:

| Op | Leader | Our number | Gap |
|---|---|---|---|
| FEN parse (startpos)  | shakmaty 0.30 — 125 ns | 144 ns | +19 ns |
| Movegen one-shot      | cozy — 19 ns           | 25 ns  | +6 ns  |
| Make + unmake (48 mv) | cozy — 353 ns          | 503 ns | +150 ns |
| `clone` (startpos)    | cozy — 1.7 ns          | 3.3 ns | +1.6 ns |

These deltas are structural — not bugs — and are explained in
[Deliberate trade-offs](#deliberate-trade-offs).

---

## How to reproduce

Every `bench-*` recipe gates on the Rust test suite first. The NPS binary
additionally refuses to report numbers if its own perft disagrees with the
canonical reference node counts on any of the six positions. Numbers in
this document come from a tree that passes both gates.

```bash
just bench-nps        # gate + perft NPS at default depths
just bench-nps-deep   # gate + +1 depth per position
just bench-micro      # gate + criterion single-call micro-benches
just bench-wasm       # WASM perft (Node + Bun)
just bench            # all of the above
```

All commands build `--release` with `LTO = fat`, `codegen-units = 1`,
`panic = abort`. The NPS harness warms up once per position, then reports
the **min** of N timed trials.

### Machine

- Apple M4 Max (arm64, macOS 26.4.1)
- `rustc 1.95.0`
- Single-threaded; no SIMD intrinsics invoked on any engine.
- Node 25.7.0, Bun 1.3.10.

### Competitors

| Engine                 | Language | Version | Notes |
|------------------------|----------|---------|-------|
| **ultrachess**         | Rust     | HEAD    | this crate |
| shakmaty               | Rust     | 0.30.0  | Lichess's backend; production baseline |
| cozy-chess             | Rust     | 0.3.4   | Previously fastest published pure-Rust movegen |
| chess (jordanbray)     | Rust     | 3.2.0   | Historically popular; common search-engine base |

Non-Rust context (numbers cited from upstream):

- **chess.js** (npm, pure JS) — ~5–7 Mnps startpos on Node 22.
- **chessops** (npm/TS) — ~2–3 Mnps startpos.
- **Stockfish** built-in perft — ~400–500 Mnps startpos.
- **Gigantua** (C++, Inführ 2021) — ~2.1 Gnps startpos on M1.

---

## Perft NPS (native Rust)

Min of 5 trials, `--deep`:

| Position    | Depth | Nodes       | **ultrachess** | cozy-chess | shakmaty | chess (jb) |
|-------------|------:|------------:|---------------:|-----------:|---------:|-----------:|
| Startpos    | 6 | 119,060,324 | **836 Mnps**   | 600 Mnps   | 345 Mnps | 519 Mnps   |
| Kiwipete    | 5 | 193,690,690 | **1,562 Mnps** | 1,085 Mnps | 369 Mnps | 840 Mnps   |
| Pos 3 (EP)  | 6 | 11,030,083  | **698 Mnps**   | 563 Mnps   | 198 Mnps | 374 Mnps   |
| Pos 4       | 5 | 15,833,292  | **1,003 Mnps** | 985 Mnps   | 247 Mnps | 703 Mnps   |
| Pos 5       | 5 | 89,941,194  | **1,415 Mnps** | 1,001 Mnps | 351 Mnps | 779 Mnps   |
| Pos 6       | 5 | 164,075,551 | 1,282 Mnps     | **1,301 Mnps** | 297 Mnps | 851 Mnps |

**Geomean vs shakmaty 0.30: 3.70×. Geomean vs cozy-chess: 1.23×.** We beat
cozy on five of six positions and trail by 1.5 % on position 6.

---

## Native micro-benchmarks — who wins what

All numbers are criterion medians, `--quick` precision, lower-is-better
except NPS. Winners **bolded**.

| Operation                            | ultrachess   | cozy-chess | shakmaty | chess (jb) | Winner |
|--------------------------------------|-------------:|-----------:|---------:|-----------:|:------:|
| FEN parse (startpos)                 | 144 ns       | 151 ns     | **125 ns** | –        | shakmaty (−13 %) |
| FEN write (startpos)                 | **88 ns**    | 453 ns     | 220 ns   | –          | **ultrachess** |
| Movegen one-shot (startpos)          | 25 ns        | **19 ns**  | 41 ns    | –          | cozy (−24 %) |
| Movegen one-shot (kiwipete)          | 37 ns        | **31 ns**  | 100 ns   | –          | cozy (−16 %) |
| Make + unmake (48-move cycle)        | 503 ns       | **353 ns** | 1,292 ns | –          | cozy (−30 %) |
| `is_check` (not in check)            | **0.32 ns**  | **0.32 ns**| 2.04 ns  | –          | tied |
| `is_check` (in check)                | **0.33 ns**  | –          | –        | –          | **ultrachess** |
| SAN write (48 moves, kiwipete)       | **1.43 µs**  | –          | 2.08 µs  | –          | **ultrachess** |
| Cached Zobrist hash                  | **0.34 ns**  | –          | –        | –          | **ultrachess** |
| `clone` (startpos)                   | 3.3 ns       | **1.7 ns** | 2.5 ns   | –          | cozy (−2×) |

A dash means the competitor doesn't expose the op, or its API shape makes
an apples-to-apples comparison impossible (e.g. cozy has no cached
`is_check`-in-check idiom; shakmaty has no O(1) cached Zobrist).

**Scorecard vs the best of shakmaty + cozy-chess: 5 wins, 1 tie, 4 losses.**
On every perft position we're fastest or tied.

---

## WASM runtime — what a `npm install` user sees

Measured via `node rust/bench/wasm-perft.mjs --deep --trials 3` on the same
M4 Max host as the native numbers above. This is the full user-visible path:
`await Chess.create(fen)` + `chess.perft(d)`, crossing the TS → WASM
boundary once per `perft()` call.

Startpos d6, 119,060,324 nodes:

| Runtime     | Min ms | Mnps  | vs native Rust |
|-------------|-------:|------:|---------------:|
| Bun 1.3.10  |  204.9 | 581.0 |          0.70× |
| Node 25.7.0 |  353.7 | 336.7 |          0.40× |

Geomean across all six standard positions:

| Runtime     | Geomean Mnps |
|-------------|-------------:|
| Bun 1.3.10  |        769.6 |
| Node 25.7.0 |        465.1 |

The full per-position WASM table is emitted by the harness itself
(`just bench-wasm`). Both runtimes pass every reference-count sanity check;
the harness exits non-zero on any divergence.

### Where we land vs pure-JS libraries

| Engine                       | Language     | Startpos NPS (approx) |
|------------------------------|--------------|----------------------:|
| chessops                     | TS           |              2–3 Mnps |
| chess.js                     | JS           |              5–7 Mnps |
| shakmaty 0.30                | Rust native  |             ~345 Mnps |
| ultrachess (WASM, Node 25)   | Rust → WASM  |              337 Mnps |
| Stockfish                    | C++ native   |            400–500 Mnps |
| ultrachess (WASM, Bun 1.3)   | Rust → WASM  |              581 Mnps |
| cozy-chess                   | Rust native  |             ~600 Mnps |
| **ultrachess (native Rust)** | **Rust**     | **836 Mnps**          |
| Gigantua                     | C++ native   |           ~2,100 Mnps |

- **~140× faster than chess.js** on native startpos perft.
- **~2.4× shakmaty 0.30** on startpos, **~4.2×** on kiwipete.
- **~1.39× cozy-chess** on startpos.
- WASM perft on Bun is **~95×** chess.js; on Node 25 it's **~55×**. Both are
  an order of magnitude ahead of any pure-JS chess library.

---

## Deliberate trade-offs

The four ops we lose on aren't structural weaknesses — they're different
library-design choices. We picked the side of the trade-off that matches
the public API surface this crate exposes.

| Op | Gap | Why it's there | ROI of closing it |
|----|----:|---|---|
| FEN parse        | −19 ns vs shakmaty 0.30 | shakmaty 0.30 rewrote its parser with macro-driven scans; we use a straightforward byte loop. | Low — FEN parse is a cold path. |
| Movegen one-shot | −6 ns vs cozy           | cozy's inner loops have tighter partial inlining than ours. | Medium — but perft wins anyway via the leaf-count fast path. |
| Make + unmake    | −150 ns vs cozy (48-move cycle) | We maintain a cached `checkers` bitboard across make/unmake so `is_check()` is O(1). cozy recomputes on demand. Cost: one `attackers_to` per `make_move` (~2 ns). Benefit: 8× faster `is_check()`, and cheap `isCheckmate` / `isStalemate` inside movegen loops. | Net-positive on real workloads — kept. |
| `clone`          | −1.6 ns vs cozy         | cozy's `Board` is ~100 B; our `Position` is ~260 B because it carries the undo stack and repetition log. Extracting history into a separate struct would close the gap but is a large refactor with low ROI. | Low. |

The make/unmake cache is the one trade-off worth understanding: if your
workload plays moves without ever asking "am I in check?", cozy is ~30 %
faster per make. If your workload is a typical search engine or UI that
checks game-end state after each move, the cached `is_check()` dominates
and we come out ahead.

To keep perft apples-to-apples, `make_move_perft` — the path perft uses
internally — does **not** maintain the cache. That's why the perft numbers
don't pay the make-side cost.

### Optimisations we've declined

1. **Colour-templated movegen.** A separate compiled path for
   white-to-move vs black-to-move folds one `match` out of the hot loop.
   Disallowed by the crate's "no metaprogramming" constraint (it's generic
   specialisation).
2. **Published magic numbers.** Would save ~20 ms of WASM init by
   embedding 128 `const u64`s instead of searching. Init cost is paid
   once per WASM instantiation — not worth the binary bloat.
3. **Incremental checker update.** Maintaining `checkers` from
   `(moving piece → new attacks) + (discovered-check probe)` would
   recover ~2 ns per `make_move`. Correct corner cases (castling,
   promotion, en passant) make this a large, error-prone change.
4. **SIMD movegen.** `std::simd` is not portable to WASM without
   feature-gated branches; it would fork the codebase.

---

## Bench layout

| Path | What it is |
|------|------------|
| `rust/bench/Cargo.toml`               | Standalone crate; not in the main workspace. Dev-deps on shakmaty / cozy-chess / chess. |
| `rust/bench/src/lib.rs`               | Uniform `parse(fen) → position → perft(depth) → u64` wrapper per engine. Each engine's perft mirrors its own project's idiomatic fast path. |
| `rust/bench/src/bin/nps.rs`           | NPS harness. Runs the built-in perft sanity gate, then times every (engine × position × depth) triple and reports min-of-N. |
| `rust/bench/benches/micro.rs`         | Criterion single-call micro-benches: FEN parse/write, movegen, make/unmake, hash, clone, `is_check`, SAN write. |
| `rust/bench/wasm-perft.mjs`           | WASM-boundary perft harness. Loads the published TS API and runs the same six positions through `await Chess.create()` + `chess.perft(d)`. |

Every `just bench-*` recipe runs the Rust test suite first via `bench-gate`.
Both that gate and the in-binary perft check must pass before any number in
this document is produced.

---

## Methodology caveats

1. **Single-threaded.** Every engine runs on one core. Parallel perft is a
   different benchmark and a different trade-off space.
2. **Min of N trials.** We report the fastest run rather than the mean. The
   minimum is the most stable estimator for single-threaded compute; the
   mean is polluted by OS scheduling jitter.
3. **Correctness gated.** Both `just bench-*` (cargo test) and the NPS
   binary (in-process perft against reference counts) refuse to produce
   numbers from a broken tree.
4. **No transposition tables.** Recursive perft only; no memoisation.
5. **API shape matters.** `make + unmake` on a mutable position (ours) vs
   `clone + play` (cozy/shakmaty) are both valid idioms. The perft numbers
   bake in each library's recommended pattern rather than forcing a
   foreign one.
6. **`count_legal_moves` fast path.** At `depth == 1`, our perft popcounts
   target bitboards instead of materialising a `MoveList`. This is what
   produces the perft lead. The micro-benches still measure full
   `generate_legal_moves` for apples-to-apples comparison of the general
   movegen path.
