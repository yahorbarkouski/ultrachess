//! Randomised property tests covering structural invariants of `Position`.
//!
//! For every reachable position in a random-game walk:
//! 1. `make_move(m)` then `unmake_move(m)` restores bit-for-bit state.
//! 2. Bitboard / mailbox / king-count invariants hold.
//! 3. `write_fen(parse_fen(s))` round-trips.
//!
//! Reproducibility: seeds are literals; panic reports include seed + ply.

mod common;

use common::{Rng, STARTING_FEN};
use ultrachess_core::fen::{parse_fen, write_fen};
use ultrachess_core::movegen::{generate_legal_moves, MoveList};
use ultrachess_core::position::Position;
use ultrachess_core::{Color, PieceType, Square};

/// Byte-level snapshot of everything that must be identical before and after
/// `make_move` + `unmake_move`.
fn snapshot(p: &Position) -> Vec<u8> {
    let mut v = Vec::with_capacity(256);
    for c in [Color::White, Color::Black] {
        v.extend(&p.color_bb(c).to_le_bytes());
        for pt in [
            PieceType::Pawn,
            PieceType::Knight,
            PieceType::Bishop,
            PieceType::Rook,
            PieceType::Queen,
            PieceType::King,
        ] {
            v.extend(&p.piece_bb(c, pt).to_le_bytes());
        }
    }
    for sq in 0..64u8 {
        v.push(p.piece_at(Square(sq)).0);
    }
    v.push(p.side_to_move as u8);
    v.push(p.castling.0);
    v.push(p.ep_square.map(|s| s.0).unwrap_or(64));
    v.extend(&p.halfmove.to_le_bytes());
    v.extend(&p.fullmove.to_le_bytes());
    v
}

fn walk(seed: u64, max_plies: usize) {
    let mut p = parse_fen(STARTING_FEN).unwrap();
    let mut rng = Rng::new(seed);
    let mut moves = MoveList::new();
    for ply in 0..max_plies {
        generate_legal_moves(&p, &mut moves);
        if moves.is_empty() || p.halfmove >= 100 {
            return;
        }
        let idx = rng.bounded(moves.len());
        let m = moves.as_slice()[idx];

        let before = snapshot(&p);

        p.make_move(m);
        p.assert_invariants();
        let fen = write_fen(&p);
        let reparsed = parse_fen(&fen).unwrap_or_else(|e| {
            panic!("seed={seed} ply={ply}: FEN round-trip failed: {e:?}\nfen={fen}")
        });
        assert_eq!(
            snapshot(&p),
            snapshot(&reparsed),
            "seed={seed} ply={ply}: FEN round-trip not bit-identical\nfen={fen}"
        );

        p.unmake_move(m);
        assert_eq!(
            before,
            snapshot(&p),
            "seed={seed} ply={ply}: make/unmake not identity (move={m})"
        );

        p.make_move(m);
        p.assert_invariants();
    }
}

#[test]
fn random_games_many_seeds() {
    for seed in 1u64..=200 {
        walk(seed, 100);
    }
}

#[test]
fn random_games_long() {
    for seed in 1u64..=10 {
        walk(seed * 7919, 400);
    }
}

#[test]
fn startpos_piece_counts() {
    let p = parse_fen(STARTING_FEN).unwrap();
    use Color::*;
    use PieceType::*;
    let count = |c, pt| p.piece_bb(c, pt).count_ones();
    assert_eq!(
        (
            count(White, Pawn),
            count(White, Knight),
            count(White, Bishop),
            count(White, Rook),
            count(White, Queen),
            count(White, King)
        ),
        (8, 2, 2, 2, 1, 1)
    );
    assert_eq!(count(Black, Pawn), 8);
    assert_eq!(count(Black, King), 1);
}
