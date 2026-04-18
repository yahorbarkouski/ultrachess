//! PGN corpus replay test — the Phase 4 gate.
//!
//! 1. Embedded corpus: 8 classic master games covering the full feature
//!    surface (captures, promotions, O-O, O-O-O, long endgames, draws,
//!    resignation, 40+ ply games). Every game must parse and replay with
//!    zero illegal-move errors.
//!
//! 2. Optional external corpus: if the `ULTRACHESS_PGN_CORPUS` environment
//!    variable points to a file, parse and replay every game in it.
//!    Intended for running against a 4000-game TWIC/Lichess sample during
//!    release validation. Run with:
//!    `ULTRACHESS_PGN_CORPUS=/path/to/corpus.pgn cargo test --release \
//!    --test pgn_corpus -- --ignored external_corpus`

use std::fs;

use ultrachess_core::pgn::{parse_pgn_many, replay};

const EMBEDDED_PGN: &str = include_str!("fixtures/classics.pgn");

#[test]
fn embedded_corpus_parses_and_replays() {
    let games = parse_pgn_many(EMBEDDED_PGN).expect("corpus must parse");
    assert!(!games.is_empty(), "corpus must have games");
    for (i, game) in games.iter().enumerate() {
        let white = game.header("White").unwrap_or("?");
        let black = game.header("Black").unwrap_or("?");
        let moves = replay(game)
            .unwrap_or_else(|e| panic!("game {i} ({white} vs {black}) failed replay: {e}"));
        assert!(!moves.is_empty(), "game {i} has zero moves");
    }
    eprintln!("replayed {} classics successfully", games.len());
}

#[test]
fn embedded_corpus_has_expected_breadth() {
    // Spot-check: at least one game ends in checkmate, one ends in draw,
    // one uses castling, one uses promotion.
    let games = parse_pgn_many(EMBEDDED_PGN).unwrap();

    let has_mate = games
        .iter()
        .any(|g| g.mainline.iter().any(|n| n.san.ends_with('#')));
    let has_draw = games
        .iter()
        .any(|g| g.termination == ultrachess_core::pgn::Termination::Draw);
    let has_castle = games
        .iter()
        .any(|g| g.mainline.iter().any(|n| n.san.starts_with("O-O")));
    let has_promotion = games
        .iter()
        .any(|g| g.mainline.iter().any(|n| n.san.contains('=')));

    assert!(has_mate, "corpus must include a mating game");
    assert!(has_draw, "corpus must include a draw");
    assert!(has_castle, "corpus must include castling");
    assert!(has_promotion, "corpus must include promotion");
}

#[test]
#[ignore = "requires ULTRACHESS_PGN_CORPUS env var; run with --ignored"]
fn external_corpus() {
    let Ok(path) = std::env::var("ULTRACHESS_PGN_CORPUS") else {
        eprintln!("skip: ULTRACHESS_PGN_CORPUS not set");
        return;
    };
    let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let games = parse_pgn_many(&src).expect("corpus must parse");
    eprintln!("external corpus: {} games", games.len());
    let mut failures = Vec::new();
    for (i, game) in games.iter().enumerate() {
        if let Err(e) = replay(game) {
            let w = game.header("White").unwrap_or("?");
            let b = game.header("Black").unwrap_or("?");
            failures.push(format!("game {i} ({w} vs {b}): {e}"));
            if failures.len() >= 10 {
                break;
            }
        }
    }
    assert!(
        failures.is_empty(),
        "first {} replay failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
