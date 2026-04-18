//! Low-level entry — exposes the raw `UltrachessAbi` surface and linear-memory
//! helpers for power users (custom search engines, transposition tables, …).
//!
//! Stability: breaking changes bump `EXPECTED_ABI_VERSION`. The ABI surface
//! is stable within a major version of `ultrachess`.

export {
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
} from "./loader.js";
