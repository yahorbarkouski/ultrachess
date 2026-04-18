import { describe, expect, it } from "vitest";
import { Chess, Color, PieceType } from "../src/index.js";

describe("Chess — put / remove", () => {
  it("put on an empty square returns null; pieceAt reflects the change", async () => {
    using chess = await Chess.create("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    expect(chess.pieceAt("d4")).toBeNull();
    const replaced = chess.put({ color: Color.White, type: PieceType.Queen }, "d4");
    expect(replaced).toBeNull();
    expect(chess.pieceAt("d4")).toEqual({
      color: Color.White,
      type: PieceType.Queen,
    });
  });

  it("put over an existing piece returns the replaced piece", async () => {
    using chess = await Chess.create();
    const replaced = chess.put({ color: Color.Black, type: PieceType.Queen }, "e2");
    expect(replaced).toEqual({ color: Color.White, type: PieceType.Pawn });
  });

  it("put rejects a second king of the same colour", async () => {
    using chess = await Chess.create();
    expect(() => chess.put({ color: Color.White, type: PieceType.King }, "d4")).toThrow(RangeError);
  });

  it("put / remove invalidate undo history", async () => {
    using chess = await Chess.create();
    chess.move("e4");
    expect(chess.history()).toHaveLength(1);
    chess.put({ color: Color.White, type: PieceType.Queen }, "d4");
    expect(chess.history()).toHaveLength(0);
    expect(chess.undo()).toBeNull();
  });

  it("remove on an empty square returns null", async () => {
    using chess = await Chess.create("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    expect(chess.remove("a1")).toBeNull();
  });

  it("remove returns the removed piece", async () => {
    using chess = await Chess.create();
    const removed = chess.remove("e2");
    expect(removed).toEqual({ color: Color.White, type: PieceType.Pawn });
    expect(chess.pieceAt("e2")).toBeNull();
  });

  it("put / remove throw on invalid square strings or out-of-range indices", async () => {
    using chess = await Chess.create();
    expect(() => chess.put({ color: Color.White, type: PieceType.Queen }, "zz")).toThrow(
      RangeError,
    );
    expect(() => chess.remove(64)).toThrow(RangeError);
  });
});
