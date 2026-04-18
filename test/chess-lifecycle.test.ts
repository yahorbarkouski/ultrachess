import { describe, expect, it } from "vitest";
import { Chess, DisposedError, InvalidFenError } from "../src/index.js";
import { STARTING_FEN } from "./_helpers/positions.js";

describe("Chess — lifecycle", () => {
  it("creates a starting position from no args", async () => {
    using chess = await Chess.create();
    expect(chess.fen()).toBe(STARTING_FEN);
  });

  it("creates from a custom FEN", async () => {
    const fen = "r1bqkb1r/pppp1Qpp/2n2n2/4p3/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 0 4";
    using chess = await Chess.create(fen);
    expect(chess.fen()).toBe(fen);
  });

  it("fromFen is an alias for create(fen)", async () => {
    using chess = await Chess.fromFen(STARTING_FEN);
    expect(chess.fen()).toBe(STARTING_FEN);
  });

  it("rejects an invalid FEN", async () => {
    await expect(Chess.create("not a fen")).rejects.toBeInstanceOf(InvalidFenError);
  });

  it("dispose() frees the handle; subsequent ops throw", async () => {
    const chess = await Chess.create();
    chess.dispose();
    expect(() => chess.fen()).toThrow(DisposedError);
    expect(() => chess.legalMoves()).toThrow(DisposedError);
  });

  it("dispose() is idempotent — double-dispose is a no-op", async () => {
    const chess = await Chess.create();
    chess.dispose();
    expect(() => chess.dispose()).not.toThrow();
  });

  it("using syntax disposes at scope end", async () => {
    let fenAtEntry: string;
    {
      using chess = await Chess.create();
      fenAtEntry = chess.fen();
    }
    expect(fenAtEntry).toBe(STARTING_FEN);
  });

  it("clone() produces an independent instance", async () => {
    using a = await Chess.create();
    using b = a.clone();
    b.move("e4");
    expect(b.fen()).not.toBe(a.fen());
    expect(a.fen()).toBe(STARTING_FEN);
  });

  it("clone is a fresh snapshot — same FEN, empty history", async () => {
    // Matches the Position::clone contract in the Rust core (see
    // rust/core/src/position.rs: "clone produces a fresh position with
    // empty history"). Consumers who want a full history copy play through
    // the moves themselves; the design here saves a heap copy per clone
    // and mirrors `new Chess(other.fen())`.
    using a = await Chess.create();
    a.move("e4");
    using b = a.clone();
    expect(b.fen()).toBe(a.fen());
    expect(b.history()).toHaveLength(0);
    expect(b.undo()).toBeNull();
  });

  it("clone after dispose throws", async () => {
    const a = await Chess.create();
    a.dispose();
    expect(() => a.clone()).toThrow(DisposedError);
  });
});
