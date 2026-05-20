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
  /expected \$\{target_sha\} \/ PGO=1\."\s*\n\s+exit 1/,
  "benchmark prep path should report the expected target and PGO marker before exiting immediately",
);

assert.match(
  workflow,
  /gs:\/\/tsz-ci_cloudbuild\/bench-prep\/\$\{prep_prefix\}\/bench-prep\.env[\s\S]+gs:\/\/tsz-ci_cloudbuild\/bench-prep\/\$\{prep_prefix\}\/bench-prep\.tar[\s\S]+tar -tf bench-prep\.tar \.target-bench\/dist\/tsz[\s\S]+tar -tf bench-prep\.tar \.target-bench\/dist\/\.bench-pgo-optimized[\s\S]+Cloud Build prep artifact already exists/,
  "Cloud Build prep reuse should only skip submit after validating both the env and tar artifacts",
);

assert.match(
  workflow,
  /target_sha="\$\{\{ env\.BENCH_TARGET_SHA \}\}"[\s\S]+gs:\/\/tsz-ci_cloudbuild\/bench-prep\/\$\{target_sha\}\/bench-prep\.env[\s\S]+gs:\/\/tsz-ci_cloudbuild\/bench-prep\/\$\{target_sha\}\/bench-prep\.tar/,
  "benchmark prep artifact polling should keep using the stable workflow target SHA",
);

assert.match(
  workflow,
  /manifest_target_sha="\$\(manifest_value BENCH_TARGET_SHA\)"[\s\S]+manifest_pgo_optimized="\$\(manifest_value BENCH_PGO_OPTIMIZED\)"[\s\S]+\$\{manifest_target_sha\}" == "\$\{target_sha\}" &&[\s\S]+\$\{manifest_pgo_optimized\}" == "1"/,
  "benchmark prep artifact polling should validate manifest target and PGO without sourcing bench-prep.env",
);

const benchmarkPrepDownload = workflow.match(
  /needs\.bench-prepare\.outputs\.should_run != 'true'[\s\S]+?      - name: Validate benchmark prep artifact/,
)?.[0] ?? "";
assert.doesNotMatch(
  benchmarkPrepDownload,
  /source bench-prep\.env/,
  "benchmark prep artifact polling must not source bench-prep.env because it can clobber BENCH_TARGET_SHA",
);

console.log("bench workflow Cloud Build prep artifact tests passed");
