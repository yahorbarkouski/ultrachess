#!/usr/bin/env node
// Runs binaryen's `wasm-opt` over `assets/ultrachess.wasm` with a feature
// set that matches what `rustc --target wasm32-unknown-unknown` actually
// emits in stable Rust.
//
// Previously we invoked wasm-opt inline from package.json with
// `--enable-bulk-memory` only and a trailing `|| true`. Two problems:
//
//  1. `rustc` 1.87+ emits `i32.extend8_s` / `i32.extend16_s` (the
//     sign-extension opcodes, a standard Wasm feature since 2019). Without
//     `--enable-sign-ext`, wasm-opt's validator rejects the input with
//     "Fatal: error validating input" and the whole optimiser bails out.
//  2. The `|| true` suppressed that error, so the UNOPTIMISED wasm got
//     shipped instead — we only noticed when the published tarball grew
//     from 1.3 MB → 1.6 MB.
//
// This script makes both failure modes loud:
//   - `wasm-opt` not on PATH → skip with a clear warning (dev-box friendly).
//   - `wasm-opt` present but optimisation fails → non-zero exit.

import { spawnSync } from "node:child_process";
import { existsSync, renameSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "..");
const wasm = resolve(repo, "assets/ultrachess.wasm");
const tmp = `${wasm}.opt`;

if (!existsSync(wasm)) {
  console.error(
    `wasm-opt: ${wasm} missing — did \`cargo build --target wasm32-unknown-unknown\` run?`,
  );
  process.exit(1);
}

// Probe directly — `spawnSync("wasm-opt", ...)` emits ENOENT when the
// binary isn't on PATH. No shell needed (avoids Node's DEP0190 warning).
const probe = spawnSync("wasm-opt", ["--version"], { stdio: "pipe" });
if (probe.error?.code === "ENOENT") {
  const sizeKb = (statSync(wasm).size / 1024).toFixed(1);
  console.warn(
    `wasm-opt: binaryen not on PATH — shipping unoptimised ${sizeKb} kB wasm.\n` +
      `  Install binaryen (\`brew install binaryen\` / \`apt-get install binaryen\`) for ~30% smaller output.`,
  );
  process.exit(0);
}

// Feature flags must cover everything stable rustc emits for
// wasm32-unknown-unknown. The `sign-ext` + `mutable-globals` +
// `nontrapping-float-to-int` + `bulk-memory` set matches the Wasm 2.0
// baseline that every current runtime (Node 16+, modern browsers,
// Bun 1+, Deno 1+) supports unconditionally.
const flags = [
  "-O4",
  "--enable-bulk-memory",
  "--enable-sign-ext",
  "--enable-mutable-globals",
  "--enable-nontrapping-float-to-int",
  "--strip-debug",
  "--strip-producers",
  wasm,
  "-o",
  tmp,
];

const before = statSync(wasm).size;
const run = spawnSync("wasm-opt", flags, { stdio: "inherit" });
if (run.status !== 0) {
  console.error(`wasm-opt: exited with ${run.status} — aborting build.`);
  process.exit(run.status ?? 1);
}

renameSync(tmp, wasm);
const after = statSync(wasm).size;
const delta = (((after - before) / before) * 100).toFixed(1);
const beforeKb = (before / 1024).toFixed(1);
const afterKb = (after / 1024).toFixed(1);
console.log(`wasm-opt: ${beforeKb} kB → ${afterKb} kB (${delta}%)`);
