//! Ladder bench — Level 3 vs Level 23 (pseudo-legal + filter vs fully-legal).
//!
//! Pairs with docs/movegen-ladder.md §L3 and §L23.
//!
//! STATUS: STUB. ultrachess-core only ships the fully-legal Ellis-Jones
//! generator. To benchmark pseudo-legal + make/unmake-filter honestly we
//! need a second implementation: one that emits every move ignoring pins
//! and checks, plays each on a scratch copy, and discards moves that
//! leave our king in check. ~250 LOC.
//!
//! Tracked here as a placeholder so `docs/movegen-ladder.md §L23` links
//! somewhere real.
//!
//! Rough expectation (from literature + chess.js behaviour, which uses
//! this strategy):
//!   pseudo+filter: startpos ~5 Mnps, kiwipete ~3 Mnps
//!   Ellis-Jones : startpos ~800 Mnps, kiwipete ~1500 Mnps
//! i.e. roughly two orders of magnitude. The specific ratio depends heavily
//! on position complexity — positions with many pieces and many pins
//! punish the filter path more.

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_pseudo_filter_vs_ellis_jones(c: &mut Criterion) {
    let mut g = c.benchmark_group("pseudo_filter_vs_ellis_jones");
    g.bench_function("STUB", |b| {
        b.iter(|| {
            // No-op. See the module doc above for what needs to exist
            // before this bench can produce honest numbers.
        });
    });
    g.finish();
}

criterion_group!(benches, bench_pseudo_filter_vs_ellis_jones);
criterion_main!(benches);
