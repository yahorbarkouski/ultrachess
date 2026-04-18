//! Tests for chess.js-compat additions: board(), reset(), load(),
//! move({from,to,promotion}), moves({square,piece}), history({before,after}),
//! pgn({newline,maxWidth}), moveNumber(), squareColor(), position-keyed
//! comments, setHeaders().

import { describe, expect, it } from "vitest";
import {
  Chess,
  Color,
  IllegalMoveError,
  InvalidFenError,
  PieceType,
  squareColor,
} from "../src/index.js";
import { KIWIPETE, SCHOLARS_MATE, STARTING_FEN } from "./_helpers/positions.js";

describe("board()", () => {
  it("returns 8 rows × 8 cols, rank 8 first", async () => {
    using chess = await Chess.create();
    const b = chess.board();
    expect(b).toHaveLength(8);
    for (const row of b) expect(row).toHaveLength(8);
    // a8 is the top-left cell → row 0, col 0.
    expect(b[0]![0]).toEqual({
      square: "a8",
      index: 56,
      color: Color.Black,
      type: PieceType.Rook,
    });
    // h1 is the bottom-right → row 7, col 7.
    expect(b[7]![7]).toEqual({
      square: "h1",
      index: 7,
      color: Color.White,
      type: PieceType.Rook,
    });
  });

  it("empty squares are null", async () => {
    using chess = await Chess.create();
    const b = chess.board();
    // Ranks 4 and 5 are empty on the startpos.
    for (let col = 0; col < 8; col++) {
      expect(b[3]![col]).toBeNull();
      expect(b[4]![col]).toBeNull();
    }
  });

  it("reflects moves", async () => {
    using chess = await Chess.create();
    chess.move("e4");
    const b = chess.board();
    // e4 → file=4, rank=3 → row = 7-3 = 4, col = 4.
    expect(b[4]![4]).toEqual({
      square: "e4",
      index: 28,
      color: Color.White,
      type: PieceType.Pawn,
    });
    // e2 is now empty.
    expect(b[6]![4]).toBeNull();
  });

  it("matches pieceAt() for every square", async () => {
    using chess = await Chess.create(KIWIPETE);
    const b = chess.board();
    for (let idx = 0; idx < 64; idx++) {
      const row = 7 - (idx >> 3);
      const col = idx & 7;
      const cell = b[row]![col];
      const piece = chess.pieceAt(idx);
      if (piece === null) {
        expect(cell).toBeNull();
      } else {
        expect(cell).not.toBeNull();
        expect(cell!.color).toBe(piece.color);
        expect(cell!.type).toBe(piece.type);
        expect(cell!.index).toBe(idx);
      }
    }
  });
});

describe("reset() / load()", () => {
  it("reset() returns to startpos and clears history + headers", async () => {
    using chess = await Chess.create();
    chess.move("e4");
    chess.move("e5");
    chess.setHeader("White", "Alice");
    chess.reset();
    expect(chess.fen()).toBe(STARTING_FEN);
    expect(chess.history()).toHaveLength(0);
    expect(chess.header("White")).toBeUndefined();
  });

  it("load(fen) swaps the position and clears history", async () => {
    using chess = await Chess.create();
    chess.move("e4");
    chess.load(KIWIPETE);
    expect(chess.fen()).toBe(KIWIPETE);
    expect(chess.history()).toHaveLength(0);
  });

  it("load(fen) with an invalid FEN throws and preserves the position", async () => {
    using chess = await Chess.create();
    chess.move("e4");
    const before = chess.fen();
    expect(() => chess.load("not a fen")).toThrow(InvalidFenError);
    expect(chess.fen()).toBe(before);
    expect(chess.history()).toHaveLength(1);
  });

  it("after reset() a fresh game plays correctly", async () => {
    using chess = await Chess.create(KIWIPETE);
    chess.reset();
    chess.move("e4");
    chess.move("e5");
    chess.move("Nf3");
    expect(chess.history()).toHaveLength(3);
    // Undo must still work after reset + moves.
    expect(chess.undo()).not.toBeNull();
    expect(chess.history()).toHaveLength(2);
  });
});

