//! Perft via WASM — every node count must match the Rust-native reference.
//! This is the Phase 5 gate: if these pass, the WASM bridge is bit-accurate.

import { describe, expect, it } from "vitest";
import { Chess } from "../src/index.js";

const POSITIONS: [string, string, ReadonlyArray<bigint>][] = [
  [
    "startpos",
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    [20n, 400n, 8_902n, 197_281n, 4_865_609n],
  ],
  [
    "kiwipete",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    [48n, 2_039n, 97_862n, 4_085_603n],
  ],
  [
    "position 3",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    [14n, 191n, 2_812n, 43_238n, 674_624n, 11_030_083n],
  ],
  [
    "position 4",
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    [6n, 264n, 9_467n, 422_333n],
  ],
  [
    "position 5",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    [44n, 1_486n, 62_379n, 2_103_487n],
  ],
  [
    "position 6",
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    [46n, 2_079n, 89_890n, 3_894_594n],
  ],
];

describe("perft via WASM matches Rust reference", () => {
  for (const [name, fen, counts] of POSITIONS) {
    for (let d = 1; d <= counts.length; d++) {
      it(`${name} depth ${d} == ${counts[d - 1]}`, async () => {
        using chess = await Chess.create(fen);
        const nodes = chess.perft(d);
        expect(nodes).toBe(counts[d - 1]);
      });
    }
  }
});
