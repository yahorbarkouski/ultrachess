//! Ladder bench — Level 34 (MoveCounter vs MoveList).
//!
//! Pairs with docs/movegen-ladder.md §L34. The claim is that at perft leaves
//! — where we only need the *number* of legal moves, not the moves themselves
//! — skipping materialisation collapses an N-iteration `pop_lsb` + `push`
//! loop into a single `popcnt`.
//!
//! Both paths share the same movegen implementation in `ultrachess-core`;
//! the only thing that varies is the `MoveSink` passed in. This isolates
//! the cost of materialisation from every other decision on the ladder.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use ultrachess_ladder::{load, KIWIPETE, POS3, STARTPOS};
use ultrachess_core::{count_legal_moves, generate_legal_moves, MoveList};

fn bench_count_vs_materialise(c: &mut Criterion) {
    for (name, fen) in [
        ("startpos", STARTPOS),
        ("kiwipete", KIWIPETE),
        ("pos3", POS3),
    ] {
        let pos = load(fen);

        let mut g = c.benchmark_group(format!("count_vs_materialise/{name}"));

        // Materialising path: what a general-purpose `moves()` API must do.
        g.bench_function("materialise (MoveList)", |b| {
            b.iter(|| {
                let mut list = MoveList::new();
                generate_legal_moves(black_box(&pos), &mut list);
                black_box(list.len())
            });
        });

        // Counter path: what perft leaves actually use.
        g.bench_function("count (MoveCounter)", |b| {
            b.iter(|| {
                let n = count_legal_moves(black_box(&pos));
                black_box(n)
            });
        });

        g.finish();
    }
}

criterion_group!(benches, bench_count_vs_materialise);
criterion_main!(benches);
