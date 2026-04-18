#!/usr/bin/env node
// WASM perft harness. Measures what a JS user actually gets through the
// WebAssembly boundary: `Chess.create()` + `chess.perft(d)`.
//
// Runs the six standard perft positions at their canonical depths,
// reports nodes/second, and exits non-zero if any leaf count diverges
// from the reference.
//
// Usage:
//   node rust/bench/wasm-perft.mjs            # standard tier (startpos d5)
//   node rust/bench/wasm-perft.mjs --deep     # adds +1 depth (startpos d6)
//   node rust/bench/wasm-perft.mjs --trials 5 # override trial count
//
// The dist/ build must exist (`just build`). Node ≥18.

import { performance } from "node:perf_hooks";
import process from "node:process";

// Import via the built package so we measure the real boundary a
// consumer sees, not the source TS.
const dist = new URL("../../dist/index.js", import.meta.url);
const { Chess, init } = await import(dist.href);

// ---------------------------------------------------------------------------
// Reference positions + node counts (same as rust/core/tests/common/mod.rs).
// ---------------------------------------------------------------------------

const STARTPOS =
  "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const KIWIPETE =
  "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
const POSITION_3 = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
const POSITION_4 =
  "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
const POSITION_5 =
  "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
const POSITION_6 =
  "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";

// Standard tier: safe depths that finish per position in well under a
// second once warm. Deep tier: one extra depth.
const STANDARD = [
  ["startpos", STARTPOS, 5, 4_865_609n],
  ["kiwipete", KIWIPETE, 4, 4_085_603n],
  ["position_3", POSITION_3, 5, 674_624n],
  ["position_4", POSITION_4, 4, 422_333n],
  ["position_5", POSITION_5, 4, 2_103_487n],
  ["position_6", POSITION_6, 4, 3_894_594n],
];

const DEEP = [
  ["startpos", STARTPOS, 6, 119_060_324n],
  ["kiwipete", KIWIPETE, 5, 193_690_690n],
  ["position_3", POSITION_3, 6, 11_030_083n],
  ["position_4", POSITION_4, 5, 15_833_292n],
  ["position_5", POSITION_5, 5, 89_941_194n],
  ["position_6", POSITION_6, 5, 164_075_551n],
];

// ---------------------------------------------------------------------------
// Argv.
// ---------------------------------------------------------------------------

const argv = process.argv.slice(2);
const deep = argv.includes("--deep");
const trialsIdx = argv.indexOf("--trials");
const trials = trialsIdx >= 0 ? parseInt(argv[trialsIdx + 1], 10) : 3;

const positions = deep ? DEEP : STANDARD;

// ---------------------------------------------------------------------------
// Warm up once so the JIT and WASM tables are hot before we time anything.
// ---------------------------------------------------------------------------

await init();
{
  const warm = await Chess.create();
  warm.perft(3);
  warm.dispose();
}

// ---------------------------------------------------------------------------
// Run.
// ---------------------------------------------------------------------------

const runtimeName = process.versions.bun
  ? `Bun ${process.versions.bun}`
  : `Node ${process.versions.node}`;

console.log(`# ultrachess WASM perft — ${runtimeName}`);
console.log(`# tier: ${deep ? "deep" : "standard"}, trials: ${trials}`);
console.log("");
console.log(
  "| Position    | Depth |          Nodes |   Min ms |        Mnps | OK |",
);
console.log(
  "|-------------|------:|---------------:|---------:|------------:|:--:|",
);

let allOk = true;
let geomeanLogSum = 0;
let geomeanCount = 0;

for (const [name, fen, depth, expected] of positions) {
  let minMs = Infinity;
  let actual = 0n;
  for (let t = 0; t < trials; t++) {
    const chess = await Chess.create(fen);
    const t0 = performance.now();
    actual = chess.perft(depth);
    const t1 = performance.now();
    chess.dispose();
    minMs = Math.min(minMs, t1 - t0);
  }
  const ok = actual === expected;
  if (!ok) allOk = false;

  // Nodes/sec from min time.
  const mnps = Number(expected) / minMs / 1000;
  geomeanLogSum += Math.log(mnps);
  geomeanCount++;

  const nodesStr = expected.toLocaleString("en-US");
  console.log(
    `| ${name.padEnd(11)} | ${String(depth).padStart(5)} | ${nodesStr.padStart(
      14,
    )} | ${minMs.toFixed(1).padStart(8)} | ${mnps
      .toFixed(1)
      .padStart(11)} | ${ok ? "✓ " : "✗ "} |`,
  );
}

const geomean = Math.exp(geomeanLogSum / geomeanCount);
console.log("");
console.log(`geomean Mnps: ${geomean.toFixed(1)}`);

if (!allOk) {
  console.error("\nERROR: one or more perft counts diverged from reference.");
  process.exit(1);
}
