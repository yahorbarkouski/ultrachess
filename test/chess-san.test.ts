import { describe, expect, it } from "vitest";
import { Chess, IllegalMoveError } from "../src/index.js";

describe("Chess — SAN round-trip + parser", () => {
  it("SAN round-trips for every legal move at startpos", async () => {
    using chess = await Chess.create();
    for (const m of chess.legalMoves()) {
      const san = chess.san(m);
      expect(chess.parseSan(san)).toBe(m);
    }
  });

  it("parseSan throws on garbage", async () => {
    using chess = await Chess.create();
    expect(() => chess.parseSan("???")).toThrow(IllegalMoveError);
  });

  it("parseSan tolerates trailing +/#", async () => {
    using chess = await Chess.create();
    expect(() => chess.parseSan("e4+")).not.toThrow();
    expect(() => chess.parseSan("e4#")).not.toThrow();
  });
});
