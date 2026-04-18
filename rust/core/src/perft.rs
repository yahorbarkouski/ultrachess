//! Perft: count leaf nodes of the game tree at a given depth.
//!
//! Correctness oracle. Matching reference node counts on the 6 standard
//! positions (see `tests/perft.rs`) is the gate for move generation.
//!
//! ## Fast paths
//! Two optimisations over the plain textbook perft, each gated by
//! correctness tests:
//! - **`depth == 1` uses `count_legal_moves`.** 95 %+ of all perft nodes
//!   at deep perft live at the leaf boundary. The counter sink collapses
//!   move-emission loops into popcount, so we never materialise moves
//!   just to read `.len()`.
//! - **`make_move_perft` / `unmake_move_perft`** at `depth >= 2`. Skips
//!   zobrist XORs, `history_hashes` book-keeping, and the halfmove /
//!   fullmove clocks — none of which affect the move tree.

use crate::chess_move::Move;
use crate::movegen::{count_legal_moves, generate_legal_moves, MoveList};
use crate::position::Position;

pub fn perft(pos: &mut Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    if depth == 1 {
        return count_legal_moves(pos) as u64;
    }
    let mut moves = MoveList::new();
    generate_legal_moves(pos, &mut moves);
    let n = moves.len();
    let mut count = 0u64;
    for i in 0..n {
        // `Move` is `Copy`; the immutable borrow ends before the mutable call.
        let m: Move = moves.as_slice()[i];
        pos.make_move_perft(m);
        count += perft(pos, depth - 1);
        pos.unmake_move_perft(m);
    }
    count
}

/// Divide perft: for each root move, report the sub-count. Useful for
/// bisecting divergences against a reference engine.
pub fn perft_divide(pos: &mut Position, depth: u32) -> Vec<(Move, u64)> {
    assert!(depth >= 1);
    let mut moves = MoveList::new();
    generate_legal_moves(pos, &mut moves);
    let n = moves.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let m: Move = moves.as_slice()[i];
        pos.make_move_perft(m);
        let sub = if depth == 1 { 1 } else { perft(pos, depth - 1) };
        pos.unmake_move_perft(m);
        out.push((m, sub));
    }
    out
}
