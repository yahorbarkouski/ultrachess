//! Precomputed attack tables, `const`-initialised at compile time.
//!
//! - `PAWN_ATTACKS[color][sq]` — diagonal attacks (not pushes).
//! - `KNIGHT_ATTACKS[sq]`, `KING_ATTACKS[sq]`.
//! - `RAY_ATTACKS[dir][sq]` — empty-board ray in one of 8 directions.
//! - `BETWEEN[a][b]` — squares strictly between `a` and `b` on a shared
//!   rank/file/diagonal (else 0). Used to block a check.
//! - `LINE[a][b]` — full rank/file/diagonal containing both (else 0).
//!   Used for pin rays.
//!
//! Sliders route through `magic.rs` on the hot path; the classical-ray
//! helpers below are kept as the correctness oracle.

use crate::bitboard::{self, Bitboard};
use crate::types::Color;

// ---------------------------------------------------------------------------
// Directions
// ---------------------------------------------------------------------------

pub const N: usize = 0;
pub const E: usize = 1;
pub const S: usize = 2;
pub const W: usize = 3;
pub const NE: usize = 4;
pub const NW: usize = 5;
pub const SE: usize = 6;
pub const SW: usize = 7;

// Positive directions shift the index upward (use `trailing_zeros` for nearest
// blocker); negative directions shift downward (use `leading_zeros`). Index
// layout is LERF, so N=+8, NE=+9, NW=+7, S=-8, SE=-7, SW=-9.
const POSITIVE: [bool; 8] = [true, true, false, false, true, true, false, false];

// ---------------------------------------------------------------------------
// Leaper tables (pawn, knight, king)
// ---------------------------------------------------------------------------

const fn compute_pawn_attacks() -> [[Bitboard; 64]; 2] {
    let mut table = [[0u64; 64]; 2];
    let mut sq = 0;
    while sq < 64 {
        let bb: Bitboard = 1u64 << sq;
        table[0][sq] = bitboard::north_east(bb) | bitboard::north_west(bb);
        table[1][sq] = bitboard::south_east(bb) | bitboard::south_west(bb);
        sq += 1;
    }
    table
}

const fn compute_knight_attacks() -> [Bitboard; 64] {
    let mut table = [0u64; 64];
    let mut sq = 0;
    while sq < 64 {
        let bb: Bitboard = 1u64 << sq;
        // 8 jumps; each masked against the inside file(s) to prevent wrap.
        let nne = (bb & bitboard::NOT_FILE_H) << 17;
        let nnw = (bb & bitboard::NOT_FILE_A) << 15;
        let ssw = (bb & bitboard::NOT_FILE_A) >> 17;
        let sse = (bb & bitboard::NOT_FILE_H) >> 15;
        let een = (bb & bitboard::NOT_FILE_GH) << 10;
        let wwn = (bb & bitboard::NOT_FILE_AB) << 6;
        let ees = (bb & bitboard::NOT_FILE_GH) >> 6;
        let wws = (bb & bitboard::NOT_FILE_AB) >> 10;
        table[sq] = nne | nnw | ssw | sse | een | wwn | ees | wws;
        sq += 1;
    }
    table
}

const fn compute_king_attacks() -> [Bitboard; 64] {
    let mut table = [0u64; 64];
    let mut sq = 0;
    while sq < 64 {
        let bb: Bitboard = 1u64 << sq;
        table[sq] = bitboard::north(bb)
            | bitboard::south(bb)
            | bitboard::east(bb)
            | bitboard::west(bb)
            | bitboard::north_east(bb)
            | bitboard::north_west(bb)
            | bitboard::south_east(bb)
            | bitboard::south_west(bb);
        sq += 1;
    }
    table
}

pub const PAWN_ATTACKS: [[Bitboard; 64]; 2] = compute_pawn_attacks();
pub const KNIGHT_ATTACKS: [Bitboard; 64] = compute_knight_attacks();
pub const KING_ATTACKS: [Bitboard; 64] = compute_king_attacks();

// ---------------------------------------------------------------------------
// Ray attacks (empty-board rays in 8 directions)
// ---------------------------------------------------------------------------

const fn ray(from: i32, delta_file: i32, delta_rank: i32) -> Bitboard {
    let mut result: Bitboard = 0;
    let mut f = (from & 7) + delta_file;
    let mut r = (from >> 3) + delta_rank;
    while f >= 0 && f < 8 && r >= 0 && r < 8 {
        result |= 1u64 << (r * 8 + f);
        f += delta_file;
        r += delta_rank;
    }
    result
}

