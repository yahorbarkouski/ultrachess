//! Coverage for `src/move.ts` — the packed-move decoders and helpers.

import { describe, expect, it } from "vitest";
import {
  Color,
  MoveKind,
  PieceType,
  decodePiece,
  encodePiece,
  moveFrom,
  moveKind,
  movePromotion,
  moveTo,
  moveToUci,
  parseSquare,
  pieceChar,
  squareName,
  type Move,
} from "../src/move.js";

describe("move — square helpers", () => {
  it("squareName maps every index to the expected algebraic string", () => {
    expect(squareName(0)).toBe("a1");
    expect(squareName(7)).toBe("h1");
    expect(squareName(8)).toBe("a2");
    expect(squareName(63)).toBe("h8");
  });

  it("parseSquare is the inverse of squareName", () => {
    for (let i = 0; i < 64; i++) {
      expect(parseSquare(squareName(i))).toBe(i);
    }
  });

  it("parseSquare rejects malformed strings", () => {
    expect(parseSquare("")).toBeNull();
    expect(parseSquare("a")).toBeNull();
    expect(parseSquare("i1")).toBeNull();
    expect(parseSquare("a9")).toBeNull();
    expect(parseSquare("a1x")).toBeNull();
  });
});

describe("move — packed Move decoders", () => {
  // Hand-build packed values matching the Rust layout.
  const pack = (
    from: number,
    to: number,
    kind: MoveKind,
    promoBits = 0,
  ): Move => ((from | (to << 6) | (promoBits << 12) | (kind << 14)) as Move);

  it("moveFrom / moveTo / moveKind decode a normal move", () => {
    const m = pack(12, 28, MoveKind.Normal);
    expect(moveFrom(m)).toBe(12);
    expect(moveTo(m)).toBe(28);
    expect(moveKind(m)).toBe(MoveKind.Normal);
  });

  it("movePromotion translates Rust bits to TS PieceType", () => {
    // Rust encoding: 0=N, 1=B, 2=R, 3=Q.
    expect(movePromotion(pack(52, 60, MoveKind.Promotion, 0))).toBe(PieceType.Knight);
    expect(movePromotion(pack(52, 60, MoveKind.Promotion, 1))).toBe(PieceType.Bishop);
    expect(movePromotion(pack(52, 60, MoveKind.Promotion, 2))).toBe(PieceType.Rook);
    expect(movePromotion(pack(52, 60, MoveKind.Promotion, 3))).toBe(PieceType.Queen);
  });

  it("moveToUci emits 4 chars for quiet moves, 5 for promotions", () => {
    expect(moveToUci(pack(12, 28, MoveKind.Normal))).toBe("e2e4");
    expect(moveToUci(pack(52, 60, MoveKind.Promotion, 3))).toBe("e7e8q");
    expect(moveToUci(pack(52, 60, MoveKind.Promotion, 0))).toBe("e7e8n");
  });

  it("kinds for en-passant and castling decode correctly", () => {
    const ep = pack(36, 45, MoveKind.EnPassant);
    expect(moveKind(ep)).toBe(MoveKind.EnPassant);
    const castle = pack(4, 6, MoveKind.Castle);
    expect(moveKind(castle)).toBe(MoveKind.Castle);
  });
});

describe("move — piece helpers", () => {
  it("decodePiece returns null for the 255 sentinel", () => {
    expect(decodePiece(255)).toBeNull();
  });

  it("decodePiece / encodePiece round-trip", () => {
    for (const color of [Color.White, Color.Black]) {
      for (const type of [
        PieceType.Pawn,
        PieceType.Knight,
        PieceType.Bishop,
        PieceType.Rook,
        PieceType.Queen,
        PieceType.King,
      ]) {
        const code = encodePiece({ color, type });
        expect(decodePiece(code)).toEqual({ color, type });
      }
    }
  });

  it("pieceChar emits upper for white, lower for black", () => {
    expect(pieceChar({ color: Color.White, type: PieceType.Pawn })).toBe("P");
    expect(pieceChar({ color: Color.Black, type: PieceType.Queen })).toBe("q");
    expect(pieceChar({ color: Color.White, type: PieceType.King })).toBe("K");
  });
});
