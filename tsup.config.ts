import { readFileSync } from "node:fs";
import { defineConfig } from "tsup";

const pkg = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8")) as {
  version: string;
};

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
  // Inline the package version at build time so `export const VERSION`
  // in src/index.ts resolves to the literal string from package.json.
  // release-please bumps package.json on every release, so this stays
  // in sync with no manual editing.
  define: {
    __ULTRACHESS_VERSION__: JSON.stringify(pkg.version),
  },
});
