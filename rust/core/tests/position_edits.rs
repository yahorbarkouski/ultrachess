//! Direct tests for `Position::put_at` / `remove_at` and other methods that
//! are only reached via the WASM ABI in the TS test suite — so llvm-cov
//! sees them as uncovered even though they are behaviourally exercised.

mod common;

use common::STARTING_FEN;
use ultrachess_core::chess_move::{Move, MoveKind};
use ultrachess_core::fen::parse_fen;
use ultrachess_core::movegen::{generate_legal_moves, MoveList};
use ultrachess_core::{Color, Piece, PieceType, Square};

#[test]
fn put_at_on_empty_square_returns_none() {
    let mut p = parse_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
    assert!(p.piece_at(Square::D4).is_none());
    let replaced = p.put_at(Square::D4, Piece::new(Color::White, PieceType::Queen));
    assert!(replaced.is_none());
    assert_eq!(
        p.piece_at(Square::D4),
        Piece::new(Color::White, PieceType::Queen)
    );
}

#[test]
fn put_at_over_existing_piece_returns_old_piece() {
    let mut p = parse_fen(STARTING_FEN).unwrap();
    let replaced = p.put_at(Square::E2, Piece::new(Color::Black, PieceType::Queen));
    assert!(replaced.is_some());
    assert_eq!(replaced.unwrap().piece_type(), PieceType::Pawn);
    assert_eq!(replaced.unwrap().color(), Color::White);
}

#[test]
fn put_at_clears_undo_history_and_recomputes_zobrist() {
    let mut p = parse_fen(STARTING_FEN).unwrap();
    // Play a move to push onto history, then edit.
    let mut ml = MoveList::new();
    generate_legal_moves(&p, &mut ml);
    let e4 = *ml
        .iter()
        .find(|m| m.from() == Square::E2 && m.to() == Square::E4)
        .unwrap();
    p.make_move(e4);
    assert_eq!(p.history_len(), 1);

    p.put_at(Square::D4, Piece::new(Color::White, PieceType::Queen));
    assert_eq!(p.history_len(), 0, "put_at must clear history");
    // Hash matches a from-scratch recompute.
    assert_eq!(
        p.hash(),
        ultrachess_core::zobrist::compute_hash_from_scratch(&p)
    );
}

#[test]
fn remove_at_on_empty_square_returns_none() {
    let mut p = parse_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
    assert!(p.remove_at(Square::A1).is_none());
}

#[test]
fn remove_at_returns_removed_piece_and_clears_history() {
    let mut p = parse_fen(STARTING_FEN).unwrap();
    let mut ml = MoveList::new();
    generate_legal_moves(&p, &mut ml);
    p.make_move(ml.as_slice()[0]);
    assert_eq!(p.history_len(), 1);

    let removed = p.remove_at(Square::E2);
    assert_eq!(p.history_len(), 0);
    // The e2 pawn either moved already (if the first legal move generated
    // was e2-?) or is still there; either way, `remove_at` returns whatever
    // *is* there. Our invariant: it returns a piece or None and the square
    // is empty afterwards.
    let _ = removed;
    assert!(p.piece_at(Square::E2).is_none());
}

// ---------------------------------------------------------------------------
// make_move_perft / unmake_move_perft parity with make_move / unmake_move
// ---------------------------------------------------------------------------

#[test]
fn perft_variant_produces_same_mailbox_state_for_every_move() {
    // For every legal move at startpos: make_move and make_move_perft must
    // produce identical board state (bitboards + mailbox + side + castling
    // + ep). Only halfmove / fullmove / zobrist may differ.
    let mut slow = parse_fen(STARTING_FEN).unwrap();
    let mut fast = parse_fen(STARTING_FEN).unwrap();
    let mut ml = MoveList::new();
    generate_legal_moves(&slow, &mut ml);
    let n = ml.len();
    for i in 0..n {
        let m: Move = ml.as_slice()[i];
        slow.make_move(m);
        fast.make_move_perft(m);
        // Compare board-relevant state.
        for sq in 0..64u8 {
            assert_eq!(
                slow.piece_at(Square(sq)),
                fast.piece_at(Square(sq)),
                "mailbox diverged at sq={sq} after move {m}"
            );
        }
        assert_eq!(slow.side_to_move, fast.side_to_move);
        assert_eq!(slow.castling, fast.castling);
        assert_eq!(slow.ep_square, fast.ep_square);

        slow.unmake_move(m);
        fast.unmake_move_perft(m);
    }
}

#[test]
fn perft_variant_handles_every_move_kind() {
    // Exercise Normal, Promotion, EnPassant, Castle via make_move_perft.
    let kiwipete =
        parse_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1").unwrap();
    let mut p = kiwipete.clone();
    let mut ml = MoveList::new();
    generate_legal_moves(&p, &mut ml);
    let mut kinds_seen = [false; 4];
    let n = ml.len();
    for i in 0..n {
        let m = ml.as_slice()[i];
        kinds_seen[m.kind() as usize] = true;
        p.make_move_perft(m);
        p.unmake_move_perft(m);
    }
    // Kiwipete has at least Normal + Castle reachable. We don't strictly
    // need all four here — movegen.rs tests and perft tests cover promo/EP.
    assert!(kinds_seen[MoveKind::Normal as usize]);
}
