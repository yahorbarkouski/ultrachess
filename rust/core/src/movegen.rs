//! Fully-legal move generation using pin masks and check masks.
//!
//! Algorithm (Peter Ellis Jones, 2017):
//! 1. Compute `checkers` — enemy pieces attacking our king.
//! 2. Compute `king_danger` — every square attacked by the enemy with our
//!    king removed from the board. King moves are restricted to squares not
//!    in this mask (else the king walks into a ray-slider's attack).
//! 3. Emit king moves to empty or capturable squares outside `king_danger`.
//! 4. If double-check: only king moves are legal. Return.
//! 5. Otherwise derive `check_mask` = either the whole board (no check) or
//!    (checker square + squares between king and checker if slider).
//! 6. Compute pinned pieces & their pin rays.
//! 7. Emit non-king moves restricted by `check_mask`; pinned pieces are
//!    additionally restricted to the line through king and pinner.
//! 8. Emit castling if legal (not in check; path clear; king's travel
//!    squares not attacked).
//! 9. En-passant has its own horizontal-discovered-check check.
//!
//! The generator is **generic over an output sink** (`MoveSink`). The same
//! code path produces either:
//! - a filled [`MoveList`] (caller uses moves), or
//! - a legal-move count (caller uses just the number).
//!
//! The counter sink collapses per-move loops into `popcount` instructions,
//! which is the big win at perft leaves (`depth == 1`): we never materialise
//! moves just to read `.len()`.

use crate::bitboard::{self, pop_lsb, Bitboard};
use crate::chess_move::Move;
use crate::position::Position;
use crate::tables;
use crate::types::{CastlingRights, Color, PieceType, Square};

// ---------------------------------------------------------------------------
// Sinks
// ---------------------------------------------------------------------------

/// A consumer of legal moves. Implemented by both [`MoveList`] (materialising
/// sink) and [`MoveCounter`] (popcount-only sink).
///
/// The API is deliberately **bulk-oriented**: callers hand over whole target
/// bitboards, and the sink decides whether to iterate (materialise) or
/// popcount (count). This is how the count path avoids ever running
/// `pop_lsb` / `push` loops.
pub trait MoveSink {
    /// Emit quiet/capture moves from a fixed `from` to each set bit in
    /// `targets`. No promotion, no EP, no castle.
    fn push_targets(&mut self, from: Square, targets: Bitboard);

    /// Emit pawn pushes/captures derived from `targets` where
    /// `from = to - offset`. `offset` is positive for white pushes/captures,
    /// negative for black. Non-promotion only.
    fn push_pawn_targets_offset(&mut self, targets: Bitboard, offset: i32);

    /// Emit pawn promotion moves derived from `targets` where
    /// `from = to - offset`. Each target becomes four moves (Q/R/B/N).
    fn push_pawn_promotions_offset(&mut self, targets: Bitboard, offset: i32);

    /// Emit a single move (castling, en-passant, or any one-off).
    fn push_one(&mut self, m: Move);
}

// -- MoveList (materialising sink) ------------------------------------------

/// Pre-allocated move list. Chess positions have at most ~218 legal moves in
/// any real position (theoretical max ~256); 256 gives us headroom.
///
/// Backed by `MaybeUninit` so `MoveList::new()` skips a 512-byte memset on
/// the hot path; reads are gated by `len` so uninitialised cells are never
/// observed.
pub struct MoveList {
    buf: [core::mem::MaybeUninit<Move>; 256],
    len: usize,
}

impl MoveList {
    #[inline(always)]
    pub fn new() -> Self {
        // Inline-const `MaybeUninit::uninit()` is the idiomatic post-1.79 way
        // to build an uninitialised array without touching `assume_init`.
        let buf = [const { core::mem::MaybeUninit::<Move>::uninit() }; 256];
        Self { buf, len: 0 }
    }

