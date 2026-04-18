//! WASM ABI for ultrachess-core.
//!
//! All exports are plain `extern "C"` with integer-only arguments. Strings
//! are transferred via linear-memory pointer+length pairs (UTF-8). Hot-path
//! operations (move generation, perft, make/unmake, state queries) cross the
//! JS↔WASM boundary exactly once per operation.
//!
//! **ABI version**: bump `ultrachess_abi_version` when the exports' signatures
//! or semantics change in a way that breaks the TS loader.

mod slab;

use core::slice;
use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};

use ultrachess_core::bitboard::pop_lsb;
use ultrachess_core::chess_move::{Move, MoveKind};
use ultrachess_core::fen::{parse_fen, write_fen};
use ultrachess_core::movegen::{generate_legal_moves, MoveList};
use ultrachess_core::perft::perft;
use ultrachess_core::pgn::{parse_pgn, Termination};
use ultrachess_core::san::{move_to_san, san_to_move};
use ultrachess_core::types::{Color, Piece, PieceType, Square};

use crate::slab::{with_pgn, with_pgn_mut, with_positions, with_positions_mut, HANDLE_INVALID};

const ABI_VERSION: u32 = 2;

// Scratch areas. WASM is single-threaded; `static mut` is sound.
const MOVE_SCRATCH_CAP: usize = 512;
const STRING_SCRATCH_CAP: usize = 65_536; // ample for large PGNs

static mut MOVE_SCRATCH: [u32; MOVE_SCRATCH_CAP] = [0u32; MOVE_SCRATCH_CAP];
static mut STRING_SCRATCH: [u8; STRING_SCRATCH_CAP] = [0u8; STRING_SCRATCH_CAP];

/// High 32 bits of the last u64-returning export. Returned via a dedicated
/// export to avoid depending on i64 in the JS↔WASM ABI.
static LAST_U64_HI: AtomicU32 = AtomicU32::new(0);

// --------------------------------------------------------------------------
// Smoke + version
// --------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn ultrachess_abi_check(a: u32, b: u32) -> u32 {
    a.wrapping_add(b)
}

#[no_mangle]
pub extern "C" fn ultrachess_abi_version() -> u32 {
    ABI_VERSION
}

// --------------------------------------------------------------------------
// Scratch accessors
// --------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn ultrachess_move_scratch_ptr() -> u32 {
    core::ptr::addr_of!(MOVE_SCRATCH) as usize as u32
}

#[no_mangle]
pub extern "C" fn ultrachess_move_scratch_cap() -> u32 {
    MOVE_SCRATCH_CAP as u32
}

#[no_mangle]
pub extern "C" fn ultrachess_string_scratch_ptr() -> u32 {
    core::ptr::addr_of!(STRING_SCRATCH) as usize as u32
}

#[no_mangle]
pub extern "C" fn ultrachess_string_scratch_cap() -> u32 {
    STRING_SCRATCH_CAP as u32
}

#[no_mangle]
pub extern "C" fn ultrachess_last_u64_hi() -> u32 {
    LAST_U64_HI.load(Ordering::Relaxed)
}

fn set_u64(value: u64) -> u32 {
    LAST_U64_HI.store((value >> 32) as u32, Ordering::Relaxed);
    value as u32
}

// --------------------------------------------------------------------------
// Position lifecycle
// --------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn ultrachess_new_startpos() -> u32 {
    with_positions_mut(|s| s.alloc(ultrachess_core::position::Position::startpos()))
}

/// # Safety
/// `fen_ptr` + `fen_len` must refer to valid UTF-8 bytes in this module's
/// linear memory for at least `fen_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ultrachess_new_from_fen(fen_ptr: u32, fen_len: u32) -> u32 {
    let slice = unsafe { slice::from_raw_parts(fen_ptr as *const u8, fen_len as usize) };
    let Ok(s) = core::str::from_utf8(slice) else {
        return HANDLE_INVALID;
    };
    match parse_fen(s) {
        Ok(p) => with_positions_mut(|slab| slab.alloc(p)),
        Err(_) => HANDLE_INVALID,
    }
}

#[no_mangle]
pub extern "C" fn ultrachess_free(handle: u32) -> u32 {
    with_positions_mut(|s| s.free(handle)) as u32
}

#[no_mangle]
pub extern "C" fn ultrachess_clone(handle: u32) -> u32 {
    let cloned = with_positions(|s| s.get(handle).cloned());
    match cloned {
        Some(p) => with_positions_mut(|slab| slab.alloc(p)),
        None => HANDLE_INVALID,
    }
}

