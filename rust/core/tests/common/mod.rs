//! Shared fixtures + helpers for every integration test.
//!
//! Keep this file intentionally small. It is the stable contract that the
//! integration suite relies on; "shared helper" is a slippery slope.

#![allow(dead_code)]

use ultrachess_core::chess_move::Move;
use ultrachess_core::movegen::{generate_legal_moves, MoveList};
use ultrachess_core::position::Position;
use ultrachess_core::Square;

// --- Canonical positions ----------------------------------------------------

pub const STARTING_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

pub const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

pub const POSITION_3: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";

pub const POSITION_4: &str = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";

pub const POSITION_5: &str = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";

pub const POSITION_6: &str =
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";

/// `(name, fen, [expected perft count per depth from 1])`.
///
/// Used by `perft.rs` and by the WASM ABI tests that validate the
/// bridge is bit-exact with the Rust-native implementation.
pub const PERFT_REFERENCE: &[(&str, &str, &[u64])] = &[
    (
        "startpos",
        STARTING_FEN,
        &[20, 400, 8_902, 197_281, 4_865_609],
    ),
    ("kiwipete", KIWIPETE, &[48, 2_039, 97_862, 4_085_603]),
    (
        "position_3",
        POSITION_3,
        &[14, 191, 2_812, 43_238, 674_624, 11_030_083],
    ),
    ("position_4", POSITION_4, &[6, 264, 9_467, 422_333]),
    ("position_5", POSITION_5, &[44, 1_486, 62_379, 2_103_487]),
    ("position_6", POSITION_6, &[46, 2_079, 89_890, 3_894_594]),
];

// --- Reproducible PRNG ------------------------------------------------------

/// xorshift64 — stable output across hosts, suitable for property tests.
/// The seed MUST be a literal in the test body; no wall-clock seeds.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // xorshift requires non-zero state.
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn bounded(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
}

// --- Tiny helpers -----------------------------------------------------------

/// Find a legal move in `pos` whose from-square and to-square match.
/// Panics with a readable message on miss.
pub fn find_move(pos: &Position, from: Square, to: Square) -> Move {
    let mut ml = MoveList::new();
    generate_legal_moves(pos, &mut ml);
    for m in ml.iter() {
        if m.from() == from && m.to() == to {
            return *m;
        }
    }
    panic!("no legal move {from}→{to} in current position")
}