const fn compute_rays() -> [[Bitboard; 64]; 8] {
    let mut table = [[0u64; 64]; 8];
    let mut sq = 0;
    while sq < 64 {
        let s = sq as i32;
        table[N][sq] = ray(s, 0, 1);
        table[E][sq] = ray(s, 1, 0);
        table[S][sq] = ray(s, 0, -1);
        table[W][sq] = ray(s, -1, 0);
        table[NE][sq] = ray(s, 1, 1);
        table[NW][sq] = ray(s, -1, 1);
        table[SE][sq] = ray(s, 1, -1);
        table[SW][sq] = ray(s, -1, -1);
        sq += 1;
    }
    table
}

pub const RAY_ATTACKS: [[Bitboard; 64]; 8] = compute_rays();

// ---------------------------------------------------------------------------
// Sliding-piece attacks from classical rays
// ---------------------------------------------------------------------------

#[inline(always)]
fn ray_attacks(sq: u8, dir: usize, occ: Bitboard) -> Bitboard {
    let ray = RAY_ATTACKS[dir][sq as usize];
    let blockers = ray & occ;
    if blockers == 0 {
        return ray;
    }
    let blocker_sq = if POSITIVE[dir] {
        blockers.trailing_zeros() as u8
    } else {
        63 - blockers.leading_zeros() as u8
    };
    // XOR removes the section of the ray from `blocker_sq` onward, leaving
    // the attack ray including the blocker itself.
    ray ^ RAY_ATTACKS[dir][blocker_sq as usize]
}

/// Classical-ray bishop attacks. Kept as a correctness reference for the
/// magic-bitboard implementation; not used by the hot path.
#[inline(always)]
pub fn bishop_attacks_classical(sq: u8, occ: Bitboard) -> Bitboard {
    ray_attacks(sq, NE, occ)
        | ray_attacks(sq, NW, occ)
        | ray_attacks(sq, SE, occ)
        | ray_attacks(sq, SW, occ)
}

/// Classical-ray rook attacks. See `bishop_attacks_classical`.
#[inline(always)]
pub fn rook_attacks_classical(sq: u8, occ: Bitboard) -> Bitboard {
    ray_attacks(sq, N, occ)
        | ray_attacks(sq, E, occ)
        | ray_attacks(sq, S, occ)
        | ray_attacks(sq, W, occ)
}

/// Bishop attacks — routed through fancy magic bitboards.
#[inline(always)]
pub fn bishop_attacks(sq: u8, occ: Bitboard) -> Bitboard {
    crate::magic::bishop_attacks(sq, occ)
}

/// Rook attacks — routed through fancy magic bitboards.
#[inline(always)]
pub fn rook_attacks(sq: u8, occ: Bitboard) -> Bitboard {
    crate::magic::rook_attacks(sq, occ)
}

/// Queen attacks — bishop + rook at the same square.
#[inline(always)]
pub fn queen_attacks(sq: u8, occ: Bitboard) -> Bitboard {
    crate::magic::queen_attacks(sq, occ)
}

#[inline(always)]
pub fn pawn_attacks(color: Color, sq: u8) -> Bitboard {
    PAWN_ATTACKS[color.index()][sq as usize]
}

#[inline(always)]
pub fn knight_attacks(sq: u8) -> Bitboard {
    KNIGHT_ATTACKS[sq as usize]
}

#[inline(always)]
pub fn king_attacks(sq: u8) -> Bitboard {
    KING_ATTACKS[sq as usize]
}

// ---------------------------------------------------------------------------
// BETWEEN and LINE tables
// ---------------------------------------------------------------------------

const fn squares_between(a: i32, b: i32) -> Bitboard {
    // Strictly between, exclusive of both endpoints. 0 if a and b don't
    // share a rank/file/diagonal.
    if a == b {
        return 0;
    }
    let fa = a & 7;
    let ra = a >> 3;
    let fb = b & 7;
    let rb = b >> 3;
    let df = fb - fa;
    let dr = rb - ra;
    let step_file;
    let step_rank;
    if df == 0 && dr != 0 {
        step_file = 0;
        step_rank = if dr > 0 { 1 } else { -1 };
    } else if dr == 0 && df != 0 {
        step_file = if df > 0 { 1 } else { -1 };
        step_rank = 0;
    } else if df == dr || df == -dr {
        step_file = if df > 0 { 1 } else { -1 };
        step_rank = if dr > 0 { 1 } else { -1 };
    } else {
        return 0;
    }
    let mut result: Bitboard = 0;
    let mut f = fa + step_file;
    let mut r = ra + step_rank;
    while (f != fb || r != rb) && f >= 0 && f < 8 && r >= 0 && r < 8 {
        result |= 1u64 << (r * 8 + f);
        f += step_file;
        r += step_rank;
    }
    result
}

