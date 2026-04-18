#!/usr/bin/env node
// CommonJS cross-runtime smoke: exercises the published CJS bundle via
// `require("../../dist/index.cjs")`. Mirrors `bun-smoke.mjs` but goes
// through the CJS entry so regressions in dual-publish (e.g. the
// `import.meta.url` → CJS syntax-error class of bug) are caught.
//
// Runs under Node only — Bun/Deno's CJS story is best-effort and is
// covered by the ESM smoke.

"use strict";

const assert = require("node:assert/strict");
const { Chess } = require("../../dist/index.cjs");

(async () => {
  const chess = await Chess.create();
  try {
    assert.equal(chess.legalMoveCount(), 20, "startpos legal move count");
    assert.equal(chess.perft(4), 197_281n, "startpos perft(4)");
    chess.move("e4");
    assert.equal(chess.turn(), 1, "turn is black after 1.e4");
    chess.undo();
    assert.equal(chess.turn(), 0, "turn is white after undo");
    console.log("cjs-smoke: OK");
  } finally {
    chess.dispose();
  }
})().catch((err) => {
  console.error("cjs-smoke: FAIL", err);
  process.exit(1);
});
