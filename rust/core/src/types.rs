//! Core types: colors, piece types, squares, castling rights, pieces.
//!
//! Square numbering is **LERF** (Little-Endian Rank-File): a1 = 0, h1 = 7,
//! a8 = 56, h8 = 63. `bit << sq` on a `u64` addresses the square directly.

use core::fmt;

/// Side to move.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    #[inline(always)]
    pub const fn opponent(self) -> Self {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    #[inline(always)]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[inline(always)]
    pub const fn from_index(i: usize) -> Self {
        match i {
            0 => Color::White,
            1 => Color::Black,
            _ => panic!("invalid color index"),
        }
    }
}

/// Piece type (no color).
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PieceType {
    Pawn = 0,
    Knight = 1,
    Bishop = 2,
    Rook = 3,
    Queen = 4,
    King = 5,
}

impl PieceType {
    #[inline(always)]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[inline(always)]
    pub const fn from_index(i: usize) -> Self {
        match i {
            0 => PieceType::Pawn,
            1 => PieceType::Knight,
            2 => PieceType::Bishop,
            3 => PieceType::Rook,
            4 => PieceType::Queen,
            5 => PieceType::King,
            _ => panic!("invalid piece type index"),
        }
    }

    pub const fn char(self) -> char {
        match self {
            PieceType::Pawn => 'p',
            PieceType::Knight => 'n',
            PieceType::Bishop => 'b',
            PieceType::Rook => 'r',
            PieceType::Queen => 'q',
            PieceType::King => 'k',
        }
    }
}

/// A colored piece, packed into a single byte (0..12 valid, 255 = empty).
///
/// Encoding: `(color << 3) | piece_type` so the piece type fits in the low
/// 3 bits; 255 is reserved for the empty-square sentinel in the mailbox.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Piece(pub u8);

impl Piece {
    pub const NONE: Self = Self(255);

    #[inline(always)]
    pub const fn new(color: Color, piece_type: PieceType) -> Self {
        Self(((color as u8) << 3) | piece_type as u8)
    }

    #[inline(always)]
    pub const fn is_none(self) -> bool {
        self.0 == 255
    }

    #[inline(always)]
    pub const fn is_some(self) -> bool {
        self.0 != 255
    }

    #[inline(always)]
    pub const fn color(self) -> Color {
        Color::from_index((self.0 >> 3) as usize)
    }

    #[inline(always)]
    pub const fn piece_type(self) -> PieceType {
        PieceType::from_index((self.0 & 0b111) as usize)
    }

    pub fn char(self) -> char {
        if self.is_none() {
            return '.';
        }
        let c = self.piece_type().char();
        match self.color() {
            Color::White => c.to_ascii_uppercase(),
            Color::Black => c,
        }
    }
}

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.char())
    }
}

/// A square on the board (0..64). 0 = a1, 7 = h1, 56 = a8, 63 = h8.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Square(pub u8);

impl Square {
    pub const A1: Self = Self(0);
    pub const B1: Self = Self(1);
    pub const C1: Self = Self(2);
    pub const D1: Self = Self(3);
    pub const E1: Self = Self(4);
    pub const F1: Self = Self(5);
    pub const G1: Self = Self(6);
    pub const H1: Self = Self(7);
    pub const A2: Self = Self(8);
    pub const D2: Self = Self(11);
    pub const E2: Self = Self(12);
    pub const E3: Self = Self(20);
    pub const D4: Self = Self(27);
    pub const E4: Self = Self(28);
    pub const D5: Self = Self(35);
    pub const E5: Self = Self(36);
    pub const F6: Self = Self(45);
    pub const A7: Self = Self(48);
    pub const E7: Self = Self(52);
    pub const A8: Self = Self(56);
    pub const B8: Self = Self(57);
    pub const C8: Self = Self(58);
    pub const D8: Self = Self(59);
    pub const E8: Self = Self(60);
    pub const F8: Self = Self(61);
    pub const G8: Self = Self(62);
    pub const H8: Self = Self(63);

    #[inline(always)]
    pub const fn new(index: u8) -> Self {
        debug_assert!(index < 64);
        Self(index)
    }

    #[inline(always)]
    pub const fn from_file_rank(file: u8, rank: u8) -> Self {
        debug_assert!(file < 8 && rank < 8);
        Self(rank * 8 + file)
    }

    #[inline(always)]
    pub const fn file(self) -> u8 {
        self.0 & 7
    }

    #[inline(always)]
    pub const fn rank(self) -> u8 {
        self.0 >> 3
    }

    #[inline(always)]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Single-bit mask for this square.
    #[inline(always)]
    pub const fn bb(self) -> u64 {
        1u64 << self.0
    }

