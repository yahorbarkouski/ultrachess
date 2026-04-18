import { describe, expect, it } from "vitest";
import { Chess, Color, IllegalMoveError, MoveKind, PieceType } from "../src/index.js";
import { findMoveBySan } from "./_helpers/move-utils.js";

describe("Chess — verboseMove + moves(verbose)", () => {
  it("verboseMove fills every field for a quiet pawn push", async () => {
    using chess = await Chess.create();
    const e4 = findMoveBySan(chess, "e4");
    const v = chess.verboseMove(e4);
    expect(v.from).toBe("e2");
    expect(v.to).toBe("e4");
    expect(v.piece).toBe(PieceType.Pawn);
    expect(v.color).toBe(Color.White);
    expect(v.kind).toBe(MoveKind.Normal);
    expect(v.san).toBe("e4");
    expect(v.uci).toBe("e2e4");
    expect(v.captured).toBeUndefined();
    expect(v.promotion).toBeUndefined();
  });

  it("captured piece type is reported for normal captures", async () => {
    using chess = await Chess.create(
      "rnbqkbnr/pppp1ppp/8/4p3/3P4/8/PPP1PPPP/RNBQKBNR w KQkq - 0 2",
    );
    const m = findMoveBySan(chess, "dxe5");
    expect(chess.verboseMove(m).captured).toBe(PieceType.Pawn);
  });

  it("promotion + capture reports both fields (and the SAN carries +)", async () => {
    using chess = await Chess.create("1r2k3/P7/8/8/8/8/8/4K3 w - - 0 1");
    const m = findMoveBySan(chess, "axb8=Q+");
    const v = chess.verboseMove(m);
    expect(v.kind).toBe(MoveKind.Promotion);
    expect(v.promotion).toBe(PieceType.Queen);
    expect(v.captured).toBe(PieceType.Rook);
  });

  it("en-passant kind captures a pawn", async () => {
    using chess = await Chess.create(
      "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3",
    );
    const ep = chess.legalMoves().find(
      (m) => chess.verboseMove(m).kind === MoveKind.EnPassant,
    )!;
    const v = chess.verboseMove(ep);
    expect(v.from).toBe("e5");
    expect(v.to).toBe("d6");
    expect(v.captured).toBe(PieceType.Pawn);
  });

  it("moves() returns SAN by default", async () => {
    using chess = await Chess.create();
    const sans = chess.moves();
    expect(sans).toContain("e4");
    expect(sans).toContain("Nf3");
    expect(sans).toHaveLength(20);
  });

  it("moves({ verbose }) returns one VerboseMove per legal move", async () => {
    using chess = await Chess.create();
    const verbose = chess.moves({ verbose: true });
    expect(verbose).toHaveLength(20);
    expect(verbose[0]).toHaveProperty("san");
    expect(verbose[0]).toHaveProperty("uci");
    expect(verbose[0]).toHaveProperty("piece");
  });

  it("verboseMove throws when the source square is empty", async () => {
    using chess = await Chess.create("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    // Construct a packed move whose from-square (a1 = 0) is empty.
    const fakeMove = 0 as unknown as ReturnType<Chess["parseSan"]>;
    expect(() => chess.verboseMove(fakeMove)).toThrow(IllegalMoveError);
  });
});
