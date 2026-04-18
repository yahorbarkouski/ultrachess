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
});