    /// Parse two ASCII bytes like b"e4".
    pub fn parse_ascii(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 2 {
            return None;
        }
        let file = bytes[0];
        let rank = bytes[1];
        if !(b'a'..=b'h').contains(&file) || !(b'1'..=b'8').contains(&rank) {
            return None;
        }
        Some(Self::from_file_rank(file - b'a', rank - b'1'))
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}",
            (b'a' + self.file()) as char,
            (b'1' + self.rank()) as char
        )
    }
}

/// Castling rights encoded as a bitfield:
/// bit 0 = White king-side, bit 1 = White queen-side,
/// bit 2 = Black king-side, bit 3 = Black queen-side.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Hash)]
pub struct CastlingRights(pub u8);

impl CastlingRights {
    pub const NONE: Self = Self(0);
    pub const WHITE_KING: u8 = 0b0001;
    pub const WHITE_QUEEN: u8 = 0b0010;
    pub const BLACK_KING: u8 = 0b0100;
    pub const BLACK_QUEEN: u8 = 0b1000;
    pub const ALL: Self = Self(0b1111);

    #[inline(always)]
    pub const fn has(self, mask: u8) -> bool {
        (self.0 & mask) != 0
    }

    #[inline(always)]
    pub fn add(&mut self, mask: u8) {
        self.0 |= mask;
    }

    #[inline(always)]
    pub fn remove(&mut self, mask: u8) {
        self.0 &= !mask;
    }

    #[inline(always)]
    pub const fn white_kingside(self) -> bool {
        self.has(Self::WHITE_KING)
    }
    #[inline(always)]
    pub const fn white_queenside(self) -> bool {
        self.has(Self::WHITE_QUEEN)
    }
    #[inline(always)]
    pub const fn black_kingside(self) -> bool {
        self.has(Self::BLACK_KING)
    }
    #[inline(always)]
    pub const fn black_queenside(self) -> bool {
        self.has(Self::BLACK_QUEEN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_file_rank_roundtrip() {
        for f in 0..8u8 {
            for r in 0..8u8 {
                let sq = Square::from_file_rank(f, r);
                assert_eq!(sq.file(), f);
                assert_eq!(sq.rank(), r);
                assert_eq!(sq.index(), (r * 8 + f) as usize);
            }
        }
    }

    #[test]
    fn square_corners() {
        assert_eq!(Square::A1.to_string(), "a1");
        assert_eq!(Square::H1.to_string(), "h1");
        assert_eq!(Square::A8.to_string(), "a8");
        assert_eq!(Square::H8.to_string(), "h8");
    }

    #[test]
    fn square_parse() {
        assert_eq!(
            Square::parse_ascii(b"e4"),
            Some(Square::from_file_rank(4, 3))
        );
        assert_eq!(Square::parse_ascii(b"a1"), Some(Square::A1));
        assert_eq!(Square::parse_ascii(b"h8"), Some(Square::H8));
        assert_eq!(Square::parse_ascii(b""), None);
        assert_eq!(Square::parse_ascii(b"i9"), None);
    }

    #[test]
    fn piece_roundtrip() {
        for c in [Color::White, Color::Black] {
            for pt in [
                PieceType::Pawn,
                PieceType::Knight,
                PieceType::Bishop,
                PieceType::Rook,
                PieceType::Queen,
                PieceType::King,
            ] {
                let p = Piece::new(c, pt);
                assert!(p.is_some());
                assert_eq!(p.color(), c);
                assert_eq!(p.piece_type(), pt);
            }
        }
        assert!(Piece::NONE.is_none());
    }

    #[test]
    fn piece_chars() {
        assert_eq!(Piece::new(Color::White, PieceType::King).char(), 'K');
        assert_eq!(Piece::new(Color::Black, PieceType::Pawn).char(), 'p');
        assert_eq!(Piece::NONE.char(), '.');
    }

    #[test]
    fn color_roundtrip() {
        assert_eq!(Color::White.opponent(), Color::Black);
        assert_eq!(Color::Black.opponent(), Color::White);
        assert_eq!(Color::from_index(0), Color::White);
        assert_eq!(Color::from_index(1), Color::Black);
    }

    #[test]
    fn castling_rights_ops() {
        let mut cr = CastlingRights::ALL;
        assert!(cr.white_kingside());
        assert!(cr.black_queenside());
        cr.remove(CastlingRights::WHITE_KING);
        assert!(!cr.white_kingside());
        assert!(cr.white_queenside());
        cr.add(CastlingRights::WHITE_KING);
        assert!(cr.white_kingside());
    }
}
