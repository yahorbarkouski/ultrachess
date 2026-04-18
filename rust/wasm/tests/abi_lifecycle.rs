//! Direct tests for the slab + handle lifecycle, independent of the TS
//! harness. These exercise the generational-handle contract: free+reuse
//! bumps the generation so stale handles fail safely.
//!
//! The `slab` module is private to the wasm crate, so we test it through
//! its public ABI surface: `ultrachess_new_startpos`, `ultrachess_free`,
//! and a query that consults the handle.

use ultrachess_wasm::{
    ultrachess_abi_version, ultrachess_clone, ultrachess_free, ultrachess_new_startpos,
    ultrachess_side_to_move,
};

const HANDLE_INVALID: u32 = u32::MAX;

#[test]
fn abi_version_is_nonzero() {
    assert!(ultrachess_abi_version() > 0);
}

#[test]
fn new_startpos_returns_a_queryable_handle() {
    let h = ultrachess_new_startpos();
    assert_ne!(h, HANDLE_INVALID);
    // White to move.
    assert_eq!(ultrachess_side_to_move(h), 0);
    ultrachess_free(h);
}

#[test]
fn free_is_idempotent_and_reports_validity() {
    let h = ultrachess_new_startpos();
    assert_eq!(ultrachess_free(h), 1, "first free succeeds");
    assert_eq!(ultrachess_free(h), 0, "double free reports failure");
}

#[test]
fn stale_handle_returns_sentinel_on_query() {
    let h = ultrachess_new_startpos();
    ultrachess_free(h);
    // After free, querying the old handle must NOT return a valid answer.
    // `ultrachess_side_to_move` returns u32::MAX on a bad handle.
    assert_eq!(ultrachess_side_to_move(h), u32::MAX);
}

#[test]
fn free_then_alloc_produces_a_different_handle() {
    // Generational bit flips on free/reuse, so even if the slab slot is
    // recycled the old handle stays invalid.
    let a = ultrachess_new_startpos();
    ultrachess_free(a);
    let b = ultrachess_new_startpos();
    assert_ne!(a, b, "generation bump must differentiate new from stale");
    // Old handle is invalid.
    assert_eq!(ultrachess_side_to_move(a), u32::MAX);
    // New handle works.
    assert_eq!(ultrachess_side_to_move(b), 0);
    ultrachess_free(b);
}

#[test]
fn clone_creates_independent_handle() {
    let a = ultrachess_new_startpos();
    let b = ultrachess_clone(a);
    assert_ne!(b, HANDLE_INVALID);
    assert_ne!(a, b);
    // Freeing b must not affect a.
    ultrachess_free(b);
    assert_eq!(ultrachess_side_to_move(a), 0);
    ultrachess_free(a);
}

#[test]
fn clone_of_invalid_handle_returns_sentinel() {
    assert_eq!(ultrachess_clone(HANDLE_INVALID), HANDLE_INVALID);
}
