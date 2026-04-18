# The Move Generation Ladder

**45 levels, from the obvious way to beating shakmaty and cozy-chess.**

This document walks the full optimization history of legal move generation
in `ultrachessjs`. It's written for engineers who have never written a
chess program, but it doesn't hide the tricks.

It covers **fully-legal move generation and everything it directly feeds**:
`moves()`, `legalMoves()`, `is_check()`, `make/unmake`, the SAN emitter's
suffix + disambiguation, and perft. FEN parse/write and `clone()` are
separate stories, covered in [BENCH.md](../BENCH.md).

At every level:

- **What was slow, and why.** Often not what you'd expect.
- **The fix**, named and linkable, so you can grep the code.
- **Prior art**, with a link — almost nothing in this field is new.
- **Numbers**, either measured (a small number of levels have a bench in
  [`rust/ladder/`](../rust/ladder)) or derived asymptotically.

## Reading the tags

Each level is tagged with where the credit lies:

- `[prior art]` — entirely known technique. We implemented a version of it.
- `[recombination]` — known pieces applied to the specific shape of this
  library. The novelty (if any) is in the *combination*, not the parts.
- `[ours]` — we haven't seen it written down elsewhere. Best-effort claim;
  PRs with prior art pointers gratefully accepted.

If you spot an attribution error, please open an issue. Chess programming
is a 70-year-old field and the lineage matters.

---

## Part 1 — Baseline: making it work (Levels 1–5)

> "Make it correct, then make it fast." The starting point is a chess
> program that plays legal moves, nothing more.

### L1 · Mailbox 8×8 + coordinate arithmetic `[prior art]`

Board as `[[Option<Piece>; 8]; 8]`. Move generation iterates all 64
squares, dispatches on piece type, walks directions with `(dx, dy)`
offsets, bails at `x < 0 || x >= 8`. The standard first pass.

*Credits:* Shannon 1950, Slate & Atkin (Chess 4.x, 1970s).

### L2 · Enum dispatch per piece type `[prior art]`

A `match piece_type { Pawn => ..., Knight => ..., ... }` in the inner
loop. Predictable, but introduces indirection the optimiser can't always
hoist.

### L3 · Pseudo-legal + make/undo filter legality `[prior art]`

Generate every move a piece *could* make ignoring pins and checks, play
each one, check if our king ends up in check, discard if so. Correct, but
~10× slower than what's coming. Most old engines (including chess.js)
still work this way.

*Credits:* Crafty, Fruit, the entire 1990s engine ecosystem.

### L4 · Copy-make vs make-unmake `[prior art]`

Two ways to explore moves without losing state: clone the position and
discard (copy-make), or play and reverse (make-unmake). Copy-make is
simpler; make-unmake is faster once positions exceed ~100 B, because
memcpy-ing a whole board for every leaf is the dominant cost.

We use **make-unmake** with a stack of `Undo` records (40 B each).
cozy-chess uses **copy-make** — one reason its `clone()` is faster than
ours, and one reason its movegen hot loop can be tighter.

### L5 · What we're actually measuring

Two benchmarks drive every decision below:

- **Perft**: count leaf nodes at depth *d* from a position. `perft(6)`
  from startpos is 119,060,324 nodes. `Mnps` = millions of leaves per
  second. Tests the movegen + make/unmake cycle under load.
- **One-shot `moves()`**: time to produce the legal move list for a
  single position. Tests the API users actually call.

The two pull in different directions. Perft rewards never materialising
moves (see L34); one-shot `moves()` must materialise them. This tension
shapes most of Part 6.

---

## Part 2 — Representation: the data does the work (Levels 6–11)

> Changing how the board is stored is the single biggest lever in chess
> programming. A `u64` per piece type replaces most of the logic above
> with bit arithmetic.

### L6 · Single occupancy bitboard `[prior art]`

One `u64` where bit *n* is set iff square *n* is occupied. `sq_is_empty
= (occ >> sq) & 1 == 0`. `first_occupied = occ.trailing_zeros()`.

*Credits:* KAISSA (Arlazarov, Donskoy, et al.), Soviet Union, 1960s–70s.

### L7 · Twelve piece bitboards `[prior art]`

One bitboard per `(color, piece_type)`. `pieces(White, Pawn) & rank_7`
is a single AND; listing white pawns on the 7th rank is now a popcount
and a pop_lsb loop. All per-square checks become table lookups or bit
tests.

