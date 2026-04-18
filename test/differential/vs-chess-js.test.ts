//! Differential fuzzer — ultrachess vs. chess.js.
//!
//! At every ply of a random game, both engines must agree on every
//! semantically-meaningful field (FEN, legal-move set as SAN, in-check,
//! mate/stale, draw + components). Any divergence is a correctness bug
//! somewhere — whichever side it is.
//!
//! ## Why ultrachess is the move source
//! We ask *ultrachess* for legal moves and feed them to both engines. Using
//! chess.js as the move source would couple the fuzzer's coverage to
//! chess.js's own correctness — any bug in chess.js's generator becomes a
//! blind spot. Asking ultrachess and having chess.js verify makes chess.js
//! a second-opinion oracle on every ply.
//!
//! ## Tiering
//! - Default (PR gate): `DEFAULT_GAMES` games × `MAX_PLIES` plies.
//! - `DIFF_FUZZ_GAMES=N`: override for nightly (10k) / pre-RC (100k) runs.
//!
//! ## Reproducibility
//! Seed is a literal. On failure the reporter prints seed, ply, FEN,
//! history, and both values for the diverging field — everything needed
//! to reproduce the position manually or under a debugger.

import { Chess as ChessJs } from "chess.js";
import { describe, expect, it } from "vitest";

import { Chess as UltraChess } from "../../src/inline.js";
import { Rng } from "../_helpers/rng.js";

// ---------------------------------------------------------------------------
// Tier configuration
// ---------------------------------------------------------------------------

const DEFAULT_GAMES = 200;
const MAX_PLIES = 150;
const HEAVY_COUNT = Number(process.env.DIFF_FUZZ_GAMES ?? 0);

// ---------------------------------------------------------------------------
// Comparison types + helpers
// ---------------------------------------------------------------------------

/**
 * First divergence observed while playing one game — captured with enough
 * context to reproduce and debug without re-running the fuzzer.
 */
interface Divergence {
  seed: number;
  ply: number;
  fenBefore: string;
  history: string[];
  field: string;
  ultra: unknown;
  chessJs: unknown;
}

/** Shallow equality good enough for the fields we compare (primitives +
 *  flat arrays of strings). */
function eq(a: unknown, b: unknown): boolean {
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
    return true;
  }
  return a === b;
}

/** Multi-line human-readable report for a single divergence. */
function formatDivergence(d: Divergence): string {
  const lines = [
    `seed=${d.seed} ply=${d.ply} field=${d.field}`,
    `  fen-before: ${d.fenBefore}`,
    `  ultra:      ${JSON.stringify(d.ultra)}`,
    `  chess.js:   ${JSON.stringify(d.chessJs)}`,
  ];
  if (d.history.length > 0) {
    lines.push(`  history:    ${d.history.join(" ")}`);
  }
  return lines.join("\n");
}

// ---------------------------------------------------------------------------
// One game
// ---------------------------------------------------------------------------

/**
 * Play one random game on both engines in lock-step. Returns `null` if the
 * two agree on every field up to termination (or `maxPlies`), otherwise
 * returns the first divergence.
 *
 * Move source is ultrachess: we ask it for legal moves, pick a random one,
 * and feed the SAN to both. If chess.js throws on that SAN, that counts
 * as a divergence (the two disagree on legality).
 */
