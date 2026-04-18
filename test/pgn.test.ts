import { describe, expect, it } from "vitest";
import { Chess, Color, InvalidPgnError, PieceType } from "../src/index.js";

describe("Chess — PGN load / save", () => {
  it("loadPgn replays a classic mate and reports checkmate", async () => {
    const pgn = `[Event "Scholar's Mate"]
[White "White"]
[Black "Black"]
[Result "1-0"]

1. e4 e5 2. Bc4 Nc6 3. Qh5 Nf6 4. Qxf7# 1-0
`;
    using chess = await Chess.loadPgn(pgn);
    expect(chess.isCheckmate()).toBe(true);
    expect(chess.history()).toHaveLength(7);
    expect(chess.header("Event")).toBe("Scholar's Mate");
    expect(chess.header("Result")).toBe("1-0");
  });

  it("loadPgn preserves SAN in history({verbose})", async () => {
    const pgn = '[Event "t"]\n\n1. e4 e5 2. Nf3 *\n';
    using chess = await Chess.loadPgn(pgn);
    const v = chess.history({ verbose: true });
    expect(v.map((m) => m.san)).toEqual(["e4", "e5", "Nf3"]);
  });

  it("loadPgn honours FEN + SetUp headers", async () => {
    const pgn = `[Event "Endgame"]
[FEN "8/P7/8/8/8/8/4k3/7K w - - 0 1"]
[SetUp "1"]

1. a8=Q *
`;
    using chess = await Chess.loadPgn(pgn);
    expect(chess.pieceAt("a8")).toEqual({
      color: Color.White,
      type: PieceType.Queen,
    });
  });

  it("loadPgn rejects unparseable input", async () => {
    await expect(Chess.loadPgn("[")).rejects.toBeInstanceOf(InvalidPgnError);
  });

  it("loadPgn rejects PGNs with illegal SAN", async () => {
    await expect(Chess.loadPgn("\n1. e4 e5 2. Qxf7 *\n")).rejects.toBeInstanceOf(InvalidPgnError);
  });

  it("pgn() emits 7-tag-roster first then mainline", async () => {
    using chess = await Chess.create();
    chess.setHeader("White", "Alice");
    chess.setHeader("Black", "Bob");
    chess.setHeader("Event", "Test");
    chess.setHeader("Result", "*");
    chess.move("e4");
    chess.move("e5");
    chess.move("Nf3");
    const pgn = chess.pgn();
    expect(pgn).toMatch(/\[Event "Test"\]/);
    expect(pgn).toMatch(/\[White "Alice"\]/);
    expect(pgn).toMatch(/\[Black "Bob"\]/);
    expect(pgn).toMatch(/1\. e4 e5 2\. Nf3 \*/);
    expect(pgn.indexOf("Event")).toBeLessThan(pgn.indexOf("Result"));
  });

  it("pgn round-trips through loadPgn", async () => {
    const src = `[Event "Round-trip"]
[White "A"]
[Black "B"]
[Result "1-0"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 1-0
`;
    using c1 = await Chess.loadPgn(src);
    const emitted = c1.pgn();
    using c2 = await Chess.loadPgn(emitted);
    expect(c2.history()).toHaveLength(c1.history().length);
    expect(c2.fen()).toBe(c1.fen());
  });

  it("setHeader(undefined) deletes the header", async () => {
    using chess = await Chess.create();
    chess.setHeader("Foo", "bar");
    expect(chess.header("Foo")).toBe("bar");
    chess.setHeader("Foo", undefined);
    expect(chess.header("Foo")).toBeUndefined();
  });

  it("headers() returns every set header", async () => {
    using chess = await Chess.create();
    chess.setHeader("Event", "E");
    chess.setHeader("Site", "S");
    expect(chess.headers()).toEqual({ Event: "E", Site: "S" });
  });
});
