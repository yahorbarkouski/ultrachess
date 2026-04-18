import { describe, expect, it } from "vitest";
import { abiCheck, EXPECTED_ABI_VERSION, init } from "../src/index.js";

describe("Phase 0: WASM roundtrip", () => {
  it("loads the WASM module without error", async () => {
    const mod = await init();
    expect(typeof mod.ultrachess_abi_check).toBe("function");
    expect(typeof mod.ultrachess_abi_version).toBe("function");
  });

  it("ABI version matches loader expectation", async () => {
    const mod = await init();
    expect(mod.ultrachess_abi_version()).toBe(EXPECTED_ABI_VERSION);
  });

  it("can call a trivial Rust function and get the expected result", async () => {
    expect(await abiCheck(2, 3)).toBe(5);
    expect(await abiCheck(0, 0)).toBe(0);
    expect(await abiCheck(1, 1)).toBe(2);
  });

  it("wraps on unsigned 32-bit overflow", async () => {
    // 0xFFFFFFFF + 1 wraps to 0.
    expect(await abiCheck(0xffffffff, 1)).toBe(0);
    expect(await abiCheck(0xfffffffe, 3)).toBe(1);
  });

  it("init() is idempotent — repeated calls return the same module", async () => {
    const m1 = await init();
    const m2 = await init();
    expect(m1).toBe(m2);
  });
});
