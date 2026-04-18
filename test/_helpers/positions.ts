//! Canonical positions used across tests.

export const STARTING_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

export const KIWIPETE = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

/** Classic checkmate: Scholar's Mate final position (Black to move, checkmated). */
export const SCHOLARS_MATE = "r1bqkb1r/pppp1Qpp/2n2n2/4p3/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 0 4";

/** Classic stalemate: king cornered, no legal moves, not in check. */
export const STALEMATE = "k7/2Q5/8/8/8/8/8/7K b - - 0 1";

/** K vs K — always insufficient material. */
export const BARE_KINGS = "8/8/8/4k3/8/4K3/8/8 w - - 0 1";
