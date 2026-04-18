//! Bitboard type alias + helpers. A `Bitboard` is a `u64` where bit `sq` is
//! set iff the corresponding square (LERF order) is "on".

use crate::types::Square;

pub type Bitboard = u64;

// File and rank masks. FILE_A is bits {0, 8, 16, ..., 56}.
pub const FILE_A: Bitboard = 0x0101_0101_0101_0101;
pub const FILE_B: Bitboard = FILE_A << 1;
pub const FILE_D: Bitboard = FILE_A << 3;
pub const FILE_G: Bitboard = FILE_A << 6;
pub const FILE_H: Bitboard = FILE_A << 7;

pub const RANK_1: Bitboard = 0x0000_0000_0000_00FF;
pub const RANK_3: Bitboard = RANK_1 << 16;
pub const RANK_4: Bitboard = RANK_1 << 24;
pub const RANK_6: Bitboard = RANK_1 << 40;
pub const RANK_8: Bitboard = RANK_1 << 56;

pub const NOT_FILE_A: Bitboard = !FILE_A;
pub const NOT_FILE_H: Bitboard = !FILE_H;
pub const NOT_FILE_AB: Bitboard = !(FILE_A | FILE_B);
pub const NOT_FILE_GH: Bitboard = !(FILE_G | FILE_H);

/// Number of set bits in a bitboard.
#[inline(always)]
pub const fn popcount(bb: Bitboard) -> u32 {
    bb.count_ones()
}

/// Index of the least-significant set bit. UB-adjacent if `bb == 0`.
#[inline(always)]
pub const fn lsb(bb: Bitboard) -> u8 {
    debug_assert!(bb != 0);
    bb.trailing_zeros() as u8
}

/// Pop and return the index of the least-significant set bit.
#[inline(always)]
pub fn pop_lsb(bb: &mut Bitboard) -> Square {
    let sq = lsb(*bb);
    *bb &= *bb - 1;
    Square(sq)
}

/// One-step shifts. Files are masked so wraparound cannot happen.
#[inline(always)]
pub const fn north(bb: Bitboard) -> Bitboard {
    bb << 8
}
#[inline(always)]
pub const fn south(bb: Bitboard) -> Bitboard {
    bb >> 8
}
#[inline(always)]
pub const fn east(bb: Bitboard) -> Bitboard {
    (bb & NOT_FILE_H) << 1
}
#[inline(always)]
pub const fn west(bb: Bitboard) -> Bitboard {
    (bb & NOT_FILE_A) >> 1
}
#[inline(always)]
pub const fn north_east(bb: Bitboard) -> Bitboard {
    (bb & NOT_FILE_H) << 9
}
#[inline(always)]
pub const fn north_west(bb: Bitboard) -> Bitboard {
    (bb & NOT_FILE_A) << 7
}
#[inline(always)]
pub const fn south_east(bb: Bitboard) -> Bitboard {
    (bb & NOT_FILE_H) >> 7
}
#[inline(always)]
pub const fn south_west(bb: Bitboard) -> Bitboard {
    (bb & NOT_FILE_A) >> 9
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_mask_shape() {
        assert_eq!(popcount(FILE_A), 8);
        assert_eq!(FILE_A & 1, 1);
        assert_eq!(FILE_A & (1 << 8), 1 << 8);
        assert_eq!(FILE_A & (1 << 16), 1 << 16);
    }

    #[test]
    fn rank_mask_shape() {
        assert_eq!(popcount(RANK_1), 8);
        assert_eq!(RANK_1, 0xFF);
        assert_eq!(RANK_8, 0xFF << 56);
    }

    #[test]
    fn shift_no_wraparound() {
        // A pawn on h-file cannot shift east onto a-file of the next rank.
        let h_file_piece = Square::new(7).bb(); // h1
        assert_eq!(east(h_file_piece), 0);
        assert_eq!(north_east(h_file_piece), 0);

        let a_file_piece = Square::new(0).bb(); // a1
        assert_eq!(west(a_file_piece), 0);
        assert_eq!(north_west(a_file_piece), 0);
    }

    #[test]
    fn shift_correctness() {
        let e4 = Square::from_file_rank(4, 3).bb();
        assert_eq!(north(e4), Square::from_file_rank(4, 4).bb()); // e5
        assert_eq!(south(e4), Square::from_file_rank(4, 2).bb()); // e3
        assert_eq!(east(e4), Square::from_file_rank(5, 3).bb()); // f4
        assert_eq!(west(e4), Square::from_file_rank(3, 3).bb()); // d4
    }

    #[test]
    fn pop_lsb_iteration() {
        let mut bb = 0b_1010u64;
        let s1 = pop_lsb(&mut bb);
        let s2 = pop_lsb(&mut bb);
        assert_eq!(s1.0, 1);
        assert_eq!(s2.0, 3);
        assert_eq!(bb, 0);
    }
}
