//! Reproducible xorshift64 PRNG.
//!
//! Identical output sequence to `Rng` in `rust/core/tests/common/mod.rs`
//! — if a divergence here bisects to a specific seed + ply, the same seed
//! can be fed to the Rust harness to reproduce the position.
//!
//! Convention (see `TESTING.md`): seeds are literals in test bodies. Never
//! seed from the clock.

/** u64 bit-mask for BigInt arithmetic. */
const MASK_64 = (1n << 64n) - 1n;
/** u32 bit-mask. */
const MASK_32 = 0xffff_ffffn;

export class Rng {
  private state: bigint;

  /**
   * Seed with any integer. Passing 0 is remapped to a non-zero constant —
   * xorshift64 cannot be seeded with zero (it would output an infinite
   * zero stream).
   */
  constructor(seed: number | bigint) {
    let s = BigInt(seed) & MASK_64;
    if (s === 0n) s = 0x9e37_79b9_7f4a_7c15n;
    this.state = s;
  }

  /** Advance the state; return the low 32 bits as a JS number. */
  next32(): number {
    let x = this.state;
    x = (x ^ (x << 13n)) & MASK_64;
    x = x ^ (x >> 7n);
    x = (x ^ (x << 17n)) & MASK_64;
    this.state = x;
    return Number(x & MASK_32) >>> 0;
  }

  /** Uniform integer in `[0, n)` — `n` must be a positive safe integer. */
  bounded(n: number): number {
    if (n <= 0) throw new RangeError(`bounded(${n}): n must be > 0`);
    return this.next32() % n;
  }
}
