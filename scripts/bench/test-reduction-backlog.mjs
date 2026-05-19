#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(ROOT, "scripts", "bench", "reduction-backlog.mjs");
const CI_SCRIPT = path.join(ROOT, "scripts", "ci", "gcp-full-ci.sh");
const BENCHMARK_DATA = path.join(ROOT, "crates", "tsz-website", "src", "_data", "benchmark_data.js");
const SUBSYSTEM_MODULE = path.join(ROOT, "scripts", "ci", "diagnostic-subsystems.mjs");

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-reduction-backlog-"));
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function writeJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

function writeJsonl(file, rows) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, rows.map((r) => JSON.stringify(r)).join("\n") + "\n");
}

function runScript(args, env = {}) {
  return spawnSync(process.execPath, [SCRIPT, ...args], {
    cwd: ROOT,
    encoding: "utf8",
    env: { ...process.env, ...env },
  });
}

const {
  readCompatibilityRows,
  groupByPattern,
  buildReductionTasks,
  renderMarkdown,
  createReductionBacklog,
} = await import(pathToFileURL(SCRIPT));

// --- readCompatibilityRows ---

withTempDir((dir) => {
  // JSONL format
  const jsonlFile = path.join(dir, "compat.jsonl");
  const rows = [
    { name: "type-fest-project", state: "yellow", exit_class: "exit success", diagnostic_codes: ["TS2322"] },
    { name: "ts-toolbelt-project", state: "red", exit_class: "nonzero exit", diagnostic_codes: [] },
  ];
  writeJsonl(jsonlFile, rows);
  const result = readCompatibilityRows(jsonlFile);
  assert.equal(result.length, 2);
  assert.equal(result[0].name, "type-fest-project");
  assert.equal(result[1].name, "ts-toolbelt-project");
});

withTempDir((dir) => {
  // Summary JSON format
  const summaryFile = path.join(dir, "summary.json");
  const rows = [
    { name: "rxjs-project", state: "green", exit_class: "exit success", diagnostic_codes: [] },
    { name: "zod-project", state: "yellow", exit_class: "exit success", diagnostic_codes: ["TS2345"] },
  ];
  writeJson(summaryFile, { rows });
  const result = readCompatibilityRows(summaryFile);
  assert.equal(result.length, 2);
  assert.equal(result[0].name, "rxjs-project");
});

// --- groupByPattern ---

{
  const rows = [
    {
      name: "type-fest-project",
      state: "yellow",
      exit_class: "exit success",
      primary_subsystem: "relations-assignability",
      diagnostic_codes: ["TS2322", "TS2345"],
      diagnostic_subsystems: [{ subsystem: "relations-assignability", codes: ["TS2322"], count: 3, examples: [] }],
      diagnostic_deltas: ["src/index.ts(10,4): error TS2322: Type 'X' is not assignable to 'Y'."],
    },
    {
      name: "ts-toolbelt-project",
      state: "red",
      exit_class: "exit success",
      primary_subsystem: "relations-assignability",
      diagnostic_codes: ["TS2322"],
      diagnostic_subsystems: [{ subsystem: "relations-assignability", codes: ["TS2322"], count: 2, examples: [] }],
      diagnostic_deltas: ["lib/Object/Path.ts(5,1): error TS2322: Type 'string' is not assignable to 'number'."],
    },
    {
      name: "rxjs-project",
      state: "green",
      exit_class: "exit success",
      diagnostic_codes: [],
      diagnostic_subsystems: [],
      diagnostic_deltas: [],
    },
    {
      name: "utility-types-project",
      state: "yellow",
      exit_class: "exit success",
      primary_subsystem: "keyspace-property-indexed",
      diagnostic_codes: ["TS2339"],
      diagnostic_subsystems: [{ subsystem: "keyspace-property-indexed", codes: ["TS2339"], count: 1, examples: [] }],
      diagnostic_deltas: ["src/index.ts(20,3): error TS2339: Property 'x' does not exist."],
    },
  ];

  const { groups, nonGreenCount: patternNonGreenCount } = groupByPattern(rows);

  assert.equal(patternNonGreenCount, 3, "should count 3 non-green rows");

  // Green rows must be excluded
  assert.ok(!Array.from(groups.values()).some((g) => g.rows.some((r) => r.name === "rxjs-project")));

  // Two distinct subsystem+code patterns
  assert.equal(groups.size, 2);

  // relations-assignability:TS2322 group has 2 rows
  const relGroup = Array.from(groups.values()).find(
    (g) => g.subsystem === "relations-assignability" && g.primary_code === "TS2322",
  );
  assert.ok(relGroup, "should have relations-assignability:TS2322 group");
  assert.equal(relGroup.rows.length, 2);
  assert.ok(relGroup.all_codes.has("TS2322"));
  assert.ok(relGroup.all_codes.has("TS2345"));

  // keyspace group has 1 row
  const ksGroup = Array.from(groups.values()).find(
    (g) => g.subsystem === "keyspace-property-indexed",
  );
  assert.ok(ksGroup, "should have keyspace-property-indexed group");
  assert.equal(ksGroup.rows.length, 1);
}

