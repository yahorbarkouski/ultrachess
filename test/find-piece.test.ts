import { describe, expect, it } from "vitest";
import { Chess, Color, PieceType } from "../src/index.js";

describe("Chess — findPiece", () => {
  it("finds all White pawns on startpos", async () => {
    using chess = await Chess.create();
    const pawns = chess
      .findPiece({ color: Color.White, type: PieceType.Pawn })
      .sort();
    expect(pawns).toEqual(["a2", "b2", "c2", "d2", "e2", "f2", "g2", "h2"]);
  });

  it("finds the single White king", async () => {
    using chess = await Chess.create();
    expect(chess.findPiece({ color: Color.White, type: PieceType.King })).toEqual(["e1"]);
  });

  it("returns empty when no piece of that type exists", async () => {
    using chess = await Chess.create("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    expect(chess.findPiece({ color: Color.White, type: PieceType.Queen })).toEqual([]);
  });

  it("both colours are queried separately", async () => {
    using chess = await Chess.create();
    const white = chess.findPiece({ color: Color.White, type: PieceType.Rook }).sort();
    const black = chess.findPiece({ color: Color.Black, type: PieceType.Rook }).sort();
    expect(white).toEqual(["a1", "h1"]);
    expect(black).toEqual(["a8", "h8"]);
  });
});
