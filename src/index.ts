//! Public entry for ultrachess.

export {
  type BoardSquare,
  Chess,
  DisposedError,
  IllegalMoveError,
  InvalidFenError,
  InvalidPgnError,
  type MoveInput,
  STARTING_FEN,
} from "./chess.js";
export {
  AbiVersionMismatchError,
  EXPECTED_ABI_VERSION,
  init,
  type UltrachessAbi,
} from "./loader.js";

export {
  Color,
  decodePiece,
  encodePiece,
  type Move,
  MoveKind,
  moveFrom,
  moveKind,
  movePromotion,
  moveTo,
  moveToUci,
  type Piece,
  PieceType,
  parseSquare,
  pieceChar,
  squareColor,
  squareName,
  type VerboseMove,
} from "./move.js";

import { init } from "./loader.js";

/** ABI round-trip smoke test. Not part of the user-facing API.
 *  @internal */
export async function abiCheck(a: number, b: number): Promise<number> {
  const m = await init();
  return m.ultrachess_abi_check(a >>> 0, b >>> 0) >>> 0;
}