### L8 · `trailing_zeros` / `count_ones` as HW intrinsics `[prior art]`

Rust's `u64::trailing_zeros` lowers to `bsf` / `tzcnt` (x86) or `rbit;
clz` (arm64). `count_ones` lowers to `popcnt`. What looks like a library
call is a single CPU instruction. Movegen inner loops lean on this.

### L9 · `pop_lsb` idiom `[prior art]`

```
sq = bb.trailing_zeros();
bb &= bb - 1;   // clears the lowest set bit
```

Iterating set bits without branching. See
[`bitboard::pop_lsb`](../rust/core/src/bitboard.rs).

### L10 · Compact `Move` packing `[prior art]`

Our `Move` is `u32` packing `from` (6), `to` (6), `promotion` (3),
`flags` (4). Stockfish uses `u16`; we trade 2 extra bytes for cheap
`piece_type` retrieval. The packing choice affects cache footprint of
the `MoveList`.

### L11 · Fixed-size stack `MoveList` vs `Vec<Move>` `[prior art]`

A position has at most ~218 legal moves (theoretical worst case ~256).
A `[Move; 256]` on the stack avoids the allocator entirely. Big win at
any recursion depth.

*Benched in [`ladder/`](../rust/ladder): `vec_vs_stack_movelist`.*

---

## Part 3 — Attack tables: the cheap half (Levels 12–16)

> Non-sliding pieces (king, knight, pawn) never need conditional logic
> for their attacks — just a 64-entry lookup. Sliding pieces need
> occupancy-aware tables (Part 4).

### L12 · Precomputed king attacks `[prior art]`

`KING_ATTACKS[64]`, each entry a bitboard of the ≤8 reachable squares.
Build it once at startup (or `const fn` at compile time — see L46).

### L13 · Precomputed knight attacks `[prior art]`

Same shape. `KNIGHT_ATTACKS[sq]` → bitboard of ≤8 destinations.

### L14 · Precomputed pawn attacks (per color) `[prior art]`

`PAWN_ATTACKS[color][sq]`. Pawns are the only piece where colour
changes direction, so the colour indirection lives in the table, not
in the code.

### L15 · Ray tables / `between[][]` / `line[][]` `[prior art]`

Two related 64×64 tables:
- `between[a][b]` = squares strictly between `a` and `b` if they share a
  ray, else empty. Used for "block the checker" in L28.
- `line[a][b]` = the full line through `a` and `b`. Used in pin
  detection: a pinned piece must stay on `line[king][pinner]`.

*Credits:* chessprogramming.org community ("Between Squares", "On the
Same Line").

### L16 · Inverse attack-table lookup `[prior art]`

Given a target square `t` and a piece type `pt`, what squares could a
`pt` reach `t` from? For king/knight/pawn this is the same table read
backwards (symmetry); for sliders it's the same magic lookup. This
powers "who attacks square X?" (`attackers_to`), which powers
`is_check`, SAN disambiguation (L44), and Ellis-Jones (Part 5).

---

## Part 4 — Sliders: the hard problem (Levels 17–22)

> Bishops, rooks, queens. Their attacks depend on the current occupancy,
> so a simple square-indexed table isn't enough. Every fast chess engine
> solves this the same way now (magic bitboards), but it took the
> community 30 years to get there.

### L17 · Classical ray-walk with early-break `[prior art]`

For each of 4 (rook) or 4 (bishop) directions, step square-by-square,
OR each step into the attack set, break when you hit an occupied
square. Correct, and actually not that slow — this is what Round 1 used
and it still did 302 Mnps perft.

*Credits:* Crafty, Fruit, Rebel — the 1990s–early 2000s baseline.

### L18 · Rotated bitboards (historical) `[prior art]`

Maintain extra bitboards for files, diagonals, anti-diagonals. Look up
rank/file/diagonal occupancy as an 8-bit byte, index into a table.
Faster than classical rays, harder to maintain (four bitboards updated
on every make/unmake). Dominated the late 90s; **we skipped it** —
straight from classical to magic.

*Credits:* DarkThought (Ernst Heinz), 1997.

### L19 · Kogge-Stone fill `[prior art]`

Parallel-prefix bit fill: compute all-destinations-along-direction in
6 shifts and 6 ORs. No table, no branches, portable. Good reference
implementation; slower than magics on modern CPUs because magics are
single-load.

*Credits:* Kogge & Stone 1973 (carry-lookahead adders); adapted for
chess attacks in the mid-2000s.

### L20 · Plain magic bitboards `[prior art]`

For each square, find a 64-bit "magic number" such that
`((occupancy & mask) * magic) >> shift` hashes the relevant occupancy
bits into a dense index. Table size per square: up to 4096 entries.
Total: ~2.3 MB.

*Credits:* Pradu Kannan, CCC forum, 2007.

### L21 · Fancy magic bitboards `[recombination]`

Same hash, but the per-square tables are sized to their *actual* number
of blocker combinations (1 for a corner rook with no blockers; 4096
for central rook). All squares share one contiguous backing array with
per-square offsets. Total: ~800 KB.

We use fancy magics. See [`magic.rs`](../rust/core/src/magic.rs).

*Credits:* Gerd Isenberg, Lasse Hansen, Volker Annuss (chessprogramming
.org, ~2008).

### L22 · Published vs runtime-generated magic numbers `[prior art]`

Magic numbers can be found by randomized search (~20 ms at startup) or
looked up from a published table (zero cost, +1 KB in the binary).
We currently compute at startup; Phase 7 swaps in the published table.

*Credits:* Tord Romstad's compendium; Volker Annuss's 2015 publication.

*Benched in [`ladder/`](../rust/ladder): `classical_vs_magic`.*

---

## Part 5 — Legality without trial-and-error (Levels 23–30)

> Going from "pseudo-legal + filter" (L3) to "generate only legal moves
> from the start". This is the Ellis-Jones approach. It's more code but
> each move emitted is guaranteed legal, so no speculative
> make/unmake/check cycle.

### L23 · The Ellis-Jones algorithm in one page `[prior art]`

Overview:
1. Compute `checkers` — enemy pieces attacking our king.
2. Compute `king_danger` — every square attacked by the enemy *with our
   king removed from the board*. Why remove it: a ray slider's attack
   passes *through* our king; if we don't remove it, the king looks
   "safe" on the next square along.
3. Emit king moves to non-danger squares.
4. If double-check, return. (Only the king can move.)
5. Derive `check_mask` — what squares non-king pieces may target.
6. Compute pin masks. Pinned pieces are clipped to their pin ray.
7. Emit non-king moves intersected with `check_mask` and pin masks.
8. Castling: check path squares are empty and king-travel squares are
   not attacked.
9. En-passant: one extra horizontal-discovered-check test.

Every move emitted is legal. No filter step.

*Credits:* Peter Ellis Jones, "Generating Legal Chess Moves Efficiently",
blog post, 2017. The canonical reference.

### L24 · The `checkers` bitboard `[prior art]`

`attackers_to(king_sq, enemy, occ)` — which enemy pieces attack the
king? popcount tells us check state (0 / 1 / 2+). This is the same
`attackers_to` that L38 caches between moves.

### L25 · King-danger mask `[prior art]`

```
occ_minus_king = occ ^ our_king_bb;
danger = sliders_attacks(enemy, occ_minus_king) | ... ;
```

The king ray point: removing our king from occupancy when computing
enemy slider attacks means the attack ray continues through where the
king stood, flagging the square behind as dangerous.

### L26 · Double-check → king only (early exit) `[prior art]`

If `checkers.count_ones() >= 2`, nothing but a king move can help. Emit
king moves, return. Skips all pin/check-mask work.

### L27 · `check_mask` derivation `[prior art]`

- Not in check: `check_mask = ALL_SQUARES`.
- In check by a jumper (knight, pawn): `check_mask = checker_bb` — the
  only way out (besides king move) is capturing it.
- In check by a slider: `check_mask = checker_bb | between[king][checker]`
  — capture or block.

Non-king moves are ANDed with this mask before emission.

### L28 · Pinned-piece detection `[prior art]`

For each enemy slider on a line with the king:
1. Compute the ray between them.
2. If exactly one friendly piece is on the ray, it's pinned.
3. Its moves are clipped to `line[king][pinner]`.

### L29 · Split `pinned_hv` / `pinned_diag` `[recombination]`

A single `pinned` bitboard forces every piece-type loop to check all
its moves against the pin ray. Splitting into "pinned along a file or
rank" and "pinned along a diagonal" means:

- A knight pinned *at all* has zero legal moves — skip it entirely.
- A bishop cares about diagonal pins but not file/rank pins.
- A rook cares about file/rank but not diagonal.
- A pawn push cares about file pins but not rank/diagonal.

Each piece type only intersects with the pin class that can restrict
it. Round 3 optimisation — 40+ Mnps on perft.

*Credits:* cozy-chess had a version of this before we did. Our
implementation is independent but the idea isn't ours.

### L30 · En-passant & castling edge cases `[prior art]`

- **EP**: after the pseudo-legal EP capture, both our pawn and the
  captured pawn are removed from the rank. If that exposes our king to
  a rook/queen on the same rank, the EP is illegal. One extra magic
  lookup.
- **Castling**: king-travel squares must be empty, king-travel squares
  must not be attacked (tested by `is_square_attacked`), rook must
  still be on its home square, castling rights flag set.

These are the two moves where Ellis-Jones doesn't fully save you from a
legality check.

---

## Part 6 — The emit side: what you do with the moves (Levels 31–35)

> The above all describes how to *compute* the legal-move bitboards.
> Now: how do you get those bits out? This is where we passed cozy.

### L31 · `MaybeUninit` move buffer `[prior art]`

`MoveList` holds `[MaybeUninit<Move>; 256]`. `MoveList::new()` is a
no-op — no 512-byte memset of "uninitialised" `Move` values. Reads are
gated by `len` so uninit cells are never observed. See
[`movegen.rs:71`](../rust/core/src/movegen.rs:71).

*Credits:* standard Rust idiom. cozy-chess uses the same pattern.

### L32 · Per-`from` bulk target push `[prior art]`

Instead of emitting moves one at a time, the generator computes "from
square `s`, all legal targets" as a bitboard, and hands the whole
bitboard to a sink method `push_targets(from, targets)`. The sink
decides whether to iterate-and-push or popcount-and-add.

### L33 · Generic `MoveSink` trait `[recombination]`

```rust
trait MoveSink {
    fn push_targets(&mut self, from: Square, targets: Bitboard);
    fn push_pawn_targets_offset(&mut self, targets: Bitboard, offset: i32);
    fn push_pawn_promotions_offset(&mut self, targets: Bitboard, offset: i32);
    fn push_one(&mut self, m: Move);
}
```

One movegen implementation, generic over the sink. Monomorphised per
caller at compile time — zero runtime dispatch. See
[`movegen.rs:45`](../rust/core/src/movegen.rs:45).

### L34 · `MoveCounter` — popcount-only sink `[recombination]`

The counter sink implements `push_targets` as `self.count +=
targets.count_ones() as usize`. No iteration, no pop_lsb, no push. At
perft leaves (depth 1), this replaces N `pop_lsb` + `push` pairs with
a single `popcnt`. The Round 3 megawin — **+337 Mnps on perft**.

For `depth == 1`, perft calls a `count_legal_moves` path that uses
`MoveCounter`. For `depth > 1` it still materialises, because it needs
to iterate to recurse.

*Credits:* Stockfish's `perft.cpp` uses a count specialisation at
leaves. Daniel Inführ's Gigantua (2021) writeup popularised it.

*Benched in [`ladder/`](../rust/ladder): `count_vs_materialise`.*

### L35 · Perft-specialised `make_move_perft` `[recombination]`

`make_move_perft` skips bookkeeping the general API needs: no `Undo`
entry, no repetition-history push, no cached-checkers maintenance. Just
the minimum to flip the board state and recurse. Paired with an equally
stripped `unmake_move_perft`.

The full `make_move` remains the public API; `make_move_perft` is
internal, used only by the perft driver and only after the count-sink
leaf node. See [`position.rs`](../rust/core/src/position.rs).

*Credits:* cozy-chess has a similar distinction; Stockfish too.

---

## Part 7 — Incremental state: the trade-offs (Levels 36–40)

> This part is about keeping derived state up to date between moves
> instead of recomputing on demand. Each cached value has a cost on
> every make/unmake but pays back on every read.

### L36 · Incremental Zobrist on make/unmake `[prior art]`

`hash` is maintained by XOR-ing the changed pieces/squares/rights into
a running value on every make/unmake. Reading `hash()` is O(1).

*Credits:* Albert Zobrist, "A New Hashing Method with Application for
Game Playing", Wisconsin TR 88, 1970.

### L37 · Bitboard-iterating scratch hash `[ours?]`

The *from-scratch* hash (used by FEN parse, and by
`recompute_zobrist` for debugging) originally iterated the mailbox:

```
for sq in 0..64 { if piece(sq).is_some() { hash ^= zobrist(piece, sq) } }
```

64 iterations, 64 branches. The new version iterates per-piece-type
bitboards:

```
for (color, pt) in COLOR_PIECETYPE_PAIRS {
    let mut bb = self.piece_bb(color, pt);
    while bb != 0 {
        let sq = pop_lsb(&mut bb);
        hash ^= zobrist(color, pt, sq);
    }
}
```

~32 iterations in a typical opening, no branches. Round 4 saved ~20 ns
on FEN parse.

*Credits:* we haven't seen this written down as a named technique, but
it's the kind of thing someone has probably done. Flag if you know.

### L38 · Cached `checkers` bitboard → O(1) `is_check` `[prior art]`

`Position` holds a `checkers: Bitboard` updated on every full
`make_move` / `unmake_move`. `is_check()` is `self.checkers != 0` — one
field load, branchless.

Cost: ~2.4 ns per `make_move` (one `attackers_to` call to refresh the
cache). Measured Round 4 regression: make+unmake 417 → 531 ns (+27%).

Win: `is_check` 3.00 ns → 0.38 ns (**8× faster**).

Net verdict: any user who calls `is_check` at least once per move comes
out ahead. That's ~every GUI, every PGN emit, every user-facing API.
The perft path doesn't use the cache (see L35) so perft is
unaffected.

*Credits:* Stockfish has `checkersBB` in its `StateInfo`. Our approach
is standard; the trade-off framing (API-visible win vs hot-path cost)
is ours in the sense that we make it deliberately rather than inherit
it.

### L39 · Incremental checker update (not done) `[prior art, skipped]`

Instead of one `attackers_to` per make (5 magic lookups), you can
compute the *delta* to `checkers`:

- Moving piece's new attacks potentially add a checker.
- Squares the moving piece vacated may open discovered checks via ray
  sliders through the from-square.

This reduces 5 magic lookups per make to ~1–2. Would recover the Round
4 regression. Not done because the corner cases (castling moves *two*
pieces, EP removes a pawn not on the to-square, promotion changes
piece type) multiply the code paths. Good candidate for Round 5.

### L40 · `Undo` with cached fields `[prior art]`

`Undo` records not just "what changed" but the pre-move values of
derived state (`prev_checkers`, `prev_hash`, `prev_castling`,
`prev_en_passant`, `prev_halfmove_clock`). `unmake_move` restores them
with memcpy-sized assignments — no recomputation.

---

## Part 8 — Polish: the last 15 Mnps (Levels 41–45)

> Everything above gets us to Round 3 (801 Mnps). Round 4 was 5% on
> perft (~unchanged) but 2–8× on the API-visible ops. This part is
> those.

### L41 · `#[inline(always)]` discipline `[prior art]`

