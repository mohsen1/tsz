#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  COMPILE_CANARY_PROJECT_ROWS,
  COMPATIBILITY_CORPUS_ROWS,
  PROJECT_ROW_DEFINITIONS,
  REQUIRED_PROJECT_ROWS,
} from "./project-rows.mjs";
import {
  BENCH_RUNNER_EXCLUDED_ROWS,
  COMPILE_GUARD_EXCLUDED_ROWS as PROJECT_COMPILE_GUARD_EXCLUDED_ROWS,
  extractBenchRunnerRows,
  extractCompileGuardRows,
  extractFixtureSourceRows,
} from "./project-row-summary.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const GENERATED_ROWS_WITH_FIXTURE_SOURCES = new Set();
const ROADMAP_REQUIRED_PROJECT_ROW_BY_LABEL = new Map([
  ["utility-types", "utility-types-project"],
  ["rxjs", "rxjs-project"],
  ["Kysely", "kysely-project"],
  ["Zod", "zod-project"],
  ["ts-toolbelt", "ts-toolbelt-project"],
  ["type-fest", "type-fest-project"],
  ["ts-essentials", "ts-essentials-project"],
  ["generated Vite app", "vite-vanilla-ts-app"],
  ["generated Next app", "nextjs-fresh-app"],
  ["large-ts-repo", "large-ts-repo"],
  ["Next.js full project", "nextjs"],
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

function roadmapRequiredProjectRows() {
  const roadmap = readRepoFile("docs/plan/ROADMAP.md");
  const rows = [];
  let inTable = false;

  for (const line of roadmap.split(/\r?\n/)) {
    if (line.trim() === "Required project rows:") {
      inTable = true;
      continue;
    }
    if (!inTable) continue;
    if (!line.startsWith("|")) {
      if (rows.length > 0) break;
      continue;
    }
    if (line.includes("---") || line.includes("| Project |")) continue;

    const label = line.split("|")[1]?.trim();
    if (label) rows.push(label);
  }

  return rows;
}

const requiredRows = sortedUnique(REQUIRED_PROJECT_ROWS);
const compileCanaryRows = sortedUnique(COMPILE_CANARY_PROJECT_ROWS);
const allTrackedRows = sortedUnique([...requiredRows, ...compileCanaryRows]);
const projectRowsByName = new Map(PROJECT_ROW_DEFINITIONS.map((row) => [row.name, row]));
const pinnedSourceRows = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.repo !== undefined || row.ref !== undefined)
  .map((row) => row.name);
const compatibilityRows = COMPATIBILITY_CORPUS_ROWS.map((row) => row.name);
const roadmapRequiredRows = roadmapRequiredProjectRows();
const mappedRoadmapRequiredRows = roadmapRequiredRows.map((label) => (
  ROADMAP_REQUIRED_PROJECT_ROW_BY_LABEL.get(label) || `unmapped roadmap row: ${label}`
));

assertNoDuplicates("REQUIRED_PROJECT_ROWS", REQUIRED_PROJECT_ROWS);
assertNoDuplicates("COMPILE_CANARY_PROJECT_ROWS", COMPILE_CANARY_PROJECT_ROWS);
assertNoDuplicates("COMPATIBILITY_CORPUS_ROWS", compatibilityRows);
assertNoDuplicates("ROADMAP required project rows", roadmapRequiredRows);
assert.deepEqual(
  sortedUnique(ROADMAP_REQUIRED_PROJECT_ROW_BY_LABEL.keys()),
  sortedUnique(roadmapRequiredRows),
  "ROADMAP required project row labels drifted from scripts/bench/test-project-rows.mjs",
);
assert.deepEqual(
  sortedUnique(mappedRoadmapRequiredRows),
  sortedUnique(mappedRoadmapRequiredRows.filter((row) => requiredRows.includes(row))),
  "docs/plan/ROADMAP.md required project rows must be benchmark_set: required in scripts/bench/project-rows.mjs",
);
assert.deepEqual(
  sortedUnique(compatibilityRows),
  allTrackedRows,
  "COMPATIBILITY_CORPUS_ROWS must describe every required and compile-canary project row",
);

const benchRunnerScript = readRepoFile("scripts/bench/bench-vs-tsgo.sh");
const benchRows = extractBenchRunnerRows(benchRunnerScript);
const compileCanaryGatedBenchmarkRows = sortedUnique(
  [...benchRunnerScript.matchAll(
    /run_[a-z0-9_]+_project_benchmarks\(\)\s*\{([\s\S]*?)\n\}/g,
  )]
    .filter((match) => match[1].includes("should_run_compile_canary_project"))
    .flatMap((match) => extractAll(match[1], /run_project_benchmark\s+"([^"]+)"/g)),
);
assert.deepEqual(
  benchRows,
  sortedUnique(without(allTrackedRows, BENCH_RUNNER_EXCLUDED_ROWS)),
  "bench-vs-tsgo project rows drifted from scripts/bench/project-rows.mjs",
);
assert.deepEqual(
  compileCanaryGatedBenchmarkRows,
  sortedUnique(compileCanaryGatedBenchmarkRows.filter((row) => compileCanaryRows.includes(row))),
  "bench-vs-tsgo required project rows must not be hidden behind compile-canary gating",
);

const projectCompileGuardRows = extractCompileGuardRows(
  readRepoFile("scripts/ci/project-compile-guard.sh"),
);
assert.deepEqual(
  projectCompileGuardRows,
  sortedUnique(without(allTrackedRows, PROJECT_COMPILE_GUARD_EXCLUDED_ROWS)),
  "project-compile-guard rows drifted from scripts/bench/project-rows.mjs",
);

const fixtureSourceRows = extractFixtureSourceRows(
  readRepoFile("scripts/bench/project-fixtures.sh"),
);
assert.deepEqual(
  fixtureSourceRows,
  sortedUnique([...pinnedSourceRows, ...GENERATED_ROWS_WITH_FIXTURE_SOURCES]),
  "project-fixtures.sh fixture source rows drifted from scripts/bench/project-rows.mjs",
);
assert.deepEqual(
  sortedUnique([...fixtureSourceRows].filter((row) => !projectRowsByName.has(row))),
  [],
  "project-fixtures.sh fixture source rows must be defined in scripts/bench/project-rows.mjs",
);
