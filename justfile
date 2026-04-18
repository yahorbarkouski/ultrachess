set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

# Default target: build WASM + TS, then run all tests.
default: test

# --- Build ------------------------------------------------------------------

# Build the WASM artifact from the Rust wasm crate, copy to `assets/`.
# Runs wasm-opt if available (shrinks ~30%); falls back to the raw output
# with a warning if binaryen isn't installed.
build-wasm:
    cargo build --release --target wasm32-unknown-unknown -p ultrachess-wasm
    mkdir -p assets
    cp target/wasm32-unknown-unknown/release/ultrachess_wasm.wasm assets/ultrachess.wasm
    @if command -v wasm-opt >/dev/null 2>&1; then \
        wasm-opt -O4 --enable-bulk-memory --strip-debug --strip-producers \
            assets/ultrachess.wasm -o assets/ultrachess.wasm.opt && \
        mv assets/ultrachess.wasm.opt assets/ultrachess.wasm ; \
    else \
        echo "warning: wasm-opt not on PATH — skipping size optimisation" ; \
    fi
    @ls -la assets/ultrachess.wasm
    @if command -v brotli >/dev/null 2>&1; then \
        printf "brotli: " && brotli -q 11 -c assets/ultrachess.wasm | wc -c ; \
    fi

# Generate `src/generated/inline-wasm.ts` — a base64-embedded copy of the
# current .wasm for the synchronous `ultrachess/inline` entry point.
build-inline: build-wasm
    mkdir -p src/generated
    node scripts/generate-inline.mjs

# Build TS (requires `npm install` first).
build-ts: build-inline
    npx tsup

# Everything.
build: build-ts

# --- Test -------------------------------------------------------------------

# Rust unit + integration tests (fast tier).
test-rust:
    cargo test --release -p ultrachess-core

# Deep perft suite (slow; exercises billion-plus nodes).
test-deep:
    cargo test --release -p ultrachess-core --test perft -- --ignored

# TS smoke tests (requires the wasm artifact in assets/).
test-ts: build-wasm
    npx vitest run

# Everything.
test: test-rust test-ts

# --- Differential fuzz vs chess.js -----------------------------------------
#
# Plays random legal games on both engines in lock-step, asserting FEN /
# legal-move set / check-flags / draw-flags agree every ply. The default
# tier runs via `just test-ts` (it's a normal vitest file); these recipes
# expose the heavier gated tiers.
#
# Conventions (see TESTING.md):
#  - PR gate:     default tier (200 games) — part of `just test`.
#  - Nightly:     `just test-diff-10k`     — ~5 min.
#  - Pre-RC:      `just test-diff-100k`    — ~55 min, the release gate.

test-diff-10k: build-wasm build-inline
    DIFF_FUZZ_GAMES=10000 npx vitest run test/differential

test-diff-100k: build-wasm build-inline
    DIFF_FUZZ_GAMES=100000 npx vitest run test/differential

# --- Coverage --------------------------------------------------------------
#
# Conventions (see TESTING.md): ≥95% lines / functions / statements;
# ≥90% branches. The build fails below those numbers.

# Shared llvm-cov env for the Rust recipes — keeps the PATH dance in one place.
# Resolved from the active rustup toolchain so the recipe is portable across
# hosts (aarch64-apple-darwin, x86_64-unknown-linux-gnu, …).
sysroot := `rustc --print sysroot`
host    := `rustc -vV | sed -n 's/^host: //p'`
cov_env := "LLVM_COV=" + sysroot + "/lib/rustlib/" + host + "/bin/llvm-cov" + \
           " LLVM_PROFDATA=" + sysroot + "/lib/rustlib/" + host + "/bin/llvm-profdata"

# Rust coverage summary — prints the per-file percentages.
coverage-rs:
    {{cov_env}} cargo llvm-cov -p ultrachess-core --summary-only

# Rust coverage with HTML report for human browsing.
coverage-rs-html:
    {{cov_env}} cargo llvm-cov -p ultrachess-core --html
    @echo "report at target/llvm-cov/html/index.html"

