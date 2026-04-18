//! Perft — the correctness gate for move generation.
//!
//! Iterates the canonical `PERFT_REFERENCE` table: every (position, depth)
//! pair must match the reference node count exactly. Divergence = movegen bug.
//!
//! The heavy (billion-plus-node) tier is marked `#[ignore]` and runs via
//! `cargo test --release --test perft -- --ignored`.

mod common;

use common::PERFT_REFERENCE;
use ultrachess_core::{fen::parse_fen, perft::perft};

fn run(fen: &str, depth: u32, expected: u64) {
    let mut pos = parse_fen(fen).unwrap_or_else(|e| panic!("bad FEN {fen:?}: {e:?}"));
    let got = perft(&mut pos, depth);
    assert_eq!(
        got, expected,
        "perft({depth}) on {fen}:\n  expected {expected}\n       got {got}"
    );
}

#[test]
fn reference_positions_fast_tier() {
    // Depths that complete in under ~50 ms release. The Phase 2 gate.
    for &(name, fen, counts) in PERFT_REFERENCE {
        for (i, &expected) in counts.iter().enumerate() {
            let depth = (i + 1) as u32;
            // Skip depths that push past ~5M nodes to keep this tier snappy.
            if expected > 5_000_000 {
                continue;
            }
            eprintln!("running perft({depth}) for {name}");
            run(fen, depth, expected);
        }
    }
}

#[test]
fn startpos_perft_5() {
    run(PERFT_REFERENCE[0].1, 5, 4_865_609);
}

#[test]
fn position_3_perft_6_exercises_ep_discovered_check() {
    // Position 3 at depth 6 is the canonical EP-horizontal-discovered-check
    // catcher: any bug in `emit_pawn_moves` EP handling diverges here.
    run(PERFT_REFERENCE[2].1, 6, 11_030_083);
}

// ---------------------------------------------------------------------------
// Deep tier — billion-plus-node; not run by default. Each position is a
// separate test so `cargo test -- --ignored` can parallelise them.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "deep perft — run with --release --ignored"]
fn startpos_perft_6() {
    run(PERFT_REFERENCE[0].1, 6, 119_060_324);
}

#[test]
#[ignore = "deep perft — run with --release --ignored"]
fn kiwipete_perft_5() {
    run(PERFT_REFERENCE[1].1, 5, 193_690_690);
}

#[test]
#[ignore = "deep perft — run with --release --ignored"]
fn position_3_perft_7() {
    run(PERFT_REFERENCE[2].1, 7, 178_633_661);
}

#[test]
#[ignore = "deep perft — run with --release --ignored"]
fn position_4_perft_5() {
    run(PERFT_REFERENCE[3].1, 5, 15_833_292);
}

#[test]
#[ignore = "deep perft — run with --release --ignored"]
fn position_5_perft_5() {
    run(PERFT_REFERENCE[4].1, 5, 89_941_194);
}

#[test]
#[ignore = "deep perft — run with --release --ignored"]
fn position_6_perft_5() {
    run(PERFT_REFERENCE[5].1, 5, 164_075_551);
}

// ---------------------------------------------------------------------------
// Divide-perft sanity: sum of divide counts must equal the total.
// ---------------------------------------------------------------------------

#[test]
fn perft_divide_sum_equals_total() {
    use ultrachess_core::perft::perft_divide;
    let mut pos = parse_fen(common::STARTING_FEN).unwrap();
    for depth in 1..=4 {
        let total = perft(&mut pos, depth);
        let divided: u64 = perft_divide(&mut pos, depth).iter().map(|(_, n)| n).sum();
        assert_eq!(total, divided, "divide mismatch at depth {depth}");
    }
}
