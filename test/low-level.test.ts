//! Coverage for `src/low-level.ts` — the raw ABI + memory helpers.

import { describe, expect, it } from "vitest";
import {
  AbiVersionMismatchError,
  EXPECTED_ABI_VERSION,
  getAbi,
  init,
  initSync,
  readStringFromMemory,
  readStringScratch,
  readU64,
  type UltrachessAbi,
  writeStringToScratch,
} from "../src/low-level.js";

describe("low-level — re-exports", () => {
  it("init / initSync / getAbi are all re-exported", () => {
    expect(typeof init).toBe("function");
    expect(typeof initSync).toBe("function");
    expect(typeof getAbi).toBe("function");
  });

  it("EXPECTED_ABI_VERSION is re-exported and is the number the loader expects", () => {
    expect(typeof EXPECTED_ABI_VERSION).toBe("number");
  });

  it("AbiVersionMismatchError is re-exported", () => {
    expect(typeof AbiVersionMismatchError).toBe("function");
    const e = new AbiVersionMismatchError(1, 2);
    expect(e).toBeInstanceOf(Error);
  });
});

describe("low-level — memory helpers", () => {
  let abi: UltrachessAbi;

  it("loads the ABI for helper tests", async () => {
    abi = await init();
    expect(abi).toBeTruthy();
  });

  it("writeStringToScratch + readStringFromMemory round-trip", async () => {
    abi = await init();
    const s = "hello ultrachess";
    const { ptr, len } = writeStringToScratch(abi, s);
    expect(len).toBe(s.length);
    expect(readStringFromMemory(abi, ptr, len)).toBe(s);
  });

  it("writeStringToScratch throws when the string exceeds scratch capacity", async () => {
    abi = await init();
    const cap = abi.ultrachess_string_scratch_cap();
    const huge = "x".repeat(cap + 1);
    expect(() => writeStringToScratch(abi, huge)).toThrow(RangeError);
  });

  it("readStringScratch reads at the scratch pointer for the given length", async () => {
    abi = await init();
    const s = "scratch-read";
    writeStringToScratch(abi, s);
    expect(readStringScratch(abi, s.length)).toBe(s);
  });

  it("readStringFromMemory clamps when given an over-long length", async () => {
    abi = await init();
    // Don't feed it a ludicrous length; the guard should clamp to the
    // remaining bytes of the buffer rather than crash.
    const ptr = abi.ultrachess_string_scratch_ptr();
    const s = readStringFromMemory(abi, ptr, 0);
    expect(s).toBe("");
  });

  it("readU64 reconstructs values > 2^32 from low + scratch high", async () => {
    abi = await init();
    // Call perft which sets the high-word scratch. Depth 5 of startpos is
    // 4,865,609 — still fits in u32, so high should be 0.
    const h = abi.ultrachess_new_startpos();
    try {
      const lo = abi.ultrachess_perft(h, 3);
      const nodes = readU64(abi, lo);
      expect(nodes).toBe(8902n);
    } finally {
      abi.ultrachess_free(h);
    }
  });
});
