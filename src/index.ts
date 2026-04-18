//! Public entry for ultrachessjs.

export {
  init,
  AbiVersionMismatchError,
  EXPECTED_ABI_VERSION,
  type UltrachessAbi,
} from "./loader.js";

export {
  Chess,
  STARTING_FEN,
  DisposedError,
  IllegalMoveError,
  InvalidFenError,
  InvalidPgnError,
} from "./chess.js";

export {
  Color,
  MoveKind,
  PieceType,
  type Move,
  type Piece,
  type VerboseMove,
  moveFrom,
  moveTo,
  moveKind,
  movePromotion,
  moveToUci,
  squareName,
  parseSquare,
  decodePiece,
  encodePiece,
  pieceChar,
} from "./move.js";

import { init } from "./loader.js";

/** Legacy Phase 0 smoke helper — still useful for health checks. */
export async function abiCheck(a: number, b: number): Promise<number> {
  const m = await init();
  return m.ultrachess_abi_check(a >>> 0, b >>> 0) >>> 0;
}
