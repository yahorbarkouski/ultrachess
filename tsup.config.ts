import { defineConfig } from "tsup";

export default defineConfig({
  entry: {
    index: "src/index.ts",
    inline: "src/inline.ts",
    "low-level": "src/low-level.ts",
  },
  format: ["esm", "cjs"],
  dts: true,
  sourcemap: true,
  clean: true,
  target: "es2022",
  outDir: "dist",
  minify: false,
  splitting: true, // share code across the three entries
  // The loader resolves `assets/ultrachess.wasm` via `new URL("...",
  // import.meta.url)`. `import.meta` is a syntax error in CJS, so without
  // this tsup's CJS bundle would fail to parse. `shims: true` injects the
  // equivalent CJS expression (pathToFileURL(__filename)) so both formats
  // behave the same at runtime — the idiomatic dual-publish fix.
  shims: true,
});
