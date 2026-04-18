#!/usr/bin/env bun
// Cross-runtime smoke: loads the published bundle, runs perft, checks one
// invariant. Runs the same way under Node, Bun, or Deno:
//   node test/cross-runtime/bun-smoke.mjs
//   bun  test/cross-runtime/bun-smoke.mjs
//   deno run --allow-read test/cross-runtime/bun-smoke.mjs
//
// Standalone (not inside vitest) so adding a new runtime never requires a
// second test runner to be configured.

import assert from "node:assert/strict";
import { Chess } from "../../dist/index.js";

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