// --------------------------------------------------------------------------
// State queries
// --------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn ultrachess_hash(handle: u32) -> u32 {
    with_positions(|s| match s.get(handle) {
        Some(p) => set_u64(p.hash()),
        None => {
            set_u64(0);
            0
        }
    })
}

#[no_mangle]
pub extern "C" fn ultrachess_side_to_move(handle: u32) -> u32 {
    with_positions(|s| match s.get(handle) {
        Some(p) => p.side_to_move.index() as u32,
        None => u32::MAX,
    })
}

#[no_mangle]
pub extern "C" fn ultrachess_halfmove(handle: u32) -> u32 {
    with_positions(|s| s.get(handle).map(|p| p.halfmove as u32).unwrap_or(u32::MAX))
}

#[no_mangle]
pub extern "C" fn ultrachess_fullmove(handle: u32) -> u32 {
    with_positions(|s| s.get(handle).map(|p| p.fullmove as u32).unwrap_or(u32::MAX))
}

macro_rules! bool_query {
    ($name:ident, $method:ident) => {
        #[no_mangle]
        pub extern "C" fn $name(handle: u32) -> u32 {
            with_positions(|s| s.get(handle).map_or(0u32, |p| p.$method() as u32))
        }
    };
}

bool_query!(ultrachess_in_check, in_check);
bool_query!(ultrachess_is_checkmate, is_checkmate);
bool_query!(ultrachess_is_stalemate, is_stalemate);
bool_query!(
    ultrachess_is_insufficient_material,
    is_insufficient_material
);
bool_query!(ultrachess_is_threefold_repetition, is_threefold_repetition);
bool_query!(ultrachess_is_fifty_move_rule, is_fifty_move_rule);
bool_query!(ultrachess_is_draw, is_draw);
bool_query!(ultrachess_is_game_over, is_game_over);

#[no_mangle]
pub extern "C" fn ultrachess_piece_at(handle: u32, sq: u32) -> u32 {
    if sq >= 64 {
        return 255;
    }
    with_positions(|s| match s.get(handle) {
        Some(p) => p.piece_at(Square(sq as u8)).0 as u32,
        None => 255,
    })
}

// --------------------------------------------------------------------------
// Attack queries
// --------------------------------------------------------------------------

/// Returns 0/1. `by_color` is 0 (White) or 1 (Black).
#[no_mangle]
pub extern "C" fn ultrachess_is_attacked(handle: u32, sq: u32, by_color: u32) -> u32 {
    if sq >= 64 || by_color >= 2 {
        return 0;
    }
    with_positions(|s| match s.get(handle) {
        Some(p) => p.is_attacked(Square(sq as u8), Color::from_index(by_color as usize)) as u32,
        None => 0,
    })
}

/// Write the list of squares whose occupants attack `sq` into `out_ptr` as
/// u8 values. Returns the count. Pass `out_ptr = 0` to use the string
/// scratch area instead (interpret as `Uint8Array`).
///
/// # Safety
/// If `out_ptr != 0` it must point to `cap` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn ultrachess_attackers(
    handle: u32,
    sq: u32,
    by_color: u32,
    out_ptr: u32,
    cap: u32,
) -> u32 {
    if sq >= 64 || by_color >= 2 {
        return 0;
    }
    with_positions(|s| {
        let Some(p) = s.get(handle) else { return 0 };
        let color = Color::from_index(by_color as usize);
        let mut bb = p.attackers_to(Square(sq as u8), color, p.occupied());
        let mut count = 0usize;
        let cap = cap as usize;
        while bb != 0 && count < cap {
            let attacker_sq = pop_lsb(&mut bb);
            unsafe {
                if out_ptr == 0 {
                    STRING_SCRATCH[count] = attacker_sq.0;
                } else {
                    *(out_ptr as *mut u8).add(count) = attacker_sq.0;
                }
            }
            count += 1;
        }
        count as u32
    })
}