// --- buildReductionTasks ---

{
  const rows = [
    {
      name: "type-fest-project",
      state: "yellow",
      exit_class: "exit success",
      oracle_classification: "tsz-fails-only",
      primary_subsystem: "relations-assignability",
      diagnostic_codes: ["TS2322"],
      diagnostic_subsystems: [{ subsystem: "relations-assignability", codes: ["TS2322"], count: 5, examples: [] }],
      diagnostic_deltas: ["src/index.ts(10,4): error TS2322: Type 'X' is not assignable to 'Y'."],
      repro: {
        tsconfig_path: "fixture/tsconfig.json",
        source_root: "fixture/src",
        first_failure_path: "fixture/src/index.ts",
        first_failure_line: 10,
        first_failure_column: 4,
        first_failure_code: "TS2322",
        reduced_repro_path: "fixture/src/index.ts",
        command: "$TSZ_BIN --noEmit -p fixture/tsconfig.json",
      },
    },
    {
      name: "ts-toolbelt-project",
      state: "red",
      exit_class: "exit success",
      oracle_classification: "tsz-fails-only",
      primary_subsystem: "relations-assignability",
      diagnostic_codes: ["TS2322"],
      diagnostic_subsystems: [{ subsystem: "relations-assignability", codes: ["TS2322"], count: 2, examples: [] }],
      diagnostic_deltas: ["lib/Path.ts(5,1): error TS2322: Type 'string' is not assignable to 'number'."],
      repro: null,
    },
  ];

  const { groups: buildGroups } = groupByPattern(rows);
  const tasks = buildReductionTasks(buildGroups, [], { minRows: 1 });

  assert.equal(tasks.length, 1);
  const task = tasks[0];

  assert.equal(task.subsystem, "relations-assignability");
  assert.equal(task.primary_code, "TS2322");
  assert.ok(Array.isArray(task.codes) && task.codes.includes("TS2322"));
  assert.equal(task.row_count, 2);
  assert.ok(task.total_occurrences > 0);
  assert.ok(task.owner_track.includes("Track 4"));
  assert.equal(task.owner_crate, "solver");
  assert.equal(task.affected_rows.length, 2);
  assert.ok(task.affected_rows.every((r) => r.name && r.state && r.oracle_classification));

  assert.equal(task.repro.source_row, "type-fest-project");
  assert.ok(task.repro.command?.includes("tsconfig.json"));
  assert.equal(task.repro.first_failure_code, "TS2322");

  assert.ok(task.examples.length >= 1);
  assert.ok(task.examples.every((ex) => ex.row && ex.delta));

  assert.ok(task.suggested_issue_title.includes("solver"));
  assert.ok(task.suggested_issue_title.includes("relations-assignability"));

  assert.ok(task.suggested_labels.includes("solver"));
  assert.ok(task.suggested_labels.includes("bench"));

  const tasksFiltered = buildReductionTasks(buildGroups, [], { minRows: 3 });
  assert.equal(tasksFiltered.length, 0, "minRows=3 should exclude all tasks with < 3 rows");
}

// --- issue linking ---

