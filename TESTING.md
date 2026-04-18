# Testing conventions

This document defines how tests are organised, named, written, and measured
across the repo. Every new test lives under one of these conventions.

---

## Guiding principles

1. **Coverage is a floor, not a goal.** ≥ 95% lines / functions, ≥ 90% branches
   on both Rust core and TS shim. The build fails below those numbers.
2. **Tests are organised by *subject*, never by phase.** `attacks.test.ts`,
   not `phase6.test.ts`. `zobrist_repetition.rs`, not `phase3.rs`.
3. **One file = one subject.** If a file covers two cleanly separable
   subjects, split it.
4. **Fixtures are shared, inputs are reproducible.** All PRNG-driven tests
   take a fixed seed; failure reports include it. Reference FENs live in a
   single module that every test imports.
5. **No ignored / skipped tests on main.** Every `#[ignore]` / `.skip` has an
   explicit reason in the attribute (`#[ignore = "…"]`) and is runnable via a
   documented escape hatch (`--include-ignored`, env var).

---

## Rust

### Layout

```
rust/core/
├── src/
│   └── *.rs                     # unit tests inline: #[cfg(test)] mod tests
└── tests/
    ├── common/
    │   ├── mod.rs               # shared: reference FENs, PRNG, perft counts
    │   └── fixtures/            # optional data files (e.g. classics.pgn)
    ├── perft.rs                 # per-subject integration test
    ├── position_invariants.rs   # ...
    ├── zobrist_repetition.rs
    ├── san_roundtrip.rs
    ├── pgn_corpus.rs
    └── ...

rust/wasm/
├── src/*.rs                     # unit tests inline
└── tests/
    ├── abi_lifecycle.rs         # slab allocation, generational handles
    └── abi_surface.rs           # every extern "C" export exercised
```

### When to use inline vs integration tests

| Kind                              | Inline (`#[cfg(test)]`) | Integration (`tests/`) |
|-----------------------------------|-------------------------|------------------------|
| Pure functions, tiny helpers      | ✅                      | ❌                     |
| Private/crate-internal behaviour  | ✅                      | ❌                     |
| Public-API behaviour              | ❌                      | ✅                     |
| Property / fuzz tests             | ❌                      | ✅                     |
| Corpus / fixture-driven           | ❌                      | ✅                     |

### Naming

- Integration test files are **nouns describing the subject**:
  `perft.rs`, `position_invariants.rs`, `san_roundtrip.rs`.
- Test function names describe the **behaviour**, not the code path:
  `empty_board_has_no_attackers`, not `test_attackers_to_fn`.
- Helper modules under `tests/common/` expose a small, stable API —
  don't treat them like dumping grounds for any shared constant.

### Property / PRNG tests

- PRNG seeds are **literals in the test body**. No wall-clock seeds. Ever.
- Seed + ply index must be visible in any panic message:
  ```rust
  panic!("seed={seed} ply={ply}: {msg}")
  ```
- Document iteration count in the test name or a doc comment
  (`random_games_many_seeds` runs 200 seeds × 100 plies).
- Multi-million-iteration gates go behind `#[ignore = "…"]` with a comment
  explaining how to run (`cargo test --release -- --ignored`).

### Shared fixtures

`tests/common/mod.rs` contains:

- `STARTING_FEN`, `KIWIPETE`, `POSITION_3`, … — canonical references.
- `PERFT_REFERENCE: &[(name, fen, counts_per_depth)]`.
- `Rng(seed: u64)` — xorshift64 with a reproducible constructor.
- Tiny helpers: `move_by_uci`, `piece_at`, etc.

Every integration test that uses these references imports `mod common;`.

### Coverage

- Run: `just coverage-rs` (uses `cargo-llvm-cov`).
- Threshold: `--fail-under-lines 95 --fail-under-functions 95` on the
  `ultrachess-core` crate. WASM ABI crate has its own relaxed threshold
  because unsafe / FFI glue is hard to fully path-cover.
- Intentionally-dead paths (e.g. `unreachable!()`) should have
  `// LCOV_EXCL_LINE` beside them OR be tested via a dedicated
  `#[should_panic]` unit test.

---

## TypeScript

### Layout

```
test/
├── _helpers/
│   ├── positions.ts             # reference FENs, expected perft counts
│   ├── rng.ts                   # reproducible xorshift64 for prop tests
│   └── move-utils.ts            # findMove(sans, chess), etc.
├── abi-smoke.test.ts            # loader.init + ABI version + memory
├── loader.test.ts               # version mismatch, sync path, cache
├── low-level.test.ts            # raw memory helpers
├── move-helpers.test.ts         # move.ts decoder / encoder coverage
├── chess-lifecycle.test.ts      # create / dispose / clone / using
├── chess-state.test.ts          # fen / hash / turn / pieceAt / ascii
├── chess-moves.test.ts          # move / undo / history / legalMoves
├── chess-end-states.test.ts     # check / mate / stale / draw
├── chess-san.test.ts            # SAN write / parse / roundtrip
├── attacks.test.ts              # isAttacked / attackers
├── find-piece.test.ts
├── edits.test.ts                # put / remove
├── verbose-moves.test.ts
├── pgn.test.ts                  # loadPgn / pgn / headers
├── inline.test.ts               # sync entry via ultrachess/inline
├── perft.test.ts                # perft matrix across reference positions
└── cross-runtime/
    └── bun-smoke.mjs            # standalone scripts runnable under Bun / Node
```

### Naming

- One file = one subject. `attacks.test.ts` holds every attack-related
  test, nothing else.
- Test descriptions read as sentences: `it("detects EP horizontal discovered check", …)`.
- Shared helpers live in `test/_helpers/`, imported with relative paths.

### Vitest conventions

- Default reporter: `default` (no forced verbose mode).
- Every test uses `using` for Chess instances — no manual `.dispose()`
  unless specifically testing disposal.
- `describe` groups match the file name: one top-level `describe` per file,
  subject-named.
- Snapshot tests are banned for this project — all assertions are
  explicit. PGN / FEN round-trips use string equality, not snapshots.

### Coverage

- Run: `just coverage-ts` (uses `@vitest/coverage-v8`).
- Thresholds (set in `vitest.config.ts`, enforced):
  - `lines: 95`, `functions: 95`, `statements: 95`, `branches: 90`.
- Exclude: `src/generated/**` (auto-generated), `test/**`, `dist/**`.
- Use `/* c8 ignore next */` sparingly, only for genuinely unreachable code
  (e.g. platform-specific runtime branches).

---

## Cross-runtime verification

Standalone scripts in `test/cross-runtime/` run under Node, Bun, and (when
available) Deno. They import from `../../dist/index.js`, so they exercise
the *published* bundle, not the source tree.

```
just test-cross   # runs each script under every runtime found on PATH
```

These scripts are small — they verify instantiation + a smoke `perft` in
each runtime. Full correctness is the job of the Rust + Vitest suites.

---

## How to run everything

| Command                          | What it does |
|----------------------------------|--------------|
| `just test`                      | Rust unit + integration + TS Vitest. Fast tier. |
| `just test-deep`                 | Adds ignored perft + million-iteration property tests. |
| `just coverage`                  | Full coverage across Rust + TS with thresholds enforced. |
| `just coverage-rs` / `coverage-ts` | Single-stack coverage. |
| `just test-cross`                | Cross-runtime smoke (Node + Bun + Deno if present). |

A PR cannot merge if `just coverage` drops below the documented thresholds,
or if any runtime in `just test-cross` fails.