Rust's default inlining is good but not aggressive on cross-module
generics. Hot-path functions (`push`, `pop_lsb`, `piece_bb`,
sink methods) carry `#[inline(always)]`. Cold paths (FEN parse error
formatting) don't. Profile-guided, not cargo-culted.

### L42 · Fat LTO + `codegen-units=1` + `panic=abort` `[prior art]`

Release profile in `Cargo.toml`:
```
[profile.release]
lto = "fat"
codegen-units = 1
panic = "abort"
```

Fat LTO lets the linker inline across crates. `codegen-units=1` gives
the optimiser the whole crate. `panic=abort` removes unwind metadata
from hot paths. The standard "I actually want performance" recipe.

### L43 · SAN `+`/`#` suffix via cached `in_check` `[recombination]`

Before Round 4, SAN emit called `is_checkmate()` which internally runs
full movegen to prove "no legal moves". ~40 ns per SAN even for quiet
moves that don't give check.

After: play the move, check the O(1) `in_check` cache. Only if it's
true, run `has_no_legal_moves`. Most moves don't give check, so most
SAN calls skip the movegen entirely. **2.9× faster SAN write.**

### L44 · SAN disambiguation via attack-table pre-filter `[ours?]`

SAN needs to disambiguate "which knight moved to f3?". The obvious
way: generate all legal moves, find ones of the same piece type
targeting the same square.

