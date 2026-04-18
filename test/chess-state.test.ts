import { describe, expect, it } from "vitest";
import { Chess, Color, PieceType } from "../src/index.js";
import { KIWIPETE, STARTING_FEN } from "./_helpers/positions.js";

describe("Chess — positional state queries", () => {
  it("turn, halfmove, fullmove on startpos", async () => {
    using chess = await Chess.create();
    expect(chess.turn()).toBe(Color.White);
    expect(chess.halfmove()).toBe(0);
    expect(chess.fullmove()).toBe(1);
  });

  it("turn flips after a move; fullmove increments only after Black", async () => {
    using chess = await Chess.create();
    chess.move("e4");
    expect(chess.turn()).toBe(Color.Black);
    expect(chess.fullmove()).toBe(1);
    chess.move("e5");
    expect(chess.turn()).toBe(Color.White);
    expect(chess.fullmove()).toBe(2);
  });

  it("pieceAt accepts both algebraic and numeric squares", async () => {
    using chess = await Chess.create();
    const e2Algebraic = chess.pieceAt("e2");
    const e2Index = chess.pieceAt(12);
    expect(e2Algebraic).toEqual({ color: Color.White, type: PieceType.Pawn });
    expect(e2Index).toEqual(e2Algebraic);
  });

  it("pieceAt returns null for empty and out-of-range squares", async () => {
    using chess = await Chess.create();
    expect(chess.pieceAt("e4")).toBeNull();
    expect(chess.pieceAt(-1)).toBeNull();
    expect(chess.pieceAt(64)).toBeNull();
    expect(chess.pieceAt("zz")).toBeNull();
  });

  it("hash() is a BigInt and stable across two instances at the same FEN", async () => {
    using a = await Chess.create(KIWIPETE);
    using b = await Chess.create(KIWIPETE);
    expect(typeof a.hash()).toBe("bigint");
    expect(a.hash()).toBe(b.hash());
  });

  it("hash() changes after a move and restores after undo", async () => {
    using chess = await Chess.create();
    const h0 = chess.hash();
    chess.move("e4");
    expect(chess.hash()).not.toBe(h0);
    chess.undo();
    expect(chess.hash()).toBe(h0);
  });

  it("FEN round-trips via parse + write", async () => {
    using chess = await Chess.create(KIWIPETE);
    expect(chess.fen()).toBe(KIWIPETE);
  });

  it("fen() reflects make + undo exactly", async () => {
    using chess = await Chess.create();
    chess.move("e4");
    const midFen = chess.fen();
    expect(midFen).not.toBe(STARTING_FEN);
    chess.undo();
    expect(chess.fen()).toBe(STARTING_FEN);
    const [chess2] = await Promise.all([Chess.create(midFen)]);
    try {
      expect(chess2.fen()).toBe(midFen);
    } finally {
      chess2.dispose();
    }
  });
});
