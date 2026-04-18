//! Perft NPS comparison harness.
//!
//! For each (position, depth) pair and each engine:
//!   1. Parse FEN once.
//!   2. **Sanity gate**: run ultrachess's perft at a shallow depth against
//!      the canonical reference node count before any timing. We refuse
//!      to publish numbers if our own engine produces the wrong answer.
//!   3. Warm up one run (CPU caches, branch predictor).
//!   4. Time N repetitions; take the minimum wall time (min is the most
//!      stable estimator for single-threaded compute; averages are polluted
//!      by OS scheduling jitter).
//!   5. Report NPS = nodes / min_time.
//!
//! Emits both a human-readable table and, on `--markdown`, a copy-pasteable
//! markdown version for the bench report.

use std::env;
use std::time::Instant;

use ultrachess_bench::engines::{cozy, jb, ours, shak};
use ultrachess_bench::{DEFAULT_DEPTHS, POSITIONS};

#[derive(Clone, Debug)]
struct Row {
    position: &'static str,
    depth: u32,
    nodes: u64,
    // NPS per engine (index matches ENGINES).
    nps: [Option<f64>; 4],
    // Wall time per engine.
    t_ms: [Option<f64>; 4],
}

const ENGINES: [&str; 4] = ["ultrachessjs", "shakmaty", "cozy-chess", "chess (jb)"];

fn main() {
    let args: Vec<String> = env::args().collect();
    let markdown = args.iter().any(|a| a == "--markdown");
    let quick = args.iter().any(|a| a == "--quick");
    let deep = args.iter().any(|a| a == "--deep");
    let trials: usize = args
        .iter()
        .position(|a| a == "--trials")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);

    // Adjust depths: `--quick` drops 1 from each (for a smoke run); `--deep`
    // bumps each by 1 (pushes startpos to d6 = 119M nodes etc.).
    let depths: Vec<u32> = DEFAULT_DEPTHS
        .iter()
        .map(|&d| {
            let d = d as i32 + if quick { -1 } else { 0 } + if deep { 1 } else { 0 };
            d.max(1) as u32
        })
        .collect();

    eprintln!(
        "# Perft NPS — trials={trials}  quick={quick}  deep={deep}  built_in={}\n",
        profile_tag()
    );

    sanity_gate();

    let mut rows: Vec<Row> = Vec::with_capacity(POSITIONS.len());

    for ((name, fen), &depth) in POSITIONS.iter().zip(depths.iter()) {
        eprintln!("==> {name}  depth {depth}");
        let mut row = Row {
            position: *name,
            depth,
            nodes: 0,
            nps: [None; 4],
            t_ms: [None; 4],
        };

        // --- ultrachessjs
        {
            let pos = ours::parse(fen);
            let (n, t) = time_it(trials, || ours::perft(&pos, depth));
            row.nodes = n;
            row.nps[0] = Some(n as f64 / t);
            row.t_ms[0] = Some(t * 1000.0);
            eprintln!("    ultrachessjs : {:>12} nodes   {:>8.2} Mnps   {:>7.1} ms", n, n as f64 / t / 1e6, t * 1000.0);
        }

        // --- shakmaty
        {
            let pos = shak::parse(fen);
            let (n, t) = time_it(trials, || shak::perft(&pos, depth));
            assert_eq!(n, row.nodes, "shakmaty disagreed on node count for {name}");
            row.nps[1] = Some(n as f64 / t);
            row.t_ms[1] = Some(t * 1000.0);
            eprintln!("    shakmaty     : {:>12} nodes   {:>8.2} Mnps   {:>7.1} ms", n, n as f64 / t / 1e6, t * 1000.0);
        }

        // --- cozy-chess
        {
            let pos = cozy::parse(fen);
            let (n, t) = time_it(trials, || cozy::perft(&pos, depth));
            assert_eq!(n, row.nodes, "cozy disagreed on node count for {name}");
            row.nps[2] = Some(n as f64 / t);
            row.t_ms[2] = Some(t * 1000.0);
            eprintln!("    cozy-chess   : {:>12} nodes   {:>8.2} Mnps   {:>7.1} ms", n, n as f64 / t / 1e6, t * 1000.0);
        }

        // --- jordanbray/chess
        {
            let pos = jb::parse(fen);
            let (n, t) = time_it(trials, || jb::perft(&pos, depth));
            if n != row.nodes {
                // jordanbray/chess is strict FEN — some positions (notably Pos5
                // with its idiosyncratic castling) can trip it. Warn but don't
                // poison the whole row.
                eprintln!(
                    "    WARN chess (jb) disagreed: got {n}, expected {}",
                    row.nodes
                );
            } else {
                row.nps[3] = Some(n as f64 / t);
                row.t_ms[3] = Some(t * 1000.0);
                eprintln!("    chess (jb)   : {:>12} nodes   {:>8.2} Mnps   {:>7.1} ms", n, n as f64 / t / 1e6, t * 1000.0);
            }
        }

        rows.push(row);
    }

    println!();
    if markdown {
        print_markdown(&rows);
    } else {
        print_plain(&rows);
    }
}