Our pre-filter:
```
candidates = attacks_from_target(piece_type, to, occ)
           & piece_bb(color, piece_type)
           & !from_bb;
```

If `candidates == 0`, no other piece of that type could even reach
`to` — no disambiguation needed, skip the legal-move call entirely.
Only when `candidates != 0` do we fall back to legality checks
(a candidate may be pinned).

*Credits:* the trick itself (inverse attack table) is L16 applied to a
different question. Writing it up as a SAN fast-path is our
recombination; we haven't seen it documented.

### L45 · Roads not taken `[prior art, deliberately skipped]`

Three optimisations we didn't do, in order of ROI:

**Colour-templated movegen.** Compile two specialisations of the movegen
inner loop — white-to-move and black-to-move — and dispatch on `stm`
once at the top. Folds out one `match` per iteration. The user's
"no metaprogramming" constraint rules this out (it's generic
specialisation by colour parameter). Stockfish does this.

**PEXT bitboards.** BMI2's `_pext_u64` replaces the magic `mul + shift`
with a single hardware instruction. ~10% faster sliders on Intel; the
catch is `_pext_u64` is microcoded on AMD Zen 1/2 (~18 cycles, much
slower than magic). Not portable to WASM. Runtime CPU dispatch possible
but complex. See Onno Garms, CCC 2013.