    /// Push a move. Theoretical chess positions have ≤ 218 legal moves, so
    /// the 256-slot buffer is always sufficient; the `debug_assert` pins
    /// that invariant for tests.
    #[inline(always)]
    pub fn push(&mut self, m: Move) {
        debug_assert!(self.len < 256);
        // SAFETY: `self.len < 256` by the debug assert and the chess
        // max-legal-moves invariant. `write` initialises the slot before
        // `self.len` is incremented, so `as_slice()` only ever observes
        // initialised cells.
        unsafe { self.buf.get_unchecked_mut(self.len).write(m) };
        self.len += 1;
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[Move] {
        // SAFETY:
        // - `MaybeUninit<T>` is guaranteed `#[repr(transparent)]` over `T`,
        //   so `&[MaybeUninit<Move>]` and `&[Move]` share layout.
        // - Entries `0..self.len` are initialised by `push` before `len` is
        //   incremented, so the reinterpret only exposes initialised cells.
        unsafe { core::slice::from_raw_parts(self.buf.as_ptr() as *const Move, self.len) }
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.len = 0;
    }

    #[inline(always)]
    pub fn iter(&self) -> core::slice::Iter<'_, Move> {
        self.as_slice().iter()
    }
}

impl Default for MoveList {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl MoveSink for MoveList {
    #[inline(always)]
    fn push_targets(&mut self, from: Square, mut targets: Bitboard) {
        while targets != 0 {
            let to = pop_lsb(&mut targets);
            self.push(Move::quiet(from, to));
        }
    }

    #[inline(always)]
    fn push_pawn_targets_offset(&mut self, mut targets: Bitboard, offset: i32) {
        while targets != 0 {
            let to = pop_lsb(&mut targets);
            let from = Square((to.0 as i32 - offset) as u8);
            self.push(Move::quiet(from, to));
        }
    }

    #[inline(always)]
    fn push_pawn_promotions_offset(&mut self, mut targets: Bitboard, offset: i32) {
        while targets != 0 {
            let to = pop_lsb(&mut targets);
            let from = Square((to.0 as i32 - offset) as u8);
            self.push(Move::promotion(from, to, PieceType::Queen));
            self.push(Move::promotion(from, to, PieceType::Rook));
            self.push(Move::promotion(from, to, PieceType::Bishop));
            self.push(Move::promotion(from, to, PieceType::Knight));
        }
    }

    #[inline(always)]
    fn push_one(&mut self, m: Move) {
        self.push(m);
    }
}

// -- MoveCounter (popcount sink) --------------------------------------------

/// Counts legal moves without materialising them. Used at perft leaves
/// where we only need the move count.
#[derive(Default)]
pub struct MoveCounter {
    count: u32,
}

impl MoveCounter {
    #[inline(always)]
    pub fn new() -> Self {
        Self { count: 0 }
    }

    #[inline(always)]
    pub fn get(&self) -> u32 {
        self.count
    }
}

impl MoveSink for MoveCounter {
    #[inline(always)]
    fn push_targets(&mut self, _from: Square, targets: Bitboard) {
        self.count += targets.count_ones();
    }

    #[inline(always)]
    fn push_pawn_targets_offset(&mut self, targets: Bitboard, _offset: i32) {
        self.count += targets.count_ones();
    }

    #[inline(always)]
    fn push_pawn_promotions_offset(&mut self, targets: Bitboard, _offset: i32) {
        self.count += 4 * targets.count_ones();
    }

