#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const COMPAT_SCRIPT = path.join(ROOT, "scripts", "ci", "project-compatibility.mjs");

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-project-compat-"));
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const tsconfig = path.join(dir, "fixture", "tsconfig.json");
  const src = path.join(dir, "fixture", "src");
  fs.mkdirSync(src, { recursive: true });

  const result = spawnSync(process.execPath, [COMPAT_SCRIPT, "record"], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      COMPAT_JSONL_FILE: jsonl,
      COMPAT_FIXTURE_ROOT: dir,
      COMPAT_NAME: "sample-project",
      COMPAT_EXIT_CLASS: "runner error",
      COMPAT_PHASE: "timing",
      COMPAT_DIAGNOSTIC_STATUS: "hyperfine failed",
      COMPAT_DIAGNOSTIC_DELTA:
        `${path.join(src, "index.ts")}(3,5): error TS2322: Type 'number' is not assignable to type 'string'.`,
      COMPAT_FILES_REACHED: "42",
      COMPAT_PEAK_MEMORY_BYTES: "1048576",
      COMPAT_TSC_EXIT_CODES: "0",
      COMPAT_TSZ_EXIT_CODES: "1 124",
      COMPAT_TSGO_EXIT_CODES: "0",
      COMPAT_TSCONFIG_PATH: tsconfig,
      COMPAT_SOURCE_ROOT: src,
    },
  });

  assert.equal(result.status, 0, result.stderr);
  const rows = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map(JSON.parse);
  assert.equal(rows.length, 1);
  const row = rows[0];
  assert.equal(row.name, "sample-project");
  assert.equal(row.state, "red");
  assert.equal(row.first_failure_class, "benchmark runner error");
  assert.deepEqual(row.known_blockers, ["benchmark runner error", "timing phase blocker", "relations-assignability"]);
  assert.deepEqual(row.exit_codes, { tsc: [0], tsz: [1, 124], tsgo: [0] });
  assert.equal(row.files_reached, 42);
  assert.equal(row.peak_memory_bytes, 1048576);
  assert.equal(row.reduced_repro_path, "fixture/src/index.ts");
  assert.equal(row.repro.tsconfig_path, "fixture/tsconfig.json");
  assert.equal(row.repro.source_root, "fixture/src");
  assert.equal(row.repro.first_failure_code, "TS2322");
});

withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const cases = [
    {
      name: "keyspace",
      diagnostic: "src/index.ts(1,1): error TS7053: Element implicitly has an 'any' type.",
      ownerTrack: "Track 5 keyspace/property/indexed access",
    },
    {
      name: "flow",
      diagnostic: "src/index.ts(2,1): error TS18048: 'value' is possibly 'undefined'.",
      ownerTrack: "Track 6 flow/narrowing",
    },
  ];

  for (const testCase of cases) {
    const result = spawnSync(process.execPath, [COMPAT_SCRIPT, "record"], {
      cwd: ROOT,
      encoding: "utf8",
      env: {
        ...process.env,
        COMPAT_JSONL_FILE: jsonl,
        COMPAT_NAME: testCase.name,
        COMPAT_EXIT_CLASS: "nonzero exit",
        COMPAT_DIAGNOSTIC_STATUS: "diagnostic mismatch",
        COMPAT_DIAGNOSTIC_DELTA: testCase.diagnostic,
      },
    });

    assert.equal(result.status, 0, result.stderr);
  }

  const rows = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map(JSON.parse);
  assert.equal(rows.length, cases.length);
  for (const [index, testCase] of cases.entries()) {
    assert.equal(rows[index].owner_track, testCase.ownerTrack);
  }
});

withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const cases = [
    {
      name: "clean",
      exitClass: "exit success",
      diagnosticStatus: "none",
      expectedState: "green",
    },
    {
      name: "diagnostic",
      exitClass: "nonzero exit",
      diagnosticStatus: "diagnostic mismatch or compiler error",
      expectedState: "yellow",
    },
    {
      name: "timeout",
      exitClass: "timeout",
      diagnosticStatus: "compiler timed out",
      expectedState: "red",
    },
    {
      name: "oom",
      exitClass: "oom",
      diagnosticStatus: "compiler OOM or killed",
      expectedState: "red",
    },
    {
      name: "crash",
      exitClass: "crash",
      diagnosticStatus: "compiler crashed",
      expectedState: "red",
    },
    {
      name: "fixture",
      exitClass: "fixture invalid",
      diagnosticStatus: "fixture invalid",
      expectedState: "gray",
    },
    {
      name: "missing-tsz",
      exitClass: "tsz unavailable",
      diagnosticStatus: "runner setup incomplete",
      expectedState: "gray",
    },
  ];

  for (const testCase of cases) {
    const result = spawnSync(process.execPath, [COMPAT_SCRIPT, "record"], {
      cwd: ROOT,
      encoding: "utf8",
      env: {
        ...process.env,
        COMPAT_JSONL_FILE: jsonl,
        COMPAT_NAME: testCase.name,
        COMPAT_EXIT_CLASS: testCase.exitClass,
        COMPAT_DIAGNOSTIC_STATUS: testCase.diagnosticStatus,
      },
    });

    assert.equal(result.status, 0, result.stderr);
  }

  const rows = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map(JSON.parse);
  assert.equal(rows.length, cases.length);
  for (const [index, testCase] of cases.entries()) {
    assert.equal(rows[index].state, testCase.expectedState, testCase.name);
  }
});

withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const diagnosticLines = Array.from(
    { length: 25 },
    (_, index) => `src/file-${index}.ts(1,1): error TS2322: mismatch ${index}`,
  );
  const result = spawnSync(process.execPath, [COMPAT_SCRIPT, "record"], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      COMPAT_JSONL_FILE: jsonl,
      COMPAT_NAME: "many-diagnostics",
      COMPAT_EXIT_CLASS: "nonzero exit",
      COMPAT_DIAGNOSTIC_STATUS: "diagnostic mismatch",
      COMPAT_DIAGNOSTIC_DELTA: diagnosticLines.join("\n"),
    },
  });

  assert.equal(result.status, 0, result.stderr);
  const [row] = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map(JSON.parse);
  assert.equal(row.diagnostic_deltas.length, 20);
  assert.equal(row.diagnostic_deltas[0], diagnosticLines[0]);
  assert.equal(row.diagnostic_deltas[19], diagnosticLines[19]);
  assert.equal(row.diagnostic_subsystems[0].count, 20);
});

withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  fs.writeFileSync(
    jsonl,
    [
      JSON.stringify({ name: "green", state: "green" }),
      JSON.stringify({ name: "red", exit_class: "nonzero exit", diagnostic_status: "diagnostic mismatch" }),
      "not json",
    ].join("\n"),
    "utf8",
  );
  const output = path.join(dir, "summary.json");
  const result = spawnSync(process.execPath, [COMPAT_SCRIPT, "summary"], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      SUMMARY_JSONL_FILE: jsonl,
      SUMMARY_OUTPUT_FILE: output,
      SUMMARY_PROJECT_SET: "required",
      SUMMARY_PROJECT_FILTER: "sample",
      SUMMARY_ALLOW_FAILURES: "1",
      SUMMARY_FAILURES: "1",
    },
  });

  assert.equal(result.status, 0, result.stderr);
  const summary = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.equal(summary.project_set, "required");
  assert.equal(summary.project_filter, "sample");
  assert.equal(summary.allow_failures, true);
  assert.equal(summary.failures, 1);
  assert.deepEqual(summary.by_state, { green: 1, yellow: 1 });
  assert.equal(summary.malformed_jsonl_lines, 1);
});
