#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  COMPILE_CANARY_PROJECT_ROWS,
  COMPATIBILITY_CORPUS_ROWS,
  REQUIRED_PROJECT_ROWS,
} from "./project-rows.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");

const BENCH_RUNNER_EXCLUDED_ROWS = new Set([
  "type-challenges-project",
  "type-challenges-solutions-project",
  "type-challenges-assertion-candidates",
]);
const PROJECT_COMPILE_GUARD_EXCLUDED_ROWS = new Set([
  "large-ts-repo",
  "nextjs",
]);

function sortedUnique(values) {
  return [...new Set(values)].sort();
}

function assertNoDuplicates(label, values) {
  const seen = new Set();
  const duplicates = new Set();
  for (const value of values) {
    if (seen.has(value)) {
      duplicates.add(value);
    }
    seen.add(value);
  }
  assert.deepEqual([...duplicates].sort(), [], `${label} contains duplicate rows`);
}

function readRepoFile(relativePath) {
  return fs.readFileSync(path.join(ROOT, relativePath), "utf8");
}

function extractAll(text, pattern) {
  return [...text.matchAll(pattern)].map((match) => match[1]);
}

function without(values, excluded) {
  return values.filter((value) => !excluded.has(value));
}

const requiredRows = sortedUnique(REQUIRED_PROJECT_ROWS);
const compileCanaryRows = sortedUnique(COMPILE_CANARY_PROJECT_ROWS);
const allTrackedRows = sortedUnique([...requiredRows, ...compileCanaryRows]);
const compatibilityRows = COMPATIBILITY_CORPUS_ROWS.map((row) => row.name);

assertNoDuplicates("REQUIRED_PROJECT_ROWS", REQUIRED_PROJECT_ROWS);
assertNoDuplicates("COMPILE_CANARY_PROJECT_ROWS", COMPILE_CANARY_PROJECT_ROWS);
assertNoDuplicates("COMPATIBILITY_CORPUS_ROWS", compatibilityRows);
assert.deepEqual(
  sortedUnique(compatibilityRows),
  allTrackedRows,
  "COMPATIBILITY_CORPUS_ROWS must describe every required and compile-canary project row",
);

const benchRows = sortedUnique(
  extractAll(
    readRepoFile("scripts/bench/bench-vs-tsgo.sh"),
    /run_project_benchmark\s+"([^"]+)"/g,
  ),
);
assert.deepEqual(
  benchRows,
  sortedUnique(without(allTrackedRows, BENCH_RUNNER_EXCLUDED_ROWS)),
  "bench-vs-tsgo project rows drifted from scripts/bench/project-rows.mjs",
);

const projectCompileGuardRows = sortedUnique(
  extractAll(
    readRepoFile("scripts/ci/project-compile-guard.sh"),
    /check_project\s+"([^"]+)"/g,
  ),
);
assert.deepEqual(
  projectCompileGuardRows,
  sortedUnique(without(allTrackedRows, PROJECT_COMPILE_GUARD_EXCLUDED_ROWS)),
  "project-compile-guard rows drifted from scripts/bench/project-rows.mjs",
);
