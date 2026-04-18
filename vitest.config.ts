import { readFileSync } from "node:fs";
import { defineConfig } from "vitest/config";

const pkg = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8")) as {
  version: string;
};

export default defineConfig({
  // Mirrors tsup.config.ts's `define`: injects the package version into
  // the `__ULTRACHESS_VERSION__` identifier so tests running against
  // source resolve `export const VERSION = __ULTRACHESS_VERSION__`
  // exactly the way the built bundle does.
  define: {
    __ULTRACHESS_VERSION__: JSON.stringify(pkg.version),
  },
  test: {
    include: ["test/**/*.test.ts"],
    environment: "node",
    reporters: ["default"],
    coverage: {
      provider: "v8",
      reporter: ["text-summary", "text", "html"],
      include: ["src/**/*.ts"],
      // src/generated/** is output from `scripts/generate-inline.mjs`; the
      // only runtime content is a base64 literal and a decoder whose behaviour
      // is verified by `test/inline.test.ts` end-to-end.
      exclude: ["src/generated/**"],
      thresholds: {
        lines: 95,
        functions: 95,
        statements: 95,
        branches: 90,
      },
    },
  },
});
