import { describe, expect, it } from "vitest";
import { Chess } from "../src/index.js";
import { BARE_KINGS, SCHOLARS_MATE, STALEMATE } from "./_helpers/positions.js";

describe("Chess — end-state classification", () => {
  it("detects checkmate", async () => {
    using chess = await Chess.create(SCHOLARS_MATE);
    expect(chess.inCheck()).toBe(true);
    expect(chess.isCheckmate()).toBe(true);
    expect(chess.isGameOver()).toBe(true);
    expect(chess.isDraw()).toBe(false);
  });

  it("detects stalemate", async () => {
    using chess = await Chess.create(STALEMATE);
    expect(chess.isStalemate()).toBe(true);
    expect(chess.isCheckmate()).toBe(false);
    expect(chess.isDraw()).toBe(true);
    expect(chess.isGameOver()).toBe(true);
  });

  it("insufficient material — K vs K", async () => {
    using chess = await Chess.create(BARE_KINGS);
    expect(chess.isInsufficientMaterial()).toBe(true);
    expect(chess.isDraw()).toBe(true);
  });

  it("sufficient material — K+R vs K", async () => {
    using chess = await Chess.create("8/8/8/4k3/8/8/4K3/5R2 w - - 0 1");
    expect(chess.isInsufficientMaterial()).toBe(false);
  });

  it("50-move rule triggers at halfmove 100", async () => {
    using at99 = await Chess.create("4k3/8/8/8/8/8/8/4K3 w - - 99 1");
    using at100 = await Chess.create("4k3/8/8/8/8/8/8/4K3 w - - 100 1");
    expect(at99.isFiftyMoveRule()).toBe(false);
    expect(at100.isFiftyMoveRule()).toBe(true);
    expect(at100.isDraw()).toBe(true);
  });

  it("threefold repetition via knight shuttle", async () => {
    using chess = await Chess.create();
    for (let i = 0; i < 2; i++) {
      chess.move("Nf3");
      chess.move("Nf6");
      chess.move("Ng1");
      chess.move("Ng8");
    }
    expect(chess.isThreefoldRepetition()).toBe(true);
    expect(chess.isDraw()).toBe(true);
  });
});
