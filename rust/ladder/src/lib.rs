//! Shared helpers for the ladder benchmarks.
//!
//! Each bench lives in `benches/<name>.rs` and pulls common test positions +
//! regressed-implementation shims from here.

use ultrachess_core::fen::parse_fen;
use ultrachess_core::position::Position;

/// Classic benchmark positions. Same ones the main bench crate uses so
/// ladder numbers compose with `rust/bench/` numbers.
pub const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
pub const KIWIPETE: &str =
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
pub const POS3: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";

/// Panic-on-error wrapper; benches should never see an invalid FEN.
pub fn load(fen: &str) -> Position {
    parse_fen(fen).expect("ladder bench FEN must parse")
}
