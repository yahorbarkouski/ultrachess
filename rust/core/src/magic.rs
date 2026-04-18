//! Fancy magic bitboards for bishop / rook attacks.
//!
//! Replaces the classical-ray path (4 dependent loads per slider query)
//! with one multiply + shift + indexed load — ~2× NPS on slider-heavy
//! positions.
//!
//! # Layout
//! One shared attack table per slider type, keyed by per-square
//! `(offset, mask, magic, shift)`:
//!
//! ```text
//! attacks[entry.offset + ((occ & entry.mask).wrapping_mul(entry.magic) >> entry.shift)]
//! ```
//!
//! Plain magics (shift = `64 - popcount(mask)`), not black magics. Tables
//! total ≈842 KiB, lazy-allocated on first use (~5-20 ms one-shot cost).
//!
//! # Magics
//! Searched at init with a seeded RNG rather than embedded as constants,
//! so we don't maintain 128 audited literals. Seed is fixed — same magics
//! every run, for reproducibility. Correctness is gated by
//! `tests/magic.rs` cross-checking against the classical path.

use crate::bitboard::Bitboard;
use std::sync::OnceLock;

// --- Public attack functions ------------------------------------------------

#[inline(always)]
pub fn bishop_attacks(sq: u8, occ: Bitboard) -> Bitboard {
    let t = tables();
    let e = &t.bishop[sq as usize];
    let idx = (((occ & e.mask).wrapping_mul(e.magic)) >> e.shift) as usize;
    // SAFETY: `idx < 1 << (64 - e.shift)` by construction; bounds-check
    // elided because this is the hottest loop in perft. Gated by the
    // perft correctness tests.
    unsafe { *t.bishop_attacks.get_unchecked(e.offset as usize + idx) }
}

#[inline(always)]
pub fn rook_attacks(sq: u8, occ: Bitboard) -> Bitboard {
    let t = tables();
    let e = &t.rook[sq as usize];
    let idx = (((occ & e.mask).wrapping_mul(e.magic)) >> e.shift) as usize;
    unsafe { *t.rook_attacks.get_unchecked(e.offset as usize + idx) }
}

#[inline(always)]
pub fn queen_attacks(sq: u8, occ: Bitboard) -> Bitboard {
    bishop_attacks(sq, occ) | rook_attacks(sq, occ)
}

// --- Static table, lazy-initialised on first slider query ------------------

#[derive(Clone, Copy)]
struct MagicEntry {
    mask: u64,
    magic: u64,
    offset: u32,
    shift: u32, // 64 - popcount(mask)
}

struct Tables {
    bishop: [MagicEntry; 64],
    rook: [MagicEntry; 64],
    bishop_attacks: Vec<u64>,
    rook_attacks: Vec<u64>,
}

static TABLES: OnceLock<Tables> = OnceLock::new();

#[inline(always)]
fn tables() -> &'static Tables {
    // Lock-free on the hot (initialised) path.
    TABLES.get_or_init(build_tables)
}

/// Force initialisation now. Use if you want the ~5-20 ms init cost paid
/// at module load rather than at first move generation.
pub fn init() {
    let _ = tables();
}

fn build_tables() -> Tables {
    let (bishop, bishop_attacks) = build_one(SliderKind::Bishop);
    let (rook, rook_attacks) = build_one(SliderKind::Rook);
    Tables {
        bishop,
        rook,
        bishop_attacks,
        rook_attacks,
    }
}

// --- Magic search ----------------------------------------------------------

#[derive(Clone, Copy)]
enum SliderKind {
    Bishop,
    Rook,
}