describe("move({ from, to, promotion })", () => {
  it("plays a normal move from the object form", async () => {
    using chess = await Chess.create();
    const m = chess.move({ from: "e2", to: "e4" });
    expect(chess.history()).toEqual([m]);
    expect(chess.pieceAt("e4")).toEqual({ color: Color.White, type: PieceType.Pawn });
  });

  it("accepts numeric indices", async () => {
    using chess = await Chess.create();
    // e2 = idx 12, e4 = idx 28.
    chess.move({ from: 12, to: 28 });
    expect(chess.pieceAt("e4")).not.toBeNull();
  });

  it("plays castling via the object form", async () => {
    using chess = await Chess.create("r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1");
    chess.move({ from: "e1", to: "g1" });
    expect(chess.pieceAt("g1")).toEqual({ color: Color.White, type: PieceType.King });
    expect(chess.pieceAt("f1")).toEqual({ color: Color.White, type: PieceType.Rook });
  });

  it("plays en-passant via the object form", async () => {
    using chess = await Chess.create(
      "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3",
    );
    chess.move({ from: "e5", to: "f6" });
    expect(chess.pieceAt("f6")).toEqual({ color: Color.White, type: PieceType.Pawn });
    expect(chess.pieceAt("f5")).toBeNull();
  });

  it("promotion requires the promotion field", async () => {
    using chess = await Chess.create("8/P7/8/8/8/8/4k3/7K w - - 0 1");
    expect(() => chess.move({ from: "a7", to: "a8" })).toThrow(IllegalMoveError);
    const m = chess.move({ from: "a7", to: "a8", promotion: PieceType.Queen });
    expect(chess.pieceAt("a8")).toEqual({ color: Color.White, type: PieceType.Queen });
    expect(m).toBeTypeOf("number");
  });

  it("throws on a non-legal move", async () => {
    using chess = await Chess.create();
    expect(() => chess.move({ from: "e2", to: "e5" })).toThrow(IllegalMoveError);
  });

  it("throws on invalid from/to", async () => {
    using chess = await Chess.create();
    expect(() => chess.move({ from: "zz", to: "e4" })).toThrow(IllegalMoveError);
    expect(() => chess.move({ from: "e2", to: "z9" })).toThrow(IllegalMoveError);
  });

  it("moveFromInput() resolves without playing", async () => {
    using chess = await Chess.create();
    const m = chess.moveFromInput({ from: "e2", to: "e4" });
    expect(chess.history()).toHaveLength(0);
    chess.move(m);
    expect(chess.history()).toHaveLength(1);
  });
});

describe("moves({ square, piece }) filters", () => {
  it("filters by source square", async () => {
    using chess = await Chess.create();
    const e2Moves = chess.moves({ square: "e2" });
    // Pawn on e2 has exactly two legal moves: e3 and e4.
    expect(new Set(e2Moves)).toEqual(new Set(["e3", "e4"]));
  });

  it("filters by piece type", async () => {
    using chess = await Chess.create();
    const knightMoves = chess.moves({ piece: PieceType.Knight });
    // Startpos white knights: Na3, Nc3, Nf3, Nh3 (4 moves).
    expect(knightMoves).toHaveLength(4);
    expect(new Set(knightMoves)).toEqual(new Set(["Na3", "Nc3", "Nf3", "Nh3"]));
  });

  it("combines square + piece filter with verbose", async () => {
    using chess = await Chess.create();
    const v = chess.moves({ verbose: true, square: "e2", piece: PieceType.Pawn });
    expect(v).toHaveLength(2);
    for (const m of v) {
      expect(m.from).toBe("e2");
      expect(m.piece).toBe(PieceType.Pawn);
    }
  });

  it("returns empty for a square with no legal moves", async () => {
    using chess = await Chess.create();
    expect(chess.moves({ square: "a3" })).toEqual([]);
  });

  it("returns empty for an out-of-range square", async () => {
    using chess = await Chess.create();
    expect(chess.moves({ square: "zz" })).toEqual([]);
  });

  it("raw+filter returns packed moves", async () => {
    using chess = await Chess.create();
    const raw = chess.moves({ raw: true, square: "e2" });
    expect(raw).toHaveLength(2);
    for (const m of raw) expect(typeof m).toBe("number");
  });
});

