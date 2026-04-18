//! `Position` — the board state, with make / unmake and attack queries.
//!
//! Layout:
//! - `pieces[color][piece_type]` — bitboards (12 total).
//! - `color_bb[color]` — per-color occupancy.
//! - `mailbox[64]` — redundant piece-per-square (`Piece::NONE` if empty).
//! - `side_to_move`, `castling`, `ep_square`, `halfmove`, `fullmove`.
//! - `history: Vec<Undo>` — incremental undo records for `unmake_move`.
//!
//! Invariants (verified by `assert_invariants` in debug builds):
//! - `color_bb[c] == OR(pieces[c][0..6])`
//! - `mailbox[sq].is_some() ⟺ sq is set in some piece bitboard`
//! - Exactly one king per color

use crate::bitboard::{self, Bitboard};
use crate::chess_move::{Move, MoveKind};
use crate::movegen::{generate_legal_moves, MoveList};
use crate::tables;
use crate::types::{CastlingRights, Color, Piece, PieceType, Square};
use crate::zobrist;

/// Data captured before a move is made so it can be undone in O(1).
///
/// Storing `prev_zobrist` avoids recomputing the hash on unmake — a flat
/// restore is ~1 ns vs the 20-100 ns that an incremental reverse would cost.
#[derive(Copy, Clone, Debug)]
pub struct Undo {
    pub captured: Piece, // Piece::NONE if quiet
    pub prev_castling: CastlingRights,
    pub prev_ep: Option<Square>,
    pub prev_halfmove: u16,
    pub prev_zobrist: u64,
    /// Cached checkers bitboard before this move — used by `unmake_move`
    /// to restore the cache without a recomputation.
    pub prev_checkers: Bitboard,
}

#[derive(Debug)]
pub struct Position {
    pieces: [[Bitboard; 6]; 2],
    color_bb: [Bitboard; 2],
    mailbox: [Piece; 64],
    pub side_to_move: Color,
    pub castling: CastlingRights,
    pub ep_square: Option<Square>,
    pub halfmove: u16,
    pub fullmove: u16,
    history: Vec<Undo>,
    /// Zobrist hash of this position — maintained incrementally.
    zobrist: u64,
    /// Hashes of positions BEFORE each move in the current game, in order.
    /// Used for threefold-repetition detection. Bounded in practice by the
    /// 50-move rule (positions older than the last irreversible move can't
    /// repeat the current one), but we keep all entries so `unmake_move`
    /// can restore state without recovering a discarded prefix.
    history_hashes: Vec<u64>,
    /// Cached bitboard of enemy pieces currently attacking our king.
    /// Maintained by the full `make_move` / `unmake_move` path. Not
    /// maintained by the perft-specific fast path (`make_move_perft`),
    /// which uses its own movegen-time computation.
    ///
    /// Zero iff we're not in check, so `in_check()` is a branch-free
    /// `self.checkers != 0`.
    checkers: Bitboard,
}

impl Clone for Position {
    /// Board-state snapshot with an **empty** history. Mirrors what
    /// `new Chess(other.fen())` does in chess.js — preserving the undo
    /// stack would cost a heap copy on every clone and surprise callers.
    /// Zobrist and checker caches are copied verbatim.
    #[inline]
    fn clone(&self) -> Self {
        Self {
            pieces: self.pieces,
            color_bb: self.color_bb,
            mailbox: self.mailbox,
            side_to_move: self.side_to_move,
            castling: self.castling,
            ep_square: self.ep_square,
            halfmove: self.halfmove,
            fullmove: self.fullmove,
            history: Vec::new(),
            zobrist: self.zobrist,
            history_hashes: Vec::new(),
            checkers: self.checkers,
        }
    }
}

impl Position {
    pub fn empty() -> Self {
        Self {
            pieces: [[0; 6]; 2],
            color_bb: [0; 2],
            mailbox: [Piece::NONE; 64],
            side_to_move: Color::White,
            castling: CastlingRights::NONE,
            ep_square: None,
            halfmove: 0,
            fullmove: 1,
            history: Vec::with_capacity(128),
            zobrist: 0,
            history_hashes: Vec::with_capacity(128),
            checkers: 0,
        }
    }

    /// Recompute `checkers` from scratch. Called by the FEN loader after
    /// placing pieces, and usable as a debug assertion that the incremental
    /// cache hasn't drifted.
    pub fn recompute_checkers(&mut self) {
        let king_sq = self.king_sq(self.side_to_move);
        let them = self.side_to_move.opponent();
        self.checkers = self.attackers_to(king_sq, them, self.occupied());
    }