**SIMD movegen.** Emit multiple sliders in parallel via `std::simd`.
Feasible on x86_64-avx2, not portable to WASM. Research territory.

---

## Mapping the 4 rounds onto the ladder

What we *actually did*, in order, with the level numbers above:

| Round | Work | Levels touched |
|-------|------|----------------|
| R1 | Classical rays, `MoveList`-materialising perft | L1–5, L6–11, L12–16, L17, L23–31 |
| R2 | Fancy magics, `MaybeUninit` movelist | L21, L31 |
| R3 | `MoveSink` + `MoveCounter`, perft-specialised make/unmake, split pins | L29, L33, L34, L35 |
| R4 | Cached `checkers`, bitboard-iter scratch hash, SAN suffix gating, SAN attack-table disambig | L37, L38, L40, L43, L44 |

Round-by-round perft (startpos, depth 6):

| Round | Mnps | vs R1 | vs shakmaty |
|-------|-----:|------:|------------:|
| R1 | 302 | 1.00× | 1.01× |
| R2 | 464 | 1.54× | 1.56× |
| R3 | 801 | 2.65× | 2.79× |
| R4 | 798 | 2.64× | 2.68× |

See [BENCH.md](../BENCH.md) for the full scoreboard across 12 ops.

---

## The ladder bench crate

