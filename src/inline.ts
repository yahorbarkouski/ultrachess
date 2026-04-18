//! Synchronous entry point — WASM is embedded as base64 and instantiated at
//! import time. Zero I/O, zero `await` needed for construction.
//!
//! Use this entry if:
//!   - You can't / don't want to use top-level await.
//!   - You're running in an environment where `fetch`/`fs.readFile` are
//!     awkward (some sandboxed runtimes, library-in-library embedding).
//!
//! Cost: the published bundle is ~33% larger because base64 inflates the
//! WASM payload. If bundle size matters and async init is fine, prefer the
//! default `ultrachess` entry.
//!
//! Example:
//!   ```ts
//!   import { Chess } from "ultrachess/inline";
//!   const chess = Chess.createSync();
//!   chess.move("e4");
//!   ```

import { decodeWasm } from "./generated/inline-wasm.js";
import { initSync } from "./loader.js";

// Eager initialisation: by the time any symbol is imported from this
// module, the WASM instance is ready. `initSync` is idempotent — a second
// call from user code is a no-op.
initSync(decodeWasm());

// Re-export the full public API from the standard entry. Consumers get the
// same classes / types / functions; the only difference is that the ABI is
// guaranteed to be initialised synchronously.
export * from "./index.js";
