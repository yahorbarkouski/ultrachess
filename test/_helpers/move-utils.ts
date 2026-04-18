//! Tiny helpers for finding / inspecting moves in test fixtures. Kept
//! intentionally small — the `Chess` API surface is already the right
//! abstraction for most assertions.

import type { Chess } from "../../src/index.js";
import { type Move } from "../../src/index.js";

/** Find the packed move with the given SAN in `chess`. Throws if absent. */
export function findMoveBySan(chess: Chess, san: string): Move {
  for (const m of chess.legalMoves()) {
    if (chess.san(m) === san) return m;
  }
  throw new Error(`no legal move ${san} in position ${chess.fen()}`);
}