[`rust/ladder/`](../rust/ladder) holds a small number of intentionally-regressed
implementations, benched against the current code, for the levels where
the asymptotic story doesn't tell you the magnitude. Specifically:

- `vec_vs_stack_movelist` (L11)
- `classical_vs_magic` (L21)
- `count_vs_materialise` (L34)
- `pseudo_filter_vs_ellis_jones` (L3 vs L23)

Run:
```
cargo bench -p ultrachess-ladder
```

Numbers are Apple M4 Max, `rustc 1.87.0`, single-threaded. Your
hardware will differ; the *ratios* are the point.

---

## References & reading order

If you want to learn this field end-to-end, in roughly this order:

1. [chessprogramming.org](https://www.chessprogramming.org) — the wiki.
   Read "Bitboards", "Sliding Piece Attacks", "Magic Bitboards",
   "Kogge-Stone".
2. Peter Ellis Jones, ["Generating Legal Chess Moves Efficiently"](https://peterellisjones.com/posts/generating-legal-chess-moves-efficiently/)
   (2017). The blueprint for Part 5.
3. Daniel Inführ, ["Gigantua: The Fastest Legal Chess Move Generator"](https://www.codeproject.com/Articles/5313417/Worlds-fastest-Bitboard-Chess-Movegenerator)
   (2021). Extreme end of the count-sink idea.
4. Stockfish source, specifically `movegen.cpp` and `perft.cpp`.
5. cozy-chess source — the closest comparable in Rust.

---

*Status: outline complete, prose being filled in act-by-act. Each level
will grow a code-where-useful expansion and measured numbers where the
ladder bench crate has an entry.*
