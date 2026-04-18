import { afterAll, describe, expect, it } from "vitest";
import {
  AbiVersionMismatchError,
  EXPECTED_ABI_VERSION,
  __resetLoaderForTests,
  getAbi,
  init,
  initSync,
  type UltrachessAbi,
} from "../src/loader.js";

describe("loader — public contract", () => {
  it("EXPECTED_ABI_VERSION is exported as a number", () => {
    expect(typeof EXPECTED_ABI_VERSION).toBe("number");
  });

  it("init() resolves with the typed ABI surface", async () => {
    const abi = await init();
    expect(typeof abi.ultrachess_abi_version).toBe("function");
    expect(abi.ultrachess_abi_version()).toBe(EXPECTED_ABI_VERSION);
  });

  it("init() is idempotent — repeated calls share the same module", async () => {
    const a = await init();
    const b = await init();
    expect(a).toBe(b);
  });

  it("init() with an override buffer still returns the cached module if already initialised", async () => {
    const original = await init();
    // Passing a nonsense buffer should still return the cache, not re-init.
    const second = await init(new Uint8Array([0, 0, 0, 0]));
    expect(second).toBe(original);
  });

  it("AbiVersionMismatchError is an Error subclass with useful fields", () => {
    const e = new AbiVersionMismatchError(5, 3);
    expect(e).toBeInstanceOf(Error);
    expect(e.name).toBe("AbiVersionMismatchError");
    expect(e.expected).toBe(5);
    expect(e.actual).toBe(3);
    expect(String(e)).toContain("5");
    expect(String(e)).toContain("3");
  });

  it("a loader that has been reset returns getAbi() === null before init", async () => {
    // `__resetLoaderForTests` is marked internal for exactly this scenario.
    // After the test we re-init so later tests still work.
    __resetLoaderForTests();
    expect(getAbi()).toBeNull();
  });

  it("initSync reseats the cache from a caller-supplied buffer", async () => {
    // Restore the cache for following tests regardless of outcome.
    __resetLoaderForTests();
    const { readFile } = await import("node:fs/promises");
    const buf = await readFile("assets/ultrachess.wasm");
    const bytes = buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength);
    const abi = initSync(bytes as ArrayBuffer);
    expect(abi.ultrachess_abi_version()).toBe(EXPECTED_ABI_VERSION);
    // Second call with the same cache must be a no-op and return the same ABI.
    const second = initSync(bytes as ArrayBuffer);
    expect(second).toBe(abi);
    expect(getAbi()).toBe(abi);
  });

  afterAll(async () => {
    // Ensure the shared cache is populated for tests that run after us.
    if (!getAbi()) {
      await init();
    }
  });
});

// Silence the unused-type lint.
const _u: UltrachessAbi | null = null;