    #[inline(always)]
    fn push_one(&mut self, _m: Move) {
        self.count += 1;
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Generate all strictly-legal moves for the side to move, materialised into
/// `moves`.
#[inline]
pub fn generate_legal_moves(pos: &Position, moves: &mut MoveList) {
    moves.clear();
    generate_moves_into(pos, moves);
}

/// Return the number of strictly-legal moves for the side to move without
/// materialising them — the perft-leaf fast path.
#[inline]
pub fn count_legal_moves(pos: &Position) -> u32 {
    let mut counter = MoveCounter::new();
    generate_moves_into(pos, &mut counter);
    counter.get()
}

// ---------------------------------------------------------------------------
// Core generator (generic over sink)
// ---------------------------------------------------------------------------

#[inline(always)]
fn generate_moves_into<S: MoveSink>(pos: &Position, sink: &mut S) {
    let us = pos.side_to_move;
    let them = us.opponent();
    let our_pieces = pos.color_bb(us);
    let their_pieces = pos.color_bb(them);
    let occ = our_pieces | their_pieces;
    let king_sq = pos.king_sq(us);

    // 1. Checkers.
    let checkers = pos.attackers_to(king_sq, them, occ);

    // 2. Enemy attacks with king removed — king-danger mask.
    let king_danger = enemy_attacks(pos, us, occ ^ king_sq.bb());

    // 3. King moves (sink-aware).
    let king_targets = tables::king_attacks(king_sq.0) & !our_pieces & !king_danger;
    sink.push_targets(king_sq, king_targets);

    // 4. Double check: only king moves.
    let num_checkers = checkers.count_ones();
    if num_checkers >= 2 {
        return;
    }

    // 5. Check mask.
    let check_mask: Bitboard = if num_checkers == 1 {
        let checker_sq = Square(bitboard::lsb(checkers));
        let checker_piece = pos.piece_at(checker_sq);
        let is_slider = matches!(
            checker_piece.piece_type(),
            PieceType::Bishop | PieceType::Rook | PieceType::Queen
        );
        if is_slider {
            checkers | tables::between(king_sq.0, checker_sq.0)
        } else {
            checkers
        }
    } else {
        !0
    };

    // 6. Pinned pieces, split by direction — eliminates the per-piece
    //    `tables::line()` dependent-load in the slider hot loop by letting
    //    us precompute "may this pinned piece move at all for my direction".
    let (pinned_hv, pinned_diag) = compute_pinned_split(pos, king_sq, us, them, occ);
    let pinned = pinned_hv | pinned_diag;

    // 7. Non-king pseudo-legal moves, filtered by check_mask and pin lines.
    emit_pawn_moves(
        pos,
        us,
        king_sq,
        pinned_hv,
        pinned_diag,
        check_mask,
        occ,
        their_pieces,
        sink,
    );
    emit_knight_moves(pos, us, pinned, check_mask, our_pieces, sink);
    emit_slider_moves(
        pos,
        us,
        king_sq,
        pinned_hv,
        pinned_diag,
        check_mask,
        occ,
        our_pieces,
        sink,
    );

    // 8. Castling — only if not in check.
    if num_checkers == 0 {
        emit_castling(pos, us, occ, king_danger, sink);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn enemy_attacks(pos: &Position, us: Color, occ: Bitboard) -> Bitboard {
    let them = us.opponent();
    let mut attacks: Bitboard = 0;

    // Pawns — bit-parallel, one shift per diagonal.
    let pawns = pos.piece_bb(them, PieceType::Pawn);
    attacks |= match them {
        Color::White => bitboard::north_east(pawns) | bitboard::north_west(pawns),
        Color::Black => bitboard::south_east(pawns) | bitboard::south_west(pawns),
    };

    let mut knights = pos.piece_bb(them, PieceType::Knight);
    while knights != 0 {
        let sq = pop_lsb(&mut knights);
        attacks |= tables::knight_attacks(sq.0);
    }

    attacks |= tables::king_attacks(pos.king_sq(them).0);

    let mut bishops_queens =
        pos.piece_bb(them, PieceType::Bishop) | pos.piece_bb(them, PieceType::Queen);
    while bishops_queens != 0 {
        let sq = pop_lsb(&mut bishops_queens);
        attacks |= tables::bishop_attacks(sq.0, occ);
    }

    let mut rooks_queens =
        pos.piece_bb(them, PieceType::Rook) | pos.piece_bb(them, PieceType::Queen);
    while rooks_queens != 0 {
        let sq = pop_lsb(&mut rooks_queens);
        attacks |= tables::rook_attacks(sq.0, occ);
    }

    attacks
}

/// Split pin-mask computation. Returns `(pinned_hv, pinned_diag)`:
/// - `pinned_hv`    : our pieces pinned by an enemy rook/queen (file/rank).
/// - `pinned_diag`  : our pieces pinned by an enemy bishop/queen (diagonal).
///
/// Splitting lets each piece kind filter against only the direction that
/// actually constrains it, and avoids a per-piece `tables::line()` lookup
/// in the slider hot loop.
fn compute_pinned_split(
    pos: &Position,
    king_sq: Square,
    us: Color,
    them: Color,
    occ: Bitboard,
) -> (Bitboard, Bitboard) {
    let our_pieces = pos.color_bb(us);
    let their_pieces = pos.color_bb(them);

    // Candidate pinners: enemy sliders that see the king assuming only enemy
    // pieces block (so the ray stops at the first enemy piece).
    let mut diag_snipers = tables::bishop_attacks(king_sq.0, their_pieces)
        & (pos.piece_bb(them, PieceType::Bishop) | pos.piece_bb(them, PieceType::Queen));
    let mut orth_snipers = tables::rook_attacks(king_sq.0, their_pieces)
        & (pos.piece_bb(them, PieceType::Rook) | pos.piece_bb(them, PieceType::Queen));

    let mut pinned_diag: Bitboard = 0;
    while diag_snipers != 0 {
        let sniper_sq = pop_lsb(&mut diag_snipers);
        let between = tables::between(king_sq.0, sniper_sq.0) & occ;
        if between.count_ones() == 1 && (between & our_pieces) != 0 {
            pinned_diag |= between;
        }
    }
    let mut pinned_hv: Bitboard = 0;
    while orth_snipers != 0 {
        let sniper_sq = pop_lsb(&mut orth_snipers);
        let between = tables::between(king_sq.0, sniper_sq.0) & occ;
        if between.count_ones() == 1 && (between & our_pieces) != 0 {
            pinned_hv |= between;
        }
    }
    (pinned_hv, pinned_diag)
}

#[inline(always)]
fn emit_knight_moves<S: MoveSink>(
    pos: &Position,
    us: Color,
    pinned: Bitboard,
    check_mask: Bitboard,
    our_pieces: Bitboard,
    sink: &mut S,
) {
    // Pinned knights can never move (they'd leave any pin line).
    let mut knights = pos.piece_bb(us, PieceType::Knight) & !pinned;
    while knights != 0 {
        let from = pop_lsb(&mut knights);
        let targets = tables::knight_attacks(from.0) & !our_pieces & check_mask;
        sink.push_targets(from, targets);
    }
}

#[inline(always)]
fn emit_slider_moves<S: MoveSink>(
    pos: &Position,
    us: Color,
    king_sq: Square,
    pinned_hv: Bitboard,
    pinned_diag: Bitboard,
    check_mask: Bitboard,
    occ: Bitboard,
    our_pieces: Bitboard,
    sink: &mut S,
) {
    let bishops = pos.piece_bb(us, PieceType::Bishop);
    let rooks = pos.piece_bb(us, PieceType::Rook);
    let queens = pos.piece_bb(us, PieceType::Queen);

    // Diagonal movers (bishops + queens). HV-pinned ones can't move at all.
    let mut diag = (bishops | queens) & !pinned_hv;
    while diag != 0 {
        let from = pop_lsb(&mut diag);
        let mut targets = tables::bishop_attacks(from.0, occ) & !our_pieces & check_mask;
        if pinned_diag & from.bb() != 0 {
            targets &= tables::line(king_sq.0, from.0);
        }
        sink.push_targets(from, targets);
    }

    // Orthogonal movers (rooks + queens). Diag-pinned ones can't move at all.
    let mut orth = (rooks | queens) & !pinned_diag;
    while orth != 0 {
        let from = pop_lsb(&mut orth);
        let mut targets = tables::rook_attacks(from.0, occ) & !our_pieces & check_mask;
        if pinned_hv & from.bb() != 0 {
            targets &= tables::line(king_sq.0, from.0);
        }
        sink.push_targets(from, targets);
    }
}

#[inline(always)]
fn emit_pawn_moves<S: MoveSink>(
    pos: &Position,
    us: Color,
    king_sq: Square,
    pinned_hv: Bitboard,
    pinned_diag: Bitboard,
    check_mask: Bitboard,
    occ: Bitboard,
    their_pieces: Bitboard,
    sink: &mut S,
) {
    let pawns = pos.piece_bb(us, PieceType::Pawn);
    let empty = !occ;
    let (promo_rank, start_rank, push_dir): (Bitboard, Bitboard, i32) = match us {
        Color::White => (bitboard::RANK_8, bitboard::RANK_3, 8),
        Color::Black => (bitboard::RANK_1, bitboard::RANK_6, -8),
    };
    let pinned = pinned_hv | pinned_diag;

    // --- Single pushes ----------------------------------------------------
    // Bulk-emit non-pinned pawns via shift; the pinned-pawn slow lane below
    // filters per-move against `tables::line(king, from)`.
    let raw_single = match us {
        Color::White => bitboard::north(pawns),
        Color::Black => bitboard::south(pawns),
    } & empty;
    let single_targets = raw_single & check_mask;

    // --- Double pushes ----------------------------------------------------
    let raw_double = match us {
        Color::White => bitboard::north(raw_single & start_rank),
        Color::Black => bitboard::south(raw_single & start_rank),
    } & empty;
    let double_targets = raw_double & check_mask;

    // --- Captures ---------------------------------------------------------
    let (cap_left_raw, cap_right_raw, cap_left_offset, cap_right_offset) = match us {
        Color::White => (
            bitboard::north_west(pawns) & their_pieces,
            bitboard::north_east(pawns) & their_pieces,
            7i32,
            9i32,
        ),
        Color::Black => (
            bitboard::south_west(pawns) & their_pieces,
            bitboard::south_east(pawns) & their_pieces,
            -9i32,
            -7i32,
        ),
    };
    let cap_left_targets = cap_left_raw & check_mask;
    let cap_right_targets = cap_right_raw & check_mask;

    // Split each target bitboard into promo / non-promo once up front.
    let single_non_promo = single_targets & !promo_rank;
    let single_promo = single_targets & promo_rank;
    let cap_left_non_promo = cap_left_targets & !promo_rank;
    let cap_left_promo = cap_left_targets & promo_rank;
    let cap_right_non_promo = cap_right_targets & !promo_rank;
    let cap_right_promo = cap_right_targets & promo_rank;

    if pinned == 0 {
        // Fast path: bulk emission, no pin check.
        sink.push_pawn_targets_offset(single_non_promo, push_dir);
        sink.push_pawn_promotions_offset(single_promo, push_dir);
        sink.push_pawn_targets_offset(double_targets, push_dir * 2);
        sink.push_pawn_targets_offset(cap_left_non_promo, cap_left_offset);
        sink.push_pawn_promotions_offset(cap_left_promo, cap_left_offset);
        sink.push_pawn_targets_offset(cap_right_non_promo, cap_right_offset);
        sink.push_pawn_promotions_offset(cap_right_promo, cap_right_offset);
    } else {
        // Split by pinned-from: bulk lane + per-move pin-line filter (rare).
        emit_pawn_class(sink, single_non_promo, push_dir, false, pinned, king_sq);
        emit_pawn_class(sink, single_promo, push_dir, true, pinned, king_sq);
        emit_pawn_class(sink, double_targets, push_dir * 2, false, pinned, king_sq);
        emit_pawn_class(
            sink,
            cap_left_non_promo,
            cap_left_offset,
            false,
            pinned,
            king_sq,
        );
        emit_pawn_class(sink, cap_left_promo, cap_left_offset, true, pinned, king_sq);
        emit_pawn_class(
            sink,
            cap_right_non_promo,
            cap_right_offset,
            false,
            pinned,
            king_sq,
        );
        emit_pawn_class(
            sink,
            cap_right_promo,
            cap_right_offset,
            true,
            pinned,
            king_sq,
        );
    }

    // --- En passant (rare) -------------------------------------------------
    if let Some(ep) = pos.ep_square {
        let attackers = tables::pawn_attacks(us.opponent(), ep.0) & pawns;
        let mut a = attackers;
        while a != 0 {
            let from = pop_lsb(&mut a);
            let cap_sq = Square::from_file_rank(ep.file(), from.rank());

            // Standard pin check along the pin line.
            if pinned & from.bb() != 0 && (tables::line(king_sq.0, from.0) & ep.bb()) == 0 {
                continue;
            }
            // Check-evasion: the EP must capture the checker or block it.
            if check_mask != !0 && (cap_sq.bb() & check_mask) == 0 && (ep.bb() & check_mask) == 0 {
                continue;
            }
            // EP horizontal-discovered-check.
            let new_occ = (occ ^ from.bb() ^ cap_sq.bb()) | ep.bb();
            let them = us.opponent();
            let rooks_queens =
                pos.piece_bb(them, PieceType::Rook) | pos.piece_bb(them, PieceType::Queen);
            let bishops_queens =
                pos.piece_bb(them, PieceType::Bishop) | pos.piece_bb(them, PieceType::Queen);
            if (tables::rook_attacks(king_sq.0, new_occ) & rooks_queens) != 0 {
                continue;
            }
            if (tables::bishop_attacks(king_sq.0, new_occ) & bishops_queens) != 0 {
                continue;
            }
            sink.push_one(Move::en_passant(from, ep));
        }
    }
}

/// Pawn-target emission path used when at least one pawn is pinned. Splits
/// `targets` into non-pinned (bulk) + pinned (per-move pin check).
#[inline(always)]
fn emit_pawn_class<S: MoveSink>(
    sink: &mut S,
    targets: Bitboard,
    offset: i32,
    is_promo: bool,
    pinned: Bitboard,
    king_sq: Square,
) {
    if targets == 0 {
        return;
    }
    // Partition targets by whether their `from` is pinned.
    let mut pinned_targets: Bitboard = 0;
    let mut tt = targets;
    while tt != 0 {
        let to = pop_lsb(&mut tt);
        let from_sq = (to.0 as i32 - offset) as u8;
        if (pinned >> from_sq) & 1 != 0 {
            pinned_targets |= 1u64 << to.0;
        }
    }
    let unpinned_targets = targets & !pinned_targets;

    // Fast lane: unpinned targets emit in bulk.
    if is_promo {
        sink.push_pawn_promotions_offset(unpinned_targets, offset);
    } else {
        sink.push_pawn_targets_offset(unpinned_targets, offset);
    }

    // Slow lane: each pinned target individually filtered by pin line.
    let mut pt = pinned_targets;
    while pt != 0 {
        let to = pop_lsb(&mut pt);
        let from = Square((to.0 as i32 - offset) as u8);
        if (tables::line(king_sq.0, from.0) & to.bb()) == 0 {
            continue;
        }
        if is_promo {
            sink.push_one(Move::promotion(from, to, PieceType::Queen));
            sink.push_one(Move::promotion(from, to, PieceType::Rook));
            sink.push_one(Move::promotion(from, to, PieceType::Bishop));
            sink.push_one(Move::promotion(from, to, PieceType::Knight));
        } else {
            sink.push_one(Move::quiet(from, to));
        }
    }
}

#[inline(always)]
fn emit_castling<S: MoveSink>(
    pos: &Position,
    us: Color,
    occ: Bitboard,
    king_danger: Bitboard,
    sink: &mut S,
) {
    let rights = pos.castling;
    match us {
        Color::White => {
            if rights.has(CastlingRights::WHITE_KING) {
                let between_empty = (Square::F1.bb() | Square::G1.bb()) & occ == 0;
                let safe = (Square::F1.bb() | Square::G1.bb()) & king_danger == 0;
                if between_empty && safe {
                    sink.push_one(Move::castle(Square::E1, Square::G1));
                }
            }
            if rights.has(CastlingRights::WHITE_QUEEN) {
                let between_empty =
                    (Square::B1.bb() | Square::C1.bb() | Square::D1.bb()) & occ == 0;
                let safe = (Square::C1.bb() | Square::D1.bb()) & king_danger == 0;
                if between_empty && safe {
                    sink.push_one(Move::castle(Square::E1, Square::C1));
                }
            }
        }
        Color::Black => {
            if rights.has(CastlingRights::BLACK_KING) {
                let between_empty = (Square::F8.bb() | Square::G8.bb()) & occ == 0;
                let safe = (Square::F8.bb() | Square::G8.bb()) & king_danger == 0;
                if between_empty && safe {
                    sink.push_one(Move::castle(Square::E8, Square::G8));
                }
            }
            if rights.has(CastlingRights::BLACK_QUEEN) {
                let between_empty =
                    (Square::B8.bb() | Square::C8.bb() | Square::D8.bb()) & occ == 0;
                let safe = (Square::C8.bb() | Square::D8.bb()) & king_danger == 0;
                if between_empty && safe {
                    sink.push_one(Move::castle(Square::E8, Square::C8));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chess_move::MoveKind;
    use crate::fen::parse_fen;

    fn gen(fen: &str) -> (Position, MoveList) {
        let p = parse_fen(fen).unwrap();
        let mut ml = MoveList::new();
        generate_legal_moves(&p, &mut ml);
        (p, ml)
    }

    /// Property: `count_legal_moves` always equals `generate_legal_moves().len()`.
    fn assert_count_matches(fen: &str) {
        let p = parse_fen(fen).unwrap();
        let mut ml = MoveList::new();
        generate_legal_moves(&p, &mut ml);
        assert_eq!(
            ml.len() as u32,
            count_legal_moves(&p),
            "count_legal_moves disagrees with generate_legal_moves on {fen}"
        );
    }

    #[test]
    fn startpos_has_20_moves() {
        let (_p, ml) = gen(crate::fen::STARTING_FEN);
        assert_eq!(ml.len(), 20);
        assert_count_matches(crate::fen::STARTING_FEN);
    }

    #[test]
    fn kiwipete_has_48_moves() {
        let fen = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
        let (_p, ml) = gen(fen);
        assert_eq!(ml.len(), 48);
        assert_count_matches(fen);
    }

    #[test]
    fn count_matches_emit_for_all_reference_positions() {
        let fens = [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
            "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
            "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        ];
        for fen in fens {
            assert_count_matches(fen);
        }
    }

    #[test]
    fn double_check_only_king_moves() {
        let fen = "7k/8/8/b7/8/5n2/8/4K3 w - - 0 1";
        let (_p, ml) = gen(fen);
        for m in ml.iter() {
            assert_eq!(m.from(), Square::E1, "only king moves on double check");
            assert!(matches!(m.kind(), MoveKind::Normal));
        }
        assert!(!ml.is_empty(), "at least some king move should exist");
        assert_count_matches(fen);
    }

    #[test]
    fn castle_through_check_disallowed() {
        let fen = "4k2r/8/8/8/8/8/8/R3K2R w KQk - 0 1";
        let (_p, ml) = gen(fen);
        let castles: Vec<_> = ml.iter().filter(|m| m.kind() == MoveKind::Castle).collect();
        assert_eq!(castles.len(), 2, "both castles should be legal here");

        let fen2 = "5rk1/8/8/8/8/8/8/R3K2R w KQ - 0 1";
        let (_p, ml2) = gen(fen2);
        let castles2: Vec<_> = ml2
            .iter()
            .filter(|m| m.kind() == MoveKind::Castle)
            .collect();
        assert_eq!(castles2.len(), 1);
        assert_eq!(castles2[0].to(), Square::C1);
        assert_count_matches(fen);
        assert_count_matches(fen2);
    }

    #[test]
    fn absolute_pin_blocks_move() {
        let fen = "4r2k/8/8/8/8/8/4B3/4K3 w - - 0 1";
        let (_p, ml) = gen(fen);
        for m in ml.iter() {
            assert_ne!(m.from(), Square::E2, "pinned bishop cannot move");
        }
        assert_count_matches(fen);
    }

    #[test]
    fn pinned_pawn_can_capture_along_diagonal() {
        let fen = "7k/8/8/8/8/2b5/3P4/4K3 w - - 0 1";
        let (_p, ml) = gen(fen);
        let c3 = Square::from_file_rank(2, 2);
        let d3 = Square::from_file_rank(3, 2);
        let e3 = Square::from_file_rank(4, 2);
        let caps_c3: Vec<_> = ml
            .iter()
            .filter(|m| m.from() == Square::D2 && m.to() == c3)
            .collect();
        let push_d3: Vec<_> = ml
            .iter()
            .filter(|m| m.from() == Square::D2 && m.to() == d3)
            .collect();
        let cap_e3: Vec<_> = ml
            .iter()
            .filter(|m| m.from() == Square::D2 && m.to() == e3)
            .collect();
        assert_eq!(caps_c3.len(), 1, "must be able to capture pinner");
        assert_eq!(push_d3.len(), 0, "push breaks pin");
        assert_eq!(cap_e3.len(), 0, "capture off the pin line is illegal");
        assert_count_matches(fen);
    }

    #[test]
    fn ep_discovered_check_is_rejected() {
        let fen = "8/8/8/KPp4r/8/8/8/4k3 w - c6 0 1";
        let (_p, ml) = gen(fen);
        let ep_moves: Vec<_> = ml
            .iter()
            .filter(|m| m.kind() == MoveKind::EnPassant)
            .collect();
        assert_eq!(
            ep_moves.len(),
            0,
            "EP must be rejected on horizontal discovered check"
        );
        assert_count_matches(fen);
    }

    #[test]
    fn checkmate_has_no_moves() {
        let fen = "r1bqkb1r/pppp1Qpp/2n2n2/4p3/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 0 4";
        let (_p, ml) = gen(fen);
        assert_eq!(ml.len(), 0);
        assert_count_matches(fen);
    }

    #[test]
    fn stalemate_has_no_moves() {
        let fen = "k7/2Q5/8/8/8/8/8/7K b - - 0 1";
        let (_p, ml) = gen(fen);
        assert_eq!(ml.len(), 0);
        assert_count_matches(fen);
    }
}
