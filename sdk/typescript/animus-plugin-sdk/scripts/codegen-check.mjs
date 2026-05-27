#!/usr/bin/env node
// CI-side drift check. Re-runs codegen into a temp directory (via the
// ANIMUS_CODEGEN_OUT_DIR override) and diffs the result against the
// committed files under src/types/. Exits non-zero if any byte differs
// so PRs that touch the Rust schemas without regenerating get caught.

import { spawnSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { dirname, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const committedDir = resolve(__dirname, "../src/types");
const codegenScript = resolve(__dirname, "codegen.mjs");

const stagingDir = mkdtempSync(resolve(tmpdir(), "animus-codegen-check-"));
mkdirSync(stagingDir, { recursive: true });

const result = spawnSync(process.execPath, [codegenScript], {
  stdio: "inherit",
  env: { ...process.env, ANIMUS_CODEGEN_OUT_DIR: stagingDir },
});
if (result.status !== 0) {
  console.error("codegen failed");
  process.exit(result.status ?? 1);
}

const committed = new Map();
for (const f of readdirSync(committedDir)) {
  if (!f.endsWith(".ts")) continue;
  committed.set(f, readFileSync(resolve(committedDir, f), "utf8"));
}

const fresh = new Map();
for (const f of readdirSync(stagingDir)) {
  if (!f.endsWith(".ts")) continue;
  fresh.set(f, readFileSync(resolve(stagingDir, f), "utf8"));
}

let drift = false;
const allFiles = new Set([...committed.keys(), ...fresh.keys()]);
for (const f of allFiles) {
  const a = committed.get(f);
  const b = fresh.get(f);
  if (a === undefined) {
    console.error(`drift: ${f} is generated but not committed`);
    drift = true;
  } else if (b === undefined) {
    console.error(`drift: ${f} is committed but no longer generated`);
    drift = true;
  } else if (a !== b) {
    console.error(`drift: ${f} differs from regenerated output`);
    drift = true;
  }
}

rmSync(stagingDir, { recursive: true, force: true });

if (drift) {
  console.error("\nRun `pnpm run codegen` and commit the result.");
  process.exit(1);
}
console.log("codegen output matches committed files");
