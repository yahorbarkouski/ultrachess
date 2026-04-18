//! Generational-handle slab allocator.
//!
//! - 24-bit index (up to 16,777,215 concurrent entries — vastly beyond any
//!   realistic usage).
//! - 8-bit generation (wraps at 256). Every free→reuse cycle bumps the
//!   generation so stale handles hit a controlled error code rather than
//!   touching unrelated state.
//! - Single-threaded (WASM); no locking needed.
//!
//! The ABI layer instantiates two slabs: one for `Position` (long-lived
//! across many make/unmake calls) and one for parsed PGN `Game` trees
//! (short-lived; typically freed after extraction).

use core::cell::RefCell;

use ultrachess_core::pgn::Game;
use ultrachess_core::Position;

pub const HANDLE_INVALID: u32 = u32::MAX;

const INDEX_MASK: u32 = 0x00FF_FFFF;
const GEN_SHIFT: u32 = 24;

struct Entry<T> {
    value: Option<T>,
    generation: u8,
}

pub struct Slab<T> {
    entries: Vec<Entry<T>>,
    free: Vec<u32>,
}

impl<T> Slab<T> {
    pub const fn new() -> Self {
        Self { entries: Vec::new(), free: Vec::new() }
    }

    /// Insert and return an opaque handle.
    pub fn alloc(&mut self, value: T) -> u32 {
        if let Some(idx) = self.free.pop() {
            let e = &mut self.entries[idx as usize];
            debug_assert!(e.value.is_none());
            e.value = Some(value);
            ((e.generation as u32) << GEN_SHIFT) | (idx & INDEX_MASK)
        } else {
            let idx = self.entries.len() as u32;
            if idx & !INDEX_MASK != 0 {
                // Index overflow — realistically unreachable (16M).
                return HANDLE_INVALID;
            }
            self.entries.push(Entry { value: Some(value), generation: 0 });
            idx & INDEX_MASK // generation = 0
        }
    }

    /// Free a handle. Returns `true` if the handle was valid.
    pub fn free(&mut self, handle: u32) -> bool {
        let (idx, gen) = decode(handle);
        let Some(e) = self.entries.get_mut(idx as usize) else { return false };
        if e.generation != gen || e.value.is_none() {
            return false;
        }
        e.value = None;
        // Bump generation so subsequent allocs at this slot produce a fresh handle.
        e.generation = e.generation.wrapping_add(1);
        self.free.push(idx);
        true
    }

    pub fn get(&self, handle: u32) -> Option<&T> {
        let (idx, gen) = decode(handle);
        let e = self.entries.get(idx as usize)?;
        if e.generation != gen {
            return None;
        }
        e.value.as_ref()
    }

    pub fn get_mut(&mut self, handle: u32) -> Option<&mut T> {
        let (idx, gen) = decode(handle);
        let e = self.entries.get_mut(idx as usize)?;
        if e.generation != gen {
            return None;
        }
        e.value.as_mut()
    }
}

fn decode(handle: u32) -> (u32, u8) {
    (handle & INDEX_MASK, (handle >> GEN_SHIFT) as u8)
}

// --- Thread-local slabs (WASM is single-threaded; no Mutex needed) --------

thread_local! {
    static POSITION_SLAB: RefCell<Slab<Position>> =
        const { RefCell::new(Slab::new()) };
    static PGN_SLAB: RefCell<Slab<Game>> =
        const { RefCell::new(Slab::new()) };
}

pub fn with_positions<R>(f: impl FnOnce(&Slab<Position>) -> R) -> R {
    POSITION_SLAB.with(|s| f(&s.borrow()))
}

pub fn with_positions_mut<R>(f: impl FnOnce(&mut Slab<Position>) -> R) -> R {
    POSITION_SLAB.with(|s| f(&mut s.borrow_mut()))
}

pub fn with_pgn<R>(f: impl FnOnce(&Slab<Game>) -> R) -> R {
    PGN_SLAB.with(|s| f(&s.borrow()))
}

pub fn with_pgn_mut<R>(f: impl FnOnce(&mut Slab<Game>) -> R) -> R {
    PGN_SLAB.with(|s| f(&mut s.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_and_get() {
        let mut s: Slab<i32> = Slab::new();
        let h = s.alloc(42);
        assert_eq!(s.get(h), Some(&42));
        assert!(s.free(h));
        assert!(s.get(h).is_none()); // stale handle now fails
    }

    #[test]
    fn reallocation_bumps_generation() {
        let mut s: Slab<i32> = Slab::new();
        let h1 = s.alloc(1);
        assert!(s.free(h1));
        let h2 = s.alloc(2);
        assert_ne!(h1, h2, "handle must differ after generation bump");
        assert!(s.get(h1).is_none());
        assert_eq!(s.get(h2), Some(&2));
    }

    #[test]
    fn double_free_is_noop() {
        let mut s: Slab<i32> = Slab::new();
        let h = s.alloc(7);
        assert!(s.free(h));
        assert!(!s.free(h), "double-free must return false");
    }
}
