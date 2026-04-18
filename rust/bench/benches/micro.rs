//! Single-call micro-benchmarks.
//!
//! Measures the cost of one FEN parse, one movegen, one make+unmake, one
//! hash. These numbers complement the bulk perft NPS by exposing per-op
//! constant overhead — important for users who invoke the library in
//! small steps from JS rather than running search inside WASM.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const KIWIPETE: &str =
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

// -- ultrachess-core ---------------------------------------------------------

fn uc_fen_parse(c: &mut Criterion) {
    let mut g = c.benchmark_group("fen_parse");
    g.bench_function("ultrachess/startpos", |b| {
        b.iter(|| ultrachess_core::fen::parse_fen(black_box(STARTPOS)).unwrap());
    });
    g.bench_function("ultrachess/kiwipete", |b| {
        b.iter(|| ultrachess_core::fen::parse_fen(black_box(KIWIPETE)).unwrap());
    });
    g.bench_function("shakmaty/startpos", |b| {
        use shakmaty::fen::Fen;
        use shakmaty::Chess;
        b.iter(|| {
            let fen: Fen = black_box(STARTPOS).parse().unwrap();
            let _: Chess = fen
                .into_position(shakmaty::CastlingMode::Standard)
                .unwrap();
        });
    });
    g.bench_function("cozy/startpos", |b| {
        use cozy_chess::Board;
        b.iter(|| black_box(STARTPOS).parse::<Board>().unwrap());
    });
    g.finish();
}

fn uc_fen_write(c: &mut Criterion) {
    use ultrachess_core::fen::write_fen;
    use ultrachess_core::position::Position;
    let pos_uc = Position::startpos();

    use shakmaty::fen::Fen;
    use shakmaty::Chess;
    let shak: Chess = STARTPOS
        .parse::<Fen>()
        .unwrap()
        .into_position(shakmaty::CastlingMode::Standard)
        .unwrap();

    use cozy_chess::Board;
    let cozy: Board = STARTPOS.parse().unwrap();

    let mut g = c.benchmark_group("fen_write");
    g.bench_function("ultrachess/startpos", |b| {
        b.iter(|| write_fen(black_box(&pos_uc)));
    });
    g.bench_function("shakmaty/startpos", |b| {
        b.iter(|| format!("{}", Fen::from_position(&black_box(shak.clone()), shakmaty::EnPassantMode::Legal)));
    });
    g.bench_function("cozy/startpos", |b| {
        b.iter(|| format!("{}", black_box(&cozy)));
    });
    g.finish();
}

fn movegen_only(c: &mut Criterion) {
    let mut g = c.benchmark_group("movegen");

    // ultrachess
    {
        use ultrachess_core::fen::parse_fen;
        use ultrachess_core::movegen::{generate_legal_moves, MoveList};
        let pos = parse_fen(STARTPOS).unwrap();
        g.bench_function("ultrachess/startpos", |b| {
            b.iter(|| {
                let mut ml = MoveList::new();
                generate_legal_moves(black_box(&pos), &mut ml);
                ml.len()
            });
        });
        let pos_kp = parse_fen(KIWIPETE).unwrap();
        g.bench_function("ultrachess/kiwipete", |b| {
            b.iter(|| {
                let mut ml = MoveList::new();
                generate_legal_moves(black_box(&pos_kp), &mut ml);
                ml.len()
            });
        });
    }

    // shakmaty
    {
        use shakmaty::{fen::Fen, CastlingMode, Chess, Position};
        let p: Chess = STARTPOS
            .parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        g.bench_function("shakmaty/startpos", |b| {
            b.iter(|| black_box(&p).legal_moves().len());
        });
        let p_kp: Chess = KIWIPETE
            .parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        g.bench_function("shakmaty/kiwipete", |b| {
            b.iter(|| black_box(&p_kp).legal_moves().len());
        });
    }

    // cozy-chess
    {
        use cozy_chess::Board;
        let b_sp: Board = STARTPOS.parse().unwrap();
        g.bench_function("cozy/startpos", |b| {
            b.iter(|| {
                let mut n = 0u32;
                black_box(&b_sp).generate_moves(|mvs| {
                    n += mvs.len() as u32;
                    false
                });
                n
            });
        });
        let b_kp: Board = KIWIPETE.parse().unwrap();
        g.bench_function("cozy/kiwipete", |b| {
            b.iter(|| {
                let mut n = 0u32;
                black_box(&b_kp).generate_moves(|mvs| {
                    n += mvs.len() as u32;
                    false
                });
                n
            });
        });
    }

    g.finish();
}