/// Write the squares holding `(color, piece_type)` pieces into `out_ptr`.
/// Returns the count.
///
/// # Safety
/// If `out_ptr != 0` it must point to `cap` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn ultrachess_find_piece(
    handle: u32,
    color: u32,
    piece_type: u32,
    out_ptr: u32,
    cap: u32,
) -> u32 {
    if color >= 2 || piece_type >= 6 {
        return 0;
    }
    with_positions(|s| {
        let Some(p) = s.get(handle) else { return 0 };
        let c = Color::from_index(color as usize);
        let pt = PieceType::from_index(piece_type as usize);
        let mut bb = p.piece_bb(c, pt);
        let mut count = 0usize;
        let cap = cap as usize;
        while bb != 0 && count < cap {
            let sq = pop_lsb(&mut bb);
            unsafe {
                if out_ptr == 0 {
                    STRING_SCRATCH[count] = sq.0;
                } else {
                    *(out_ptr as *mut u8).add(count) = sq.0;
                }
            }
            count += 1;
        }
        count as u32
    })
}

// --------------------------------------------------------------------------
// Position edits (put / remove) — invalidate history
// --------------------------------------------------------------------------

/// Place a piece on a square. Invalidates move history. `piece_code` follows
/// Rust's `Piece` encoding: `(color << 3) | piece_type`, i.e. 0..12.
///
/// Returns the replaced piece code (0..12), or 255 if the square was empty,
/// or 254 on invalid args (bad handle / bad code / bad square).
#[no_mangle]
pub extern "C" fn ultrachess_put(handle: u32, piece_code: u32, sq: u32) -> u32 {
    if sq >= 64 {
        return 254;
    }
    // Valid piece codes: 0..=5 (white pawn..king), 8..=13 (black pawn..king).
    let color_bits = piece_code >> 3;
    let type_bits = piece_code & 0b111;
    if color_bits > 1 || type_bits > 5 || (piece_code & !0b0000_1111) != 0 {
        return 254;
    }
    with_positions_mut(|s| {
        let Some(p) = s.get_mut(handle) else {
            return 254;
        };
        // Two-king invariant: disallow putting a king if one already exists
        // for that color (unless we're replacing it on the same square).
        let new_piece = Piece(piece_code as u8);
        if new_piece.piece_type() == PieceType::King {
            let existing = p.piece_bb(new_piece.color(), PieceType::King);
            if existing != 0 && existing != Square(sq as u8).bb() {
                return 254;
            }
        }
        match p.put_at(Square(sq as u8), new_piece) {
            Some(replaced) => replaced.0 as u32,
            None => 255,
        }
    })
}

/// Remove the piece at `sq`. Invalidates move history. Returns the removed
/// piece code, or 255 if empty, 254 if handle invalid / sq out of range.
#[no_mangle]
pub extern "C" fn ultrachess_remove(handle: u32, sq: u32) -> u32 {
    if sq >= 64 {
        return 254;
    }
    with_positions_mut(|s| {
        let Some(p) = s.get_mut(handle) else {
            return 254;
        };
        match p.remove_at(Square(sq as u8)) {
            Some(removed) => removed.0 as u32,
            None => 255,
        }
    })
}

// --------------------------------------------------------------------------
// Move generation / make / unmake
// --------------------------------------------------------------------------

thread_local! {
    static SHARED_MOVE_LIST: RefCell<MoveList> = RefCell::new(MoveList::new());
}

/// # Safety
/// If `out_ptr != 0` it must point to `cap * 4` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn ultrachess_generate_moves(handle: u32, out_ptr: u32, cap: u32) -> u32 {
    with_positions(|s| {
        let Some(p) = s.get(handle) else { return 0 };
        SHARED_MOVE_LIST.with(|ml| {
            let mut ml = ml.borrow_mut();
            generate_legal_moves(p, &mut ml);
            let n = ml.len().min(cap as usize);
            unsafe {
                let dst = if out_ptr == 0 {
                    &mut MOVE_SCRATCH[..n]
                } else {
                    slice::from_raw_parts_mut(out_ptr as *mut u32, n)
                };
                for (i, m) in ml.as_slice()[..n].iter().enumerate() {
                    dst[i] = m.0 as u32;
                }
            }
            n as u32
        })
    })
}

/// 0 = success; 1 = invalid handle; 2 = illegal move.
#[no_mangle]
pub extern "C" fn ultrachess_make_move(handle: u32, packed: u32) -> u32 {
    with_positions_mut(|s| {
        let Some(p) = s.get_mut(handle) else { return 1 };
        let m = Move(packed as u16);
        let mut ml = MoveList::new();
        generate_legal_moves(p, &mut ml);
        if !ml.iter().any(|x| x.0 == m.0) {
            return 2;
        }
        p.make_move(m);
        0
    })
}

