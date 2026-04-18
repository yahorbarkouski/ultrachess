#!/usr/bin/env bun
// Bun-runtime smoke: loads the async entry, runs perft, checks one invariant.
// Runs under `bun test/bun-smoke.mjs` or `node test/bun-smoke.mjs`.
//
// Keeping this as a standalone script (not inside vitest) makes it trivial
// to run under different runtimes without configuring a second test runner.

import assert from "node:assert/strict";
import { Chess } from "../dist/index.js";

const chess = await Chess.create();
try {
  assert.equal(chess.legalMoveCount(), 20, "startpos legal move count");
  assert.equal(chess.perft(4), 197_281n, "startpos perft(4)");
  chess.move("e4");
  assert.equal(chess.turn(), 1, "turn is black after 1.e4");
  chess.undo();
  assert.equal(chess.turn(), 0, "turn is white after undo");
  console.log("bun-smoke: OK");
} finally {
  chess.dispose();
}
