#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  COMPILE_CANARY_PROJECT_ROWS,
  COMPILE_GUARD_CANARY_PROJECT_ROWS,
  COMPILE_GUARD_REQUIRED_ROWS,
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
  rowRequiresFixtureSource,
} from "./project-row-summary.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
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

function runShellScript(script, env = {}) {
  const result = spawnSync("bash", ["-lc", script], {
    cwd: ROOT,
    env: { ...process.env, ...env },
    encoding: "utf8",
  });
  return result;
}

function shellFixtureSources(rowName, env = {}) {
  const script = `
set -euo pipefail
source "${path.join(ROOT, "scripts/bench/project-fixtures.sh")}"
tsz_project_fixture_sources "${rowName}"
`;
  const result = runShellScript(script, env);
  assert.equal(
    result.status,
    0,
    `tsz_project_fixture_sources ${rowName} failed:\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
  );
  return result.stdout.trim().split(/\r?\n/).filter(Boolean);
}

function shellProjectConfig(writer) {
  const script = `
set -euo pipefail
source "${path.join(ROOT, "scripts/bench/project-fixtures.sh")}"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
output="$dir/tsconfig.json"
${writer} "$output"
cat "$output"
`;
  const result = runShellScript(script);
  assert.equal(
    result.status,
    0,
    `${writer} failed:\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
  );
  return JSON.parse(result.stdout);
}

function shellProjectConfigFiles(writer) {
  const script = `
set -euo pipefail
source "${path.join(ROOT, "scripts/bench/project-fixtures.sh")}"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
output="$dir/tsconfig.json"
${writer} "$output"
cd "$dir"
find . -type f | sed 's#^./##' | sort
`;
  const result = runShellScript(script);
  assert.equal(
    result.status,
    0,
    `${writer} failed:\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
  );
  return result.stdout.trim().split(/\r?\n/).filter(Boolean);
}

function shellProjectGeneratedFile(writer, relativePath) {
  const script = `
set -euo pipefail
source "${path.join(ROOT, "scripts/bench/project-fixtures.sh")}"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
output="$dir/tsconfig.json"
${writer} "$output"
cat "$dir/${relativePath}"
`;
  const result = runShellScript(script);
  assert.equal(
    result.status,
    0,
    `${writer} generated file ${relativePath} failed:\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
  );
  return result.stdout;
}

function shellSyncedProjectRowGroups() {
  const script = `
set -euo pipefail
source "${path.join(ROOT, "scripts/bench/project-fixtures.sh")}"
tsz_sync_project_row_groups
printf 'required\\n'
printf '%s\\n' "\${TSZ_COMPILE_GUARD_REQUIRED_ROWS[@]}"
printf 'canary\\n'
printf '%s\\n' "\${TSZ_COMPILE_GUARD_CANARY_ROWS[@]}"
`;
  const result = runShellScript(script);
  assert.equal(
    result.status,
    0,
    `tsz_sync_project_row_groups failed:\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
  );
  const lines = result.stdout.trim().split(/\r?\n/).filter(Boolean);
  const requiredIndex = lines.indexOf("required");
  const canaryIndex = lines.indexOf("canary");
  assert.equal(requiredIndex, 0, "synced project row groups must start with required marker");
  assert.ok(canaryIndex > requiredIndex, "synced project row groups must include canary marker");
  return {
    required: lines.slice(requiredIndex + 1, canaryIndex),
    canary: lines.slice(canaryIndex + 1),
  };
}

function shellPreloadedRowMetadata() {
  const script = `
set -euo pipefail
source "${path.join(ROOT, "scripts/bench/project-fixtures.sh")}"
printf '%s\\n' "\${_TSZ_PACKED_GUARD_REQUIRED_ROWS}"
printf 'CANARY\\n'
printf '%s\\n' "\${_TSZ_PACKED_CANARY_ROWS}"
printf 'COMPAT\\n'
printf '%s\\n' "\${_TSZ_PACKED_COMPAT_ROWS}"
`;
  const result = runShellScript(script);
  assert.equal(
    result.status,
    0,
    `pre-loaded row metadata check failed:\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
  );
  const lines = result.stdout.trim().split(/\r?\n/);
  const canaryIdx = lines.indexOf("CANARY");
  const compatIdx = lines.indexOf("COMPAT");
  return {
    guardRequired: lines.slice(0, canaryIdx).join("").split("|").filter(Boolean),
    canary: lines.slice(canaryIdx + 1, compatIdx).join("").split("|").filter(Boolean),
    compat: lines.slice(compatIdx + 1).join("").split("|").filter(Boolean),
  };
}

