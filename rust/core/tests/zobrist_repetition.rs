//! Zobrist hash correctness + threefold repetition + insufficient material.
//!
//! Organised by behaviour, not by phase. The heavy 1M-iteration gate stays
//! `#[ignore]` — run with `cargo test --release --test zobrist_repetition --
//! --ignored million_iteration`.

mod common;

use common::{find_move, Rng, STARTING_FEN};
use ultrachess_core::chess_move::Move;
use ultrachess_core::fen::{parse_fen, write_fen};
use ultrachess_core::movegen::{generate_legal_moves, MoveList};
use ultrachess_core::zobrist;
use ultrachess_core::Square;

// ---------------------------------------------------------------------------
// Hash consistency + FEN round-trip
// ---------------------------------------------------------------------------

#[test]
fn incremental_hash_matches_from_scratch_on_random_walks() {
    for seed in 1u64..=1_000 {
        let mut p = parse_fen(STARTING_FEN).unwrap();
        let mut rng = Rng::new(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut ml = MoveList::new();
        for _ in 0..100 {
            generate_legal_moves(&p, &mut ml);
            if ml.is_empty() || p.halfmove >= 100 {
                break;
            }
            assert_eq!(p.hash(), zobrist::compute_hash_from_scratch(&p));
            let m: Move = ml.as_slice()[rng.bounded(ml.len())];
            let before = p.hash();
            p.make_move(m);
            assert_eq!(p.hash(), zobrist::compute_hash_from_scratch(&p));
            p.unmake_move(m);
            assert_eq!(p.hash(), before);
            p.make_move(m);
        }
    }
}

#[test]
fn fen_roundtrip_preserves_hash() {
    let mut p = parse_fen(STARTING_FEN).unwrap();
    let mut rng = Rng::new(42);
    let mut ml = MoveList::new();
    for _ in 0..500 {
        generate_legal_moves(&p, &mut ml);
        if ml.is_empty() {
            break;
        }
        p.make_move(ml.as_slice()[rng.bounded(ml.len())]);
    }
    let fen = write_fen(&p);
    assert_eq!(parse_fen(&fen).unwrap().hash(), p.hash());
}

#[test]
fn starting_position_hash_is_nonzero_and_stable() {
    let p = parse_fen(STARTING_FEN).unwrap();
    assert_ne!(p.hash(), 0);
    // Same FEN must always produce the same hash (Zobrist keys are
    // const-init from a fixed seed). Parse twice → compare.
    assert_eq!(parse_fen(STARTING_FEN).unwrap().hash(), p.hash());
}

// ---------------------------------------------------------------------------
// Threefold repetition
// ---------------------------------------------------------------------------

#[test]
fn knight_shuttle_trips_threefold_on_third_visit() {
    let mut p = parse_fen(STARTING_FEN).unwrap();
    let start_hash = p.hash();
    let g1 = Square::from_file_rank(6, 0);
    let f3 = Square::from_file_rank(5, 2);
    let g8 = Square::from_file_rank(6, 7);
    let f6 = Square::from_file_rank(5, 5);

    assert!(!p.is_threefold_repetition());

    // Visit 2.
    p.make_move(find_move(&p, g1, f3));
    p.make_move(find_move(&p, g8, f6));
    p.make_move(find_move(&p, f3, g1));
    p.make_move(find_move(&p, f6, g8));
    assert_eq!(p.hash(), start_hash);
    assert!(!p.is_threefold_repetition(), "only 2 occurrences so far");

    // Visit 3.
    p.make_move(find_move(&p, g1, f3));
    p.make_move(find_move(&p, g8, f6));
    p.make_move(find_move(&p, f3, g1));
    p.make_move(find_move(&p, f6, g8));
    assert_eq!(p.hash(), start_hash);
    assert!(p.is_threefold_repetition());
}

#[test]
fn repetition_scope_tracks_halfmove_clock() {
    // Drive a legitimate threefold, then play an irreversible pawn push.
    // The post-pawn position is novel and has halfmove=0, so the scan
    // window is empty and threefold must NOT trigger.
    let mut p = parse_fen(STARTING_FEN).unwrap();
    let g1 = Square::from_file_rank(6, 0);
    let f3 = Square::from_file_rank(5, 2);
    let g8 = Square::from_file_rank(6, 7);
    let f6 = Square::from_file_rank(5, 5);
    for _ in 0..2 {
        p.make_move(find_move(&p, g1, f3));
        p.make_move(find_move(&p, g8, f6));
        p.make_move(find_move(&p, f3, g1));
        p.make_move(find_move(&p, f6, g8));
    }
    assert!(p.is_threefold_repetition());

    // Pawn push resets the era.
    let mut ml = MoveList::new();
    generate_legal_moves(&p, &mut ml);
    let e2e4 = *ml
        .iter()
        .find(|m| m.from() == Square::E2 && m.to() == Square::E4)
        .unwrap();
    p.make_move(e2e4);
    assert_eq!(p.halfmove, 0);
    assert!(!p.is_threefold_repetition());
}

// ---------------------------------------------------------------------------
// Insufficient material classification — hand-curated positions.
// ---------------------------------------------------------------------------

#[test]
fn insufficient_material_known_positions() {
    let insufficient = [
        "8/8/8/4k3/8/4K3/8/8 w - - 0 1",     // K vs K
        "8/8/8/4k3/8/4K3/8/5N2 w - - 0 1",   // K+N vs K
        "8/8/8/4k3/8/4K3/8/5B2 w - - 0 1",   // K+B vs K
        "8/8/8/4k3/4B3/8/4K3/1b6 w - - 0 1", // same-colour bishops (1+1)
        // Generalised same-colour case: K+B vs K+B+B, ALL bishops on the
        // light complex. Caught by the 100k differential fuzzer against
        // chess.js — see `git log` for the exact seed.
        "4k3/8/6b1/8/8/1B6/8/3K1b2 b - - 0 1",
    ];
    for fen in insufficient {
        let p = parse_fen(fen).unwrap();
        assert!(p.is_insufficient_material(), "expected insufficient: {fen}");
        assert!(p.is_draw(), "expected draw: {fen}");
    }

    let sufficient = [
        "8/8/8/4k3/8/8/4K3/b6B w - - 0 1",  // opposite-colour bishops
        "8/8/8/4k3/8/8/4K3/5N1N w - - 0 1", // K+NN vs K (chess.js convention)
        "8/8/8/4k3/8/8/4K3/5Q2 w - - 0 1",  // K+Q vs K
        "8/8/8/4k3/8/8/4K3/5R2 w - - 0 1",  // K+R vs K
        "8/8/8/4k3/8/8/4PK2/8 w - - 0 1",   // K+P vs K
        // K+B vs K+N — can't be all-bishops; knight keeps it sufficient.
        "4k3/8/8/8/8/8/4K3/3NB3 w - - 0 1",
        // K+B+B vs K, but bishops on OPPOSITE colours — different-colour
        // bishops on one side defeat the "all same complex" rule.
        "4k3/8/8/8/8/8/4K3/2B2B2 w - - 0 1",
    ];
    for fen in sufficient {
        let p = parse_fen(fen).unwrap();
        assert!(!p.is_insufficient_material(), "expected sufficient: {fen}");
    }
}

// ---------------------------------------------------------------------------
// Mate / stale / 50-move
// ---------------------------------------------------------------------------

#[test]
fn checkmate_detected() {
    let p =
        parse_fen("r1bqkb1r/pppp1Qpp/2n2n2/4p3/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 0 4").unwrap();
    assert!(p.is_checkmate());
    assert!(!p.is_stalemate());
    assert!(p.is_game_over());
    assert!(!p.is_draw());
}

#[test]
fn stalemate_detected() {
    let p = parse_fen("k7/2Q5/8/8/8/8/8/7K b - - 0 1").unwrap();
    assert!(p.is_stalemate());
    assert!(!p.is_checkmate());
    assert!(p.is_draw());
}

#[test]
fn fifty_move_rule_threshold() {
    assert!(!parse_fen("4k3/8/8/8/8/8/8/4K3 w - - 99 1")
        .unwrap()
        .is_fifty_move_rule());
    assert!(parse_fen("4k3/8/8/8/8/8/8/4K3 w - - 100 1")
        .unwrap()
        .is_fifty_move_rule());
}

// ---------------------------------------------------------------------------
// Heavy 1M-iteration gate — opt-in.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "1M+ iterations — run with --release --ignored million_iteration"]
fn million_iteration_hash_consistency() {
    let mut total: u64 = 0;
    for seed in 1u64..=4_000 {
        let mut p = parse_fen(STARTING_FEN).unwrap();
        let mut rng = Rng::new(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut ml = MoveList::new();
        for _ in 0..100 {
            generate_legal_moves(&p, &mut ml);
            if ml.is_empty() || p.halfmove >= 100 {
                break;
            }
            assert_eq!(p.hash(), zobrist::compute_hash_from_scratch(&p));
            total += 1;
            let m = ml.as_slice()[rng.bounded(ml.len())];
            let before = p.hash();
            p.make_move(m);
            assert_eq!(p.hash(), zobrist::compute_hash_from_scratch(&p));
            total += 1;
            p.unmake_move(m);
            assert_eq!(p.hash(), before);
            total += 1;
            p.make_move(m);
        }
    }
    assert!(
        total >= 1_000_000,
        "only {total} assertions ran; target ≥ 1M"
    );
}