/// 0 = success; 1 = invalid handle; 2 = empty history.
#[no_mangle]
pub extern "C" fn ultrachess_undo_with_move(handle: u32, packed: u32) -> u32 {
    with_positions_mut(|s| {
        let Some(p) = s.get_mut(handle) else { return 1 };
        if p.history_len() == 0 {
            return 2;
        }
        p.unmake_move(Move(packed as u16));
        0
    })
}

// --------------------------------------------------------------------------
// Perft
// --------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn ultrachess_perft(handle: u32, depth: u32) -> u32 {
    with_positions_mut(|s| match s.get_mut(handle) {
        Some(p) => {
            let n = perft(p, depth);
            set_u64(n)
        }
        None => {
            set_u64(0);
            0
        }
    })
}

// --------------------------------------------------------------------------
// FEN / SAN / ASCII strings
// --------------------------------------------------------------------------

/// Write the current FEN into `out_ptr` (or string scratch if 0). Returns
/// the full byte length (even if truncated). The TS caller should pass
/// `STRING_SCRATCH_CAP` on the first try — 128 is ample for any FEN.
///
/// # Safety
/// If `out_ptr != 0` it must point to `cap` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn ultrachess_fen_write(handle: u32, out_ptr: u32, cap: u32) -> u32 {
    with_positions(|s| {
        let Some(p) = s.get(handle) else { return 0 };
        unsafe { write_bytes_to_scratch(write_fen(p).as_bytes(), out_ptr, cap) }
    })
}

/// # Safety
/// As `ultrachess_fen_write`.
#[no_mangle]
pub unsafe extern "C" fn ultrachess_san_write(
    handle: u32,
    packed: u32,
    out_ptr: u32,
    cap: u32,
) -> u32 {
    with_positions_mut(|s| {
        let Some(p) = s.get_mut(handle) else { return 0 };
        let san = move_to_san(p, Move(packed as u16));
        unsafe { write_bytes_to_scratch(san.as_bytes(), out_ptr, cap) }
    })
}

/// Parse SAN against the current position.
///
/// # Safety
/// `utf8_ptr` + `utf8_len` must be a valid UTF-8 byte slice.
#[no_mangle]
pub unsafe extern "C" fn ultrachess_san_parse(handle: u32, utf8_ptr: u32, utf8_len: u32) -> u32 {
    let slice = unsafe { slice::from_raw_parts(utf8_ptr as *const u8, utf8_len as usize) };
    let Ok(san) = core::str::from_utf8(slice) else {
        return u32::MAX;
    };
    with_positions(|s| match s.get(handle) {
        Some(p) => match san_to_move(p, san) {
            Ok(m) => m.0 as u32,
            Err(_) => u32::MAX,
        },
        None => u32::MAX,
    })
}

/// Render the position as a human-readable ASCII grid.
///
/// # Safety
/// As `ultrachess_fen_write`.
#[no_mangle]
pub unsafe extern "C" fn ultrachess_ascii_write(handle: u32, out_ptr: u32, cap: u32) -> u32 {
    with_positions(|s| {
        let Some(p) = s.get(handle) else { return 0 };
        let ascii = p.ascii();
        unsafe { write_bytes_to_scratch(ascii.as_bytes(), out_ptr, cap) }
    })
}

// --------------------------------------------------------------------------
// PGN parsing (separate slab)
// --------------------------------------------------------------------------

/// Parse a PGN string and return a handle to the parsed game tree. Use the
/// `ultrachess_pgn_*` accessors to extract headers and moves; call
/// `ultrachess_pgn_free` when done.
///
/// # Safety
/// `pgn_ptr` + `pgn_len` must be a valid UTF-8 byte slice.
#[no_mangle]
pub unsafe extern "C" fn ultrachess_pgn_parse(pgn_ptr: u32, pgn_len: u32) -> u32 {
    let slice = unsafe { slice::from_raw_parts(pgn_ptr as *const u8, pgn_len as usize) };
    let Ok(s) = core::str::from_utf8(slice) else {
        return HANDLE_INVALID;
    };
    match parse_pgn(s) {
        Ok(game) => with_pgn_mut(|slab| slab.alloc(game)),
        Err(_) => HANDLE_INVALID,
    }
}

#[no_mangle]
pub extern "C" fn ultrachess_pgn_free(handle: u32) -> u32 {
    with_pgn_mut(|s| s.free(handle)) as u32
}

#[no_mangle]
pub extern "C" fn ultrachess_pgn_header_count(handle: u32) -> u32 {
    with_pgn(|s| s.get(handle).map(|g| g.headers.len() as u32).unwrap_or(0))
}

