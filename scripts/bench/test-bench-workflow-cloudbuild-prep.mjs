#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const BENCH_WORKFLOW = path.join(ROOT, ".github", "workflows", "bench.yml");

const workflow = fs.readFileSync(BENCH_WORKFLOW, "utf8");

const failFastMessages = workflow.match(
  /Cloud Build benchmark prep \$\{cloudbuild_id\} succeeded, but its manifest artifact is for/g,
) ?? [];
assert.equal(
  failFastMessages.length,
  2,
  "both Cloud Build prep artifact paths should fail fast on stale manifest artifacts after build success",
);

assert.doesNotMatch(
  workflow,
  /Cloud Build manifest artifact is for .*waiting for/,
  "successful Cloud Build prep with a stale manifest artifact must not wait until the 150 minute deadline",
);

assert.match(
  workflow,
  /expected \$\{target_sha\} \/ PGO=1\."\s*\n\s+exit 1/,
  "PGO prep path should report the expected target and exit immediately",
);

assert.match(
  workflow,
  /expected \$\{\{ env\.BENCH_TARGET_SHA \}\}\."\s*\n\s+exit 1/,
  "benchmark prep path should report the expected target and exit immediately",
);

console.log("bench workflow Cloud Build prep artifact tests passed");