fn build_one(kind: SliderKind) -> ([MagicEntry; 64], Vec<u64>) {
    let mut entries = [MagicEntry {
        mask: 0,
        magic: 0,
        offset: 0,
        shift: 0,
    }; 64];

    // First pass: figure out table size (sum of 1 << bits).
    let mut total_size: usize = 0;
    let mut sizes = [0usize; 64];
    let mut masks = [0u64; 64];
    for sq in 0..64u8 {
        let mask = relevant_mask(sq, kind);
        let bits = mask.count_ones() as usize;
        let size = 1usize << bits;
        masks[sq as usize] = mask;
        sizes[sq as usize] = size;
        total_size += size;
    }

    // Second pass: find a magic per square and populate the shared table.
    let mut attacks = vec![0u64; total_size];
    let mut rng = SplitMix64::new(0xCBF2_9CE4_8422_2325);

    let mut cursor: u32 = 0;
    for sq in 0..64u8 {
        let mask = masks[sq as usize];
        let bits = mask.count_ones();
        let size = sizes[sq as usize];
        let shift = 64 - bits;

        // Enumerate all 2^bits occupancy subsets of `mask`, and compute the
        // corresponding classical-ray attacks once.
        let mut occupancies: Vec<u64> = Vec::with_capacity(size);
        let mut classical: Vec<u64> = Vec::with_capacity(size);
        for i in 0..size {
            let occ = carry_rippler_index(i as u64, mask);
            occupancies.push(occ);
            classical.push(classical_attacks(sq, occ, kind));
        }

        // Search for a magic that produces a collision-free (or identity-
        // collision-only) index map.
        let slice = &mut attacks[cursor as usize..cursor as usize + size];
        let magic = find_magic(&mut rng, mask, &occupancies, &classical, shift, slice);

        entries[sq as usize] = MagicEntry {
            mask,
            magic,
            offset: cursor,
            shift,
        };
        cursor += size as u32;
    }
    debug_assert_eq!(cursor as usize, total_size);
    (entries, attacks)
}

fn find_magic(
    rng: &mut SplitMix64,
    mask: u64,
    occupancies: &[u64],
    classical: &[u64],
    shift: u32,
    table: &mut [u64],
) -> u64 {
    let size = occupancies.len();
    let mut attempt: u64 = 0;
    loop {
        attempt += 1;
        // Sparse candidates — AND-of-three biases popcount into 10-20, which
        // converges much faster than uniform random.
        let m = rng.next() & rng.next() & rng.next();
        // chessprogramming-wiki heuristic: top-byte popcount of (mask * magic)
        // should be ≥ 6 for the candidate to have a chance.
        if mask.wrapping_mul(m).wrapping_shr(56).count_ones() < 6 {
            continue;
        }

        // Clear-and-test: the table is small enough that stamping schemes
        // aren't worth the complexity.
        for t in table.iter_mut() {
            *t = 0;
        }
        let mut ok = true;
        for i in 0..size {
            let idx = ((occupancies[i].wrapping_mul(m)) >> shift) as usize;
            if table[idx] == 0 {
                table[idx] = classical[i];
            } else if table[idx] != classical[i] {
                ok = false;
                break;
            }
        }
        if ok {
            return m;
        }
        if attempt > 10_000_000 {
            panic!("could not find magic within 10M attempts (shift={shift})");
        }
    }
}

/// SplitMix64 — dependency-free deterministic RNG for magic search.
struct SplitMix64(u64);
impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }
    #[inline(always)]
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// "Carry-rippler" enumeration of subsets of `mask`. Given an index i in
/// [0, 2^popcount(mask)), returns the i-th subset of `mask` under a
/// canonical ordering (low-bit first). Used to enumerate all occupancies
/// of a relevant-mask in order.
#[inline]
fn carry_rippler_index(i: u64, mask: u64) -> u64 {
    // PDEP-equivalent without the BMI2 intrinsic: deposit the bits of `i`
    // into the positions set in `mask`.
    let mut m = mask;
    let mut result: u64 = 0;
    let mut src = i;
    while m != 0 {
        let bit = m & m.wrapping_neg();
        if src & 1 != 0 {
            result |= bit;
        }
        src >>= 1;
        m &= m - 1;
    }
    result
}

// --- Relevant masks & classical attack reference ---------------------------