# TS coverage via vitest + v8. Enforces the thresholds in vitest.config.ts.
coverage-ts: build-wasm
    npx vitest run --coverage

# Full coverage across Rust + TS with enforced thresholds.
coverage: coverage-rs coverage-ts

# --- Benchmarks -------------------------------------------------------------
#
# Every bench recipe gates on the Rust test suite first — running a
# benchmark over a broken implementation produces misleading numbers and
# has burned us before. If tests fail, we refuse to bench.

# Fast sanity gate used by every `bench-*` recipe: runs the Rust unit +
# integration tests (perft reference positions included).
bench-gate:
    @echo "=== gate: running Rust tests before benchmark ==="
    cargo test --release -p ultrachess-core

# Native perft NPS (ours vs shakmaty / cozy-chess / chess). Standard tier.
# The bench binary also runs an internal perft-count sanity check per
# position against the reference node count (`--no-gate` skips the cargo
# test gate if you've just run it).
bench-nps: bench-gate
    cd rust/bench && cargo run --release --bin nps -- --trials 3

# Deeper NPS run (adds one depth to every position — startpos d6 etc.).
bench-nps-deep: bench-gate
    cd rust/bench && cargo run --release --bin nps -- --trials 5 --deep

# Single-call micro-benches via criterion (FEN, movegen, make/unmake, hash).
bench-micro: bench-gate
    cd rust/bench && cargo bench --bench micro

# WASM perft through the real TS → WASM boundary. Measures what a
# `npm install` user actually gets (not the native Rust numbers). Runs
# on both Node and Bun if Bun is on PATH.
bench-wasm:
    @echo "=== WASM perft (Node) ==="
    node rust/bench/wasm-perft.mjs --deep --trials 3
    @if command -v bun >/dev/null 2>&1; then \
        echo "" ; \
        echo "=== WASM perft (Bun) ===" ; \
        bun rust/bench/wasm-perft.mjs --deep --trials 3 ; \
    else \
        echo "note: bun not on PATH — skipping Bun measurement" ; \
    fi

# Everything: gate + NPS + micro + WASM.
bench: bench-gate bench-nps bench-micro bench-wasm

# --- Format + lint ---------------------------------------------------------
#
# Enforced in CI as a gate before tests. `just fmt` rewrites, `just fmt-check`
# is non-destructive.

# Rustfmt (writes) + Biome format (writes) across the whole repo.
fmt:
    cargo fmt --all
    npx biome format --write .

# Non-destructive format check — exits non-zero if anything would change.
fmt-check:
    cargo fmt --all --check
    npx biome format .

# Lint Rust (clippy, warnings = errors) + TS (Biome).
lint:
    cargo clippy --all-targets --workspace -- -D warnings
    npx biome lint .

# Full gate: format check + lint across both languages.
check: fmt-check lint

# --- Cross-runtime smoke ---------------------------------------------------
#
# Runs the standalone scripts under `test/cross-runtime/` against every
# JavaScript runtime found on PATH. Each script imports from `dist/`, so we
# build the bundle first to exercise the *published* artefact rather than
# the source tree.

test-cross: build
    @echo "=== cross-runtime smoke ==="
    @echo "--- node ---"
    node test/cross-runtime/bun-smoke.mjs
    @if command -v bun >/dev/null 2>&1; then \
        echo "--- bun ---" ; \
        bun test/cross-runtime/bun-smoke.mjs ; \
    else \
        echo "note: bun not on PATH — skipping" ; \
    fi
    @if command -v deno >/dev/null 2>&1; then \
        echo "--- deno ---" ; \
        deno run --allow-read --allow-env test/cross-runtime/bun-smoke.mjs ; \
    else \
        echo "note: deno not on PATH — skipping" ; \
    fi

# --- Misc -------------------------------------------------------------------

clean:
    cargo clean
    rm -rf dist assets node_modules src/generated
