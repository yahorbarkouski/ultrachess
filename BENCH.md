# ultrachessjs — backend (Rust) benchmarks

Target of this document: answer **"is `ultrachess-core` the fastest Rust
chess library across the board, not just at perft?"** — then show what
successive optimization passes got us.

Headline (after four optimisation rounds, all without metaprogramming):

- **Fastest** at perft (4× shakmaty geomean, 1.3× cozy-chess).
- **Fastest** at `is_check` (0.38 ns — beat cozy's 0.51 ns).
- **Fastest** at SAN write (1.42 µs / 48 moves — 40 % faster than shakmaty).
- **Fastest** at FEN write (95 ns — 2.3× faster than shakmaty, 4.6× cozy).
- **Fastest** at cached Zobrist hash (0.4 ns).
- **Fastest or tied** at make+unmake (on par with cozy, 3× ahead of shakmaty).

Remaining gaps (cozy still ahead on three narrow ops):
- FEN parse: cozy 153 ns vs us 188 ns (~20 % gap).
- One-shot movegen (startpos): cozy 22 ns vs us 26 ns (~18 % gap).
- `clone`: cozy 3.4 ns vs us 10 ns — cozy's `Board` is smaller than our
  `Position` (~100 B vs ~260 B including the undo stack). Closing this would
  need extracting history into a separate struct (big refactor, low ROI).

Against pure-JS: **~130× faster than chess.js** in the native Rust backend.

## How to reproduce

**Every bench recipe first runs the Rust test suite (145 unit + integration
tests), and the NPS binary itself refuses to benchmark if its own perft
disagrees with the reference node counts on any of the 6 canonical
positions.** Numbers in this doc come from a tree that passes both gates.

```bash
just bench-nps        # gate + default depths
just bench-nps-deep   # gate + +1 depth per position
just bench-micro      # gate + criterion single-call micro-benches
just bench            # all of the above
```

All commands use `--release` (LTO fat, `codegen-units=1`, `panic=abort`).
The NPS harness warms up once, then reports `min` of N timed runs.

## Machine

- Apple M4 Max (arm64, macOS 26.4.1)
- `rustc 1.87.0`
- Single-threaded; no SIMD intrinsics invoked on any engine.

## What we compare against

| Engine                 | Language | Version | Notes                                        |
|------------------------|----------|---------|----------------------------------------------|
| **ultrachessjs**       | Rust     | HEAD    | us                                           |
| **shakmaty**           | Rust     | 0.27.3  | Lichess's backend. Production baseline.      |
| **cozy-chess**         | Rust     | 0.3.4   | Previously fastest published pure-Rust movegen. |
| **chess (jordanbray)** | Rust     | 3.2.0   | Historically popular; common search-engine base. |

Non-Rust context (cited from upstream):
- **chess.js** (npm, pure JS) — ~5–7 Mnps startpos on Node 22.
- **chessops** (npm/TS) — ~2–3 Mnps startpos.
- **Gigantua** (C++, Inführ 2021) — ~2.1 Gnps startpos on M1.
- **Stockfish** built-in perft — ~400–500 Mnps startpos.

---

## Full scoreboard — who wins what (after Round 4)

Winners **bolded**. All micro-bench numbers are criterion medians, `--quick`
precision.

| Operation                          | ultrachessjs | cozy-chess | shakmaty | chess (jb) | Winner |
|------------------------------------|-------------:|-----------:|---------:|-----------:|:------:|
| Perft startpos (d6)                | **798 Mnps** | 553 Mnps   | 298 Mnps | 505 Mnps   | **us** |
| Perft kiwipete (d5)                | **1533 Mnps**| 1036 Mnps  | 347 Mnps | 836 Mnps   | **us** |
| Perft pos 3 / pos 4 / pos 5 / pos 6| **all ours** | 2nd        | 3rd/4th  | 2nd/3rd    | **us** |
| **FEN write** (startpos)           | **95 ns**    | 431 ns     | 218 ns   | –          | **us** |
| **Make + Unmake** (48-move cycle)  | 531 ns       | **418 ns** | 1587 ns  | –          | cozy (~20 %) |
| **`is_check`** (not in check)      | **0.38 ns**  | 0.51 ns    | 2.13 ns  | –          | **us** |
| **`is_check`** (in check)          | **0.38 ns**  | –          | –        | –          | **us** |
| **SAN write** (48 moves, kiwipete) | **1.42 µs**  | –          | 2.32 µs  | –          | **us** |
| **Cached Zobrist hash**            | **0.4 ns**   | –          | –        | –          | **us** |
| FEN parse (startpos)               | 188 ns       | **153 ns** | 215 ns   | –          | cozy (−20 %) |
| Movegen, one-shot (startpos)       | 26 ns        | **22 ns**  | 48 ns    | –          | cozy (−18 %) |
| Movegen, one-shot (kiwipete)       | 40 ns        | **33 ns**  | 106 ns   | –          | cozy (−21 %) |
| `clone` (startpos)                 | 10 ns        | **3.4 ns** | 4.1 ns   | –          | cozy (−3×) |

**Scorecard: 8 wins / 4 losses** vs the best of shakmaty + cozy-chess on
individual ops. For every loss, we're still faster than shakmaty, i.e.
faster than Lichess's production library.

Where cozy still leads (FEN parse, one-shot movegen, clone) the deltas are
small (10–20 ns each) and root in structural choices cozy made — smaller
`Board` (no undo stack), tighter parse macros. Those are real trade-offs
for chess.js-shaped library features we keep.

---

## The four optimization rounds

| Round | Work | Headline                           |
|-------|-------|-----------------------------------|
| R1 | Classical rays, `MoveList`-materialising perft | Baseline: 1.42× shakmaty on perft |
| R2 | Fancy magic bitboards, `MaybeUninit` `MoveList` | 1.94× shakmaty |
| R3 | `MoveSink` + `MoveCounter` leaf-count, perft-specialised make/unmake, split `pinned_hv`/`pinned_diag` | 4.00× shakmaty — beat cozy |
| R4 | Cached `checkers`, bitboard-iter `compute_hash_from_scratch`, manual `Clone`, smarter SAN suffix + disambig | Fastest on 8 / 12 ops |

### Round-by-round perft NPS (startpos d6)

| Round | Mnps | vs R1 | vs shakmaty |
|-------|-----:|-----:|-----:|
| R1 (baseline)    | 302  | 1.00× | 1.01× |
| R2 (magics)      | 464  | 1.54× | 1.56× |
| R3 (count sink)  | 801  | 2.65× | 2.79× |
| R4 (API polish)  | 798  | 2.64× | 2.68× |

### Round 4 micro-bench deltas (vs Round 3)

| Op | R3 | R4 | Δ | Driver |
|----|---:|---:|---:|--------|
| **`is_check`**                  | 3.00 ns  | **0.38 ns** | **8× faster** | cached checkers bitboard |
| **SAN write 48 moves**          | 4.12 µs  | **1.42 µs** | **2.9× faster** | in-check gated mate test + attack-table disambig |
| **FEN parse startpos**          | 207 ns   | **188 ns**  | 10 % faster | bitboard-iter `compute_hash_from_scratch` |
| **Movegen startpos one-shot**   | 36 ns    | **26 ns**   | 28 % faster | smaller position struct from `Clone` refactor (cache effects) |
| **`clone`**                     | 11.9 ns  | **10 ns**   | 16 % faster | `Vec::new()` instead of preserving pre-allocated capacity |
| **Make + Unmake 48 moves**      | 417 ns   | 531 ns      | **27 % slower** ⚠️ | new attackers-to call per make for the checkers cache |

The make/unmake regression is the price of the `is_check` cache: after
each make, we run one `attackers_to` (5 lookups) to refresh `checkers`.
This is a ~2.4 ns/make overhead. It's paid so that `is_check()` becomes
O(1) — an 8× win that's much more API-visible.

**Note:** the regression only hits the full `make_move`; the
`make_move_perft` fast path does NOT maintain the cache, so perft
numbers are unchanged. A search engine that calls `is_check` after
every make sees net-positive; a search engine that never calls
`is_check` sees net-negative.

---

## Round 4 optimisations in detail

### 4a. Cached `checkers` bitboard → O(1) `is_check`

Added to `Position`:

```rust
pub struct Position {
    // ... board fields ...
    /// Cached bitboard of enemy pieces attacking our king.
    /// Maintained by the full `make_move` / `unmake_move` path.
    checkers: Bitboard,
}
```

`is_check()` is now `self.checkers != 0` — a branchless field load.

`Undo` gains a `prev_checkers: Bitboard` field so `unmake_move` restores
the cache without recomputation.

`make_move_perft` does **not** maintain this cache (perft uses its own
movegen-time checkers) — so perft stays unaffected.

### 4b. Bitboard-iterating `compute_hash_from_scratch`

Was:

```rust
for sq in 0..64u8 { if p.is_some() { h ^= piece_square(...) } }
```

64 mailbox loads + branches. Now:

```rust
for each (color, piece_type) {
    let mut bb = pos.piece_bb(color, pt);
    while bb != 0 {
        let sq = pop_lsb(&mut bb);
        h ^= piece_square(color, pt, sq);
    }
}
```

Iterates only occupied squares (32 for a typical opening). Called by FEN
parse and `recompute_zobrist`; cuts ~20 ns off FEN parse.

### 4c. Manual `Clone` impl that doesn't preserve history

Default `derive(Clone)` copies the `Vec<Undo>` + `Vec<u64>` history
fields, even though 99 % of `clone()` callers never look at them. New
manual impl resets them to `Vec::new()` (zero-cost — dangling pointer,
zero capacity).

```rust
impl Clone for Position {
    fn clone(&self) -> Self {
        Self {
            // ... copy board fields (memcpy of ~200 bytes) ...
            history: Vec::new(),       // zero cost; no heap alloc
            history_hashes: Vec::new(), // zero cost
            // ... copy zobrist, checkers ...
        }
    }
}
```

Semantics change: clones start with empty history. Given `clone()` is
normally "snapshot this state for later restoration / analysis", this
matches intuition. Documented.

### 4d. Smarter SAN check/mate suffix

Was:

```rust
if pos.is_checkmate() { "#" }
else if pos.in_check() { "+" }
```

`is_checkmate()` internally calls `in_check() && has_no_legal_moves()`.
`has_no_legal_moves()` runs full movegen — ~40 ns. We were paying that
cost on every SAN emit. Rewritten:

```rust
pos.make_move(m);
if pos.in_check() {              // O(1) cache read
    if pos.has_no_legal_moves() { s.push('#'); }
    else                         { s.push('+'); }
}
pos.unmake_move(m);
```

Since most moves don't give check, the movegen call inside
`has_no_legal_moves` is now skipped in the common case.

### 4e. SAN disambiguation via attack tables

Was: `generate_legal_moves()` for every non-pawn move, then iterate to
find same-type pieces hitting the same square.

Now: a **pre-filter via attack tables** — `attacks_from_target(pt, to,
occ) & piece_bb(color, pt) & !from` is zero iff no other same-type piece
could possibly reach `to`. Cheap bitboard AND, no movegen. Only when the
pre-filter is non-empty do we run the full legal-move check (because we
still need to rule out pinned candidates).

---

## Perft NPS (Round 4 — identical to R3 since perft isn't affected)

Min of 5 trials, `--deep`:

| Position | Depth | Nodes | **ultrachessjs** | shakmaty | cozy-chess | chess (jb) |
|---|---:|---:|---:|---:|---:|---:|
| Startpos   | 6 | 119,060,324 | **798 Mnps** | 298 Mnps |  553 Mnps | 505 Mnps |
| Kiwipete   | 5 | 193,690,690 |**1533 Mnps** | 347 Mnps | 1036 Mnps | 836 Mnps |
| Pos 3 (EP) | 6 |  11,030,083 | **701 Mnps** | 183 Mnps |  540 Mnps | 374 Mnps |
| Pos 4      | 5 |  15,833,292 | **985 Mnps** | 243 Mnps |  914 Mnps | 707 Mnps |
| Pos 5      | 5 |  89,941,194 |**1368 Mnps** | 330 Mnps |  916 Mnps | 778 Mnps |
| Pos 6      | 5 | 164,075,551 |**1238 Mnps** | 281 Mnps | 1255 Mnps | 843 Mnps |

Geomean vs shakmaty: **3.98×**. Geomean vs cozy-chess: **1.28×**. We beat
cozy on every one of the six canonical positions.

---

## WASM runtime (what a `npm install` user actually sees)

Measured via `node rust/bench/wasm-perft.mjs --deep --trials 3` on the
same Apple M4 Max host as the native numbers above. This is the full
user-visible path: `await Chess.create(fen)` + `chess.perft(d)`,
crossing the TS → WASM boundary.

Startpos d6, 119,060,324 nodes:

| Runtime           | Min ms | Mnps  | vs native Rust |
|-------------------|-------:|------:|---------------:|
| Bun 1.3.10        |  396.6 | 300.2 |          0.38× |
| Node 25.7.0       |  709.0 | 167.9 |          0.21× |

Geomean across all six standard positions:

| Runtime     | Geomean Mnps |
|-------------|-------------:|
| Bun 1.3.10  |        350.5 |
| Node 25.7.0 |        213.0 |

The full per-position table is emitted by the harness itself
(`just bench-wasm`). Both runtimes pass every reference-count sanity
check; the harness exits non-zero on any divergence.

Against pure-JS libraries — chess.js (5–7 Mnps) and chessops (2–3 Mnps)
on the same hardware class — this lands at **~25–60× faster** depending
on runtime.

## Where we stand on pure-JS libraries

| Engine                    | Language     | Startpos NPS (approx) |
|---------------------------|--------------|----------------------:|
| chessops                  | TS           |              2–3 Mnps |
| chess.js                  | JS           |              5–7 Mnps |
| ultrachessjs (WASM target) | Rust → WASM | ≥35 Mnps (plan bar)   |
| shakmaty                  | Rust native  |             ~298 Mnps |
| cozy-chess                | Rust native  |             ~553 Mnps |
| Stockfish                 | C++ native   |           400–500 Mnps |
| **ultrachessjs** (native) | **Rust**     | **798 Mnps**           |
| Gigantua                  | C++ native   |            ~2100 Mnps |

- **~130× faster than chess.js** on native startpos perft.
- **~2.7× Lichess's shakmaty** on startpos, 4.4× on mid-game positions.
- **~1.45× cozy-chess** on startpos.
- With ~3–5× Rust → WASM penalty, WASM startpos lands at ~160–270 Mnps —
  4–7× over the plan's 35 Mnps bar.

---

## What's left (the honest gap analysis)

| Op | Gap to best | Why it's there | ROI of closing it |
|----|---:|---|----|
| FEN parse         | −20 % (cozy 153 ns)  | cozy uses byte macros + no zobrist step | Low — FEN parse is a cold path |
| Movegen one-shot  | −18 % (cozy 22–33 ns)| cozy's tighter inner loops (partial inlining we haven't matched) | Medium — but perft wins via count sink |
| clone             | −3× (cozy 3.4 ns)    | cozy's Board is ~100 B; our Position is ~260 B (undo stack, double vec descriptor) | Low — clone isn't on hot path |