{
  const rows = [
    {
      name: "type-fest-project",
      state: "yellow",
      exit_class: "exit success",
      oracle_classification: "tsz-fails-only",
      primary_subsystem: "keyspace-property-indexed",
      diagnostic_codes: ["TS2339"],
      diagnostic_subsystems: [{ subsystem: "keyspace-property-indexed", codes: ["TS2339"], count: 1, examples: [] }],
      diagnostic_deltas: ["src/a.ts(1,1): error TS2339: Property 'x' does not exist."],
      repro: null,
    },
  ];
  const issues = [
    {
      number: 123,
      title: "fix(solver): TS2339 property not found in keyspace evaluation",
      labels: ["solver", "keyspace-property-indexed"],
      state: "open",
      html_url: "https://github.com/mohsen1/tsz/issues/123",
    },
    {
      number: 456,
      title: "unrelated issue about type narrowing",
      labels: ["flow-narrowing"],
      state: "open",
      html_url: "https://github.com/mohsen1/tsz/issues/456",
    },
    {
      number: 789,
      title: "closed issue about TS2339",
      labels: ["solver"],
      state: "closed",
      html_url: "https://github.com/mohsen1/tsz/issues/789",
    },
  ];

  const { groups } = groupByPattern(rows);
  const tasks = buildReductionTasks(groups, issues, { minRows: 1 });
  assert.equal(tasks.length, 1);
  const task = tasks[0];

  // Issue 123 matches by label and code in title
  assert.ok(task.linked_issues.some((i) => i.number === 123), "should link matching open issue");
  // Issue 456 should NOT match (different subsystem, no code match)
  assert.ok(!task.linked_issues.some((i) => i.number === 456), "should not link unrelated issue");
  // Issue 789 is closed, should NOT be linked
  assert.ok(!task.linked_issues.some((i) => i.number === 789), "should not link closed issue");
}

// --- exit-class rows without diagnostic codes ---

{
  const rows = [
    {
      name: "kysely-project",
      state: "red",
      exit_class: "crash",
      primary_subsystem: null,
      diagnostic_codes: [],
      diagnostic_subsystems: [],
      diagnostic_deltas: [],
      repro: null,
    },
    {
      name: "zod-project",
      state: "red",
      exit_class: "crash",
      primary_subsystem: null,
      diagnostic_codes: [],
      diagnostic_subsystems: [],
      diagnostic_deltas: [],
      repro: null,
    },
  ];
  const { groups } = groupByPattern(rows);
  const tasks = buildReductionTasks(groups, [], { minRows: 1 });

  assert.equal(tasks.length, 1);
  assert.equal(tasks[0].subsystem, "runtime-crash");
  assert.equal(tasks[0].row_count, 2);
  assert.equal(tasks[0].primary_code, null);
}

// --- sorting: highest row count first ---

{
  const rows = [
    {
      name: "a",
      state: "yellow",
      exit_class: "exit success",
      primary_subsystem: "flow-narrowing",
      diagnostic_codes: ["TS2367"],
      diagnostic_subsystems: [{ subsystem: "flow-narrowing", codes: ["TS2367"], count: 1, examples: [] }],
      diagnostic_deltas: [],
      repro: null,
    },
    {
      name: "b",
      state: "yellow",
      exit_class: "exit success",
      primary_subsystem: "relations-assignability",
      diagnostic_codes: ["TS2322"],
      diagnostic_subsystems: [{ subsystem: "relations-assignability", codes: ["TS2322"], count: 3, examples: [] }],
      diagnostic_deltas: [],
      repro: null,
    },
    {
      name: "c",
      state: "yellow",
      exit_class: "exit success",
      primary_subsystem: "relations-assignability",
      diagnostic_codes: ["TS2322"],
      diagnostic_subsystems: [{ subsystem: "relations-assignability", codes: ["TS2322"], count: 2, examples: [] }],
      diagnostic_deltas: [],
      repro: null,
    },
  ];
  const { groups } = groupByPattern(rows);
  const tasks = buildReductionTasks(groups, [], { minRows: 1 });

  assert.equal(tasks.length, 2);
  // relations-assignability appears in 2 rows, flow-narrowing in 1 → relations first
  assert.equal(tasks[0].subsystem, "relations-assignability");
  assert.equal(tasks[1].subsystem, "flow-narrowing");
}

// --- renderMarkdown ---

{
  const report = {
    generated_at: "2026-05-19T00:00:00.000Z",
    source: "compat.jsonl",
    totals: { non_green_rows: 2, unique_patterns: 1, reduction_tasks: 1 },
    reduction_tasks: [
      {
        subsystem: "relations-assignability",
        primary_code: "TS2322",
        codes: ["TS2322", "TS2345"],
        owner_track: "Track 4 relation diagnostics/compatibility",
        owner_crate: "solver",
        row_count: 2,
        total_occurrences: 5,
        affected_rows: [
          { name: "type-fest-project", state: "yellow", oracle_classification: "tsz-fails-only", occurrence_count: 3 },
          { name: "ts-toolbelt-project", state: "red", oracle_classification: "tsz-fails-only", occurrence_count: 2 },
        ],
        examples: [
          { row: "type-fest-project", delta: "src/index.ts(10,4): error TS2322: Type 'X' is not assignable to 'Y'." },
        ],
        repro: {
          command: "$TSZ_BIN --noEmit -p fixture/tsconfig.json",
          first_failure_path: "fixture/src/index.ts",
          first_failure_line: 10,
          first_failure_column: 4,
          first_failure_code: "TS2322",
          source_row: "type-fest-project",
        },
        suggested_issue_title: "fix(solver): resolve relations-assignability (TS2322, TS2345) divergence across 2 project rows",
        suggested_labels: ["bench", "solver"],
        linked_issues: [],
      },
    ],
  };

  const md = renderMarkdown(report);
  assert.ok(md.includes("# Reduction Backlog"), "should have h1 heading");
  assert.ok(md.includes("relations-assignability"), "should include subsystem name");
  assert.ok(md.includes("TS2322"), "should include diagnostic code");
  assert.ok(md.includes("Track 4"), "should include owner track");
  assert.ok(md.includes("type-fest-project"), "should include affected row");
  assert.ok(md.includes("$TSZ_BIN"), "should include repro command");
}

