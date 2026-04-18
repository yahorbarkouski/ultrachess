//! FEN (Forsyth–Edwards Notation) parsing and writing.
//!
//! Format: `<placement> <side> <castling> <ep> <halfmove> <fullmove>`
//! Placement: ranks 8..1, `/`-separated. Uppercase = white, lowercase = black,
//! digits 1..8 = runs of empty squares.

use crate::position::Position;
use crate::types::{CastlingRights, Color, Piece, PieceType, Square};
use core::fmt;

pub const STARTING_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FenError {
    MissingField(&'static str),
    BadPlacement(String),
    BadSide,
    BadCastling(String),
    BadEnPassant(String),
    BadNumber(&'static str),
    PieceCountMismatch,
    MissingKing(Color),
    TooManyKings(Color),
    PawnOnBackRank,
    EnPassantWithoutPawn,
}

impl fmt::Display for FenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FenError::MissingField(n) => write!(f, "missing FEN field: {n}"),
            FenError::BadPlacement(s) => write!(f, "invalid placement: {s}"),
            FenError::BadSide => write!(f, "side to move must be 'w' or 'b'"),
            FenError::BadCastling(s) => write!(f, "invalid castling field: {s}"),
            FenError::BadEnPassant(s) => write!(f, "invalid en-passant field: {s}"),
            FenError::BadNumber(n) => write!(f, "invalid number in field: {n}"),
            FenError::PieceCountMismatch => write!(f, "placement does not fill 64 squares"),
            FenError::MissingKing(c) => write!(f, "missing king for {c:?}"),
            FenError::TooManyKings(c) => write!(f, "too many kings for {c:?}"),
            FenError::PawnOnBackRank => write!(f, "pawn on first or eighth rank"),
            FenError::EnPassantWithoutPawn => write!(f, "en-passant square has no adjacent pawn"),
        }
    }
}

pub fn parse_fen(s: &str) -> Result<Position, FenError> {
    let mut it = s.split_ascii_whitespace();

    let placement = it.next().ok_or(FenError::MissingField("placement"))?;
    let side = it.next().ok_or(FenError::MissingField("side"))?;
    let castling = it.next().ok_or(FenError::MissingField("castling"))?;
    let ep = it.next().ok_or(FenError::MissingField("en-passant"))?;
    // Halfmove / fullmove fields are optional — if absent, default to 0 / 1.
    let halfmove_s = it.next();
    let fullmove_s = it.next();

    let mut pos = Position::empty();

    // Placement: ranks listed 8 down to 1.
    let mut rank: i32 = 7;
    let mut file: i32 = 0;
    for ch in placement.chars() {
        if ch == '/' {
            if file != 8 {
                return Err(FenError::BadPlacement(format!(
                    "rank {rank} has {file} squares"
                )));
            }
            rank -= 1;
            file = 0;
            continue;
        }
        if let Some(d) = ch.to_digit(10) {
            if d == 0 || d > 8 {
                return Err(FenError::BadPlacement(format!("invalid digit {ch}")));
            }
            file += d as i32;
            if file > 8 {
                return Err(FenError::BadPlacement(format!(
                    "rank {rank} overflowed past h"
                )));
            }
            continue;
        }
        if !(0..8).contains(&rank) || !(0..8).contains(&file) {
            return Err(FenError::BadPlacement(format!(
                "position {file},{rank} out of range"
            )));
        }
        let piece = piece_from_char(ch)
            .ok_or_else(|| FenError::BadPlacement(format!("unknown piece char '{ch}'")))?;
        pos.place_piece(Square::from_file_rank(file as u8, rank as u8), piece);
        file += 1;
    }
    if rank != 0 || file != 8 {
        return Err(FenError::PieceCountMismatch);
    }

    pos.side_to_move = match side {
        "w" => Color::White,
        "b" => Color::Black,
        _ => return Err(FenError::BadSide),
    };

    let mut cr = CastlingRights::NONE;
    if castling != "-" {
        for ch in castling.chars() {
            match ch {
                'K' => cr.add(CastlingRights::WHITE_KING),
                'Q' => cr.add(CastlingRights::WHITE_QUEEN),
                'k' => cr.add(CastlingRights::BLACK_KING),
                'q' => cr.add(CastlingRights::BLACK_QUEEN),
                _ => return Err(FenError::BadCastling(castling.to_string())),
            }
        }
    }
    pos.castling = cr;

    if ep != "-" {
        let sq = Square::parse_ascii(ep.as_bytes())
            .ok_or_else(|| FenError::BadEnPassant(ep.to_string()))?;
        pos.ep_square = Some(sq);
    }

    pos.halfmove = match halfmove_s {
        Some(s) => s
            .parse::<u16>()
            .map_err(|_| FenError::BadNumber("halfmove"))?,
        None => 0,
    };
    pos.fullmove = match fullmove_s {
        Some(s) => {
            let n: u16 = s.parse().map_err(|_| FenError::BadNumber("fullmove"))?;
            if n == 0 {
                1 // tolerate malformed but unambiguous "0"
            } else {
                n
            }
        }
        None => 1,
    };

    validate(&pos)?;
    // X-FEN 2020: downgrade `ep_square` to None unless the capture is
    // fully legal, so that `ep_square.is_some() ⇒ legal EP exists` holds
    // uniformly across `parse_fen` / `make_move` / `make_move_perft`.
    if let Some(ep) = pos.ep_square {
        if !pos.ep_capture_is_legal_for(ep, pos.side_to_move) {
            pos.ep_square = None;
        }
    }
    // One-shot seed; both are maintained incrementally from here.
    pos.recompute_zobrist();
    pos.recompute_checkers();
    Ok(pos)
}