fn relevant_mask(sq: u8, kind: SliderKind) -> u64 {
    match kind {
        SliderKind::Bishop => bishop_relevant_mask(sq),
        SliderKind::Rook => rook_relevant_mask(sq),
    }
}

// Relevant masks exclude edge squares — edge occupancy doesn't change
// what a slider sees (the edge is an implicit blocker), so those bits
// don't need to be in the hash input.

fn bishop_relevant_mask(sq: u8) -> u64 {
    let f = (sq & 7) as i32;
    let r = (sq >> 3) as i32;
    let mut bb: u64 = 0;
    for (df, dr) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
        let mut ff = f + df;
        let mut rr = r + dr;
        while (1..=6).contains(&ff) && (1..=6).contains(&rr) {
            bb |= 1u64 << (rr * 8 + ff);
            ff += df;
            rr += dr;
        }
    }
    bb
}

fn rook_relevant_mask(sq: u8) -> u64 {
    let f = (sq & 7) as i32;
    let r = (sq >> 3) as i32;
    let mut bb: u64 = 0;
    for rr in 1..=6 {
        if rr != r {
            bb |= 1u64 << (rr * 8 + f);
        }
    }
    for ff in 1..=6 {
        if ff != f {
            bb |= 1u64 << (r * 8 + ff);
        }
    }
    bb
}

/// Classical ray-scan reference used only during magic-table construction.
/// Must agree with `tables::bishop_attacks` / `tables::rook_attacks`.
fn classical_attacks(sq: u8, occ: u64, kind: SliderKind) -> u64 {
    let f = (sq & 7) as i32;
    let r = (sq >> 3) as i32;
    let dirs: &[(i32, i32)] = match kind {
        SliderKind::Bishop => &[(1, 1), (-1, 1), (1, -1), (-1, -1)],
        SliderKind::Rook => &[(0, 1), (0, -1), (1, 0), (-1, 0)],
    };
    let mut bb: u64 = 0;
    for (df, dr) in dirs {
        let mut ff = f + df;
        let mut rr = r + dr;
        while (0..8).contains(&ff) && (0..8).contains(&rr) {
            let s = (rr * 8 + ff) as u64;
            bb |= 1u64 << s;
            if (occ >> s) & 1 != 0 {
                break;
            }
            ff += df;
            rr += dr;
        }
    }
    bb
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Cross-check magic against classical for every square and a large set of
    // random occupancies.
    #[test]
    fn magic_matches_classical_random() {
        let mut rng = SplitMix64::new(0xDEAD_BEEF);
        for _ in 0..5_000 {
            let occ = rng.next() & rng.next(); // biased sparser
            for sq in 0..64u8 {
                assert_eq!(
                    bishop_attacks(sq, occ),
                    classical_attacks(sq, occ, SliderKind::Bishop),
                    "bishop mismatch at sq={sq} occ={occ:#x}"
                );
                assert_eq!(
                    rook_attacks(sq, occ),
                    classical_attacks(sq, occ, SliderKind::Rook),
                    "rook mismatch at sq={sq} occ={occ:#x}"
                );
            }
        }
    }

    #[test]
    fn magic_matches_classical_empty_board() {
        for sq in 0..64u8 {
            assert_eq!(
                bishop_attacks(sq, 0),
                classical_attacks(sq, 0, SliderKind::Bishop)
            );
            assert_eq!(
                rook_attacks(sq, 0),
                classical_attacks(sq, 0, SliderKind::Rook)
            );
        }
    }

    #[test]
    fn magic_matches_classical_full_board() {
        let occ = !0u64;
        for sq in 0..64u8 {
            // With a full board, a slider sees only its 1-step neighbours in
            // each direction.
            assert_eq!(
                bishop_attacks(sq, occ),
                classical_attacks(sq, occ, SliderKind::Bishop)
            );
            assert_eq!(
                rook_attacks(sq, occ),
                classical_attacks(sq, occ, SliderKind::Rook)
            );
        }
    }
}
