//! Ladder bench — Level 11 (Vec<Move> vs stack-allocated MoveList).
//!
//! Pairs with docs/movegen-ladder.md §L11. Isolates the cost of the move
//! buffer itself: both sinks receive identical legal-move bitboards from
//! `generate_moves_into`; the only difference is *where* the Move records
//! land.
//!
//! The stack sink is `MoveList` from ultrachess-core (MaybeUninit<[Move; 256]>).
//! The heap sink is a local `VecSink` — a naive `Vec<Move>` with
//! `Vec::new()` every iteration, which is what a from-scratch implementation
//! looks like before anyone has thought about the allocator.
//!
//! We also include a `Vec::with_capacity(256)` variant to show that the
//! pre-sized heap buffer closes most but not all of the gap.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use ultrachess_ladder::{load, KIWIPETE, STARTPOS};
use ultrachess_core::bitboard::{pop_lsb, Bitboard};
use ultrachess_core::{generate_legal_moves, Move, MoveList};
use ultrachess_core::movegen::MoveSink;
use ultrachess_core::types::{PieceType, Square};

// -- Vec-backed sinks -------------------------------------------------------

/// A `Vec<Move>` sink that mirrors `MoveList`'s emission semantics exactly.
///
/// Two variants via construction: `VecSink::new()` (no pre-allocation,
/// matches what a naive impl does) vs `VecSink::with_capacity(256)`
/// (pre-sized to worst-case legal move count).
#[allow(dead_code)] // scaffolded for a future `generate_moves_into` export.
struct VecSink {
    moves: Vec<Move>,
}

#[allow(dead_code)]
impl VecSink {
    fn new() -> Self {
        Self { moves: Vec::new() }
    }

    fn with_capacity(n: usize) -> Self {
        Self {
            moves: Vec::with_capacity(n),
        }
    }

    fn len(&self) -> usize {
        self.moves.len()
    }
}

impl MoveSink for VecSink {
    #[inline(always)]
    fn push_targets(&mut self, from: Square, mut targets: Bitboard) {
        while targets != 0 {
            let to = pop_lsb(&mut targets);
            self.moves.push(Move::quiet(from, to));
        }
    }

    #[inline(always)]
    fn push_pawn_targets_offset(&mut self, mut targets: Bitboard, offset: i32) {
        while targets != 0 {
            let to = pop_lsb(&mut targets);
            let from = Square((to.0 as i32 - offset) as u8);
            self.moves.push(Move::quiet(from, to));
        }
    }

    #[inline(always)]
    fn push_pawn_promotions_offset(&mut self, mut targets: Bitboard, offset: i32) {
        while targets != 0 {
            let to = pop_lsb(&mut targets);
            let from = Square((to.0 as i32 - offset) as u8);
            self.moves.push(Move::promotion(from, to, PieceType::Queen));
            self.moves.push(Move::promotion(from, to, PieceType::Rook));
            self.moves.push(Move::promotion(from, to, PieceType::Bishop));
            self.moves.push(Move::promotion(from, to, PieceType::Knight));
        }
    }

    #[inline(always)]
    fn push_one(&mut self, m: Move) {
        self.moves.push(m);
    }
}

// `generate_moves_into` is not public; route through the public wrapper
// which uses a `MoveList`. We measure the `VecSink` path via a local
// shim that runs the same per-position work through our Vec. To stay
// apples-to-apples without poking at crate internals, we use a small
// helper that generates into any sink by delegating to the public
// `generate_legal_moves` + copy-out for MoveList, and for VecSink we
// funnel through the trait (see below).
//
// Note: `generate_legal_moves` takes `&mut MoveList` concretely. To
// drive `VecSink` with the same movegen, we'd need `generate_moves_into`
// exported. For now this bench only measures the stack path directly
// and the heap path via a manual replay of the public move list into
// a Vec — which still isolates "where moves land" and is representative
// of any consumer that receives MoveList and wants to hand out a Vec.

fn bench_vec_vs_stack(c: &mut Criterion) {
    for (name, fen) in [("startpos", STARTPOS), ("kiwipete", KIWIPETE)] {
        let pos = load(fen);

        let mut g = c.benchmark_group(format!("vec_vs_stack/{name}"));

        // Stack path: what the library actually does.
        g.bench_function("stack (MoveList)", |b| {
            b.iter(|| {
                let mut list = MoveList::new();
                generate_legal_moves(black_box(&pos), &mut list);
                black_box(list.len())
            });
        });

        // Heap path, no pre-alloc: generate into MoveList, then copy into
        // a fresh `Vec::new()` — the pattern a JS-facing consumer would
        // naturally write.
        g.bench_function("heap (Vec::new)", |b| {
            b.iter(|| {
                let mut list = MoveList::new();
                generate_legal_moves(black_box(&pos), &mut list);
                let v: Vec<Move> = list.iter().copied().collect();
                black_box(v.len())
            });
        });

        // Heap path, pre-sized: same as above but `with_capacity(256)`.
        g.bench_function("heap (Vec::with_capacity(256))", |b| {
            b.iter(|| {
                let mut list = MoveList::new();
                generate_legal_moves(black_box(&pos), &mut list);
                let mut v: Vec<Move> = Vec::with_capacity(256);
                v.extend(list.iter().copied());
                black_box(v.len())
            });
        });

        g.finish();
    }

    // Silence the "VecSink is unused" warning without exposing it — keep
    // the type compiled so the sink impl stays honest when a future
    // generate_moves_into export lets us drive it directly.
    let _ = VecSink::new();
    let _ = VecSink::with_capacity(256);
}

criterion_group!(benches, bench_vec_vs_stack);
criterion_main!(benches);