function sharedConfigWriterName(row) {
  if (row.generated_by !== undefined) return null;
  // Application rows compile with the app's OWN tsconfig (jsx + paths), not a
  // generated shared config, so they have no tsz_write_<stem>_config writer.
  if (row.category === "application") return null;
  if (row.guard_set === null || row.guard_set === undefined) return null;
  if (typeof row.fixture_dir !== "string") return null;

  const writerStem = row.fixture_dir
    .replace(/[^A-Za-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
  return `tsz_write_${writerStem}_config`;
}

function extractAll(text, pattern) {
  return [...text.matchAll(pattern)].map((match) => match[1]);
}

function without(values, excluded) {
  return values.filter((value) => !excluded.has(value));
}

function roadmapTrackedProjectRowCount() {
  const roadmap = readRepoFile("docs/plan/ROADMAP.md");
  const match = roadmap.match(/all\s+(\d+)\s+rows visible\b/i);
  assert.ok(match, "ROADMAP must state how many retained project rows stay visible");
  return Number(match[1]);
}

const requiredRows = sortedUnique(REQUIRED_PROJECT_ROWS);
const compileCanaryRows = sortedUnique(COMPILE_CANARY_PROJECT_ROWS);
const allTrackedRows = sortedUnique([...requiredRows, ...compileCanaryRows]);
const projectRowsByName = new Map(PROJECT_ROW_DEFINITIONS.map((row) => [row.name, row]));
const fixtureSourceMetadataRows = PROJECT_ROW_DEFINITIONS
  .filter(rowRequiresFixtureSource)
  .map((row) => row.name);
const pinnedSourceRows = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.repo !== undefined || row.ref !== undefined)
  .map((row) => row.name);
const compatibilityRows = COMPATIBILITY_CORPUS_ROWS.map((row) => row.name);
const roadmapTrackedRows = roadmapTrackedProjectRowCount();

assertNoDuplicates("REQUIRED_PROJECT_ROWS", REQUIRED_PROJECT_ROWS);
assertNoDuplicates("COMPILE_GUARD_REQUIRED_ROWS", COMPILE_GUARD_REQUIRED_ROWS);
assertNoDuplicates("COMPILE_CANARY_PROJECT_ROWS", COMPILE_CANARY_PROJECT_ROWS);
assertNoDuplicates("COMPILE_GUARD_CANARY_PROJECT_ROWS", COMPILE_GUARD_CANARY_PROJECT_ROWS);
assertNoDuplicates("COMPATIBILITY_CORPUS_ROWS", compatibilityRows);
assert.deepEqual(
  shellSyncedProjectRowGroups(),
  {
    required: COMPILE_GUARD_REQUIRED_ROWS,
    canary: COMPILE_GUARD_CANARY_PROJECT_ROWS,
  },
  "project-fixtures.sh runtime row groups must sync from scripts/bench/project-rows.mjs",
);

const preloadedMeta = shellPreloadedRowMetadata();
assert.deepEqual(
  preloadedMeta.guardRequired.sort(),
  COMPILE_GUARD_REQUIRED_ROWS.slice().sort(),
  "_TSZ_PACKED_GUARD_REQUIRED_ROWS must be pre-loaded at module init from scripts/bench/project-rows.mjs",
);
assert.deepEqual(
  preloadedMeta.canary.sort(),
  COMPILE_GUARD_CANARY_PROJECT_ROWS.slice().sort(),
  "_TSZ_PACKED_CANARY_ROWS must be pre-loaded at module init from scripts/bench/project-rows.mjs",
);
assert.deepEqual(
  preloadedMeta.compat.sort(),
  sortedUnique([...REQUIRED_PROJECT_ROWS, ...COMPILE_CANARY_PROJECT_ROWS]),
  "_TSZ_PACKED_COMPAT_ROWS must cover all compatibility rows (required ∪ canary) from project-rows.mjs",
);
assert.equal(
  PROJECT_ROW_DEFINITIONS.length,
  roadmapTrackedRows,
  "ROADMAP retained project-row count drifted from scripts/bench/project-rows.mjs",
);
assert.deepEqual(
  sortedUnique(compatibilityRows),
  allTrackedRows,
  "COMPATIBILITY_CORPUS_ROWS must describe every required and compile-canary project row",
);

const benchRunnerScript = [
  readRepoFile("scripts/bench/bench-vs-tsgo.sh"),
  readRepoFile("scripts/bench/lib/bench-vs-tsgo-results.sh"),
].join("\n");
const projectFixturesScript = readRepoFile("scripts/bench/project-fixtures.sh");
const projectCompileGuardScript = readRepoFile("scripts/ci/project-compile-guard.sh");
const benchRows = extractBenchRunnerRows(benchRunnerScript);
assert.match(
  benchRunnerScript,
  /is_project_compatibility_row\(\)[\s\S]+REQUIRED_PROJECT_ROWS[\s\S]+COMPILE_CANARY_PROJECT_ROWS[\s\S]+record_fixture_failure\(\)[\s\S]+is_project_compatibility_row "\$label"[\s\S]+record_project_compatibility[\s\S]+fixture failed before project benchmark recorded compatibility/,
  "bench fixture failures for project rows must record compatibility metadata before publishing degraded rows",
);
assert.doesNotMatch(
  benchRunnerScript,
  /\[ "\$name" != "nextjs" \] && \[ "\$name" != "large-ts-repo" \]/,
  "Next.js benchmark rows must collect the tsc oracle before they can be green",
);
const compileCanaryGatedBenchmarkRows = sortedUnique(
  [...benchRunnerScript.matchAll(
    /run_[a-z0-9_]+_project_benchmarks\(\)\s*\{([\s\S]*?)\n\}/g,
  )]
    .filter((match) => match[1].includes("should_run_compile_canary_project"))
    .flatMap((match) => extractAll(match[1], /run_project_benchmark\s+"([^"]+)"/g)),
);
// Application rows are optional perf benchmarks only when opted in with
// perf_timed. Untimed applications remain compile-guard canaries only.
const untimedApplicationRowNames = new Set(
  PROJECT_ROW_DEFINITIONS
    .filter((row) => row.category === "application" && row.perf_timed !== true)
    .map((row) => row.name),
);
const benchExcludedRows = new Set([...BENCH_RUNNER_EXCLUDED_ROWS, ...untimedApplicationRowNames]);
assert.deepEqual(
  benchRows,
  sortedUnique(without(allTrackedRows, benchExcludedRows)),
  "bench-vs-tsgo project rows drifted from scripts/bench/project-rows.mjs",
);
assert.deepEqual(
  compileCanaryGatedBenchmarkRows,
  sortedUnique(compileCanaryGatedBenchmarkRows.filter((row) => compileCanaryRows.includes(row))),
  "bench-vs-tsgo required project rows must not be hidden behind compile-canary gating",
);

const projectCompileGuardRows = extractCompileGuardRows(
  projectCompileGuardScript,
);
assert.deepEqual(
  projectCompileGuardRows,
  sortedUnique(without(allTrackedRows, PROJECT_COMPILE_GUARD_EXCLUDED_ROWS)),
  "project-compile-guard rows drifted from scripts/bench/project-rows.mjs",
);

const fixtureSourceRows = extractFixtureSourceRows(
  projectFixturesScript,
);
assert.deepEqual(
  fixtureSourceRows,
  sortedUnique(fixtureSourceMetadataRows),
  "project-fixtures.sh fixture source rows drifted from scripts/bench/project-rows.mjs",
);
assert.deepEqual(
  sortedUnique([...fixtureSourceRows].filter((row) => !projectRowsByName.has(row))),
  [],
  "project-fixtures.sh fixture source rows must be defined in scripts/bench/project-rows.mjs",
);

for (const row of PROJECT_ROW_DEFINITIONS) {
  const writer = sharedConfigWriterName(row);
  if (writer === null) continue;

  assert.match(
    projectFixturesScript,
    new RegExp(`^${writer}\\(\\) \\{`, "m"),
    `${row.name} shared config writer must be defined in project-fixtures.sh`,
  );
  assert.match(
    projectCompileGuardScript,
    new RegExp(`\\b${writer}\\b`),
    `${row.name} project-compile-guard must use the shared ${writer} writer`,
  );
  if (!BENCH_RUNNER_EXCLUDED_ROWS.has(row.name)) {
    assert.match(
      benchRunnerScript,
      new RegExp(`\\b${writer}\\b`),
      `${row.name} bench-vs-tsgo must use the shared ${writer} writer`,
    );
  }
}

assert.equal(
  shellProjectConfig("tsz_write_drizzle_orm_config").compilerOptions.allowImportingTsExtensions,
  true,
  "drizzle-orm guard config must match upstream .ts import extension support",
);
assert.deepEqual(
  shellProjectConfig("tsz_write_drizzle_orm_config").compilerOptions.paths,
  { "~/*": ["./drizzle-orm/src/*"], "*": ["./tsz-bench-external-module.d.ts"] },
  "drizzle-orm guard config must match upstream tilde import path support (./-prefixed: tsc 6 rejects non-relative paths targets without baseUrl)",
);
assert.equal(
  shellProjectConfig("tsz_write_drizzle_orm_config").compilerOptions.baseUrl,
  undefined,
  "drizzle-orm guard config must not use TS7's removed baseUrl option",
);
assert.equal(
  shellProjectConfig("tsz_write_drizzle_orm_config").compilerOptions.ignoreDeprecations,
  undefined,
  "drizzle-orm guard config must not carry a TS6-only deprecation workaround",
);
assert.deepEqual(
  shellProjectConfigFiles("tsz_write_drizzle_orm_config"),
  [
    "node_modules/@cloudflare/workers-types/index.d.ts",
    "node_modules/bun-types/index.d.ts",
    "tsconfig.json",
    "tsz-bench-external-module.d.ts",
    "tsz-bench-external-named-modules.d.ts",
  ],
  "drizzle-orm guard config must write local stubs for external package types",
);
{
  const externalStub = shellProjectGeneratedFile(
    "tsz_write_drizzle_orm_config",
    "tsz-bench-external-named-modules.d.ts",
  );
  for (const expected of [
    "declare module '@aws-sdk/client-rds-data'",
    "export class RDSDataClient",
    "export interface Field",
    "declare module 'better-sqlite3'",
    "export const Database",
    "declare module 'bun:sqlite'",
    "declare module '@libsql/client'",
    "export const createClient",
    "declare module '@prisma/client'",
    "export const Prisma",
    "declare const Buffer",
    "interface DurableObjectStorage",
    "type SqlStorageCursor",
  ]) {
    assert.match(
      externalStub,
      new RegExp(expected.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")),
      `drizzle-orm external stub must include ${expected}`,
    );
  }
}

for (const rowName of pinnedSourceRows) {
  const row = projectRowsByName.get(rowName);
  const sources = shellFixtureSources(rowName);
  assert.equal(sources.length, 1, `${rowName} should emit exactly one fixture source`);
  const [, repository, ref] = sources[0].split("|");
  assert.equal(repository, row.repo, `${rowName} fixture source repository drifted from project-rows.mjs`);
  assert.equal(ref, row.ref, `${rowName} fixture source ref drifted from project-rows.mjs`);
}

for (const rowName of pinnedSourceRows) {
  const row = projectRowsByName.get(rowName);
  const overrideRepo = `https://example.invalid/${rowName}.git`;
  const overrideRef = `feedface${rowName.length.toString(16).padStart(4, "0")}`;
  const sources = shellFixtureSources(rowName, {
    [row.repo_env]: overrideRepo,
    [row.ref_env]: overrideRef,
  });
  assert.equal(sources.length, 1, `${rowName} should emit exactly one override fixture source`);
  const [, repository, ref] = sources[0].split("|");
  assert.equal(repository, overrideRepo, `${rowName} fixture source should honor shell repo overrides`);
  assert.equal(ref, overrideRef, `${rowName} fixture source should honor shell ref overrides`);
}