function compareGame(seed: number, maxPlies: number): Divergence | null {
  using ultra = UltraChess.createSync();
  const chessJs = new ChessJs();
  const rng = new Rng(seed);
  const history: string[] = [];

  const div = (field: string, ultraVal: unknown, chessJsVal: unknown): Divergence => ({
    seed,
    ply: history.length,
    fenBefore: chessJs.fen(),
    history: [...history],
    field,
    ultra: ultraVal,
    chessJs: chessJsVal,
  });

  for (let ply = 0; ply < maxPlies; ply++) {
    // --- 1. FEN must match byte-for-byte before every move ----------------
    const ultraFen = ultra.fen();
    const chessJsFen = chessJs.fen();
    if (ultraFen !== chessJsFen) {
      return div("fen", ultraFen, chessJsFen);
    }

    // --- 2. Legal-move sets (SAN) must match ------------------------------
    const ultraMoves = [...ultra.moves()].sort();
    const chessJsMoves = [...chessJs.moves()].sort();
    if (!eq(ultraMoves, chessJsMoves)) {
      return div("legal_moves", ultraMoves, chessJsMoves);
    }

    // --- 3. State flags ----------------------------------------------------
    // Method name maps: ultra.inCheck() ↔ chessJs.isCheck().
    if (ultra.inCheck() !== chessJs.isCheck()) {
      return div("in_check", ultra.inCheck(), chessJs.isCheck());
    }
    if (ultra.isCheckmate() !== chessJs.isCheckmate()) {
      return div("is_checkmate", ultra.isCheckmate(), chessJs.isCheckmate());
    }
    if (ultra.isStalemate() !== chessJs.isStalemate()) {
      return div("is_stalemate", ultra.isStalemate(), chessJs.isStalemate());
    }
    if (ultra.isInsufficientMaterial() !== chessJs.isInsufficientMaterial()) {
      return div(
        "is_insufficient_material",
        ultra.isInsufficientMaterial(),
        chessJs.isInsufficientMaterial(),
      );
    }
    if (ultra.isThreefoldRepetition() !== chessJs.isThreefoldRepetition()) {
      return div(
        "is_threefold_repetition",
        ultra.isThreefoldRepetition(),
        chessJs.isThreefoldRepetition(),
      );
    }
    if (ultra.isDraw() !== chessJs.isDraw()) {
      return div("is_draw", ultra.isDraw(), chessJs.isDraw());
    }

    // --- 4. Terminal state — stop cleanly ---------------------------------
    if (ultraMoves.length === 0) {
      return null; // game over by mate/stale; both engines agreed on flags above
    }

    // --- 5. Play a random legal move on both ------------------------------
    const san = ultraMoves[rng.bounded(ultraMoves.length)]!;
    history.push(san);

    try {
      ultra.move(san);
    } catch (e) {
      // ultrachess generated this SAN but won't re-parse it — internal bug.
      return div("ultra_rejected_own_san", (e as Error).message, "(accepted)");
    }
    try {
      chessJs.move(san);
    } catch (e) {
      // chess.js rejected a SAN ultrachess listed as legal — legality diff.
      return div("chess_js_rejected_ultra_move", "(accepted)", (e as Error).message);
    }
  }

  // Hit the ply cap without diverging — success.
  return null;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("differential vs chess.js", () => {
  it(`${DEFAULT_GAMES} random games × ≤${MAX_PLIES} plies agree on every field`, {
    timeout: 120_000,
  }, () => {
    const failures: Divergence[] = [];
    for (let seed = 1; seed <= DEFAULT_GAMES; seed++) {
      const div = compareGame(seed, MAX_PLIES);
      if (div) {
        failures.push(div);
        // Collect a handful of distinct root causes, then bail — keeps
        // the error message readable without hiding systemic failures.
        if (failures.length >= 3) break;
      }
    }
    if (failures.length > 0) {
      const message =
        `found ${failures.length} divergence${failures.length === 1 ? "" : "s"} ` +
        `in the first ${DEFAULT_GAMES} games:\n\n` +
        failures.map(formatDivergence).join("\n\n");
      expect.fail(message);
    }
  });

  // Heavy tier — env-gated so it doesn't inflate the default test run.
  // Run with e.g. `DIFF_FUZZ_GAMES=10000 npx vitest run test/differential`.
  const heavy = HEAVY_COUNT > 0 ? it : it.skip;
  heavy(
    `${HEAVY_COUNT || 0} random games (env-gated; pre-RC gate = 100k)`,
    // Generous ceiling: 100k games × ~1ms/ply avg ≈ 2.5h. Allow 6h.
    { timeout: 6 * 60 * 60_000 },
    () => {
      for (let seed = 1; seed <= HEAVY_COUNT; seed++) {
        const div = compareGame(seed, MAX_PLIES);
        if (div) {
          expect.fail(formatDivergence(div));
        }
      }
    },
  );
});
