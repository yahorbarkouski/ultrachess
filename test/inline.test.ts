//! Inline entry smoke test — verifies the base64-embedded WASM instantiates
//! synchronously and `Chess.createSync` works without any top-level await.

import { describe, expect, it } from "vitest";

describe("ultrachess/inline — sync construction", () => {
  it("loads WASM synchronously at import time", async () => {
    // The inline entry eager-initialises at import. If that failed, the
    // import itself would throw — so the mere fact this test runs is
    // evidence the sync path works.
    const mod = await import("../src/inline.js");
    expect(typeof mod.Chess).toBe("function");
    // initSync and init should both be available; init() returns the
    // already-instantiated module.
    expect(typeof mod.init).toBe("function");
  });

  it("Chess.createSync() works without await", async () => {
    const { Chess, STARTING_FEN, Color } = await import("../src/inline.js");
    const chess = Chess.createSync();
    try {
      expect(chess.fen()).toBe(STARTING_FEN);
      expect(chess.turn()).toBe(Color.White);
      expect(chess.legalMoveCount()).toBe(20);
    } finally {
      chess.dispose();
    }
  });

  it("sync + async entries share the same cached ABI", async () => {
    const inline = await import("../src/inline.js");
    const standard = await import("../src/index.js");
    // Both should resolve to the same underlying module.
    const a = await inline.init();
    const b = await standard.init();
    expect(a).toBe(b);
  });

  it("inline perft matches the reference", async () => {
    const { Chess } = await import("../src/inline.js");
    const chess = Chess.createSync();
    try {
      expect(chess.perft(4)).toBe(197_281n);
    } finally {
      chess.dispose();
    }
  });
});