fn validate(pos: &Position) -> Result<(), FenError> {
    for c in [Color::White, Color::Black] {
        let kings = pos.piece_bb(c, PieceType::King).count_ones();
        match kings {
            0 => return Err(FenError::MissingKing(c)),
            1 => {}
            _ => return Err(FenError::TooManyKings(c)),
        }
    }
    let pawn_mask =
        pos.piece_bb(Color::White, PieceType::Pawn) | pos.piece_bb(Color::Black, PieceType::Pawn);
    if pawn_mask & (crate::bitboard::RANK_1 | crate::bitboard::RANK_8) != 0 {
        return Err(FenError::PawnOnBackRank);
    }
    // EP square must be on the opponent's 3rd/6th rank, with the pushed
    // pawn sitting "behind" it.
    if let Some(ep) = pos.ep_square {
        let expected_rank = match pos.side_to_move {
            Color::White => 5,
            Color::Black => 2,
        };
        if ep.rank() != expected_rank {
            return Err(FenError::BadEnPassant(ep.to_string()));
        }
        let pawn_sq = match pos.side_to_move {
            Color::White => Square::from_file_rank(ep.file(), ep.rank() - 1),
            Color::Black => Square::from_file_rank(ep.file(), ep.rank() + 1),
        };
        let pawn = pos.piece_at(pawn_sq);
        if pawn.is_none()
            || pawn.piece_type() != PieceType::Pawn
            || pawn.color() == pos.side_to_move
        {
            return Err(FenError::EnPassantWithoutPawn);
        }
    }
    Ok(())
}

fn piece_from_char(ch: char) -> Option<Piece> {
    let color = if ch.is_ascii_uppercase() {
        Color::White
    } else {
        Color::Black
    };
    let pt = match ch.to_ascii_lowercase() {
        'p' => PieceType::Pawn,
        'n' => PieceType::Knight,
        'b' => PieceType::Bishop,
        'r' => PieceType::Rook,
        'q' => PieceType::Queen,
        'k' => PieceType::King,
        _ => return None,
    };
    Some(Piece::new(color, pt))
}