describe("history({ verbose, before, after })", () => {
  it("populates before FEN for each move", async () => {
    using chess = await Chess.create();
    chess.move("e4");
    chess.move("e5");
    const h = chess.history({ verbose: true, before: true });
    expect(h).toHaveLength(2);
    expect(h[0]!.before).toBe(STARTING_FEN);
    // Before Black's reply, the pawn is on e4 (`4P3` in the 4th rank field).
    expect(h[1]!.before).toMatch(/\/4P3\//);
  });

  it("populates after FEN for each move", async () => {
    using chess = await Chess.create();
    chess.move("e4");
    chess.move("e5");
    const h = chess.history({ verbose: true, after: true });
    expect(h[1]!.after).toBe(chess.fen());
  });

  it("both before and after", async () => {
    using chess = await Chess.create();
    chess.move("e4");
    const [{ before, after }] = chess.history({ verbose: true, before: true, after: true });
    expect(before).toBe(STARTING_FEN);
    expect(after).toBe(chess.fen());
    expect(after).toMatch(/\/4P3\//);
  });

  it("verbose without before/after leaves the fields undefined (no extra cost)", async () => {
    using chess = await Chess.create();
    chess.move("e4");
    const [m] = chess.history({ verbose: true });
    expect(m!.before).toBeUndefined();
    expect(m!.after).toBeUndefined();
  });
});

describe("pgn({ newline, maxWidth })", () => {
  it("honours custom newline", async () => {
    using chess = await Chess.create();
    chess.setHeader("Event", "Test");
    chess.move("e4");
    chess.move("e5");
    const pgn = chess.pgn({ newline: "\r\n" });
    expect(pgn).toBe(`[Event "Test"]\r\n\r\n1. e4 e5 *\r\n`);
    // No bare LF should appear anywhere except as the second char of CRLF.
    for (let i = 0; i < pgn.length; i++) {
      if (pgn[i] === "\n") {
        expect(pgn[i - 1]).toBe("\r");
      }
    }
  });

  it("wraps at the requested width", async () => {
    using chess = await Chess.create();
    // Play a handful of moves; default wrap is 80 — force a shorter wrap.
    chess.move("e4");
    chess.move("e5");
    chess.move("Nf3");
    chess.move("Nc6");
    chess.move("Bb5");
    chess.move("a6");
    const pgn = chess.pgn({ maxWidth: 20 });
    // Movetext block (everything after the blank line, then split on \n).
    const lines = pgn.split("\n").filter((l) => l && !l.startsWith("["));
    for (const line of lines) {
      expect(line.length).toBeLessThanOrEqual(20);
    }
  });

  it("maxWidth=0 puts everything on one movetext line", async () => {
    using chess = await Chess.create();
    for (const san of ["e4", "e5", "Nf3", "Nc6", "Bb5", "a6"]) chess.move(san);
    const pgn = chess.pgn({ maxWidth: 0 });
    const nonHeaderLines = pgn.split("\n").filter((l) => l.length > 0 && !l.startsWith("["));
    expect(nonHeaderLines).toHaveLength(1);
    expect(nonHeaderLines[0]).toMatch(/1\. e4 e5 2\. Nf3 Nc6 3\. Bb5 a6 \*/);
  });
});

describe("moveNumber()", () => {
  it("equals fullmove()", async () => {
    using chess = await Chess.create();
    expect(chess.moveNumber()).toBe(chess.fullmove());
    chess.move("e4");
    chess.move("e5");
    expect(chess.moveNumber()).toBe(chess.fullmove());
    expect(chess.moveNumber()).toBe(2);
  });
});

describe("squareColor()", () => {
  it("a1 is dark, h1 is light", () => {
    expect(squareColor("a1")).toBe("dark");
    expect(squareColor("h1")).toBe("light");
  });
  it("a8 is light, h8 is dark", () => {
    expect(squareColor("a8")).toBe("light");
    expect(squareColor("h8")).toBe("dark");
  });
  it("accepts numeric indices", () => {
    expect(squareColor(0)).toBe("dark"); // a1
    expect(squareColor(63)).toBe("dark"); // h8
  });
  it("returns null on invalid input", () => {
    expect(squareColor("zz")).toBeNull();
    expect(squareColor(-1)).toBeNull();
    expect(squareColor(64)).toBeNull();
  });
});

describe("position-keyed comments", () => {
  it("setComment/getComment round-trip at a single position", async () => {
    using chess = await Chess.create();
    expect(chess.getComment()).toBeUndefined();
    chess.setComment("startpos");
    expect(chess.getComment()).toBe("startpos");
    chess.setComment(undefined);
    expect(chess.getComment()).toBeUndefined();
  });

  it("comments are keyed by position (not by ply)", async () => {
    using chess = await Chess.create();
    chess.setComment("start");
    chess.move("e4");
    chess.setComment("after e4");
    expect(chess.getComment()).toBe("after e4");
    chess.undo();
    expect(chess.getComment()).toBe("start");
  });

  it("getComments() walks the reached positions and emits in order", async () => {
    using chess = await Chess.create();
    chess.setComment("start");
    chess.move("e4");
    chess.setComment("after e4");
    chess.move("e5");
    chess.setComment("after e5");
    const all = chess.getComments();
    expect(all).toHaveLength(3);
    expect(all[0]!.comment).toBe("start");
    expect(all[1]!.comment).toBe("after e4");
    expect(all[2]!.comment).toBe("after e5");
    // FENs are real.
    for (const entry of all) expect(entry.fen).toMatch(/ w | b /);
  });

  it("removeComment() clears only the current position's comment", async () => {
    using chess = await Chess.create();
    chess.setComment("start");
    chess.move("e4");
    chess.setComment("after e4");
    chess.undo();
    expect(chess.removeComment()).toBe("start");
    chess.move("e4");
    expect(chess.getComment()).toBe("after e4");
  });

  it("removeComments() clears everything", async () => {
    using chess = await Chess.create();
    chess.setComment("x");
    chess.move("e4");
    chess.setComment("y");
    chess.removeComments();
    expect(chess.getComments()).toEqual([]);
  });

  it("comments survive clone()", async () => {
    using chess = await Chess.create();
    chess.setComment("shared");
    chess.move("e4");
    chess.setComment("after");
    const copy = chess.clone();
    try {
      // Clone is at the same current position; the comment-at-current should match.
      expect(copy.getComment()).toBe("after");
    } finally {
      copy.dispose();
    }
  });

  it("load() and reset() wipe comments", async () => {
    using chess = await Chess.create();
    chess.setComment("x");
    chess.move("e4");
    chess.setComment("y");
    chess.reset();
    expect(chess.getComments()).toEqual([]);
    chess.setComment("fresh");
    chess.load(KIWIPETE);
    expect(chess.getComments()).toEqual([]);
  });
});

describe("setHeaders()", () => {
  it("sets multiple headers from a record", async () => {
    using chess = await Chess.create();
    chess.setHeaders({ White: "Alice", Black: "Bob", Event: "Derby" });
    expect(chess.header("White")).toBe("Alice");
    expect(chess.header("Black")).toBe("Bob");
    expect(chess.header("Event")).toBe("Derby");
  });

  it("undefined values delete", async () => {
    using chess = await Chess.create();
    chess.setHeader("White", "Alice");
    chess.setHeaders({ White: undefined, Site: "Somewhere" });
    expect(chess.header("White")).toBeUndefined();
    expect(chess.header("Site")).toBe("Somewhere");
  });
});

describe("SCHOLARS_MATE parity — verbose history with before/after", () => {
  it("each move's before/after FEN matches reality", async () => {
    using chess = await Chess.create();
    for (const san of ["e4", "e5", "Bc4", "Nc6", "Qh5", "Nf6", "Qxf7#"]) {
      chess.move(san);
    }
    expect(chess.fen()).toBe(SCHOLARS_MATE);
    const h = chess.history({ verbose: true, before: true, after: true });
    expect(h).toHaveLength(7);
    expect(h[0]!.before).toBe(STARTING_FEN);
    expect(h[h.length - 1]!.after).toBe(SCHOLARS_MATE);
    // Each step's `after` equals the next step's `before`.
    for (let i = 0; i < h.length - 1; i++) {
      expect(h[i]!.after).toBe(h[i + 1]!.before);
    }
  });
});