fn make_unmake_pair(c: &mut Criterion) {
    let mut g = c.benchmark_group("make_unmake");

    // ultrachess: make/unmake all legal moves then rewind — measures per-move
    // cost of the pair.
    {
        use ultrachess_core::fen::parse_fen;
        use ultrachess_core::movegen::{generate_legal_moves, MoveList};
        let pos = parse_fen(KIWIPETE).unwrap();
        let mut ml = MoveList::new();
        generate_legal_moves(&pos, &mut ml);
        let moves: Vec<_> = ml.iter().copied().collect();
        g.bench_function("ultrachess/kiwipete_all48", |b| {
            let mut p = pos.clone();
            b.iter(|| {
                for &m in &moves {
                    p.make_move(m);
                    p.unmake_move(m);
                }
                black_box(&p);
            });
        });
    }

    // cozy: play(mv) returns a new Board (functional) — the apples-to-apples
    // comparison is "apply move to a working copy and drop it".
    {
        use cozy_chess::Board;
        let b_kp: Board = KIWIPETE.parse().unwrap();
        let mut moves = Vec::new();
        b_kp.generate_moves(|mvs| {
            for m in mvs {
                moves.push(m);
            }
            false
        });
        g.bench_function("cozy/kiwipete_all48_play", |b| {
            b.iter(|| {
                for &m in &moves {
                    let mut c = black_box(&b_kp).clone();
                    c.play_unchecked(m);
                    black_box(c);
                }
            });
        });
    }

    // shakmaty: Chess::play() takes ownership and returns a new Position.
    {
        use shakmaty::{fen::Fen, CastlingMode, Chess, Position};
        let p: Chess = KIWIPETE
            .parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        let moves = p.legal_moves();
        g.bench_function("shakmaty/kiwipete_all48_play", |b| {
            b.iter(|| {
                for m in &moves {
                    let c = black_box(&p).clone();
                    // shakmaty ≥0.30 takes Move by value.
                    let _ = c.play(m.clone()).unwrap();
                }
            });
        });
    }

    g.finish();
}

fn hash_op(c: &mut Criterion) {
    use ultrachess_core::fen::parse_fen;
    let pos = parse_fen(KIWIPETE).unwrap();

    let mut g = c.benchmark_group("hash_cached");
    g.bench_function("ultrachess/kiwipete", |b| {
        b.iter(|| black_box(&pos).hash());
    });
    g.finish();
}

// ---------------------------------------------------------------------------
// Expanded coverage — clone, is_check, attackers, SAN write/parse
// ---------------------------------------------------------------------------

fn clone_op(c: &mut Criterion) {
    let mut g = c.benchmark_group("clone");

    {
        use ultrachess_core::position::Position;
        let p = Position::startpos();
        g.bench_function("ultrachess/startpos", |b| b.iter(|| black_box(&p).clone()));

        let pk = ultrachess_core::fen::parse_fen(KIWIPETE).unwrap();
        g.bench_function("ultrachess/kiwipete", |b| b.iter(|| black_box(&pk).clone()));
    }
    {
        use shakmaty::{fen::Fen, CastlingMode, Chess};
        let p: Chess = STARTPOS
            .parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        g.bench_function("shakmaty/startpos", |b| b.iter(|| black_box(&p).clone()));
    }
    {
        use cozy_chess::Board;
        let b_sp: Board = STARTPOS.parse().unwrap();
        g.bench_function("cozy/startpos", |b| b.iter(|| black_box(&b_sp).clone()));
    }

    g.finish();
}

fn is_check_op(c: &mut Criterion) {
    let mut g = c.benchmark_group("is_check");

    {
        use ultrachess_core::fen::parse_fen;
        let p = parse_fen("rnbqkbnr/ppp2ppp/8/3pp3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 0 3").unwrap();
        g.bench_function("ultrachess/not_in_check", |b| b.iter(|| black_box(&p).in_check()));
        // A simple check position: black king attacked by white queen on h5.
        let pc = parse_fen("rnb1kbnr/pppp1ppp/8/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 3 3").unwrap();
        g.bench_function("ultrachess/in_check", |b| b.iter(|| black_box(&pc).in_check()));
    }
    {
        use shakmaty::{fen::Fen, CastlingMode, Chess, Position};
        let p: Chess = "rnbqkbnr/ppp2ppp/8/3pp3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 0 3"
            .parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        g.bench_function("shakmaty/not_in_check", |b| b.iter(|| black_box(&p).is_check()));
    }
    {
        use cozy_chess::Board;
        let p: Board = "rnbqkbnr/ppp2ppp/8/3pp3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 0 3"
            .parse()
            .unwrap();
        g.bench_function("cozy/not_in_check", |b| b.iter(|| !black_box(&p).checkers().is_empty()));
    }

    g.finish();
}

fn san_write_op(c: &mut Criterion) {
    let mut g = c.benchmark_group("san_write");
    // We SAN-format every legal move at kiwipete and measure the full cycle.
    {
        use ultrachess_core::fen::parse_fen;
        use ultrachess_core::movegen::{generate_legal_moves, MoveList};
        use ultrachess_core::san::move_to_san;
        let p = parse_fen(KIWIPETE).unwrap();
        let mut ml = MoveList::new();
        generate_legal_moves(&p, &mut ml);
        let moves: Vec<_> = ml.iter().copied().collect();
        g.bench_function("ultrachess/kiwipete_all48", |b| {
            b.iter(|| {
                let mut p2 = p.clone();
                let mut acc = 0usize;
                for &m in &moves {
                    acc += move_to_san(&mut p2, m).len();
                }
                black_box(acc);
            });
        });
    }
    {
        use shakmaty::san::San;
        use shakmaty::{fen::Fen, CastlingMode, Chess, Position};
        let p: Chess = KIWIPETE
            .parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        let legal = p.legal_moves();
        g.bench_function("shakmaty/kiwipete_all48", |b| {
            b.iter(|| {
                let mut acc = 0usize;
                for m in &legal {
                    // shakmaty ≥0.30 takes Move by value.
                    let san = San::from_move(black_box(&p), m.clone());
                    acc += format!("{san}").len();
                }
                black_box(acc);
            });
        });
    }

    g.finish();
}

criterion_group!(
    benches,
    uc_fen_parse,
    uc_fen_write,
    movegen_only,
    make_unmake_pair,
    hash_op,
    clone_op,
    is_check_op,
    san_write_op,
);
criterion_main!(benches);
