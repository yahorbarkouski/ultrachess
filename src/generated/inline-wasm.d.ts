// Type-only stub for `src/generated/inline-wasm.ts`.
//
// The real `inline-wasm.ts` is produced by `scripts/generate-inline.mjs`
// (run as part of `npm run build:inline`) and is intentionally gitignored —
// the embedded base64 changes whenever the .wasm changes, which would
// otherwise produce noisy diffs on every rebuild.
//
// This `.d.ts` lets `tsc --noEmit` succeed on a fresh clone where the
// generated `.ts` file does not yet exist. When the generated `.ts` is
// present, TypeScript prefers it over this declaration file.

/** Base64-encoded copy of `assets/ultrachess.wasm`. Empty in the stub. */
export const WASM_BASE64: string;

/** Decode the embedded WASM into a fresh ArrayBuffer. Used once by
 *  `ultrachess/inline` at module load. */
export function decodeWasm(): ArrayBuffer;
