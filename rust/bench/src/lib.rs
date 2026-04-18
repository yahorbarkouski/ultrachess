//! Shared perft driver used by both the `nps` binary and the criterion benches.
//!
//! Each engine is wrapped behind a thin module so the harness looks the same
//! at the call site: parse FEN → perft(depth) → u64 node count.
//! The implementations deliberately mirror each library's recommended
//! "fastest perft" idiom (taken from each project's own bench / example code).

pub mod engines {
    // ---- ultrachess-core (ours) ------------------------------------------
    pub mod ours {
        use ultrachess_core::{fen::parse_fen, perft::perft as uc_perft, position::Position};

        pub const NAME: &str = "ultrachessjs";

        pub fn parse(fen: &str) -> Position {
            parse_fen(fen).expect("valid fen")
        }

        pub fn perft(pos: &Position, depth: u32) -> u64 {
            // perft takes &mut because it reuses the position's history stack;
            // we clone so repeat runs start from the same state.
            let mut p = pos.clone();
            uc_perft(&mut p, depth)
        }
    }

    // ---- shakmaty (Lichess) ----------------------------------------------
    pub mod shak {
        use shakmaty::{fen::Fen, CastlingMode, Chess, Position};

        pub const NAME: &str = "shakmaty (Lichess)";

        pub fn parse(fen: &str) -> Chess {
            fen.parse::<Fen>()
                .expect("valid fen")
                .into_position(CastlingMode::Standard)
                .expect("legal position")
        }

        // Idiomatic recursive perft for shakmaty. `play_unchecked` is the
        // fast path (skips legality re-check).
        pub fn perft(pos: &Chess, depth: u32) -> u64 {
            if depth == 0 {
                return 1;
            }
            let moves = pos.legal_moves();
            if depth == 1 {
                return moves.len() as u64;
            }
            let mut n = 0u64;
            for m in &moves {
                let mut child = pos.clone();
                child.play_unchecked(m);
                n += perft(&child, depth - 1);
            }
            n
        }
    }

    // ---- cozy-chess ------------------------------------------------------
    pub mod cozy {
        use cozy_chess::Board;

        pub const NAME: &str = "cozy-chess";

        pub fn parse(fen: &str) -> Board {
            fen.parse::<Board>().expect("valid fen")
        }

        // Pattern taken from cozy-chess's own README — short-circuit at
        // depth==1 by asking the movegen for its move count rather than
        // materialising a list.
        pub fn perft(pos: &Board, depth: u32) -> u64 {
            fn rec(b: &Board, depth: u32) -> u64 {
                if depth == 0 {
                    return 1;
                }
                let mut n = 0u64;
                if depth == 1 {
                    b.generate_moves(|mvs| {
                        n += mvs.len() as u64;
                        false
                    });
                    return n;
                }
                b.generate_moves(|mvs| {
                    for m in mvs {
                        let mut child = b.clone();
                        child.play_unchecked(m);
                        n += rec(&child, depth - 1);
                    }
                    false
                });
                n
            }
            rec(pos, depth)
        }
    }

    // ---- jordanbray/chess -------------------------------------------------
    pub mod jb {
        use chess::{Board, ChessMove, MoveGen};
        use std::str::FromStr;

        pub const NAME: &str = "chess (jordanbray)";

        pub fn parse(fen: &str) -> Board {
            Board::from_str(fen).expect("valid fen")
        }

        pub fn perft(pos: &Board, depth: u32) -> u64 {
            if depth == 0 {
                return 1;
            }
            let mg = MoveGen::new_legal(pos);
            if depth == 1 {
                return mg.len() as u64;
            }
            let mut n = 0u64;
            let moves: Vec<ChessMove> = mg.collect();
            for m in moves {
                let child = pos.make_move_new(m);
                n += perft(&child, depth - 1);
            }
            n
        }
    }
}

/// Canonical test positions — the same six that gate correctness.
pub const POSITIONS: &[(&str, &str)] = &[
    (
        "Startpos",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    ),
    (
        "Kiwipete",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    ),
    ("Pos3 (EP)", "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"),
    (
        "Pos4",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    ),
    (
        "Pos5",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    ),
    (
        "Pos6",
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    ),
];

/// Default depth per position. Chosen so node count lands in ~1M..150M
/// (long enough for a stable timing, short enough to finish < ~5 s per
/// engine on modern hardware).
pub const DEFAULT_DEPTHS: &[u32] = &[5, 4, 5, 4, 4, 4];