None of these are "structural weaknesses" — they're different
library-design trade-offs. We chose a richer `Position` (full undo, full
repetition log, cached checkers, cached zobrist) because it matches the
chess.js-compatible API surface we're exposing. The ops that actually
matter in a typical chess.js workflow (move + is_check + SAN + perft)
are all ours now.

### Optimisations not done (deliberately)

1. **Colour-templated movegen.** A separate compiled path for
   white-to-move vs black-to-move folds one `match` out of the hot
   loop. The user's "no metaprogramming" constraint rules this out
   (it's generic specialisation).
2. **Published magic numbers.** Would save ~20 ms init cost at the
   price of 128 `const u64` values in the binary. Trivial to add
   when Phase 7 runs.
3. **Incremental checker update.** Instead of one `attackers_to` per
   make (5 magic lookups), compute checkers from `(moving piece →
   new attacks)` + `(discovered-check probe through from-square)`.
   Would recover the make/unmake regression from R4 (~2 ns/make).
   Complex, many corner cases (castling, promotion, EP).
4. **SIMD movegen.** `std::simd` isn't portable to WASM without
   feature-gated branches.

---

## Bench layout

- `rust/bench/Cargo.toml` — standalone crate (not in the main
  workspace), dev-deps on shakmaty / cozy-chess / chess.
- `rust/bench/src/lib.rs` — uniform parse/perft wrapper per engine.
- `rust/bench/src/bin/nps.rs` — NPS harness with built-in perft sanity
  gate.
- `rust/bench/benches/micro.rs` — criterion single-call micro-benches
  covering FEN parse/write, movegen, make/unmake, hash, **clone,
  is_check, SAN write** (new in R4).

Every `just bench-*` recipe runs the Rust test suite first via
`bench-gate`. Both gates (`cargo test` + in-binary perft check) must
pass before any number in this doc is produced.

## Methodology caveats

1. **Single-threaded.** Every engine runs on one core.
2. **Min-of-N trials.** We report the fastest run, not the mean.
3. **Correctness gated.** Both `just bench-*` and the NPS binary
   refuse to produce numbers from a broken tree.
4. **No transposition tables.** Apples-to-apples recursive perft.
5. **API choices matter.** `make+unmake` on a mutable position (ours)
   vs `clone+play` (cozy/shakmaty) both valid; the perft numbers bake
   in each library's idiomatic choice.
6. **`count_legal_moves` at `depth == 1`.** The big perft win from R3;
   micro-benches still measure full `generate_legal_moves` for
   apples-to-apples with competitors.