const fn line_through(a: i32, b: i32) -> Bitboard {
    if a == b {
        return 0;
    }
    let fa = a & 7;
    let ra = a >> 3;
    let fb = b & 7;
    let rb = b >> 3;
    let df = fb - fa;
    let dr = rb - ra;
    let step_file;
    let step_rank;
    if df == 0 && dr != 0 {
        step_file = 0;
        step_rank = 1;
    } else if dr == 0 && df != 0 {
        step_file = 1;
        step_rank = 0;
    } else if df == dr || df == -dr {
        step_file = if df == dr { 1 } else { -1 };
        step_rank = 1;
    } else {
        return 0;
    }
    // Back up to the board boundary, then sweep forward to fill the line.
    let mut f = fa;
    let mut r = ra;
    while f - step_file >= 0 && f - step_file < 8 && r - step_rank >= 0 && r - step_rank < 8 {
        f -= step_file;
        r -= step_rank;
    }
    let mut result: Bitboard = 0;
    while f >= 0 && f < 8 && r >= 0 && r < 8 {
        result |= 1u64 << (r * 8 + f);
        f += step_file;
        r += step_rank;
    }
    result
}

const fn compute_between() -> [[Bitboard; 64]; 64] {
    let mut table = [[0u64; 64]; 64];
    let mut a = 0;
    while a < 64 {
        let mut b = 0;
        while b < 64 {
            table[a][b] = squares_between(a as i32, b as i32);
            b += 1;
        }
        a += 1;
    }
    table
}

const fn compute_line() -> [[Bitboard; 64]; 64] {
    let mut table = [[0u64; 64]; 64];
    let mut a = 0;
    while a < 64 {
        let mut b = 0;
        while b < 64 {
            table[a][b] = line_through(a as i32, b as i32);
            b += 1;
        }
        a += 1;
    }
    table
}

pub static BETWEEN: [[Bitboard; 64]; 64] = compute_between();
pub static LINE: [[Bitboard; 64]; 64] = compute_line();

#[inline(always)]
pub fn between(a: u8, b: u8) -> Bitboard {
    BETWEEN[a as usize][b as usize]
}

