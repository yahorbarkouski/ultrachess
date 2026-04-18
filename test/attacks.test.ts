import { describe, expect, it } from "vitest";
import { Chess, Color } from "../src/index.js";

describe("Chess — attackers / isAttacked", () => {
  it("startpos: f7 is attacked by Black king on e8", async () => {
    using chess = await Chess.create();
    expect(chess.isAttacked("f7", Color.Black)).toBe(true);
    expect(chess.attackers("f7", Color.Black)).toContain("e8");
  });

  it("startpos: e4 is not attacked by White", async () => {
    using chess = await Chess.create();
    expect(chess.isAttacked("e4", Color.White)).toBe(false);
    expect(chess.attackers("e4", Color.White)).toEqual([]);
  });

  it("detects rook attack on b1 from a1", async () => {
    using chess = await Chess.create("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1");
    expect(chess.isAttacked("b1", Color.White)).toBe(true);
    expect(chess.attackers("b1", Color.White)).toContain("a1");
  });

  it("invalid squares return false / empty rather than throwing", async () => {
    using chess = await Chess.create();
    expect(chess.isAttacked("i9", Color.White)).toBe(false);
    expect(chess.isAttacked(99, Color.White)).toBe(false);
    expect(chess.attackers("i9", Color.White)).toEqual([]);
    expect(chess.attackers(64, Color.White)).toEqual([]);
  });

  it("numeric square index works identically to algebraic", async () => {
    using chess = await Chess.create();
    const byString = chess.attackers("f7", Color.Black);
    const byNumber = chess.attackers(53, Color.Black); // f7 = file 5 + rank 6*8 = 53
    expect(byNumber).toEqual(byString);
  });
});