/// Refuse to benchmark if our own engine disagrees with canonical perft
/// node counts at a shallow depth. Cheap (~50 ms total) and catches the
/// class of bug where a "faster" implementation is quietly wrong.
fn sanity_gate() {
    // (fen, depth, expected nodes) — reference values from Chessprogramming
    // Wiki. Shallow enough to finish in ~ms, deep enough to exercise
    // castling / EP / promotion paths on the relevant positions.
    const CASES: &[(&str, u32, u64)] = &[
        ("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", 4, 197_281),
        ("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1", 4, 4_085_603),
        ("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", 5, 674_624),
        ("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1", 4, 422_333),
        ("rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8", 4, 2_103_487),
        ("r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10", 4, 3_894_594),
    ];
    eprintln!("sanity gate: checking ultrachess perft against reference node counts...");
    for (fen, depth, expected) in CASES {
        let pos = ours::parse(fen);
        let got = ours::perft(&pos, *depth);
        if got != *expected {
            eprintln!(
                "FATAL: ultrachess perft mismatch\n  FEN: {fen}\n  depth: {depth}\n  expected: {expected}\n       got: {got}"
            );
            eprintln!("refusing to benchmark a broken implementation.");
            std::process::exit(2);
        }
    }
    eprintln!("sanity gate: PASS — all {} reference positions agree.\n", CASES.len());
}

fn time_it<F, R>(trials: usize, mut f: F) -> (u64, f64)
where
    F: FnMut() -> R,
    R: Into<u64> + Copy,
{
    // Warmup once (discarded).
    let _ = f();

    let mut min_t = f64::INFINITY;
    let mut nodes: u64 = 0;
    for _ in 0..trials {
        let t0 = Instant::now();
        let r = f();
        let t = t0.elapsed().as_secs_f64();
        if t < min_t {
            min_t = t;
        }
        nodes = r.into();
    }
    (nodes, min_t)
}

fn profile_tag() -> &'static str {
    if cfg!(debug_assertions) {
        "debug (SLOW — use --release!)"
    } else {
        "release (lto=fat, codegen-units=1)"
    }
}

// --- table formatting -------------------------------------------------------

fn print_plain(rows: &[Row]) {
    println!(
        "{:<14} {:>5} {:>14}  {:>11}  {:>11}  {:>11}  {:>11}",
        "Position",
        "Depth",
        "Nodes",
        ENGINES[0],
        ENGINES[1],
        ENGINES[2],
        ENGINES[3]
    );
    println!("{}", "-".repeat(96));
    for r in rows {
        print!("{:<14} {:>5} {:>14}", r.position, r.depth, fmt_u64(r.nodes));
        for i in 0..4 {
            match r.nps[i] {
                Some(n) => print!("  {:>9.2} M", n / 1e6),
                None => print!("  {:>11}", "—"),
            }
        }
        println!();
    }
    println!();
    // Speedup row relative to shakmaty.
    println!("Speedup vs shakmaty (×):");
    println!(
        "{:<14} {:>5} {:>14}  {:>11}  {:>11}  {:>11}  {:>11}",
        "", "", "", ENGINES[0], ENGINES[1], ENGINES[2], ENGINES[3]
    );
    for r in rows {
        let base = r.nps[1];
        print!("{:<14} {:>5} {:>14}", r.position, r.depth, "");
        for i in 0..4 {
            match (r.nps[i], base) {
                (Some(n), Some(b)) => print!("  {:>10.2}×", n / b),
                _ => print!("  {:>11}", "—"),
            }
        }
        println!();
    }
}

fn print_markdown(rows: &[Row]) {
    println!("### Perft NPS (release build, min of N trials)\n");
    println!(
        "| Position | Depth | Nodes | {} | {} | {} | {} |",
        ENGINES[0], ENGINES[1], ENGINES[2], ENGINES[3]
    );
    println!("|---|---:|---:|---:|---:|---:|---:|");
    for r in rows {
        print!("| {} | {} | {} |", r.position, r.depth, fmt_u64(r.nodes));
        for i in 0..4 {
            match r.nps[i] {
                Some(n) => print!(" {:.2} Mnps |", n / 1e6),
                None => print!(" — |"),
            }
        }
        println!();
    }

    println!("\n### Wall time (ms)\n");
    println!(
        "| Position | Depth | {} | {} | {} | {} |",
        ENGINES[0], ENGINES[1], ENGINES[2], ENGINES[3]
    );
    println!("|---|---:|---:|---:|---:|---:|");
    for r in rows {
        print!("| {} | {} |", r.position, r.depth);
        for i in 0..4 {
            match r.t_ms[i] {
                Some(t) => print!(" {:.1} |", t),
                None => print!(" — |"),
            }
        }
        println!();
    }

    println!("\n### Speedup vs shakmaty (higher = faster than Lichess's engine)\n");
    println!(
        "| Position | {} | {} | {} | {} |",
        ENGINES[0], ENGINES[1], ENGINES[2], ENGINES[3]
    );
    println!("|---|---:|---:|---:|---:|");
    for r in rows {
        let base = r.nps[1];
        print!("| {} |", r.position);
        for i in 0..4 {
            match (r.nps[i], base) {
                (Some(n), Some(b)) => print!(" {:.2}× |", n / b),
                _ => print!(" — |"),
            }
        }
        println!();
    }
}

fn fmt_u64(n: u64) -> String {
    // Thousands separators.
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}
