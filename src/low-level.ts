//! Low-level entry — exposes the raw `UltrachessAbi` surface and linear-memory
//! helpers for power users building on top of the library (e.g. implementing
//! their own search engine or transposition table).
//!
//! **Stability**: this entry tracks the WASM ABI version directly. Breaking
//! changes bump `EXPECTED_ABI_VERSION`. If you pin to a major version of
//! `ultrachessjs`, the ABI surface is stable within that major.

export {
  init,
  initSync,
  getAbi,
  AbiVersionMismatchError,
  EXPECTED_ABI_VERSION,
  writeStringToScratch,
  readStringFromMemory,
  readStringScratch,
  readU64,
  type UltrachessAbi,
} from "./loader.js";