#[inline(always)]
pub fn line(a: u8, b: u8) -> Bitboard {
    LINE[a as usize][b as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::popcount;
    use crate::types::Square;

    fn sq(file: u8, rank: u8) -> Square {
        Square::from_file_rank(file, rank)
    }

    fn bb_of(squares: &[Square]) -> Bitboard {
        squares.iter().fold(0, |a, s| a | s.bb())
    }

    #[test]
    fn knight_attack_counts() {
        // Corner: 2. On 2nd-rank-from-edge corner (b2, g2, b7, g7): 4. Center (d4-e5): 8.
        assert_eq!(popcount(knight_attacks(Square::A1.0)), 2);
        assert_eq!(popcount(knight_attacks(Square::H1.0)), 2);
        assert_eq!(popcount(knight_attacks(sq(3, 3).0)), 8); // d4
        assert_eq!(popcount(knight_attacks(sq(1, 1).0)), 4); // b2
    }

    #[test]
    fn knight_attack_shape_from_d4() {
        // Expected targets: b3, b5, c2, c6, e2, e6, f3, f5
        let expected = bb_of(&[
            sq(1, 2),
            sq(1, 4),
            sq(2, 1),
            sq(2, 5),
            sq(4, 1),
            sq(4, 5),
            sq(5, 2),
            sq(5, 4),
        ]);
        assert_eq!(knight_attacks(sq(3, 3).0), expected);
    }

    #[test]
    fn king_attack_counts() {
        assert_eq!(popcount(king_attacks(Square::A1.0)), 3); // corner
        assert_eq!(popcount(king_attacks(Square::E1.0)), 5); // edge
        assert_eq!(popcount(king_attacks(sq(3, 3).0)), 8); // center d4
    }

    #[test]
    fn pawn_attacks_shape() {
        // White pawn on e4 attacks d5 and f5.
        let e4 = sq(4, 3).0;
        assert_eq!(pawn_attacks(Color::White, e4), bb_of(&[sq(3, 4), sq(5, 4)]));
        // Black pawn on e5 attacks d4 and f4.
        let e5 = sq(4, 4).0;
        assert_eq!(pawn_attacks(Color::Black, e5), bb_of(&[sq(3, 3), sq(5, 3)]));
        // White pawn on h-file only attacks g-file.
        let h4 = sq(7, 3).0;
        assert_eq!(pawn_attacks(Color::White, h4), sq(6, 4).bb());
    }

    #[test]
    fn rook_attacks_empty_board() {
        // Rook on d4 on an empty board attacks the whole d-file + 4th rank minus d4.
        let d4 = sq(3, 3).0;
        let attacks = rook_attacks(d4, 0);
        let d_file = bitboard::FILE_D;
        let rank4 = bitboard::RANK_4;
        let expected = (d_file | rank4) & !sq(3, 3).bb();
        assert_eq!(attacks, expected);
    }

    #[test]
    fn rook_attacks_with_blocker_stops_at_blocker() {
        // Rook on a1. Pawn on a4. Rook sees a2, a3, a4 (captures blocker) but not a5..a8.
        let a1 = Square::A1.0;
        let occ = sq(0, 3).bb(); // a4
        let attacks = rook_attacks(a1, occ);
        // North ray: a2, a3, a4 (blocker is captured, i.e. included)
        // Other rays: b1..h1 since empty.
        let expected_north = bb_of(&[sq(0, 1), sq(0, 2), sq(0, 3)]);
        let expected = expected_north | (bitboard::RANK_1 & !Square::A1.bb());
        assert_eq!(attacks, expected);
    }

    #[test]
    fn bishop_attacks_empty_board() {
        // Bishop on a1: a1..h8 diagonal minus a1 itself.
        let a1 = Square::A1.0;
        let attacks = bishop_attacks(a1, 0);
        let expected = bb_of(&[
            sq(1, 1),
            sq(2, 2),
            sq(3, 3),
            sq(4, 4),
            sq(5, 5),
            sq(6, 6),
            sq(7, 7),
        ]);
        assert_eq!(attacks, expected);
    }

    #[test]
    fn queen_is_rook_plus_bishop() {
        for s in 0..64u8 {
            let occ: Bitboard = 0xDEAD_BEEF_CAFE_BABE;
            assert_eq!(
                queen_attacks(s, occ),
                rook_attacks(s, occ) | bishop_attacks(s, occ)
            );
        }
    }

    #[test]
    fn between_and_line_straight() {
        // d4 and d8: between is d5, d6, d7; line is entire d-file.
        let d4 = sq(3, 3).0;
        let d8 = sq(3, 7).0;
        assert_eq!(between(d4, d8), bb_of(&[sq(3, 4), sq(3, 5), sq(3, 6)]));
        assert_eq!(line(d4, d8), bitboard::FILE_D);
    }

    #[test]
    fn between_and_line_diagonal() {
        // a1 and h8: between is b2..g7; line is the a1-h8 diagonal.
        let a1 = Square::A1.0;
        let h8 = Square::H8.0;
        let expected_between = bb_of(&[sq(1, 1), sq(2, 2), sq(3, 3), sq(4, 4), sq(5, 5), sq(6, 6)]);
        assert_eq!(between(a1, h8), expected_between);

        let expected_line = expected_between | Square::A1.bb() | Square::H8.bb();
        assert_eq!(line(a1, h8), expected_line);
    }

    #[test]
    fn between_zero_for_unaligned() {
        // a1 and b3 are not on a common line.
        let a1 = Square::A1.0;
        let b3 = sq(1, 2).0;
        assert_eq!(between(a1, b3), 0);
        assert_eq!(line(a1, b3), 0);
    }

    #[test]
    fn ray_empty_board_north_from_d4() {
        let d4 = sq(3, 3).0 as usize;
        assert_eq!(
            RAY_ATTACKS[N][d4],
            bb_of(&[sq(3, 4), sq(3, 5), sq(3, 6), sq(3, 7)])
        );
    }

    // -- coverage fills: classical-ray parity, every direction ---------------

    // -- const-fn initializer coverage --------------------------------------
    //
    // The builder functions run at compile time to populate `static` tables.
    // `cargo-llvm-cov` sees compile-time execution as uncovered — so we
    // invoke each builder once more at *runtime* to exercise the code paths.
    // The resulting arrays must match the compiled-in tables bit-for-bit.

    #[test]
    fn const_pawn_attack_builder_matches_compiled_table() {
        let built = compute_pawn_attacks();
        assert_eq!(built, PAWN_ATTACKS);
    }

    #[test]
    fn const_knight_attack_builder_matches_compiled_table() {
        let built = compute_knight_attacks();
        assert_eq!(built, KNIGHT_ATTACKS);
    }

    #[test]
    fn const_king_attack_builder_matches_compiled_table() {
        let built = compute_king_attacks();
        assert_eq!(built, KING_ATTACKS);
    }

    #[test]
    fn const_ray_builder_matches_compiled_table() {
        let built = compute_rays();
        assert_eq!(built, RAY_ATTACKS);
    }

    #[test]
    fn const_between_builder_matches_compiled_table() {
        let built = compute_between();
        // Arrays this large aren't `PartialEq`-comparable by value; compare
        // element-wise.
        for a in 0..64 {
            for b in 0..64 {
                assert_eq!(built[a][b], BETWEEN[a][b], "between[{a}][{b}]");
            }
        }
    }

    #[test]
    fn const_line_builder_matches_compiled_table() {
        let built = compute_line();
        for a in 0..64 {
            for b in 0..64 {
                assert_eq!(built[a][b], LINE[a][b], "line[{a}][{b}]");
            }
        }
    }

    #[test]
    fn ray_helper_runs_at_runtime() {
        // `ray(from, df, dr)` is a const fn; exercise it at runtime for
        // coverage. Compare against RAY_ATTACKS for the N direction.
        for sq in 0..64i32 {
            assert_eq!(ray(sq, 0, 1), RAY_ATTACKS[N][sq as usize]);
        }
    }

    #[test]
    fn squares_between_and_line_through_run_at_runtime() {
        for a in 0..64i32 {
            for b in 0..64i32 {
                assert_eq!(squares_between(a, b), BETWEEN[a as usize][b as usize]);
                assert_eq!(line_through(a, b), LINE[a as usize][b as usize]);
            }
        }
    }

    #[test]
    fn classical_agrees_with_magic_for_every_square_and_several_occupancies() {
        // `bishop_attacks_classical` / `rook_attacks_classical` are kept as
        // the correctness oracle for the magic implementation. If magic ever
        // diverges from classical, perft would catch it end-to-end — but
        // this direct equivalence makes the failure mode immediate.
        let occupancies: [Bitboard; 8] = [
            0,
            !0,
            0x00FF_00FF_00FF_00FF,
            0xF0F0_0F0F_F0F0_0F0F,
            0x0000_0000_FFFF_FFFF,
            0x1234_5678_9ABC_DEF0,
            0xDEAD_BEEF_CAFE_BABE,
            0x8000_0000_0000_0001,
        ];
        for sq in 0..64u8 {
            for &occ in &occupancies {
                assert_eq!(
                    bishop_attacks(sq, occ),
                    bishop_attacks_classical(sq, occ),
                    "bishop mismatch at sq={sq} occ={occ:#x}"
                );
                assert_eq!(
                    rook_attacks(sq, occ),
                    rook_attacks_classical(sq, occ),
                    "rook mismatch at sq={sq} occ={occ:#x}"
                );
            }
        }
    }

    #[test]
    fn line_covers_every_direction() {
        // Horizontal (rank).
        assert_eq!(line(Square::A1.0, Square::H1.0), bitboard::RANK_1);
        // Vertical (file).
        assert_eq!(line(Square::A1.0, Square::A8.0), bitboard::FILE_A);
        // a1-h8 diagonal.
        assert_ne!(line(Square::A1.0, Square::H8.0), 0);
        // a8-h1 anti-diagonal.
        assert_ne!(line(Square::A8.0, Square::H1.0), 0);
        // Same square → 0.
        assert_eq!(line(Square::A1.0, Square::A1.0), 0);
        // Not aligned → 0.
        assert_eq!(line(Square::A1.0, sq(1, 2).0), 0);
    }

    #[test]
    fn between_excludes_endpoints_and_handles_descending_directions() {
        // Descending rank (southbound).
        let d4 = sq(3, 3).0;
        let d1 = Square::D1.0;
        assert_eq!(between(d4, d1), bb_of(&[Square::D2, sq(3, 2)])); // d2, d3

        // Descending file (westbound).
        let h1 = Square::H1.0;
        let a1 = Square::A1.0;
        let between_h_a = between(h1, a1);
        assert_eq!(between_h_a.count_ones(), 6); // b1..g1
        assert_eq!(between_h_a & Square::A1.bb(), 0);
        assert_eq!(between_h_a & Square::H1.bb(), 0);

        // Descending diagonal.
        let h8 = Square::H8.0;
        assert_eq!(
            between(h8, a1),
            bb_of(&[sq(1, 1), sq(2, 2), sq(3, 3), sq(4, 4), sq(5, 5), sq(6, 6)])
        );

        // Same square → 0.
        assert_eq!(between(a1, a1), 0);
        // Not aligned → 0.
        assert_eq!(between(a1, sq(1, 2).0), 0);
    }
}