pub fn write_fen(pos: &Position) -> String {
    let mut s = String::with_capacity(90);
    for rank in (0..8u8).rev() {
        let mut empty: u32 = 0;
        for file in 0..8u8 {
            let sq = Square::from_file_rank(file, rank);
            let p = pos.piece_at(sq);
            if p.is_none() {
                empty += 1;
            } else {
                if empty > 0 {
                    s.push(char::from_digit(empty, 10).unwrap());
                    empty = 0;
                }
                s.push(p.char());
            }
        }
        if empty > 0 {
            s.push(char::from_digit(empty, 10).unwrap());
        }
        if rank != 0 {
            s.push('/');
        }
    }
    s.push(' ');
    s.push(match pos.side_to_move {
        Color::White => 'w',
        Color::Black => 'b',
    });
    s.push(' ');
    if pos.castling.0 == 0 {
        s.push('-');
    } else {
        if pos.castling.white_kingside() {
            s.push('K');
        }
        if pos.castling.white_queenside() {
            s.push('Q');
        }
        if pos.castling.black_kingside() {
            s.push('k');
        }
        if pos.castling.black_queenside() {
            s.push('q');
        }
    }
    s.push(' ');
    // `write!` into a `String` is infallible, but returns `Result` for the
    // trait's sake. Using the pushing API avoids allocating a format args
    // frame for the no-argument `{sq}` case.
    match pos.ep_square {
        Some(sq) => {
            use core::fmt::Write as _;
            // Infallible: writes into a `String`.
            s.write_fmt(format_args!("{sq}")).unwrap();
        }
        None => s.push('-'),
    }
    {
        use core::fmt::Write as _;
        // Infallible: writes into a `String`.
        s.write_fmt(format_args!(" {} {}", pos.halfmove, pos.fullmove))
            .unwrap();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_fen_roundtrip() {
        let p = parse_fen(STARTING_FEN).unwrap();
        p.assert_invariants();
        assert_eq!(write_fen(&p), STARTING_FEN);
    }

    #[test]
    fn kiwipete_roundtrip() {
        let fen = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
        let p = parse_fen(fen).unwrap();
        p.assert_invariants();
        assert_eq!(write_fen(&p), fen);
    }

    #[test]
    fn position_3_roundtrip() {
        let fen = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
        let p = parse_fen(fen).unwrap();
        assert_eq!(write_fen(&p), fen);
    }

    #[test]
    fn position_4_roundtrip() {
        let fen = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
        let p = parse_fen(fen).unwrap();
        assert_eq!(write_fen(&p), fen);
    }

    #[test]
    fn position_5_roundtrip() {
        let fen = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
        let p = parse_fen(fen).unwrap();
        assert_eq!(write_fen(&p), fen);
    }

    #[test]
    fn position_6_roundtrip() {
        let fen = "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";
        let p = parse_fen(fen).unwrap();
        assert_eq!(write_fen(&p), fen);
    }

    #[test]
    fn missing_king_rejected() {
        let fen = "8/8/8/8/8/8/8/8 w - - 0 1";
        let err = parse_fen(fen).unwrap_err();
        assert!(matches!(err, FenError::MissingKing(_)));
    }

    #[test]
    fn pawn_on_back_rank_rejected() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNP w KQkq - 0 1";
        assert!(parse_fen(fen).is_err());
    }

    #[test]
    fn tolerates_missing_clocks() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -";
        let p = parse_fen(fen).unwrap();
        assert_eq!(p.halfmove, 0);
        assert_eq!(p.fullmove, 1);
    }

    #[test]
    fn bad_ep_rejected() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq e4 0 1";
        // e4 is on rank 4; ep after white-moves should be rank 6.
        assert!(parse_fen(fen).is_err());
    }

    // -- exhaustive error-path coverage --------------------------------------

    #[test]
    fn every_missing_field_reports_its_name() {
        // "" → missing placement.
        let e = parse_fen("").unwrap_err();
        assert!(matches!(e, FenError::MissingField("placement")));
        // Only placement → missing side.
        let e = parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR").unwrap_err();
        assert!(matches!(e, FenError::MissingField("side")));
        // Missing castling.
        let e = parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w").unwrap_err();
        assert!(matches!(e, FenError::MissingField("castling")));
        // Missing en-passant.
        let e = parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq").unwrap_err();
        assert!(matches!(e, FenError::MissingField("en-passant")));
    }

    #[test]
    fn bad_placement_errors() {
        // Invalid piece character.
        assert!(matches!(
            parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNZ w KQkq - 0 1").unwrap_err(),
            FenError::BadPlacement(_)
        ));
        // Digit '0' invalid.
        assert!(matches!(
            parse_fen("rnbqkbnr/pppppppp/0/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap_err(),
            FenError::BadPlacement(_)
        ));
        // Digit '9' invalid (overflows rank).
        assert!(matches!(
            parse_fen("rnbqkbnr/pppppppp/9/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap_err(),
            FenError::BadPlacement(_)
        ));
        // Short rank.
        assert!(matches!(
            parse_fen("rnbqkbnr/ppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap_err(),
            FenError::BadPlacement(_) | FenError::PieceCountMismatch
        ));
    }

    #[test]
    fn bad_side_errors() {
        let e = parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1").unwrap_err();
        assert!(matches!(e, FenError::BadSide));
    }

    #[test]
    fn bad_castling_chars_rejected() {
        let e = parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KZkq - 0 1").unwrap_err();
        assert!(matches!(e, FenError::BadCastling(_)));
    }

    #[test]
    fn bad_ep_square_chars_rejected() {
        let e = parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq xx 0 1").unwrap_err();
        assert!(matches!(e, FenError::BadEnPassant(_)));
    }

    #[test]
    fn bad_halfmove_or_fullmove_rejected() {
        // Halfmove must parse as u16.
        let e =
            parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - nope 1").unwrap_err();
        assert!(matches!(e, FenError::BadNumber("halfmove")));
        let e =
            parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 nope").unwrap_err();
        assert!(matches!(e, FenError::BadNumber("fullmove")));
    }

    #[test]
    fn fullmove_zero_is_tolerated_as_one() {
        let p = parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 0").unwrap();
        assert_eq!(p.fullmove, 1);
    }

    #[test]
    fn too_many_kings_rejected() {
        let fen = "rkbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1";
        assert!(matches!(
            parse_fen(fen).unwrap_err(),
            FenError::TooManyKings(_)
        ));
    }

    #[test]
    fn ep_without_capturing_pawn_rejected() {
        // ep=d6 but no black pawn on d5 to have just double-pushed.
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq d6 0 1";
        assert!(matches!(
            parse_fen(fen).unwrap_err(),
            FenError::EnPassantWithoutPawn | FenError::BadEnPassant(_)
        ));
    }

    #[test]
    fn error_display_is_human_readable() {
        // Exercise the Display impl for every variant.
        let errs: Vec<FenError> = vec![
            FenError::MissingField("x"),
            FenError::BadPlacement("s".into()),
            FenError::BadSide,
            FenError::BadCastling("?".into()),
            FenError::BadEnPassant("?".into()),
            FenError::BadNumber("x"),
            FenError::PieceCountMismatch,
            FenError::MissingKing(Color::White),
            FenError::TooManyKings(Color::Black),
            FenError::PawnOnBackRank,
            FenError::EnPassantWithoutPawn,
        ];
        for e in errs {
            let s = format!("{e}");
            assert!(!s.is_empty(), "empty Display for {e:?}");
        }
    }

    #[test]
    fn write_fen_handles_no_castling_rights() {
        let p = parse_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert!(write_fen(&p).contains(" - - "));
    }

    #[test]
    fn write_fen_emits_ep_square_when_set() {
        // Capturable ep: white pawn on d5 can take on e6 after black's
        // hypothetical e7-e5. Under X-FEN 2020 this is the case where
        // `write_fen` emits the ep square.
        let p = parse_fen("rnbqkbnr/ppp1pppp/8/3Pp3/8/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2").unwrap();
        assert!(write_fen(&p).contains(" e6 "));
    }
}