// --- empty input (all green) ---

{
  const rows = [
    { name: "rxjs-project", state: "green", exit_class: "exit success", diagnostic_codes: [], diagnostic_subsystems: [], diagnostic_deltas: [], repro: null },
  ];
  const { groups } = groupByPattern(rows);
  const tasks = buildReductionTasks(groups, [], { minRows: 1 });
  assert.equal(tasks.length, 0, "all-green corpus should produce no tasks");
}

// --- createReductionBacklog end-to-end (JSON input) ---

withTempDir((dir) => {
  const jsonlFile = path.join(dir, "compat.jsonl");
  const rows = [
    {
      name: "type-fest-project",
      state: "yellow",
      exit_class: "exit success",
      oracle_classification: "tsz-fails-only",
      primary_subsystem: "evaluation-inference-instantiation",
      diagnostic_codes: ["TS2344"],
      diagnostic_subsystems: [{ subsystem: "evaluation-inference-instantiation", codes: ["TS2344"], count: 4, examples: [] }],
      diagnostic_deltas: ["src/index.ts(5,3): error TS2344: Type 'A' does not satisfy constraint 'B'."],
      repro: {
        tsconfig_path: "fixture/tsconfig.json",
        first_failure_path: "fixture/src/index.ts",
        first_failure_line: 5,
        first_failure_column: 3,
        first_failure_code: "TS2344",
        command: "$TSZ_BIN --noEmit -p fixture/tsconfig.json",
      },
    },
    {
      name: "rxjs-project",
      state: "green",
      exit_class: "exit success",
      diagnostic_codes: [],
      diagnostic_subsystems: [],
      diagnostic_deltas: [],
      repro: null,
    },
  ];
  writeJsonl(jsonlFile, rows);

  const report = createReductionBacklog(jsonlFile);
  assert.equal(report.totals.non_green_rows, 1);
  assert.equal(report.totals.unique_patterns, 1);
  assert.equal(report.totals.reduction_tasks, 1);
  assert.equal(report.reduction_tasks[0].subsystem, "evaluation-inference-instantiation");
  assert.equal(report.reduction_tasks[0].row_count, 1);
  assert.equal(report.reduction_tasks[0].repro.source_row, "type-fest-project");
});

// --- CLI: stdout JSON when --output is omitted ---

withTempDir((dir) => {
  const jsonlFile = path.join(dir, "compat.jsonl");
  writeJsonl(jsonlFile, [
    {
      name: "ts-toolbelt-project",
      state: "red",
      exit_class: "nonzero exit",
      oracle_classification: "tsz-fails-only",
      primary_subsystem: "relations-assignability",
      diagnostic_codes: ["TS2322"],
      diagnostic_subsystems: [{ subsystem: "relations-assignability", codes: ["TS2322"], count: 1, examples: [] }],
      diagnostic_deltas: ["src/A.ts(1,1): error TS2322: Type mismatch."],
      repro: null,
    },
  ]);

  const result = runScript([jsonlFile]);
  assert.equal(result.status, 0, result.stderr);
  // stdout contains the JSON report
  const json = result.stdout;
  const parsed = JSON.parse(json);
  assert.equal(parsed.totals.reduction_tasks, 1);
  assert.equal(parsed.reduction_tasks[0].subsystem, "relations-assignability");
});

// --- CLI: --output and --markdown flags ---

