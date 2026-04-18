import { describe, expect, it } from "vitest";
import { Chess } from "../src/index.js";

describe("Chess — ascii", () => {
  it("startpos renders 8 lines, rank 8 first, rank 1 last", async () => {
    using chess = await Chess.create();
    const ascii = chess.ascii();
    const lines = ascii.trim().split("\n");
    expect(lines).toHaveLength(8);
    expect(lines[0]).toMatch(/^rnbqkbnr/);
    expect(lines[7]).toMatch(/^RNBQKBNR/);
  });

  it("empty squares are rendered as '.'", async () => {
    using chess = await Chess.create("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    const ascii = chess.ascii();
    // Most of the board is empty; the rendering should be dense '.'s.
    expect(ascii.match(/\./g)!.length).toBeGreaterThan(60);
    expect(ascii).toContain("k");
    expect(ascii).toContain("K");
  });
});
