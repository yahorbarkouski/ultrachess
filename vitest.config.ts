import { defineConfig } from "vitest/config";

export default defineConfig({
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