    /// Recompute the Zobrist hash from scratch and cache it. Called by FEN
    /// loaders after placing pieces; also useful as a debug assertion that
    /// incremental updates haven't drifted.
    pub fn recompute_zobrist(&mut self) {
        self.zobrist = zobrist::compute_hash_from_scratch(self);
    }

    /// Current Zobrist hash (cached; O(1)).
    #[inline(always)]
    pub fn hash(&self) -> u64 {
        self.zobrist
    }

    pub fn startpos() -> Self {
        crate::fen::parse_fen(crate::fen::STARTING_FEN)
            .expect("starting FEN is hard-coded and valid")
    }

    // -- board accessors -----------------------------------------------------

    #[inline(always)]
    pub fn piece_bb(&self, color: Color, pt: PieceType) -> Bitboard {
        self.pieces[color.index()][pt.index()]
    }

    #[inline(always)]
    pub fn color_bb(&self, color: Color) -> Bitboard {
        self.color_bb[color.index()]
    }

    #[inline(always)]
    pub fn occupied(&self) -> Bitboard {
        self.color_bb[0] | self.color_bb[1]
    }

    #[inline(always)]
    pub fn piece_at(&self, sq: Square) -> Piece {
        self.mailbox[sq.index()]
    }

    #[inline(always)]
    pub fn king_sq(&self, color: Color) -> Square {
        let bb = self.piece_bb(color, PieceType::King);
        debug_assert!(bb != 0, "missing king for color");
        Square(bitboard::lsb(bb))
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    // -- public position edits (NOT moves) -----------------------------------

    /// Place `piece` on `sq`, replacing whatever was there. This is a
    /// *position edit*, not a move: the move history and repetition-era
    /// hashes are both cleared (undo has no meaning after an arbitrary edit).
    /// The Zobrist hash is recomputed from scratch; callers should expect
    /// this to be O(64) rather than the O(1) of `make_move`.
    ///
    /// Returns the piece that was replaced, if any.
    pub fn put_at(&mut self, sq: Square, piece: Piece) -> Option<Piece> {
        let replaced = if self.mailbox[sq.index()].is_some() {
            Some(self.remove_piece(sq))
        } else {
            None
        };
        self.place_piece(sq, piece);
        self.history.clear();
        self.history_hashes.clear();
        self.recompute_zobrist();
        replaced
    }

    /// Remove the piece at `sq`. Like `put_at`, invalidates history and
    /// recomputes the hash. Returns the removed piece, or `None` if `sq`
    /// was empty.
    pub fn remove_at(&mut self, sq: Square) -> Option<Piece> {
        if self.mailbox[sq.index()].is_none() {
            return None;
        }
        let removed = self.remove_piece(sq);
        self.history.clear();
        self.history_hashes.clear();
        self.recompute_zobrist();
        Some(removed)
    }

    // -- ep legality ---------------------------------------------------------

    /// True iff at least one *fully legal* en-passant capture to `ep_sq`
    /// exists for `capturer` — i.e. pseudo-legal AND not leaving the king
    /// in check (the classic "EP discovered check" edge case, where
    /// removing both pawns on the same rank exposes a slider).
    ///
    /// X-FEN 2020 and chess.js require full legality; looser definitions
    /// would make FEN output and Zobrist hashes conflate semantically-
    /// distinct positions. Caching the answer in `ep_square` at
    /// make-move time lets `movegen` skip the entire EP block whenever
    /// `ep_square` is `None`.
    ///
    /// Takes `capturer` explicitly so both callers work unchanged:
    /// `make_move` (pre-flip, capturer = them) and `parse_fen`
    /// (capturer = side_to_move).
    pub(crate) fn ep_capture_is_legal_for(&self, ep_sq: Square, capturer: Color) -> bool {
        let pusher = capturer.opponent();

        // Pawn-attack geometry is mirror-symmetric, so the squares from
        // which a `capturer` pawn attacks `ep_sq` are `pawn_attacks(pusher, ep_sq)`.
        let attack_from_mask = tables::pawn_attacks(pusher, ep_sq.0);
        let mut capturers = attack_from_mask & self.piece_bb(capturer, PieceType::Pawn);
        if capturers == 0 {
            return false;
        }

        let pushed_pawn_sq = match capturer {
            Color::White => Square::from_file_rank(ep_sq.file(), ep_sq.rank() - 1),
            Color::Black => Square::from_file_rank(ep_sq.file(), ep_sq.rank() + 1),
        };

        let king_sq = self.king_sq(capturer);
        let pusher_rq =
            self.piece_bb(pusher, PieceType::Rook) | self.piece_bb(pusher, PieceType::Queen);
        let pusher_bq =
            self.piece_bb(pusher, PieceType::Bishop) | self.piece_bb(pusher, PieceType::Queen);
        let occ = self.occupied();

        while capturers != 0 {
            let from = bitboard::pop_lsb(&mut capturers);
            // EP delta: two removals (both pawns) + one addition (at ep_sq).
            // Only sliders' visibility changes; non-slider threats are unaffected.
            let sim_occ = (occ ^ from.bb() ^ pushed_pawn_sq.bb()) | ep_sq.bb();

            if tables::rook_attacks(king_sq.0, sim_occ) & pusher_rq != 0 {
                continue;
            }
            if tables::bishop_attacks(king_sq.0, sim_occ) & pusher_bq != 0 {
                continue;
            }
            return true;
        }
        false
    }

    // -- board mutators (low-level; used by `make_move` / `unmake_move` and
    //    by FEN parser) -------------------------------------------------------

    pub(crate) fn place_piece(&mut self, sq: Square, piece: Piece) {
        debug_assert!(self.mailbox[sq.index()].is_none());
        self.pieces[piece.color().index()][piece.piece_type().index()] |= sq.bb();
        self.color_bb[piece.color().index()] |= sq.bb();
        self.mailbox[sq.index()] = piece;
    }

    pub(crate) fn remove_piece(&mut self, sq: Square) -> Piece {
        let p = self.mailbox[sq.index()];
        debug_assert!(p.is_some());
        self.pieces[p.color().index()][p.piece_type().index()] &= !sq.bb();
        self.color_bb[p.color().index()] &= !sq.bb();
        self.mailbox[sq.index()] = Piece::NONE;
        p
    }

    pub(crate) fn move_piece(&mut self, from: Square, to: Square) {
        let p = self.mailbox[from.index()];
        debug_assert!(p.is_some());
        debug_assert!(self.mailbox[to.index()].is_none());
        let mask = from.bb() | to.bb();
        self.pieces[p.color().index()][p.piece_type().index()] ^= mask;
        self.color_bb[p.color().index()] ^= mask;
        self.mailbox[from.index()] = Piece::NONE;
        self.mailbox[to.index()] = p;
    }

    // -- attack queries ------------------------------------------------------

    /// Returns the bitboard of pieces of `by` that attack `sq` given the
    /// current occupancy. Used for check detection and castling safety.
    pub fn attackers_to(&self, sq: Square, by: Color, occ: Bitboard) -> Bitboard {
        let by_i = by.index();
        let mut attackers = 0u64;

        // Pawns: we need *enemy* pawns that would capture sq, which is the same
        // as asking "where does a pawn of our color on sq attack from?" — flip
        // the color to get the squares they'd come from.
        attackers |=
            tables::pawn_attacks(by.opponent(), sq.0) & self.pieces[by_i][PieceType::Pawn.index()];
        attackers |= tables::knight_attacks(sq.0) & self.pieces[by_i][PieceType::Knight.index()];
        attackers |= tables::king_attacks(sq.0) & self.pieces[by_i][PieceType::King.index()];

        let bishops_queens = self.pieces[by_i][PieceType::Bishop.index()]
            | self.pieces[by_i][PieceType::Queen.index()];
        let rooks_queens = self.pieces[by_i][PieceType::Rook.index()]
            | self.pieces[by_i][PieceType::Queen.index()];
        attackers |= tables::bishop_attacks(sq.0, occ) & bishops_queens;
        attackers |= tables::rook_attacks(sq.0, occ) & rooks_queens;

        attackers
    }

    #[inline]
    pub fn is_attacked(&self, sq: Square, by: Color) -> bool {
        self.attackers_to(sq, by, self.occupied()) != 0
    }

    /// The set of enemy pieces currently attacking our king. Cached and
    /// maintained incrementally by `make_move` / `unmake_move` — O(1) read.
    #[inline(always)]
    pub fn checkers(&self) -> Bitboard {
        self.checkers
    }

    /// Is the side to move in check? O(1) cache read.
    #[inline(always)]
    pub fn in_check(&self) -> bool {
        self.checkers != 0
    }

    // -- make / unmake -------------------------------------------------------

    pub fn make_move(&mut self, m: Move) {
        let us = self.side_to_move;
        let them = us.opponent();
        let from = m.from();
        let to = m.to();
        let piece = self.mailbox[from.index()];
        debug_assert!(
            piece.is_some() && piece.color() == us,
            "moving from empty or wrong-color square"
        );
        let pt = piece.piece_type();

        let prev_ep = self.ep_square;
        let prev_castling = self.castling;
        let prev_halfmove = self.halfmove;
        let prev_zobrist = self.zobrist;
        let prev_checkers = self.checkers;

        // Record current hash for threefold-repetition lookup.
        self.history_hashes.push(prev_zobrist);

        // Strip the old EP key before deciding the new one.
        if let Some(ep) = prev_ep {
            self.zobrist ^= zobrist::ep_file(ep.file());
        }

        let mut captured = Piece::NONE;

        match m.kind() {
            MoveKind::Normal => {
                if self.mailbox[to.index()].is_some() {
                    let cap = self.remove_piece(to);
                    self.zobrist ^= zobrist::piece_square(cap.color(), cap.piece_type(), to);
                    captured = cap;
                }
                self.move_piece(from, to);
                self.zobrist ^= zobrist::piece_square(us, pt, from);
                self.zobrist ^= zobrist::piece_square(us, pt, to);
            }
            MoveKind::Promotion => {
                if self.mailbox[to.index()].is_some() {
                    let cap = self.remove_piece(to);
                    self.zobrist ^= zobrist::piece_square(cap.color(), cap.piece_type(), to);
                    captured = cap;
                }
                self.remove_piece(from);
                let promoted_pt = m.promotion_piece();
                self.place_piece(to, Piece::new(us, promoted_pt));
                self.zobrist ^= zobrist::piece_square(us, PieceType::Pawn, from);
                self.zobrist ^= zobrist::piece_square(us, promoted_pt, to);
            }
            MoveKind::EnPassant => {
                let cap_sq = Square::from_file_rank(to.file(), from.rank());
                let cap = self.remove_piece(cap_sq);
                self.zobrist ^= zobrist::piece_square(cap.color(), cap.piece_type(), cap_sq);
                captured = cap;
                self.move_piece(from, to);
                self.zobrist ^= zobrist::piece_square(us, PieceType::Pawn, from);
                self.zobrist ^= zobrist::piece_square(us, PieceType::Pawn, to);
            }
            MoveKind::Castle => {
                self.move_piece(from, to);
                self.zobrist ^= zobrist::piece_square(us, PieceType::King, from);
                self.zobrist ^= zobrist::piece_square(us, PieceType::King, to);
                let (rook_from, rook_to) = match to.0 {
                    6 => (Square::H1, Square::F1),
                    2 => (Square::A1, Square::D1),
                    62 => (Square::H8, Square::F8),
                    58 => (Square::A8, Square::D8),
                    _ => unreachable!("invalid castle destination"),
                };
                self.move_piece(rook_from, rook_to);
                self.zobrist ^= zobrist::piece_square(us, PieceType::Rook, rook_from);
                self.zobrist ^= zobrist::piece_square(us, PieceType::Rook, rook_to);
            }
        }

        // New EP square: only set on a double pawn push AND only when the
        // capture is fully legal for the opponent (X-FEN 2020). Caching
        // legality here lets `movegen` skip its EP block when `ep_square`
        // is None — see `ep_capture_is_legal_for`.
        let mut new_ep = None;
        if pt == PieceType::Pawn {
            let diff = to.0 as i32 - from.0 as i32;
            if diff == 16 || diff == -16 {
                let ep_sq = Square((from.0 as i32 + diff / 2) as u8);
                if self.ep_capture_is_legal_for(ep_sq, them) {
                    new_ep = Some(ep_sq);
                }
            }
        }
        self.ep_square = new_ep;
        if let Some(ep) = new_ep {
            self.zobrist ^= zobrist::ep_file(ep.file());
        }

        // Castling rights: clear entries touched by this move; update hash.
        let new_castling = update_castling(self.castling, from, to);
        if new_castling != self.castling {
            self.zobrist ^= zobrist::castling(self.castling.0);
            self.zobrist ^= zobrist::castling(new_castling.0);
            self.castling = new_castling;
        }

        // `halfmove` uses `saturating_add`: wrapping `u16::MAX → 0` on a
        // pathological FEN would silently defeat the 50-move rule. Undo
        // restores from `prev_halfmove`, so saturation is symmetric.
        // `fullmove` stays wrapping so `unmake_move` can mirror with
        // `wrapping_sub` (no `prev_fullmove` is stored).
        if pt == PieceType::Pawn || captured.is_some() {
            self.halfmove = 0;
        } else {
            self.halfmove = prev_halfmove.saturating_add(1);
        }
        if us == Color::Black {
            self.fullmove = self.fullmove.wrapping_add(1);
        }

        self.side_to_move = them;
        self.zobrist ^= zobrist::side_to_move();

        // Refresh the `checkers` cache so `in_check()` stays O(1).
        let new_king_sq = self.king_sq(them);
        let new_occ = self.color_bb[0] | self.color_bb[1];
        self.checkers = self.attackers_to(new_king_sq, us, new_occ);

        self.history.push(Undo {
            captured,
            prev_castling,
            prev_ep,
            prev_halfmove,
            prev_zobrist,
            prev_checkers,
        });
    }

    /// Perft-only make. Identical to [`Self::make_move`] for every field
    /// that affects legality (board, side-to-move, castling rights, EP),
    /// but **skips**:
    /// - Zobrist incremental XORs (~6-7 u64 ops per ply)
    /// - `history_hashes` push (one `Vec::push` per ply)
    /// - Halfmove / fullmove clocks (unused in leaf-count search)
    ///
    /// Safe to call only in contexts where draw / repetition checks won't
    /// be consulted. `perft()` is the canonical caller.
    #[inline]
    pub fn make_move_perft(&mut self, m: Move) {
        let us = self.side_to_move;
        let them = us.opponent();
        let from = m.from();
        let to = m.to();
        let piece = self.mailbox[from.index()];
        debug_assert!(piece.is_some() && piece.color() == us);
        let pt = piece.piece_type();

        let prev_ep = self.ep_square;
        let prev_castling = self.castling;

        let mut captured = Piece::NONE;

        match m.kind() {
            MoveKind::Normal => {
                if self.mailbox[to.index()].is_some() {
                    captured = self.remove_piece(to);
                }
                self.move_piece(from, to);
            }
            MoveKind::Promotion => {
                if self.mailbox[to.index()].is_some() {
                    captured = self.remove_piece(to);
                }
                self.remove_piece(from);
                self.place_piece(to, Piece::new(us, m.promotion_piece()));
            }
            MoveKind::EnPassant => {
                let cap_sq = Square::from_file_rank(to.file(), from.rank());
                captured = self.remove_piece(cap_sq);
                self.move_piece(from, to);
            }
            MoveKind::Castle => {
                self.move_piece(from, to);
                let (rook_from, rook_to) = match to.0 {
                    6 => (Square::H1, Square::F1),
                    2 => (Square::A1, Square::D1),
                    62 => (Square::H8, Square::F8),
                    58 => (Square::A8, Square::D8),
                    _ => unreachable!("invalid castle destination"),
                };
                self.move_piece(rook_from, rook_to);
            }
        }

        // Same full-legality rule as `make_move`, without the hash update.
        let mut new_ep = None;
        if pt == PieceType::Pawn {
            let diff = to.0 as i32 - from.0 as i32;
            if diff == 16 || diff == -16 {
                let ep_sq = Square((from.0 as i32 + diff / 2) as u8);
                if self.ep_capture_is_legal_for(ep_sq, them) {
                    new_ep = Some(ep_sq);
                }
            }
        }
        self.ep_square = new_ep;

        let new_castling = update_castling(self.castling, from, to);
        if new_castling != self.castling {
            self.castling = new_castling;
        }

        self.side_to_move = them;

        // Zero the fields unused by `unmake_move_perft` — if the slow
        // `unmake_move` is accidentally called after a perft make, the
        // mismatch shows up in invariants instead of corrupting state.
        self.history.push(Undo {
            captured,
            prev_castling,
            prev_ep,
            prev_halfmove: 0,
            prev_zobrist: 0,
            prev_checkers: 0,
        });
    }

    /// Perft-only undo. Mirrors [`Self::make_move_perft`].
    #[inline]
    pub fn unmake_move_perft(&mut self, m: Move) {
        let undo = self.history.pop().expect("unmake_perft with empty history");
        let them = self.side_to_move;
        let us = them.opponent();
        let from = m.from();
        let to = m.to();

        self.side_to_move = us;
        self.castling = undo.prev_castling;
        self.ep_square = undo.prev_ep;

        match m.kind() {
            MoveKind::Normal => {
                self.move_piece(to, from);
                if undo.captured.is_some() {
                    self.place_piece(to, undo.captured);
                }
            }
            MoveKind::Promotion => {
                self.remove_piece(to);
                self.place_piece(from, Piece::new(us, PieceType::Pawn));
                if undo.captured.is_some() {
                    self.place_piece(to, undo.captured);
                }
            }
            MoveKind::EnPassant => {
                self.move_piece(to, from);
                let cap_sq = Square::from_file_rank(to.file(), from.rank());
                self.place_piece(cap_sq, undo.captured);
            }
            MoveKind::Castle => {
                self.move_piece(to, from);
                let (rook_from, rook_to) = match to.0 {
                    6 => (Square::H1, Square::F1),
                    2 => (Square::A1, Square::D1),
                    62 => (Square::H8, Square::F8),
                    58 => (Square::A8, Square::D8),
                    _ => unreachable!("invalid castle destination"),
                };
                self.move_piece(rook_to, rook_from);
            }
        }
    }

    pub fn unmake_move(&mut self, m: Move) {
        let undo = self.history.pop().expect("unmake with empty history");
        let _ = self
            .history_hashes
            .pop()
            .expect("history_hashes out of sync");
        let them = self.side_to_move;
        let us = them.opponent();
        let from = m.from();
        let to = m.to();

        self.side_to_move = us;
        self.castling = undo.prev_castling;
        self.ep_square = undo.prev_ep;
        self.halfmove = undo.prev_halfmove;
        self.zobrist = undo.prev_zobrist; // flat restore; no XOR reversal
        self.checkers = undo.prev_checkers;
        if us == Color::Black {
            self.fullmove = self.fullmove.wrapping_sub(1);
        }

        match m.kind() {
            MoveKind::Normal => {
                self.move_piece(to, from);
                if undo.captured.is_some() {
                    self.place_piece(to, undo.captured);
                }
            }
            MoveKind::Promotion => {
                self.remove_piece(to);
                self.place_piece(from, Piece::new(us, PieceType::Pawn));
                if undo.captured.is_some() {
                    self.place_piece(to, undo.captured);
                }
            }
            MoveKind::EnPassant => {
                self.move_piece(to, from);
                let cap_sq = Square::from_file_rank(to.file(), from.rank());
                self.place_piece(cap_sq, undo.captured);
            }
            MoveKind::Castle => {
                self.move_piece(to, from);
                let (rook_from, rook_to) = match to.0 {
                    6 => (Square::H1, Square::F1),
                    2 => (Square::A1, Square::D1),
                    62 => (Square::H8, Square::F8),
                    58 => (Square::A8, Square::D8),
                    _ => unreachable!("invalid castle destination"),
                };
                self.move_piece(rook_to, rook_from);
            }
        }
    }

    // -- draw / mate / termination detection ---------------------------------

    /// The side to move has no legal moves. Combined with `in_check` this
    /// distinguishes checkmate from stalemate.
    pub fn has_no_legal_moves(&self) -> bool {
        let mut ml = MoveList::new();
        generate_legal_moves(self, &mut ml);
        ml.is_empty()
    }

    pub fn is_checkmate(&self) -> bool {
        self.in_check() && self.has_no_legal_moves()
    }

    pub fn is_stalemate(&self) -> bool {
        !self.in_check() && self.has_no_legal_moves()
    }

    /// Strict 50-move rule: halfmove clock ≥ 100 plies.
    #[inline]
    pub fn is_fifty_move_rule(&self) -> bool {
        self.halfmove >= 100
    }

    /// Material considered insufficient to force mate (FIDE Art. 5.2.2 /
    /// Art. 9.3). Matches chess.js's classification:
    /// - K vs K
    /// - K + single minor vs K
    /// - Any number of bishops, from either side, all confined to one
    ///   colour complex (no knights, no pawns / rooks / queens)
    ///
    /// The third case generalises "K+B vs K+B same colour" to cover
    /// positions like K+B+B vs K+B with all bishops on light squares —
    /// these arise from captures in real games (the 100k differential
    /// fuzz vs chess.js caught one at ply 73 of seed 3 before this
    /// generalisation was in place).
    pub fn is_insufficient_material(&self) -> bool {
        // Pawns or majors: always sufficient (pawns promote; a rook or
        // queen mates alongside a bare king).
        let pawns = self.piece_bb(Color::White, PieceType::Pawn)
            | self.piece_bb(Color::Black, PieceType::Pawn);
        if pawns != 0 {
            return false;
        }
        let rooks_queens = self.piece_bb(Color::White, PieceType::Rook)
            | self.piece_bb(Color::White, PieceType::Queen)
            | self.piece_bb(Color::Black, PieceType::Rook)
            | self.piece_bb(Color::Black, PieceType::Queen);
        if rooks_queens != 0 {
            return false;
        }

        let w_knights = self.piece_bb(Color::White, PieceType::Knight);
        let b_knights = self.piece_bb(Color::Black, PieceType::Knight);
        let w_bishops = self.piece_bb(Color::White, PieceType::Bishop);
        let b_bishops = self.piece_bb(Color::Black, PieceType::Bishop);
        let total_minors = (w_knights | b_knights | w_bishops | b_bishops).count_ones();

        // K vs K and K + one minor vs K are always insufficient.
        if total_minors <= 1 {
            return true;
        }

        // With more than one minor, insufficient only if:
        //   1. No knights — knights + anything else can checkmate
        //      (`chess.js` treats even K+NN vs K as sufficient, matching
        //      us). Note: a lone knight + a bishop of any colour is
        //      already ruled out by `total_minors <= 1` above.
        //   2. All bishops occupy the same colour complex — the losing
        //      side's king can always flee to the opposite colour.
        if (w_knights | b_knights) != 0 {
            return false;
        }
        const LIGHT_SQUARES: Bitboard = 0x55AA_55AA_55AA_55AA;
        let bishops = w_bishops | b_bishops;
        let on_light = bishops & LIGHT_SQUARES;
        // All on light (== full set) or all on dark (== empty).
        on_light == bishops || on_light == 0
    }

    /// Threefold repetition: the current position (hash) has occurred ≥ 3
    /// times in the game, scanning back to the last irreversible move.
    pub fn is_threefold_repetition(&self) -> bool {
        let lookback = self.halfmove as usize;
        let start = self.history_hashes.len().saturating_sub(lookback);
        let mut count = 1u32; // current position counts once
        for h in &self.history_hashes[start..] {
            if *h == self.zobrist {
                count += 1;
            }
        }
        count >= 3
    }

    pub fn is_draw(&self) -> bool {
        self.is_fifty_move_rule()
            || self.is_insufficient_material()
            || self.is_threefold_repetition()
            || self.is_stalemate()
    }

    pub fn is_game_over(&self) -> bool {
        self.is_checkmate() || self.is_draw()
    }

    // -- diagnostics ---------------------------------------------------------

    /// Verifies structural invariants: color_bb == OR(piece_bb), mailbox
    /// agrees with bitboards, exactly one king per side. Available in release
    /// so integration / property tests can run it.
    pub fn assert_invariants(&self) {
        for c in [Color::White, Color::Black] {
            let mut combined = 0u64;
            for pt in [
                PieceType::Pawn,
                PieceType::Knight,
                PieceType::Bishop,
                PieceType::Rook,
                PieceType::Queen,
                PieceType::King,
            ] {
                combined |= self.piece_bb(c, pt);
            }
            assert_eq!(self.color_bb(c), combined, "color_bb mismatch for {c:?}");
            assert_eq!(
                self.piece_bb(c, PieceType::King).count_ones(),
                1,
                "wrong number of kings for {c:?}"
            );
        }
        for sq in 0..64u8 {
            let p = self.mailbox[sq as usize];
            let bit = 1u64 << sq;
            let occupied = self.occupied() & bit != 0;
            assert_eq!(p.is_some(), occupied, "mailbox/bitboard mismatch at {sq}");
            if p.is_some() {
                assert!(
                    self.pieces[p.color().index()][p.piece_type().index()] & bit != 0,
                    "mailbox says {p:?} at {sq} but piece bitboard disagrees"
                );
            }
        }
    }

    pub fn ascii(&self) -> String {
        let mut s = String::with_capacity(128);
        for r in (0..8u8).rev() {
            for f in 0..8u8 {
                let sq = Square::from_file_rank(f, r);
                s.push(self.piece_at(sq).char());
            }
            s.push('\n');
        }
        s
    }
}

/// Bit-flag table keyed by square, XORed off the castling state whenever a
/// piece leaves or arrives on a castling-relevant square (king or corner
/// rook). The value is 0 for squares that don't affect castling rights.
const fn castling_clear_table() -> [u8; 64] {
    let mut t = [0u8; 64];
    t[Square::A1.0 as usize] = CastlingRights::WHITE_QUEEN;
    t[Square::H1.0 as usize] = CastlingRights::WHITE_KING;
    t[Square::E1.0 as usize] = CastlingRights::WHITE_KING | CastlingRights::WHITE_QUEEN;
    t[Square::A8.0 as usize] = CastlingRights::BLACK_QUEEN;
    t[Square::H8.0 as usize] = CastlingRights::BLACK_KING;
    t[Square::E8.0 as usize] = CastlingRights::BLACK_KING | CastlingRights::BLACK_QUEEN;
    t
}
const CASTLING_CLEAR: [u8; 64] = castling_clear_table();

#[inline(always)]
fn update_castling(c: CastlingRights, from: Square, to: Square) -> CastlingRights {
    let cleared = CASTLING_CLEAR[from.index()] | CASTLING_CLEAR[to.index()];
    CastlingRights(c.0 & !cleared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startpos_invariants() {
        let p = Position::startpos();
        p.assert_invariants();
        assert_eq!(p.side_to_move, Color::White);
        assert_eq!(p.castling, CastlingRights::ALL);
        assert_eq!(p.ep_square, None);
        assert_eq!(p.halfmove, 0);
        assert_eq!(p.fullmove, 1);
    }

    #[test]
    fn empty_position() {
        let p = Position::empty();
        assert_eq!(p.occupied(), 0);
        for sq in 0..64u8 {
            assert!(p.piece_at(Square(sq)).is_none());
        }
    }

    #[test]
    fn quiet_move_make_unmake_roundtrip() {
        let mut p = Position::startpos();
        let before = p.clone();
        let m = Move::quiet(Square::E2, Square::E4);
        p.make_move(m);
        p.assert_invariants();
        assert_eq!(p.side_to_move, Color::Black);
        // X-FEN 2020: ep_square is only set when a pawn can actually capture.
        // At startpos+1.e4 there are no black pawns on d4 / f4, so ep = None.
        assert_eq!(p.ep_square, None);
        assert!(p.piece_at(Square::E2).is_none());
        assert_eq!(
            p.piece_at(Square::E4),
            Piece::new(Color::White, PieceType::Pawn)
        );
        p.unmake_move(m);
        p.assert_invariants();
        // Direct field equality: bitboards + mailbox + state must all match.
        assert_eq!(p.ascii(), before.ascii());
        assert_eq!(p.side_to_move, before.side_to_move);
        assert_eq!(p.ep_square, before.ep_square);
        assert_eq!(p.castling, before.castling);
    }

    #[test]
    fn capture_make_unmake() {
        // Position where white knight on f3 captures black pawn on e5.
        let fen = "rnbqkbnr/pppp1ppp/8/4p3/8/5N2/PPPPPPPP/RNBQKB1R w KQkq - 0 2";
        let mut p = crate::fen::parse_fen(fen).unwrap();
        let before = p.clone();
        let m = Move::quiet(Square::from_file_rank(5, 2), Square::E5); // f3 → e5
        p.make_move(m);
        p.assert_invariants();
        assert_eq!(
            p.piece_at(Square::E5),
            Piece::new(Color::White, PieceType::Knight)
        );
        p.unmake_move(m);
        p.assert_invariants();
        assert_eq!(p.ascii(), before.ascii());
    }

    #[test]
    fn castling_updates_rights() {
        // After e1→g1 (kingside castle), White loses both castling rights.
        let fen = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
        let mut p = crate::fen::parse_fen(fen).unwrap();
        let m = Move::castle(Square::E1, Square::G1);
        p.make_move(m);
        assert_eq!(
            p.piece_at(Square::G1),
            Piece::new(Color::White, PieceType::King)
        );
        assert_eq!(
            p.piece_at(Square::F1),
            Piece::new(Color::White, PieceType::Rook)
        );
        assert!(!p.castling.white_kingside());
        assert!(!p.castling.white_queenside());
        // Black still has rights.
        assert!(p.castling.black_kingside());
        assert!(p.castling.black_queenside());
        p.unmake_move(m);
        p.assert_invariants();
        assert!(p.castling.white_kingside());
    }

    #[test]
    fn en_passant_make_unmake() {
        let fen = "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3";
        let mut p = crate::fen::parse_fen(fen).unwrap();
        let before = p.clone();
        let m = Move::en_passant(Square::E5, Square::from_file_rank(3, 5)); // e5 → d6
        p.make_move(m);
        p.assert_invariants();
        assert_eq!(
            p.piece_at(Square::from_file_rank(3, 5)),
            Piece::new(Color::White, PieceType::Pawn)
        );
        // Captured pawn on d5 gone.
        assert!(p.piece_at(Square::D5).is_none());
        p.unmake_move(m);
        p.assert_invariants();
        assert_eq!(p.ascii(), before.ascii());
    }

    #[test]
    fn promotion_make_unmake() {
        let fen = "8/P7/8/8/8/8/8/k6K w - - 0 1";
        let mut p = crate::fen::parse_fen(fen).unwrap();
        let before = p.clone();
        let m = Move::promotion(Square::A7, Square::A8, PieceType::Queen);
        p.make_move(m);
        p.assert_invariants();
        assert_eq!(
            p.piece_at(Square::A8),
            Piece::new(Color::White, PieceType::Queen)
        );
        p.unmake_move(m);
        assert_eq!(p.ascii(), before.ascii());
    }

    #[test]
    fn castling_rights_cleared_on_rook_capture() {
        // Black rook on h1 captures white pawn there? Simpler: white rook captured on a8.
        // After Bxa8, black loses queen-side right.
        let fen = "r3k2r/8/8/8/8/8/8/B3K2R w Kkq - 0 1";
        let mut p = crate::fen::parse_fen(fen).unwrap();
        // White bishop a1 → a8 captures rook.
        let m = Move::quiet(Square::A1, Square::A8);
        p.make_move(m);
        assert!(!p.castling.black_queenside());
        assert!(p.castling.black_kingside());
    }
}
