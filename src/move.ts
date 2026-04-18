//! Branded numeric `Move` + decode helpers + verbose-move representation.
//!
//! Packed layout matches the Rust side (u16):
//!   bits 0..5  : from square
//!   bits 6..11 : to square
//!   bits 12..13: promotion piece (0=N, 1=B, 2=R, 3=Q) — only meaningful
//!                if `kind == Promotion`
//!   bits 14..15: kind (0=Normal, 1=Promotion, 2=EnPassant, 3=Castle)

export type Move = number & { readonly __brand: unique symbol };

export enum MoveKind {
  Normal = 0,
  Promotion = 1,
  EnPassant = 2,
  Castle = 3,
}

export enum PieceType {
  Pawn = 0,
  Knight = 1,
  Bishop = 2,
  Rook = 3,
  Queen = 4,
  King = 5,
}

export enum Color {
  White = 0,
  Black = 1,
}

// --- Packed-move decoders ---------------------------------------------------

export function moveFrom(m: Move): number {
  return (m as number) & 0x3f;
}

export function moveTo(m: Move): number {
  return ((m as number) >> 6) & 0x3f;
}

export function moveKind(m: Move): MoveKind {
  return (((m as number) >> 14) & 0x3) as MoveKind;
}

/** Promotion piece if `moveKind(m) === Promotion`; otherwise `Knight` (ignore). */
export function movePromotion(m: Move): PieceType {
  const bits = ((m as number) >> 12) & 0x3;
  // Rust: 0=N, 1=B, 2=R, 3=Q. TS PieceType: Knight=1..Queen=4.
  return (bits + 1) as PieceType;
}

// --- Square / piece helpers -------------------------------------------------

export function squareName(sq: number): string {
  const file = String.fromCharCode(97 + (sq & 7));
  const rank = String.fromCharCode(49 + (sq >> 3));
  return `${file}${rank}`;
}

export function parseSquare(name: string): number | null {
  if (name.length !== 2) return null;
  const f = name.charCodeAt(0) - 97;
  const r = name.charCodeAt(1) - 49;
  if (f < 0 || f > 7 || r < 0 || r > 7) return null;
  return r * 8 + f;
}

/** UCI representation: e.g. "e2e4", "e7e8q". */
export function moveToUci(m: Move): string {
  const from = squareName(moveFrom(m));
  const to = squareName(moveTo(m));
  if (moveKind(m) === MoveKind.Promotion) {
    const promoChar = ["n", "b", "r", "q"][((m as number) >> 12) & 0x3] ?? "?";
    return `${from}${to}${promoChar}`;
  }
  return `${from}${to}`;
}

/** Rust encoding: `(color << 3) | pieceType`; 255 = empty. */
export interface Piece {
  color: Color;
  type: PieceType;
}

export function decodePiece(code: number): Piece | null {
  if (code === 255) return null;
  return {
    color: ((code >> 3) & 1) as Color,
    type: (code & 0b111) as PieceType,
  };
}

export function encodePiece(piece: Piece): number {
  return (piece.color << 3) | (piece.type & 0b111);
}

export function pieceChar(p: Piece): string {
  const letters = "pnbrqk";
  const ch = letters[p.type] ?? "?";
  return p.color === Color.White ? ch.toUpperCase() : ch;
}

// --- Verbose move -----------------------------------------------------------

/** Human-readable, self-descriptive move — built on demand from a packed
 *  `Move`. Never allocated inside hot-path move generation. */
export interface VerboseMove {
  /** Source square (0..63, LERF: 0 = a1, 63 = h8). */
  fromIndex: number;
  /** Target square (same encoding as `fromIndex`). */
  toIndex: number;
  /** Source square in algebraic notation, e.g. `"e2"`. */
  from: string;
  /** Target square in algebraic notation. */
  to: string;
  /** Moving piece type. */
  piece: PieceType;
  /** Moving piece color. */
  color: Color;
  /** Captured piece type (if any — EP removes an enemy pawn). */
  captured?: PieceType;
  /** Promoted-to piece type (pawn promotions only). */
  promotion?: PieceType;
  /** Move classification: normal / promotion / en-passant / castling. */
  kind: MoveKind;
  /** Standard Algebraic Notation for the move. */
  san: string;
  /** Long algebraic / UCI representation: `"e2e4"`, `"e7e8q"`. */
  uci: string;
}