withTempDir((dir) => {
  const jsonlFile = path.join(dir, "compat.jsonl");
  const outputFile = path.join(dir, "report.json");
  const markdownFile = path.join(dir, "report.md");

  writeJsonl(jsonlFile, [
    {
      name: "utility-types-project",
      state: "yellow",
      exit_class: "exit success",
      oracle_classification: "tsz-fails-only",
      primary_subsystem: "keyspace-property-indexed",
      diagnostic_codes: ["TS2339"],
      diagnostic_subsystems: [{ subsystem: "keyspace-property-indexed", codes: ["TS2339"], count: 2, examples: [] }],
      diagnostic_deltas: ["src/b.ts(3,5): error TS2339: Property 'y' does not exist."],
      repro: null,
    },
  ]);

  const result = runScript([jsonlFile, "--output", outputFile, "--markdown", markdownFile]);
  assert.equal(result.status, 0, result.stderr);

  const report = JSON.parse(fs.readFileSync(outputFile, "utf8"));
  assert.equal(report.totals.reduction_tasks, 1);
  assert.equal(report.reduction_tasks[0].subsystem, "keyspace-property-indexed");

  const md = fs.readFileSync(markdownFile, "utf8");
  assert.ok(md.includes("keyspace-property-indexed"));
  assert.ok(md.includes("TS2339"));
});

// --- CLI: --min-rows filter ---

withTempDir((dir) => {
  const jsonlFile = path.join(dir, "compat.jsonl");
  writeJsonl(jsonlFile, [
    {
      name: "type-fest-project",
      state: "yellow",
      exit_class: "exit success",
      oracle_classification: "tsz-fails-only",
      primary_subsystem: "relations-assignability",
      diagnostic_codes: ["TS2322"],
      diagnostic_subsystems: [{ subsystem: "relations-assignability", codes: ["TS2322"], count: 1, examples: [] }],
      diagnostic_deltas: [],
      repro: null,
    },
  ]);

  const result = runScript([jsonlFile, "--min-rows", "2"]);
  assert.equal(result.status, 0, result.stderr);
  const parsed = JSON.parse(result.stdout);
  assert.equal(parsed.totals.reduction_tasks, 0, "--min-rows 2 should exclude single-row tasks");
});

// --- CLI: error on missing input ---

{
  const result = runScript([]);
  assert.notEqual(result.status, 0, "should exit non-zero with no input");
  assert.ok(result.stderr.includes("usage:"), "should print usage on missing input");
}

// --- gcp-full-ci.sh wires up the test ---

{
  const ciScript = fs.readFileSync(CI_SCRIPT, "utf8");
  assert.match(
    ciScript,
    /node scripts\/bench\/test-reduction-backlog\.mjs/,
    "gcp-full-ci.sh must run test-reduction-backlog.mjs in run_lint",
  );
}

// --- benchmark_data.js uses the shared classifier, not a local fork ---
// Guards against diagnostic-subsystems.mjs and benchmark_data.js silently drifting apart.
// If benchmark_data.js re-introduces its own DIAGNOSTIC_SUBSYSTEM_RULES constant,
// or stops importing from the shared module, these assertions catch it.

{
  const dashboardSrc = fs.readFileSync(BENCHMARK_DATA, "utf8");

  // Must not define its own subsystem table.
  assert.doesNotMatch(
    dashboardSrc,
    /const DIAGNOSTIC_SUBSYSTEM_RULES\s*=/,
    "benchmark_data.js must not define its own DIAGNOSTIC_SUBSYSTEM_RULES — use the shared module",
  );

  // Must import from the shared diagnostic-subsystems module.
  assert.match(
    dashboardSrc,
    /from\s+["'][^"']*diagnostic-subsystems\.mjs["']/,
    "benchmark_data.js must import from scripts/ci/diagnostic-subsystems.mjs",
  );
}

// --- shared classifier produces consistent results for known codes ---
// Ensures subsystemForCode (used by both reduction-backlog and benchmark_data.js)
// maps representative codes from each subsystem correctly.

{
  const { subsystemForCode, DIAGNOSTIC_SUBSYSTEM_RULES } = await import(
    pathToFileURL(SUBSYSTEM_MODULE).href
  );

  // Each subsystem's first code must round-trip through subsystemForCode.
  for (const [subsystem, codes] of DIAGNOSTIC_SUBSYSTEM_RULES) {
    const firstCode = [...codes][0];
    const classified = subsystemForCode(firstCode);
    assert.equal(
      classified,
      subsystem,
      `subsystemForCode("${firstCode}") should return "${subsystem}" but got "${classified}"`,
    );
  }

  // Unknown codes fall through to "unclassified diagnostic".
  assert.equal(subsystemForCode("TS9999"), "unclassified diagnostic");
}
