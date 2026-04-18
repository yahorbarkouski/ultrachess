//! PGN (Portable Game Notation) parsing and writing.
//!
//! Supports the full PGN spec: Seven-Tag-Roster headers, supplemental tags,
//! main-line SAN, nested variations (parentheses), comments (`{...}` block
//! and `;...\n` line), NAGs (`$N`), and termination markers
//! (`1-0 / 0-1 / 1/2-1/2 / *`).
//!
//! The AST is **syntactic** — SAN moves are stored as strings. Resolving
//! them to `Move` values is done by `replay()`, which walks the tree against
//! a `Position`. This keeps the parser fast and lets consumers choose
//! whether to validate immediately or lazily.

use crate::position::Position;
use crate::san::{san_to_move, SanError};
use core::fmt;

// --------------------------------------------------------------------------
// AST
// --------------------------------------------------------------------------

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Termination {
    WhiteWon,
    BlackWon,
    Draw,
    #[default]
    Unknown,
}

impl Termination {
    pub fn as_str(self) -> &'static str {
        match self {
            Termination::WhiteWon => "1-0",
            Termination::BlackWon => "0-1",
            Termination::Draw => "1/2-1/2",
            Termination::Unknown => "*",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MoveNode {
    /// SAN text as it appears in the PGN source (e.g. "Nxe4+").
    pub san: String,
    /// Comment attached BEFORE this move (rare; usually before move 1).
    pub comment_before: Option<String>,
    /// Comment attached AFTER this move (most common).
    pub comment_after: Option<String>,
    /// Numeric Annotation Glyphs.
    pub nags: Vec<u8>,
    /// Nested variations at this move (each is its own move sequence).
    pub variations: Vec<Vec<MoveNode>>,
}

#[derive(Clone, Debug, Default)]
pub struct Game {
    pub headers: Vec<(String, String)>,
    pub mainline: Vec<MoveNode>,
    pub termination: Termination,
}

impl Game {
    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find_map(|(k, v)| if k == key { Some(v.as_str()) } else { None })
    }

    pub fn set_header(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        if let Some(entry) = self.headers.iter_mut().find(|(k, _)| k == &key) {
            entry.1 = value;
        } else {
            self.headers.push((key, value));
        }
    }
}

// --------------------------------------------------------------------------
// Errors
// --------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PgnError {
    Syntax(String),
    UnclosedComment,
    UnclosedHeader,
    UnclosedVariation,
    MoveError {
        san: String,
        ply: usize,
        reason: String,
    },
}

impl fmt::Display for PgnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PgnError::Syntax(s) => write!(f, "PGN syntax error: {s}"),
            PgnError::UnclosedComment => f.write_str("unclosed comment `{` without `}`"),
            PgnError::UnclosedHeader => f.write_str("unclosed header `[` without `]`"),
            PgnError::UnclosedVariation => f.write_str("unclosed variation `(` without `)`"),
            PgnError::MoveError { san, ply, reason } => {
                write!(f, "illegal SAN {san:?} at ply {ply}: {reason}")
            }
        }
    }
}

