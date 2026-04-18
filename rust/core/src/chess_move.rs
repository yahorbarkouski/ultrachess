//! Packed 16-bit move encoding.
//!
//! Layout (LSB-first):
//! - bits 0..=5:   from-square (0..64)
//! - bits 6..=11:  to-square (0..64)
//! - bits 12..=13: promotion piece (00=N, 01=B, 10=R, 11=Q)
//! - bits 14..=15: move kind (00=normal, 01=promotion, 10=en-passant, 11=castling)
//!
//! "Capture" is *not* stored — it is recovered from the destination mailbox.

use crate::types::{PieceType, Square};
use core::fmt;

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MoveKind {
    Normal = 0,
    Promotion = 1,
    EnPassant = 2,
    Castle = 3,
}

impl MoveKind {
    #[inline(always)]
    const fn from_bits(b: u16) -> Self {
        match b & 0b11 {
            0 => MoveKind::Normal,
            1 => MoveKind::Promotion,
            2 => MoveKind::EnPassant,
            3 => MoveKind::Castle,
            _ => unreachable!(),
        }
    }
}

#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Move(pub u16);

impl Move {
    pub const NULL: Self = Self(0xFFFF);

    /// Sentinel 0xFFFF indicates "no move" (different from Move(0) which is a1→a1).
    #[inline(always)]
    pub const fn is_null(self) -> bool {
        self.0 == 0xFFFF
    }

    #[inline(always)]
    pub const fn new(from: Square, to: Square, kind: MoveKind, promo: PieceType) -> Self {
        let promo_bits = match promo {
            PieceType::Knight => 0u16,
            PieceType::Bishop => 1,
            PieceType::Rook => 2,
            PieceType::Queen => 3,
            _ => 0, // unused unless kind == Promotion
        };
        Self((from.0 as u16) | ((to.0 as u16) << 6) | (promo_bits << 12) | ((kind as u16) << 14))
    }

    #[inline(always)]
    pub const fn quiet(from: Square, to: Square) -> Self {
        Self::new(from, to, MoveKind::Normal, PieceType::Knight)
    }

    #[inline(always)]
    pub const fn promotion(from: Square, to: Square, promo: PieceType) -> Self {
        Self::new(from, to, MoveKind::Promotion, promo)
    }

    #[inline(always)]
    pub const fn en_passant(from: Square, to: Square) -> Self {
        Self::new(from, to, MoveKind::EnPassant, PieceType::Knight)
    }

    #[inline(always)]
    pub const fn castle(from: Square, to: Square) -> Self {
        Self::new(from, to, MoveKind::Castle, PieceType::Knight)
    }

    #[inline(always)]
    pub const fn from(self) -> Square {
        Square((self.0 & 0b111111) as u8)
    }

    #[inline(always)]
    pub const fn to(self) -> Square {
        Square(((self.0 >> 6) & 0b111111) as u8)
    }

    #[inline(always)]
    pub const fn kind(self) -> MoveKind {
        MoveKind::from_bits(self.0 >> 14)
    }

    #[inline(always)]
    pub const fn promotion_piece(self) -> PieceType {
        match (self.0 >> 12) & 0b11 {
            0 => PieceType::Knight,
            1 => PieceType::Bishop,
            2 => PieceType::Rook,
            3 => PieceType::Queen,
            _ => unreachable!(),
        }
    }

    /// UCI representation: "e2e4", "e7e8q", "e1g1" (castling uses king's to-sq).
    pub fn to_uci(self) -> [u8; 5] {
        let mut buf = [0u8; 5];
        let f = self.from();
        let t = self.to();
        buf[0] = b'a' + f.file();
        buf[1] = b'1' + f.rank();
        buf[2] = b'a' + t.file();
        buf[3] = b'1' + t.rank();
        if self.kind() == MoveKind::Promotion {
            buf[4] = self.promotion_piece().char() as u8;
        }
        buf
    }
}

impl fmt::Debug for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_null() {
            return f.write_str("Move(null)");
        }
        let bytes = self.to_uci();
        let end = if bytes[4] == 0 { 4 } else { 5 };
        // Safety: only ASCII written into the buffer.
        let s = core::str::from_utf8(&bytes[..end]).unwrap_or("?");
        write!(f, "{s}")
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_roundtrip() {
        let m = Move::quiet(Square::E2, Square::E4);
        assert_eq!(m.from(), Square::E2);
        assert_eq!(m.to(), Square::E4);
        assert_eq!(m.kind(), MoveKind::Normal);
    }

    #[test]
    fn promotion_roundtrip() {
        for promo in [
            PieceType::Knight,
            PieceType::Bishop,
            PieceType::Rook,
            PieceType::Queen,
        ] {
            let m = Move::promotion(Square::E7, Square::E8, promo);
            assert_eq!(m.kind(), MoveKind::Promotion);
            assert_eq!(m.promotion_piece(), promo);
            assert_eq!(m.from(), Square::E7);
            assert_eq!(m.to(), Square::E8);
        }
    }

    #[test]
    fn castle_and_ep_roundtrip() {
        let castle = Move::castle(Square::E1, Square::G1);
        assert_eq!(castle.kind(), MoveKind::Castle);
        let ep = Move::en_passant(Square::E5, Square::F6);
        assert_eq!(ep.kind(), MoveKind::EnPassant);
    }

    #[test]
    fn uci_formatting() {
        assert_eq!(&Move::quiet(Square::E2, Square::E4).to_uci()[..4], b"e2e4");
        assert_eq!(
            &Move::promotion(Square::E7, Square::E8, PieceType::Queen).to_uci()[..5],
            b"e7e8q"
        );
    }

    #[test]
    fn null_sentinel() {
        assert!(Move::NULL.is_null());
        assert!(!Move::quiet(Square::A1, Square::A2).is_null());
    }

    // -- coverage fills ------------------------------------------------------

    #[test]
    fn move_display_and_debug_match_uci() {
        let m = Move::quiet(Square::E2, Square::E4);
        assert_eq!(format!("{m}"), "e2e4");
        assert_eq!(format!("{m:?}"), "e2e4");

        let promo = Move::promotion(Square::E7, Square::E8, PieceType::Queen);
        assert_eq!(format!("{promo}"), "e7e8q");

        assert_eq!(format!("{:?}", Move::NULL), "Move(null)");
    }

    #[test]
    fn every_move_kind_decodes() {
        let kinds = [
            (MoveKind::Normal, Move::quiet(Square::E2, Square::E4)),
            (
                MoveKind::Promotion,
                Move::promotion(Square::E7, Square::E8, PieceType::Queen),
            ),
            (
                MoveKind::EnPassant,
                Move::en_passant(Square::E5, Square::F6),
            ),
            (MoveKind::Castle, Move::castle(Square::E1, Square::G1)),
        ];
        for (expected, m) in kinds {
            assert_eq!(m.kind(), expected);
        }
    }

    #[test]
    fn move_equality_and_hash() {
        use std::collections::HashSet;
        let a = Move::quiet(Square::E2, Square::E4);
        let b = Move::quiet(Square::E2, Square::E4);
        assert_eq!(a, b);

        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }
}
