//! SAN round-trip across the reference positions.
//!
//! For every legal move reachable to depth 3 from each canonical position,
//! `san_to_move(pos, move_to_san(pos, m))` must equal `m`. This exercises
//! disambiguation, promotion, check/mate suffixes, and castling against
//! realistic geometry.

mod common;

use common::PERFT_REFERENCE;
use ultrachess_core::chess_move::Move;
use ultrachess_core::fen::parse_fen;
use ultrachess_core::movegen::{generate_legal_moves, MoveList};
use ultrachess_core::position::Position;
use ultrachess_core::san::{move_to_san, san_to_move};

fn roundtrip_tree(pos: &mut Position, depth: u32) {
    if depth == 0 {
        return;
    }
    let mut ml = MoveList::new();
    generate_legal_moves(pos, &mut ml);
    let n = ml.len();
    for i in 0..n {
        let m: Move = ml.as_slice()[i];
        let san = move_to_san(pos, m);
        let parsed = san_to_move(pos, &san).unwrap_or_else(|e| {
            panic!(
                "parse failed: san={san:?} error={e:?} move={m:?}\nFEN={}",
                ultrachess_core::fen::write_fen(pos)
            )
        });
        assert_eq!(
            parsed.0, m.0,
            "roundtrip: move={m:?} san={san:?} parsed={parsed:?}"
        );
        if depth > 1 {
            pos.make_move(m);
            roundtrip_tree(pos, depth - 1);
            pos.unmake_move(m);
        }
    }
}

#[test]
fn reference_positions_depth_2() {
    for &(name, fen, _) in PERFT_REFERENCE {
        let mut p = parse_fen(fen).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        roundtrip_tree(&mut p, 2);
    }
}

#[test]
fn reference_positions_depth_3() {
    for &(name, fen, _) in PERFT_REFERENCE {
        let mut p = parse_fen(fen).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        roundtrip_tree(&mut p, 3);
    }
}
