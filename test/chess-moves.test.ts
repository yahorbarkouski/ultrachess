import { describe, expect, it } from "vitest";
import {
  Chess,
  Color,
  IllegalMoveError,
  MoveKind,
  moveFrom,
  moveKind,
  moveTo,
  squareName,
} from "../src/index.js";
import { KIWIPETE, STARTING_FEN } from "./_helpers/positions.js";

describe("Chess — move generation + make/undo", () => {
  it("starting position has 20 legal moves", async () => {
    using chess = await Chess.create();
    expect(chess.legalMoves()).toHaveLength(20);
    expect(chess.legalMoveCount()).toBe(20);
  });

  it("kiwipete has 48 legal moves", async () => {
    using chess = await Chess.create(KIWIPETE);
    expect(chess.legalMoveCount()).toBe(48);
  });

  it("decodes a packed move into from/to/kind", async () => {
    using chess = await Chess.create();
    const e4 = chess
      .legalMoves()
      .find((m) => squareName(moveFrom(m)) === "e2" && squareName(moveTo(m)) === "e4")!;
    expect(e4).toBeDefined();
    expect(moveKind(e4)).toBe(MoveKind.Normal);
  });

  it("plays a SAN move and undoes it", async () => {
    using chess = await Chess.create();
    const m = chess.move("e4");
    expect(squareName(moveTo(m))).toBe("e4");
    expect(chess.turn()).toBe(Color.Black);
    chess.undo();
    expect(chess.fen()).toBe(STARTING_FEN);
    expect(chess.turn()).toBe(Color.White);
  });

  it("move accepts both a packed Move and a SAN string", async () => {
    using chess = await Chess.create();
    const packed = chess.parseSan("e4");
    const played = chess.move(packed);
    expect(played).toBe(packed);
  });

  it("undo returns the move that was undone, or null when empty", async () => {
    using chess = await Chess.create();
    expect(chess.undo()).toBeNull();
    const m = chess.move("e4");
    expect(chess.undo()).toBe(m);
    expect(chess.undo()).toBeNull();
  });

  it("history is insertion-order across multiple plies", async () => {
    using chess = await Chess.create();
    chess.move("e4");
    chess.move("e5");
    chess.move("Nf3");
    const history = chess.history();
    expect(history).toHaveLength(3);
    expect(history.map((m) => moveTo(m)).map(squareName)).toEqual(["e4", "e5", "f3"]);
  });

  it("illegal SAN throws IllegalMoveError", async () => {
    using chess = await Chess.create();
    expect(() => chess.move("Qh5")).toThrow(IllegalMoveError);
  });

  it("UCI-style strings work as a SAN fallback", async () => {
    using chess = await Chess.create();
    const m = chess.move("e2e4");
    expect(squareName(moveTo(m))).toBe("e4");
  });

  it("history({ verbose }) preserves SAN for every ply", async () => {
    using chess = await Chess.create();
    chess.move("e4");
    chess.move("e5");
    chess.move("Nf3");
    const verbose = chess.history({ verbose: true });
    expect(verbose.map((v) => v.san)).toEqual(["e4", "e5", "Nf3"]);
    expect(verbose.map((v) => v.color)).toEqual([Color.White, Color.Black, Color.White]);
  });

  it("history({ raw }) is equivalent to history() with no options", async () => {
    using chess = await Chess.create();
    chess.move("e4");
    expect(chess.history({ raw: true })).toEqual(chess.history());
  });

  it("moves({ raw }) returns packed Move integers", async () => {
    using chess = await Chess.create();
    const packed = chess.moves({ raw: true });
    expect(packed).toHaveLength(20);
    expect(typeof packed[0]).toBe("number");
  });
});