/// Write the `idx`th header's key into `out_ptr`/scratch. Returns byte length.
///
/// # Safety
/// As `ultrachess_fen_write`.
#[no_mangle]
pub unsafe extern "C" fn ultrachess_pgn_header_key(
    handle: u32,
    idx: u32,
    out_ptr: u32,
    cap: u32,
) -> u32 {
    with_pgn(|s| {
        let Some(g) = s.get(handle) else { return 0 };
        let Some((k, _)) = g.headers.get(idx as usize) else {
            return 0;
        };
        unsafe { write_bytes_to_scratch(k.as_bytes(), out_ptr, cap) }
    })
}

/// Write the `idx`th header's value. See `ultrachess_pgn_header_key`.
///
/// # Safety
/// As `ultrachess_fen_write`.
#[no_mangle]
pub unsafe extern "C" fn ultrachess_pgn_header_value(
    handle: u32,
    idx: u32,
    out_ptr: u32,
    cap: u32,
) -> u32 {
    with_pgn(|s| {
        let Some(g) = s.get(handle) else { return 0 };
        let Some((_, v)) = g.headers.get(idx as usize) else {
            return 0;
        };
        unsafe { write_bytes_to_scratch(v.as_bytes(), out_ptr, cap) }
    })
}

#[no_mangle]
pub extern "C" fn ultrachess_pgn_mainline_len(handle: u32) -> u32 {
    with_pgn(|s| s.get(handle).map(|g| g.mainline.len() as u32).unwrap_or(0))
}

/// Write the SAN of the `idx`th mainline move. Returns byte length.
///
/// # Safety
/// As `ultrachess_fen_write`.
#[no_mangle]
pub unsafe extern "C" fn ultrachess_pgn_mainline_san(
    handle: u32,
    idx: u32,
    out_ptr: u32,
    cap: u32,
) -> u32 {
    with_pgn(|s| {
        let Some(g) = s.get(handle) else { return 0 };
        let Some(node) = g.mainline.get(idx as usize) else {
            return 0;
        };
        unsafe { write_bytes_to_scratch(node.san.as_bytes(), out_ptr, cap) }
    })
}

/// 0 = Unknown, 1 = WhiteWon, 2 = BlackWon, 3 = Draw.
#[no_mangle]
pub extern "C" fn ultrachess_pgn_termination(handle: u32) -> u32 {
    with_pgn(|s| {
        s.get(handle).map_or(0, |g| match g.termination {
            Termination::Unknown => 0,
            Termination::WhiteWon => 1,
            Termination::BlackWon => 2,
            Termination::Draw => 3,
        })
    })
}

// --------------------------------------------------------------------------
// Move introspection (cheap helpers so TS doesn't re-implement the layout)
// --------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn ultrachess_move_from(packed: u32) -> u32 {
    Move(packed as u16).from().0 as u32
}

#[no_mangle]
pub extern "C" fn ultrachess_move_to(packed: u32) -> u32 {
    Move(packed as u16).to().0 as u32
}

#[no_mangle]
pub extern "C" fn ultrachess_move_kind(packed: u32) -> u32 {
    Move(packed as u16).kind() as u32
}

#[no_mangle]
pub extern "C" fn ultrachess_move_promotion(packed: u32) -> u32 {
    let m = Move(packed as u16);
    if m.kind() == MoveKind::Promotion {
        m.promotion_piece().index() as u32
    } else {
        u32::MAX
    }
}

// --------------------------------------------------------------------------
// Internal helpers
// --------------------------------------------------------------------------

/// Copy `bytes` into either the caller-supplied buffer at `out_ptr` (with
/// capacity `cap`), or — if `out_ptr == 0` — into `STRING_SCRATCH`.
///
/// Returns the *full* byte length (even if truncated by `cap`), so a caller
/// can detect truncation and retry with a larger buffer.
///
/// # Safety
/// If `out_ptr != 0` it must point to `cap` writable bytes.
unsafe fn write_bytes_to_scratch(bytes: &[u8], out_ptr: u32, cap: u32) -> u32 {
    let n = bytes.len().min(cap as usize);
    unsafe {
        let dst = if out_ptr == 0 {
            &mut STRING_SCRATCH[..n]
        } else {
            slice::from_raw_parts_mut(out_ptr as *mut u8, n)
        };
        dst.copy_from_slice(&bytes[..n]);
    }
    bytes.len() as u32
}
