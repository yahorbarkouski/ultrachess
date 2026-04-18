//! Ladder bench — Level 21 (classical ray walk vs fancy magic bitboards).
//!
//! Pairs with docs/movegen-ladder.md §L17–L21.
//!
//! STATUS: STUB. The current ultrachess-core uses fancy magics unconditionally;
//! there is no feature flag or fallback for classical rays. Implementing this
//! bench faithfully requires ~150 LOC of standalone classical-ray code in
//! `src/classical.rs` (ray tables + rook_attacks_classical +
//! bishop_attacks_classical), then a driver that re-runs `generate_legal_moves`
//! with the magic lookup points redirected to classical.
//!
//! That's a non-trivial chunk. It is tracked here as a placeholder so
//! `docs/movegen-ladder.md §L21` links somewhere real.
//!
//! Rough expectation (from historical Round 1 → Round 2 data):
//!   classical: ~465 Mnps perft startpos
//!   magic    : ~800 Mnps perft startpos
//! i.e. ~1.7× for magics. This bench would measure the *per-call* delta on
//! `rook_attacks` / `bishop_attacks` in isolation, which should be much larger
//! (~3–5×) because classical's branching inner loop amortises into perft's
//! other costs.

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_classical_vs_magic(c: &mut Criterion) {
    let mut g = c.benchmark_group("classical_vs_magic");
    g.bench_function("STUB", |b| {
        b.iter(|| {
            // No-op. See the module doc above for what needs to exist
            // before this bench can produce honest numbers.
        });
    });
    g.finish();
}

criterion_group!(benches, bench_classical_vs_magic);
criterion_main!(benches);