// --------------------------------------------------------------------------
// Parser
// --------------------------------------------------------------------------

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            s: s.as_bytes(),
            i: 0,
        }
    }

    fn eof(&self) -> bool {
        self.i >= self.s.len()
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn skip_whitespace_and_line_comments(&mut self) {
        loop {
            match self.peek() {
                Some(b) if b.is_ascii_whitespace() => {
                    self.i += 1;
                }
                Some(b';') => {
                    // Line comment up to newline.
                    while let Some(c) = self.peek() {
                        self.i += 1;
                        if c == b'\n' {
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
    }

    fn parse_headers(&mut self) -> Result<Vec<(String, String)>, PgnError> {
        let mut headers = Vec::new();
        loop {
            self.skip_whitespace_and_line_comments();
            if self.peek() != Some(b'[') {
                break;
            }
            self.i += 1; // consume '['
                         // Key (alphanumeric).
            let key_start = self.i;
            while let Some(c) = self.peek() {
                if c.is_ascii_alphanumeric() || c == b'_' {
                    self.i += 1;
                } else {
                    break;
                }
            }
            let key = core::str::from_utf8(&self.s[key_start..self.i])
                .map_err(|_| PgnError::Syntax("non-UTF8 header key".into()))?
                .to_string();
            // Whitespace, then '"value"'.
            self.skip_whitespace_and_line_comments();
            if self.peek() != Some(b'"') {
                return Err(PgnError::Syntax(format!(
                    "expected quoted value after header key {key:?}"
                )));
            }
            self.i += 1; // consume '"'
            let value_start = self.i;
            while let Some(c) = self.peek() {
                if c == b'\\' {
                    // Escape — skip next char.
                    self.i += 2;
                } else if c == b'"' {
                    break;
                } else {
                    self.i += 1;
                }
            }
            if self.peek() != Some(b'"') {
                return Err(PgnError::UnclosedHeader);
            }
            let value = core::str::from_utf8(&self.s[value_start..self.i])
                .map_err(|_| PgnError::Syntax("non-UTF8 header value".into()))?
                .to_string();
            self.i += 1; // consume closing '"'
            self.skip_whitespace_and_line_comments();
            if self.peek() != Some(b']') {
                return Err(PgnError::UnclosedHeader);
            }
            self.i += 1; // consume ']'
            headers.push((key, value));
        }
        Ok(headers)
    }

    /// Parse movetext until a termination marker or an unmatched ')'.
    /// Returns (nodes, Option<Termination>).
    fn parse_movetext(
        &mut self,
        inside_variation: bool,
    ) -> Result<(Vec<MoveNode>, Option<Termination>), PgnError> {
        let mut nodes: Vec<MoveNode> = Vec::new();
        let mut pending_comment: Option<String> = None;
        let mut termination: Option<Termination> = None;

        loop {
            self.skip_whitespace_and_line_comments();
            let Some(c) = self.peek() else { break };

            match c {
                b'{' => {
                    let comment = self.parse_brace_comment()?;
                    if let Some(last) = nodes.last_mut() {
                        match &mut last.comment_after {
                            Some(prev) => {
                                prev.push(' ');
                                prev.push_str(&comment);
                            }
                            None => last.comment_after = Some(comment),
                        }
                    } else {
                        pending_comment = Some(match pending_comment {
                            Some(mut prev) => {
                                prev.push(' ');
                                prev.push_str(&comment);
                                prev
                            }
                            None => comment,
                        });
                    }
                }
                b'(' => {
                    self.i += 1;
                    let (var, _) = self.parse_movetext(true)?;
                    if let Some(last) = nodes.last_mut() {
                        last.variations.push(var);
                    } else {
                        return Err(PgnError::Syntax("variation `(` before any move".into()));
                    }
                }
                b')' => {
                    if !inside_variation {
                        return Err(PgnError::Syntax("unmatched `)`".into()));
                    }
                    self.i += 1;
                    return Ok((nodes, termination));
                }
                b'$' => {
                    self.i += 1;
                    let num = self.parse_unsigned::<u32>()?;
                    let nag = if num > 255 { 0 } else { num as u8 };
                    if let Some(last) = nodes.last_mut() {
                        last.nags.push(nag);
                    }
                }
                _ => {
                    // A "word" token: SAN, move number, or termination.
                    let tok = self.parse_token();
                    if tok.is_empty() {
                        continue;
                    }
                    if let Some(t) = as_termination(&tok) {
                        termination = Some(t);
                        break;
                    }
                    if is_move_number(&tok) {
                        continue; // move numbers are purely decorative
                    }
                    // SAN move. Strip leading move-number artefacts if fused
                    // (e.g. "1.e4" or "1...e5"): we already split on whitespace
                    // so these are rare, but be tolerant.
                    let san = strip_fused_number(&tok);
                    if san.is_empty() {
                        continue;
                    }
                    let node = MoveNode {
                        san: san.to_string(),
                        comment_before: pending_comment.take(),
                        ..MoveNode::default()
                    };
                    nodes.push(node);
                }
            }
        }

        if inside_variation {
            return Err(PgnError::UnclosedVariation);
        }
        Ok((nodes, termination))
    }

    fn parse_brace_comment(&mut self) -> Result<String, PgnError> {
        debug_assert_eq!(self.peek(), Some(b'{'));
        self.i += 1;
        let start = self.i;
        while let Some(c) = self.peek() {
            if c == b'}' {
                break;
            }
            self.i += 1;
        }
        if self.peek() != Some(b'}') {
            return Err(PgnError::UnclosedComment);
        }
        let body = core::str::from_utf8(&self.s[start..self.i])
            .map_err(|_| PgnError::Syntax("non-UTF8 comment".into()))?
            .trim()
            .to_string();
        self.i += 1; // consume '}'
        Ok(body)
    }

    fn parse_unsigned<T: core::str::FromStr>(&mut self) -> Result<T, PgnError> {
        let start = self.i;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.i += 1;
            } else {
                break;
            }
        }
        let slice = core::str::from_utf8(&self.s[start..self.i])
            .map_err(|_| PgnError::Syntax("non-UTF8 number".into()))?;
        slice
            .parse::<T>()
            .map_err(|_| PgnError::Syntax(format!("invalid number {slice:?}")))
    }

    fn parse_token(&mut self) -> String {
        let start = self.i;
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() || c == b'{' || c == b'(' || c == b')' || c == b';' {
                break;
            }
            self.i += 1;
        }
        core::str::from_utf8(&self.s[start..self.i])
            .unwrap_or("")
            .to_string()
    }
}

fn as_termination(tok: &str) -> Option<Termination> {
    match tok {
        "1-0" => Some(Termination::WhiteWon),
        "0-1" => Some(Termination::BlackWon),
        "1/2-1/2" => Some(Termination::Draw),
        "*" => Some(Termination::Unknown),
        _ => None,
    }
}

fn is_move_number(tok: &str) -> bool {
    // "1.", "25.", "1...", "1. ..."  — digits then one or more '.'
    let mut bytes = tok.as_bytes().iter();
    let mut saw_digit = false;
    for &b in bytes.by_ref() {
        if b.is_ascii_digit() {
            saw_digit = true;
        } else {
            // first non-digit byte; must be '.' and the rest must be '.'
            if !saw_digit || b != b'.' {
                return false;
            }
            return bytes.all(|&b| b == b'.');
        }
    }
    // Pure digit token — ambiguous. Could be an UCI piece, but in PGN context
    // treating bare numbers as move numbers is wrong. Require a dot suffix.
    false
}

fn strip_fused_number(tok: &str) -> &str {
    // Accept "1.e4" or "1...e5" as SAN "e4"/"e5". Find the last occurrence
    // of "." and trim through it.
    if let Some(dot_end) = tok.rfind('.') {
        // All chars up to dot_end must be digits or dots.
        if tok[..dot_end]
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.')
        {
            return &tok[dot_end + 1..];
        }
    }
    tok
}

// --------------------------------------------------------------------------
// Public entry points
// --------------------------------------------------------------------------

/// Parse a PGN string into a `Game` AST. Does NOT validate SAN moves against
/// board legality — use `replay()` for that.
pub fn parse_pgn(s: &str) -> Result<Game, PgnError> {
    let mut p = Parser::new(s);
    let headers = p.parse_headers()?;
    let (mainline, termination) = p.parse_movetext(false)?;
    Ok(Game {
        headers,
        mainline,
        termination: termination.unwrap_or(Termination::Unknown),
    })
}

/// Parse a PGN that may contain multiple games separated by blank lines or
/// two consecutive newlines. Returns every game found.
pub fn parse_pgn_many(s: &str) -> Result<Vec<Game>, PgnError> {
    let mut out = Vec::new();
    let mut p = Parser::new(s);
    loop {
        p.skip_whitespace_and_line_comments();
        if p.eof() {
            break;
        }
        let headers = p.parse_headers()?;
        let (mainline, termination) = p.parse_movetext(false)?;
        if headers.is_empty() && mainline.is_empty() {
            break;
        }
        out.push(Game {
            headers,
            mainline,
            termination: termination.unwrap_or(Termination::Unknown),
        });
    }
    Ok(out)
}

/// Replay the mainline against `start` (default: standard starting position
/// unless a `FEN` header is present). Returns the list of resolved moves.
///
/// Any illegal SAN → error with ply index for diagnostics.
pub fn replay(game: &Game) -> Result<Vec<crate::chess_move::Move>, PgnError> {
    let mut pos = if let Some(fen) = game.header("FEN") {
        crate::fen::parse_fen(fen)
            .map_err(|e| PgnError::Syntax(format!("bad FEN header: {e:?}")))?
    } else {
        Position::startpos()
    };

    let mut moves = Vec::with_capacity(game.mainline.len());
    for (ply, node) in game.mainline.iter().enumerate() {
        let m = san_to_move(&pos, &node.san).map_err(|e| PgnError::MoveError {
            san: node.san.clone(),
            ply,
            reason: san_error_reason(&e),
        })?;
        pos.make_move(m);
        moves.push(m);
    }
    Ok(moves)
}

fn san_error_reason(e: &SanError) -> String {
    match e {
        SanError::Empty => "empty".into(),
        SanError::Syntax(s) => format!("syntax: {s}"),
        SanError::NoLegalMove(s) => format!("no legal move: {s}"),
        SanError::Ambiguous(s) => format!("ambiguous: {s}"),
    }
}

// --------------------------------------------------------------------------
// Writer
// --------------------------------------------------------------------------

/// Emit a PGN string from a `Game`. Mainline is wrapped at ~80 columns per
/// spec §8.2.6.
pub fn write_pgn(game: &Game) -> String {
    let mut out = String::with_capacity(256);
    // Seven-Tag-Roster order first, then any others in insertion order.
    let seven = ["Event", "Site", "Date", "Round", "White", "Black", "Result"];
    let mut seen = std::collections::HashSet::new();
    for key in &seven {
        if let Some(v) = game.header(key) {
            out.push_str(&format!("[{key} \"{}\"]\n", escape_value(v)));
            seen.insert(*key);
        }
    }
    for (k, v) in &game.headers {
        if !seen.contains(k.as_str()) {
            out.push_str(&format!("[{k} \"{}\"]\n", escape_value(v)));
        }
    }
    if !game.headers.is_empty() {
        out.push('\n');
    }

    let mut line = String::with_capacity(80);
    let push_token = |line: &mut String, out: &mut String, tok: &str| {
        if !line.is_empty() && line.len() + 1 + tok.len() > 80 {
            out.push_str(line);
            out.push('\n');
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(tok);
    };

    let mut move_number: u32 = 1;
    let mut white_to_move = true;
    for node in &game.mainline {
        if let Some(c) = &node.comment_before {
            push_token(&mut line, &mut out, &format!("{{ {c} }}"));
        }
        if white_to_move {
            push_token(&mut line, &mut out, &format!("{move_number}."));
        } else if node.comment_before.is_some()
            || std::ptr::eq(node, game.mainline.first().unwrap())
        {
            // Ellipsis after header-comment or at start of black's turn.
            push_token(&mut line, &mut out, &format!("{move_number}..."));
        }
        push_token(&mut line, &mut out, &node.san);
        for nag in &node.nags {
            push_token(&mut line, &mut out, &format!("${nag}"));
        }
        if let Some(c) = &node.comment_after {
            push_token(&mut line, &mut out, &format!("{{ {c} }}"));
        }
        for var in &node.variations {
            push_token(&mut line, &mut out, "(");
            let sub = write_variation(var, move_number, white_to_move);
            for tok in sub.split_whitespace() {
                push_token(&mut line, &mut out, tok);
            }
            push_token(&mut line, &mut out, ")");
        }
        if !white_to_move {
            move_number += 1;
        }
        white_to_move = !white_to_move;
    }
    push_token(&mut line, &mut out, game.termination.as_str());
    if !line.is_empty() {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn write_variation(nodes: &[MoveNode], start_num: u32, white_to_move: bool) -> String {
    let mut s = String::new();
    let mut n = start_num;
    let mut white = white_to_move;
    for (i, node) in nodes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        if white {
            s.push_str(&format!("{n}. "));
        } else if i == 0 {
            s.push_str(&format!("{n}... "));
        }
        s.push_str(&node.san);
        if !white {
            n += 1;
        }
        white = !white;
    }
    s
}

fn escape_value(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"")
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_game() {
        let pgn = r#"[Event "Casual"]
[Site "?"]
[Date "2025.01.01"]
[Round "-"]
[White "Alice"]
[Black "Bob"]
[Result "1-0"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 1-0
"#;
        let game = parse_pgn(pgn).unwrap();
        assert_eq!(game.header("Event"), Some("Casual"));
        assert_eq!(game.header("White"), Some("Alice"));
        assert_eq!(game.termination, Termination::WhiteWon);
        assert_eq!(game.mainline.len(), 8);
        assert_eq!(game.mainline[0].san, "e4");
        assert_eq!(game.mainline[7].san, "Nf6");
    }

    #[test]
    fn parse_with_variation_nags_and_comments() {
        let pgn = r#"[Event "Test"]

1. e4 { strong opening } e5 $14 (1... c5 { Sicilian } 2. Nf3) 2. Nf3 Nc6 *
"#;
        let game = parse_pgn(pgn).unwrap();
        assert_eq!(game.termination, Termination::Unknown);
        assert_eq!(game.mainline.len(), 4);
        assert_eq!(game.mainline[0].san, "e4");
        assert_eq!(
            game.mainline[0].comment_after.as_deref(),
            Some("strong opening")
        );
        assert_eq!(game.mainline[1].san, "e5");
        assert_eq!(game.mainline[1].nags, vec![14]);
        assert_eq!(game.mainline[1].variations.len(), 1);
        assert_eq!(game.mainline[1].variations[0].len(), 2);
        assert_eq!(game.mainline[1].variations[0][0].san, "c5");
        assert_eq!(
            game.mainline[1].variations[0][0].comment_after.as_deref(),
            Some("Sicilian")
        );
    }

    #[test]
    fn replay_validates_moves() {
        let pgn = r#"[Event "Test"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 *
"#;
        let game = parse_pgn(pgn).unwrap();
        let moves = replay(&game).unwrap();
        assert_eq!(moves.len(), 6);
    }

    #[test]
    fn replay_reports_error_for_illegal_san() {
        let pgn = r#"[Event "Test"]

1. e4 e5 2. Qxf7 *
"#;
        let game = parse_pgn(pgn).unwrap();
        let err = replay(&game).unwrap_err();
        assert!(matches!(err, PgnError::MoveError { .. }));
    }

    #[test]
    fn roundtrip_simple_game_has_same_moves() {
        let src = r#"[Event "Casual"]
[Site "?"]
[Date "2025.01.01"]
[Round "-"]
[White "Alice"]
[Black "Bob"]
[Result "1-0"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 1-0
"#;
        let game = parse_pgn(src).unwrap();
        let reemitted = write_pgn(&game);
        let reparsed = parse_pgn(&reemitted).unwrap();
        assert_eq!(reparsed.termination, game.termination);
        assert_eq!(reparsed.mainline.len(), game.mainline.len());
        for (a, b) in reparsed.mainline.iter().zip(game.mainline.iter()) {
            assert_eq!(a.san, b.san);
        }
    }

    #[test]
    fn parse_headers_with_escaped_quotes() {
        let pgn = r#"[Event "He said \"go\""]
[White "A"]
[Black "B"]

1. e4 *
"#;
        let game = parse_pgn(pgn).unwrap();
        // The raw value retains the backslash-escape as emitted; we don't
        // unescape at parse time (PGN consumers typically do this themselves).
        assert!(game.header("Event").unwrap().contains("go"));
    }

    #[test]
    fn multi_game_parser() {
        let pgn = r#"[Event "G1"]

1. e4 e5 1-0

[Event "G2"]

1. d4 d5 0-1
"#;
        let games = parse_pgn_many(pgn).unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].header("Event"), Some("G1"));
        assert_eq!(games[0].termination, Termination::WhiteWon);
        assert_eq!(games[1].header("Event"), Some("G2"));
        assert_eq!(games[1].termination, Termination::BlackWon);
    }

    #[test]
    fn custom_start_position_via_fen_header() {
        let pgn = r#"[Event "Endgame study"]
[FEN "8/P7/8/8/8/8/4k3/7K w - - 0 1"]
[SetUp "1"]

1. a8=Q *
"#;
        let game = parse_pgn(pgn).unwrap();
        let moves = replay(&game).unwrap();
        assert_eq!(moves.len(), 1);
    }

    // -- error-path coverage -------------------------------------------------

    #[test]
    fn unclosed_comment_errors() {
        let pgn = "1. e4 { never closed\n";
        assert!(matches!(
            parse_pgn(pgn).unwrap_err(),
            PgnError::UnclosedComment
        ));
    }

    #[test]
    fn unclosed_header_errors() {
        let pgn = "[Event \"Half quoted\n\n1. e4 *";
        assert!(matches!(
            parse_pgn(pgn).unwrap_err(),
            PgnError::UnclosedHeader
        ));
    }

    #[test]
    fn unclosed_variation_errors() {
        let pgn = "1. e4 (1... c5 *\n";
        assert!(matches!(
            parse_pgn(pgn).unwrap_err(),
            PgnError::UnclosedVariation
        ));
    }

    #[test]
    fn unmatched_close_paren_errors() {
        let pgn = "1. e4 ) *";
        assert!(matches!(parse_pgn(pgn).unwrap_err(), PgnError::Syntax(_)));
    }

    #[test]
    fn variation_before_any_move_errors() {
        let pgn = "[Event \"x\"]\n\n(1. e4) 1. d4 *";
        assert!(matches!(parse_pgn(pgn).unwrap_err(), PgnError::Syntax(_)));
    }

    #[test]
    fn header_without_quoted_value_errors() {
        let pgn = "[Event bare]\n\n1. e4 *";
        assert!(matches!(parse_pgn(pgn).unwrap_err(), PgnError::Syntax(_)));
    }

    #[test]
    fn line_comment_is_consumed() {
        let pgn = "; this is a comment to end of line\n1. e4 e5 *\n";
        let game = parse_pgn(pgn).unwrap();
        assert_eq!(game.mainline.len(), 2);
    }

    #[test]
    fn pgn_error_display_is_human_readable() {
        for e in [
            PgnError::Syntax("x".into()),
            PgnError::UnclosedComment,
            PgnError::UnclosedHeader,
            PgnError::UnclosedVariation,
            PgnError::MoveError {
                san: "e4".into(),
                ply: 0,
                reason: "why".into(),
            },
        ] {
            assert!(!format!("{e}").is_empty());
        }
    }

    #[test]
    fn termination_roundtrip_strings() {
        assert_eq!(Termination::WhiteWon.as_str(), "1-0");
        assert_eq!(Termination::BlackWon.as_str(), "0-1");
        assert_eq!(Termination::Draw.as_str(), "1/2-1/2");
        assert_eq!(Termination::Unknown.as_str(), "*");
    }

    #[test]
    fn game_header_helpers() {
        let mut g = Game::default();
        assert!(g.header("Event").is_none());
        g.set_header("Event", "Test");
        assert_eq!(g.header("Event"), Some("Test"));
        // set_header on an existing key replaces.
        g.set_header("Event", "Replaced");
        assert_eq!(g.header("Event"), Some("Replaced"));
    }

    #[test]
    fn write_pgn_round_trip_preserves_mainline() {
        let src = "[Event \"Unit\"]\n[Result \"*\"]\n\n1. e4 e5 2. Nf3 Nc6 *\n";
        let game = parse_pgn(src).unwrap();
        let out = write_pgn(&game);
        assert!(out.contains("[Event \"Unit\"]"));
        assert!(out.contains("1. e4"));
        // Round-trip through the parser again.
        let reparsed = parse_pgn(&out).unwrap();
        assert_eq!(
            reparsed
                .mainline
                .iter()
                .map(|n| n.san.as_str())
                .collect::<Vec<_>>(),
            game.mainline
                .iter()
                .map(|n| n.san.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn parse_pgn_many_stops_cleanly_at_eof() {
        // Two games, no trailing newline.
        let pgn = "[Event \"a\"]\n\n1. e4 *\n\n[Event \"b\"]\n\n1. d4 *";
        let games = parse_pgn_many(pgn).unwrap();
        assert_eq!(games.len(), 2);
    }

    #[test]
    fn parse_pgn_many_handles_empty_input() {
        assert_eq!(parse_pgn_many("").unwrap().len(), 0);
        assert_eq!(parse_pgn_many("   \n\n  ").unwrap().len(), 0);
    }

    // -- additional coverage: leading comments and writer branches ----------

    #[test]
    fn leading_comment_before_move_one_attaches_to_first_move() {
        let pgn = r#"[Event "x"]

{ opening idea } { second } 1. e4 *
"#;
        let game = parse_pgn(pgn).unwrap();
        assert_eq!(game.mainline.len(), 1);
        let before = game.mainline[0].comment_before.as_deref().unwrap();
        // Both pending comments merged into one.
        assert!(before.contains("opening idea"));
        assert!(before.contains("second"));
    }

    #[test]
    fn second_comment_after_same_move_is_appended() {
        let pgn = r#"[Event "x"]

1. e4 { first } { second } *
"#;
        let game = parse_pgn(pgn).unwrap();
        let after = game.mainline[0].comment_after.as_deref().unwrap();
        assert!(after.contains("first"));
        assert!(after.contains("second"));
    }

    #[test]
    fn strip_fused_number_handles_non_numeric_dots() {
        // `Bd.f5` contains a dot but not a move-number prefix — the full
        // token should be preserved.
        assert_eq!(strip_fused_number("Bd.f5"), "Bd.f5");
        assert_eq!(strip_fused_number("1."), "");
        assert_eq!(strip_fused_number("1...e5"), "e5");
        assert_eq!(strip_fused_number("no_dot_here"), "no_dot_here");
    }

    #[test]
    fn is_move_number_edge_cases() {
        assert!(is_move_number("1."));
        assert!(is_move_number("123..."));
        assert!(!is_move_number("1")); // digits only, no dot
        assert!(!is_move_number(".1")); // leading dot
        assert!(!is_move_number("Nf3"));
        assert!(!is_move_number(""));
        assert!(!is_move_number("1.e4")); // dots must be trailing
    }

    #[test]
    fn write_pgn_emits_variations_and_comments() {
        let mut g = Game::default();
        g.set_header("Event", "w");
        g.mainline.push(MoveNode {
            san: "e4".into(),
            comment_before: Some("opening".into()),
            comment_after: Some("centre".into()),
            nags: vec![1, 14],
            variations: vec![vec![MoveNode {
                san: "d4".into(),
                ..Default::default()
            }]],
        });
        g.mainline.push(MoveNode {
            san: "e5".into(),
            ..Default::default()
        });
        g.termination = Termination::Draw;
        let out = write_pgn(&g);
        assert!(out.contains("[Event \"w\"]"));
        assert!(out.contains("{ opening }"));
        assert!(out.contains("e4"));
        assert!(out.contains("$1"));
        assert!(out.contains("(")); // variation open
        assert!(out.contains(")")); // variation close
        assert!(out.contains("1/2-1/2"));
    }

    #[test]
    fn write_pgn_without_headers_emits_only_movetext() {
        let mut g = Game::default();
        g.mainline.push(MoveNode {
            san: "e4".into(),
            ..Default::default()
        });
        g.termination = Termination::Unknown;
        let out = write_pgn(&g);
        assert!(!out.starts_with('['));
        assert!(out.contains("1. e4"));
        assert!(out.contains(" *"));
    }

    #[test]
    fn header_helpers_preserve_insertion_order() {
        let mut g = Game::default();
        g.set_header("A", "1");
        g.set_header("B", "2");
        g.set_header("A", "3"); // replace, not push
        assert_eq!(g.headers.len(), 2);
        assert_eq!(g.header("A"), Some("3"));
    }
}
