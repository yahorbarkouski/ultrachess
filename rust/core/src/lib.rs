//! ultrachess-core — high-performance chess engine core.
//!
//! Board representation:
//! - **Bitboards** keyed by (color, piece-type) plus per-color occupancy.
//! - **Mailbox** (`[Piece; 64]`) redundantly stored for O(1) "what is on this square".
//! - **LERF** square numbering: a1=0, h1=7, a8=56, h8=63.
//!
//! Move generation:
//! - **Fully legal** via pin mask + check mask (never emits an illegal move,
//!   never uses make/unmake to test legality).
//! - En-passant discovered-check edge case handled explicitly.

#![deny(rust_2018_idioms)]
#![allow(clippy::inline_always)]
#![allow(clippy::too_many_arguments)]

pub mod bitboard;
pub mod chess_move;
pub mod fen;
pub mod magic;
pub mod movegen;
pub mod perft;
pub mod pgn;
pub mod position;
pub mod san;
pub mod tables;
pub mod types;
pub mod zobrist;

pub use bitboard::Bitboard;
pub use chess_move::{Move, MoveKind};
pub use fen::{parse_fen, write_fen, FenError, STARTING_FEN};
pub use movegen::{count_legal_moves, generate_legal_moves, MoveCounter, MoveList, MoveSink};
pub use perft::perft;
pub use position::{Position, Undo};
pub use types::{CastlingRights, Color, Piece, PieceType, Square};
