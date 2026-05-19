#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(ROOT, "scripts", "ci", "project-compatibility.mjs");

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-project-compat-"));
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function runProjectCompatibility(args, env) {
  return spawnSync(process.execPath, [SCRIPT, ...args], {
    cwd: ROOT,
    env: { ...process.env, ...env },
    encoding: "utf8",
  });
}

withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const tsconfig = path.join(dir, "fixture", "tsconfig.json");
  const sourceRoot = path.join(dir, "fixture", "src");
  fs.mkdirSync(sourceRoot, { recursive: true });

  const result = runProjectCompatibility(["record"], {
    COMPAT_JSONL_FILE: jsonl,
    COMPAT_NAME: "type-fest-project",
    COMPAT_EXIT_CLASS: "nonzero exit",
    COMPAT_PHASE: "check",
    COMPAT_DIAGNOSTIC_STATUS: "diagnostic mismatch or compiler error",
    COMPAT_DIAGNOSTIC_DELTA: [
      `${path.join(sourceRoot, "index.ts")}(10,4): error TS2344: Type 'false' does not satisfy the constraint 'true'.`,
      "tsgo: internal runner note without a diagnostic code",
    ].join("\n"),
    COMPAT_FILES_REACHED: "42",
    COMPAT_PEAK_MEMORY_BYTES: "1048576",
    COMPAT_TSC_EXIT_CODES: "0",
    COMPAT_TSZ_EXIT_CODES: "2 2",
    COMPAT_TSGO_EXIT_CODES: "1",
    COMPAT_TSCONFIG_PATH: tsconfig,
    COMPAT_SOURCE_ROOT: sourceRoot,
    COMPAT_FIXTURE_ROOT: path.join(dir, "fixture"),
    COMPAT_GENERATED_AT: "2026-05-19T01:02:03.000Z",
    COMPAT_SOURCE_COMMIT: "abcdef1234567890",
    COMPAT_WORKFLOW_NAME: "CI",
    COMPAT_WORKFLOW_RUN_ID: "12345",
    COMPAT_WORKFLOW_RUN_URL: "https://github.com/mohsen1/tsz/actions/runs/12345",
    COMPAT_WORKFLOW_RUN_ATTEMPT: "2",
    COMPAT_RUN_STATUS: "completed",
    COMPAT_FIXTURE_SOURCES: [
      "type-fest|https://github.com/sindresorhus/type-fest.git|4005f60",
      "type-fest|https://github.com/sindresorhus/type-fest.git|4005f60",
    ].join("\n"),
  });

  assert.equal(result.status, 0, result.stderr);
  const rows = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map((line) => JSON.parse(line));
  assert.equal(rows.length, 1);
  const [row] = rows;

  assert.equal(row.name, "type-fest-project");
  assert.equal(row.generated_at, "2026-05-19T01:02:03.000Z");
  assert.equal(row.source_commit, "abcdef1234567890");
  assert.equal(row.workflow_name, "CI");
  assert.equal(row.workflow_run_id, "12345");
  assert.equal(row.workflow_run_url, "https://github.com/mohsen1/tsz/actions/runs/12345");
  assert.equal(row.workflow_run_attempt, "2");
  assert.equal(row.run_status, "completed");
  assert.equal(row.state, "yellow");
  assert.equal(row.first_failure_class, "evaluation-inference-instantiation");
  assert.equal(row.owner_track, "Track 2/3 conditional, mapped, inference, instantiation");
  assert.equal(row.phase, "check");
  assert.equal(row.last_successful_phase, null);
  assert.deepEqual(row.diagnostic_codes, ["TS2344"]);
  assert.deepEqual(row.exit_codes, { tsc: [0], tsz: [2], tsgo: [1] });
  assert.equal(row.files_reached, 42);
  assert.equal(row.peak_memory_bytes, 1048576);
  assert.equal(row.repro.tsconfig_path, "tsconfig.json");
  assert.equal(row.repro.source_root, "src");
  assert.equal(row.repro.first_failure_path, "src/index.ts");
  assert.equal(row.repro.first_failure_code, "TS2344");
  assert.deepEqual(row.fixture_sources, [
    {
      name: "type-fest",
      repository: "https://github.com/sindresorhus/type-fest.git",
      ref: "4005f60",
    },
  ]);
  assert.equal(row.diagnostic_subsystems[0].subsystem, "evaluation-inference-instantiation");
  assert.equal(row.diagnostic_subsystems[1].subsystem, "uncoded diagnostic");
  assert.equal(row.oracle_classification, "tsz-fails-only");
  assert.deepEqual(row.diagnostic_counts, { tsc: 0, tsz: 1, tsgo: 1 });
  assert.deepEqual(row.tsc_diagnostic_codes, []);
  assert.deepEqual(row.tsz_diagnostic_codes, ["TS2344"]);
  assert.deepEqual(row.tsgo_diagnostic_codes, []);
});

// Oracle classification matrix: each row pins the per-side exit codes and
// the per-side diagnostic lines the classification is derived from. The
// unified delta is synthesized from the per-side arrays so the test cannot
// drift between the `tsc:`/`tsz:` label prefix and the explicit envs.
function makeOracleCase({ name, exitClass, phase = "check", diagnosticStatus, tscExit, tszExit, tscLines = [], tszLines = [], expected }) {
  const env = {
    COMPAT_EXIT_CLASS: exitClass,
    COMPAT_PHASE: phase,
    COMPAT_DIAGNOSTIC_STATUS: diagnosticStatus,
  };
  if (tscExit !== undefined) env.COMPAT_TSC_EXIT_CODES = tscExit;
  if (tszExit !== undefined) env.COMPAT_TSZ_EXIT_CODES = tszExit;
  if (tscLines.length) env.COMPAT_TSC_DIAGNOSTIC_DELTA = tscLines.join("\n");
  if (tszLines.length) env.COMPAT_TSZ_DIAGNOSTIC_DELTA = tszLines.join("\n");
  const unified = [
    ...tscLines.map((line) => `tsc: ${line}`),
    ...tszLines.map((line) => `tsz: ${line}`),
  ];
  if (unified.length) env.COMPAT_DIAGNOSTIC_DELTA = unified.join("\n");
  return { name, env, expected };
}

withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const cases = [
    makeOracleCase({
      name: "both-pass",
      exitClass: "exit success",
      diagnosticStatus: "none",
      tscExit: "0",
      tszExit: "0",
      expected: { oracle: "both-pass", state: "green", counts: { tsc: 0, tsz: 0, tsgo: 0 }, tscCodes: [], tszCodes: [] },
    }),
    makeOracleCase({
      name: "tsc-fails-only",
      exitClass: "fixture invalid",
      phase: "fixture setup",
      diagnosticStatus: "tsc fixture failed",
      tscExit: "1",
      tszExit: "0",
      tscLines: [
        "src/a.ts(1,1): error TS2304: Cannot find name 'foo'.",
        "src/b.ts(2,2): error TS2304: Cannot find name 'bar'.",
      ],
      expected: { oracle: "tsc-fails-only", state: "gray", counts: { tsc: 2, tsz: 0, tsgo: 0 }, tscCodes: ["TS2304"], tszCodes: [] },
    }),
    makeOracleCase({
      name: "tsz-fails-only",
      exitClass: "nonzero exit",
      diagnosticStatus: "diagnostic mismatch or compiler error",
      tscExit: "0",
      tszExit: "1",
      tszLines: ["src/c.ts(3,3): error TS2322: assignability failed."],
      expected: { oracle: "tsz-fails-only", state: "yellow", counts: { tsc: 0, tsz: 1, tsgo: 0 }, tscCodes: [], tszCodes: ["TS2322"] },
    }),
    makeOracleCase({
      name: "both-fail-same",
      exitClass: "nonzero exit",
      diagnosticStatus: "diagnostic mismatch or compiler error",
      tscExit: "1",
      tszExit: "1",
      tscLines: ["src/d.ts(4,4): error TS2322: assignability failed."],
      tszLines: ["src/d.ts(4,4): error TS2322: assignability failed."],
      expected: { oracle: "both-fail-same", state: "yellow", counts: { tsc: 1, tsz: 1, tsgo: 0 }, tscCodes: ["TS2322"], tszCodes: ["TS2322"] },
    }),
    makeOracleCase({
      name: "both-fail-same-no-codes",
      exitClass: "crash",
      diagnosticStatus: "compiler crashed",
      tscExit: "139",
      tszExit: "139",
      expected: { oracle: "both-fail-same", state: "red", counts: { tsc: 0, tsz: 0, tsgo: 0 }, tscCodes: [], tszCodes: [] },
    }),
    makeOracleCase({
      name: "both-fail-different",
      exitClass: "nonzero exit",
      diagnosticStatus: "diagnostic mismatch or compiler error",
      tscExit: "1",
      tszExit: "1",
      tscLines: ["src/e.ts(5,5): error TS2304: Cannot find name 'baz'."],
      tszLines: [
        "src/e.ts(5,5): error TS2322: assignability failed.",
        "src/e.ts(6,6): error TS2345: argument mismatch.",
      ],
      expected: { oracle: "both-fail-different", state: "yellow", counts: { tsc: 1, tsz: 2, tsgo: 0 }, tscCodes: ["TS2304"], tszCodes: ["TS2322", "TS2345"] },
    }),
    // tsz-fails-only requires an explicit tsc success signal; without one → unknown.
    makeOracleCase({
      name: "no-tsc-signal-tsz-exit-fail",
      exitClass: "nonzero exit",
      diagnosticStatus: "diagnostic mismatch or compiler error",
      tszExit: "1",
      expected: { oracle: "unknown", state: "yellow", counts: { tsc: 0, tsz: 0, tsgo: 0 }, tscCodes: [], tszCodes: [] },
    }),
    makeOracleCase({
      name: "no-tsc-signal-tsz-diagnostic-only",
      exitClass: "nonzero exit",
      diagnosticStatus: "diagnostic mismatch or compiler error",
      tszLines: ["src/a.ts(1,1): error TS2322: assignability failed."],
      expected: { oracle: "unknown", state: "yellow", counts: { tsc: 0, tsz: 1, tsgo: 0 }, tscCodes: [], tszCodes: ["TS2322"] },
    }),
    makeOracleCase({
      name: "no-tsc-signal-tsz-exit-and-diagnostic",
      exitClass: "nonzero exit",
      diagnosticStatus: "diagnostic mismatch or compiler error",
      tszExit: "2",
      tszLines: ["src/b.ts(3,3): error TS2345: argument type mismatch."],
      expected: { oracle: "unknown", state: "yellow", counts: { tsc: 0, tsz: 1, tsgo: 0 }, tscCodes: [], tszCodes: ["TS2345"] },
    }),
  ];

  for (const testCase of cases) {
    const result = runProjectCompatibility(["record"], {
      COMPAT_JSONL_FILE: jsonl,
      COMPAT_NAME: testCase.name,
      COMPAT_PHASE: "check",
      ...testCase.env,
    });
    assert.equal(result.status, 0, `${testCase.name}: ${result.stderr}`);
  }

  const rows = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map(JSON.parse);
  assert.equal(rows.length, cases.length);
  for (const [index, testCase] of cases.entries()) {
    const row = rows[index];
    assert.equal(row.oracle_classification, testCase.expected.oracle, `${testCase.name}: oracle_classification`);
    assert.equal(row.state, testCase.expected.state, `${testCase.name}: state`);
    assert.deepEqual(row.diagnostic_counts, testCase.expected.counts, `${testCase.name}: diagnostic_counts`);
    assert.deepEqual(row.tsc_diagnostic_codes, testCase.expected.tscCodes, `${testCase.name}: tsc_diagnostic_codes`);
    assert.deepEqual(row.tsz_diagnostic_codes, testCase.expected.tszCodes, `${testCase.name}: tsz_diagnostic_codes`);
  }
});

// Single-sided failures classify as *-fails-only; single-sided passes
// classify as unknown so missing oracle data is not read as parity.
withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const cases = [
    {
      name: "tsc-failed-tsz-not-run",
      env: {
        COMPAT_EXIT_CLASS: "fixture invalid",
        COMPAT_PHASE: "fixture setup",
        COMPAT_DIAGNOSTIC_STATUS: "tsc fixture failed",
        COMPAT_TSC_EXIT_CODES: "1",
        COMPAT_DIAGNOSTIC_DELTA: "tsc: src/g.ts(1,1): error TS2304: Cannot find name 'baz'.",
      },
      expected: "tsc-fails-only",
    },
    {
      name: "tsz-skipped-after-tsc-pass",
      env: {
        COMPAT_EXIT_CLASS: "tsz unavailable",
        COMPAT_PHASE: "check",
        COMPAT_DIAGNOSTIC_STATUS: "tsz skipped by runner",
        COMPAT_TSC_EXIT_CODES: "0",
      },
      expected: "unknown",
    },
  ];

  for (const testCase of cases) {
    const result = runProjectCompatibility(["record"], {
      COMPAT_JSONL_FILE: jsonl,
      COMPAT_NAME: testCase.name,
      ...testCase.env,
    });
    assert.equal(result.status, 0, `${testCase.name}: ${result.stderr}`);
  }

  const rows = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map(JSON.parse);
  for (const [index, testCase] of cases.entries()) {
    assert.equal(
      rows[index].oracle_classification,
      testCase.expected,
      `${testCase.name}: oracle_classification`,
    );
  }
});

// No tsc oracle signal at all → unknown.
withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const result = runProjectCompatibility(["record"], {
    COMPAT_JSONL_FILE: jsonl,
    COMPAT_NAME: "no-oracle-signal",
    COMPAT_EXIT_CLASS: "exit success",
    COMPAT_PHASE: "check",
    COMPAT_DIAGNOSTIC_STATUS: "none",
    COMPAT_TSZ_EXIT_CODES: "0",
  });

  assert.equal(result.status, 0, result.stderr);
  const [row] = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map(JSON.parse);
  assert.equal(row.oracle_classification, "unknown");
  assert.deepEqual(row.exit_codes.tsc, []);
  assert.deepEqual(row.exit_codes.tsz, [0]);
  assert.deepEqual(row.diagnostic_counts, { tsc: 0, tsz: 0, tsgo: 0 });
});

// Legacy path: a unified COMPAT_DIAGNOSTIC_DELTA without per-side envs is
// attributed by `tsc:`/`tsz:` label prefix.
withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const result = runProjectCompatibility(["record"], {
    COMPAT_JSONL_FILE: jsonl,
    COMPAT_NAME: "legacy-mixed",
    COMPAT_EXIT_CLASS: "nonzero exit",
    COMPAT_PHASE: "check",
    COMPAT_DIAGNOSTIC_STATUS: "diagnostic mismatch or compiler error",
    COMPAT_TSC_EXIT_CODES: "1",
    COMPAT_TSZ_EXIT_CODES: "1",
    COMPAT_DIAGNOSTIC_DELTA: [
      "tsc: src/f.ts(7,7): error TS2304: Cannot find name 'qux'.",
      "tsz: src/f.ts(7,7): error TS2322: assignability failed.",
      "tsz: src/f.ts(8,8): error TS2345: argument mismatch.",
    ].join("\n"),
  });

  assert.equal(result.status, 0, result.stderr);
  const [row] = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map(JSON.parse);
  assert.equal(row.oracle_classification, "both-fail-different");
  assert.deepEqual(row.diagnostic_counts, { tsc: 1, tsz: 2, tsgo: 0 });
  assert.deepEqual(row.tsc_diagnostic_codes, ["TS2304"]);
  assert.deepEqual(row.tsz_diagnostic_codes, ["TS2322", "TS2345"]);
});

withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const cases = [
    {
      name: "",
      message: "COMPAT_NAME must be a lowercase hyphenated project row slug",
    },
    {
      name: "TypeFest",
      message: "COMPAT_NAME must be a lowercase hyphenated project row slug",
    },
    {
      name: "../type-fest-project",
      message: "COMPAT_NAME must be a lowercase hyphenated project row slug",
    },
  ];

  for (const testCase of cases) {
    const result = runProjectCompatibility(["record"], {
      COMPAT_JSONL_FILE: jsonl,
      COMPAT_NAME: testCase.name,
      COMPAT_EXIT_CLASS: "exit success",
      COMPAT_PHASE: "check",
      COMPAT_DIAGNOSTIC_STATUS: "none",
    });

    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, new RegExp(testCase.message));
  }
  assert.equal(fs.existsSync(jsonl), false);
});

withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const cases = [
    {
      source: "malformed",
      message: "line 1 must be name|repository|ref",
    },
    {
      source: "fixture|https://example.invalid/repo.git|",
      message: "line 1 must be name|repository|ref",
    },
    {
      source: "fixture|https://example.invalid/repo.git|abc123|extra",
      message: "line 1 must be name|repository|ref",
    },
  ];

  for (const testCase of cases) {
    const result = runProjectCompatibility(["record"], {
      COMPAT_JSONL_FILE: jsonl,
      COMPAT_NAME: "type-fest-project",
      COMPAT_EXIT_CLASS: "exit success",
      COMPAT_PHASE: "check",
      COMPAT_DIAGNOSTIC_STATUS: "none",
      COMPAT_FIXTURE_SOURCES: testCase.source,
    });

    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, new RegExp(testCase.message));
  }
  assert.equal(fs.existsSync(jsonl), false);
});

withTempDir((dir) => {
  const outside = path.join(path.dirname(dir), `${path.basename(dir)}-outside.jsonl`);
  const result = runProjectCompatibility(["record"], {
    COMPAT_JSONL_FILE: outside,
    COMPAT_OUTPUT_ROOT: dir,
    COMPAT_NAME: "sample-project",
    COMPAT_EXIT_CLASS: "exit success",
    COMPAT_PHASE: "check",
    COMPAT_DIAGNOSTIC_STATUS: "none",
  });

  assert.equal(result.status, 1, result.stderr);
  assert.match(result.stderr, /project compatibility JSONL must stay inside output root/);
  assert.equal(fs.existsSync(outside), false);
});

withTempDir((dir) => {
  const directoryOutput = path.join(dir, "compat.jsonl");
  fs.mkdirSync(directoryOutput);
  const result = runProjectCompatibility(["record"], {
    COMPAT_JSONL_FILE: directoryOutput,
    COMPAT_OUTPUT_ROOT: dir,
    COMPAT_NAME: "sample-project",
    COMPAT_EXIT_CLASS: "exit success",
    COMPAT_PHASE: "check",
    COMPAT_DIAGNOSTIC_STATUS: "none",
  });

  assert.equal(result.status, 1, result.stderr);
  assert.match(result.stderr, /project compatibility JSONL path is not a file/);
});

withTempDir((dir) => {
  const result = runProjectCompatibility(["record"], {
    COMPAT_JSONL_FILE: path.join(dir, "missing", "compat.jsonl"),
    COMPAT_OUTPUT_ROOT: dir,
    COMPAT_NAME: "sample-project",
    COMPAT_EXIT_CLASS: "exit success",
    COMPAT_PHASE: "check",
    COMPAT_DIAGNOSTIC_STATUS: "none",
  });

  assert.equal(result.status, 1, result.stderr);
  assert.match(result.stderr, /project compatibility JSONL parent directory does not exist/);
});

withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const result = runProjectCompatibility(["record"], {
    COMPAT_JSONL_FILE: jsonl,
    COMPAT_NAME: "sample-project",
    COMPAT_EXIT_CLASS: "runner error",
    COMPAT_PHASE: "timing",
    COMPAT_DIAGNOSTIC_STATUS: "benchmark runner failed",
    COMPAT_DIAGNOSTIC_DELTA: "src/index.ts(1,1): error TS2322: assignability failed",
    COMPAT_FILES_REACHED: "42",
    COMPAT_PEAK_MEMORY_BYTES: "1048576",
    COMPAT_TSC_EXIT_CODES: "0",
    COMPAT_TSZ_EXIT_CODES: "1 124",
    COMPAT_TSGO_EXIT_CODES: "0",
  });

  assert.equal(result.status, 0, result.stderr);
  const [row] = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map((line) => JSON.parse(line));
  assert.equal(row.name, "sample-project");
  assert.equal(row.state, "red");
  assert.equal(row.first_failure_class, "benchmark runner error");
  assert.deepEqual(row.known_blockers, [
    "benchmark runner error",
    "timing phase blocker",
    "relations-assignability",
  ]);
  assert.deepEqual(row.exit_codes, { tsc: [0], tsz: [1, 124], tsgo: [0] });
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
    const result = runProjectCompatibility(["record"], {
      COMPAT_JSONL_FILE: jsonl,
      COMPAT_NAME: testCase.name,
      COMPAT_EXIT_CLASS: "nonzero exit",
      COMPAT_DIAGNOSTIC_STATUS: "diagnostic mismatch",
      COMPAT_DIAGNOSTIC_DELTA: testCase.diagnostic,
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
  const result = runProjectCompatibility(["record"], {
    COMPAT_JSONL_FILE: jsonl,
    COMPAT_NAME: "large-ts-repo",
    COMPAT_EXIT_CLASS: "fixture invalid",
    COMPAT_PHASE: "fixture setup",
    COMPAT_DIAGNOSTIC_STATUS: "tsc fixture failed",
    COMPAT_DIAGNOSTIC_DELTA: "tsc: fixture setup failed",
  });

  assert.equal(result.status, 0, result.stderr);
  const [row] = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map((line) => JSON.parse(line));
  assert.equal(row.state, "gray");
  assert.equal(row.first_failure_class, "reference fixture invalid");
  assert.equal(row.owner_track, "Track 1 project-corpus harness/config");
  assert.deepEqual(row.known_blockers, [
    "reference fixture invalid",
    "fixture setup phase blocker",
    "uncoded diagnostic",
  ]);
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
    const result = runProjectCompatibility(["record"], {
      COMPAT_JSONL_FILE: jsonl,
      COMPAT_NAME: testCase.name,
      COMPAT_EXIT_CLASS: testCase.exitClass,
      COMPAT_DIAGNOSTIC_STATUS: testCase.diagnosticStatus,
    });

    assert.equal(result.status, 0, result.stderr);
  }

  const rows = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map((line) => JSON.parse(line));
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
  const result = runProjectCompatibility(["record"], {
    COMPAT_JSONL_FILE: jsonl,
    COMPAT_NAME: "many-diagnostics",
    COMPAT_EXIT_CLASS: "nonzero exit",
    COMPAT_DIAGNOSTIC_STATUS: "diagnostic mismatch",
    COMPAT_DIAGNOSTIC_DELTA: diagnosticLines.join("\n"),
  });

  assert.equal(result.status, 0, result.stderr);
  const [row] = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map((line) => JSON.parse(line));
  assert.equal(row.diagnostic_deltas.length, 20);
  assert.equal(row.diagnostic_deltas[0], diagnosticLines[0]);
  assert.equal(row.diagnostic_deltas[19], diagnosticLines[19]);
  assert.equal(row.diagnostic_subsystems[0].count, 20);
});

withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const summary = path.join(dir, "summary.json");
  fs.writeFileSync(
    jsonl,
    [
      JSON.stringify({ name: "a", state: "green" }),
      JSON.stringify({
        name: "b",
        exit_class: "timeout",
        diagnostic_status: "compiler timed out",
      }),
      "not-json",
    ].join("\n"),
    "utf8",
  );

  const result = runProjectCompatibility(["summary"], {
    SUMMARY_JSONL_FILE: jsonl,
    SUMMARY_OUTPUT_FILE: summary,
    SUMMARY_PROJECT_SET: "canary",
    SUMMARY_PROJECT_FILTER: "type",
    SUMMARY_ALLOW_FAILURES: "1",
    SUMMARY_FAILURES: "1",
    SUMMARY_GENERATED_AT: "2026-05-19T02:03:04.000Z",
    SUMMARY_SOURCE_COMMIT: "123456abcdef",
    SUMMARY_WORKFLOW_NAME: "Project compile guard",
    SUMMARY_WORKFLOW_RUN_ID: "67890",
    SUMMARY_WORKFLOW_RUN_URL: "https://github.com/mohsen1/tsz/actions/runs/67890",
    SUMMARY_WORKFLOW_RUN_ATTEMPT: "1",
    SUMMARY_RUN_STATUS: "completed",
  });

  assert.equal(result.status, 0, result.stderr);
  const payload = JSON.parse(fs.readFileSync(summary, "utf8"));
  assert.equal(payload.generated_at, "2026-05-19T02:03:04.000Z");
  assert.equal(payload.source_commit, "123456abcdef");
  assert.equal(payload.workflow_name, "Project compile guard");
  assert.equal(payload.workflow_run_id, "67890");
  assert.equal(payload.workflow_run_url, "https://github.com/mohsen1/tsz/actions/runs/67890");
  assert.equal(payload.workflow_run_attempt, "1");
  assert.equal(payload.run_status, "completed");
  assert.equal(payload.project_set, "canary");
  assert.equal(payload.project_filter, "type");
  assert.equal(payload.allow_failures, true);
  assert.equal(payload.failures, 1);
  assert.equal(payload.row_count, 2);
  assert.equal(payload.malformed_jsonl_lines, 1);
  assert.equal(payload.by_state.green, 1);
  assert.equal(payload.by_state.red, 1);
});

// Summary surfaces oracle classification counts and the first three deltas
// (with subsystem) ordered by row severity (red → yellow → gray → green).
withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const summary = path.join(dir, "summary.json");
  const rows = [
    {
      name: "alpha",
      state: "green",
      oracle_classification: "both-pass",
      diagnostic_deltas: [],
      diagnostic_subsystems: [],
    },
    {
      name: "beta",
      state: "yellow",
      oracle_classification: "tsz-fails-only",
      diagnostic_deltas: [
        "src/index.ts(1,1): error TS2322: mismatch one",
        "src/index.ts(2,2): error TS2322: mismatch two",
      ],
      diagnostic_subsystems: [
        {
          subsystem: "relations-assignability",
          codes: ["TS2322"],
          count: 2,
          examples: [
            "src/index.ts(1,1): error TS2322: mismatch one",
            "src/index.ts(2,2): error TS2322: mismatch two",
          ],
        },
      ],
    },
    {
      name: "gamma",
      state: "red",
      oracle_classification: "tsc-fails-only",
      diagnostic_deltas: [
        "tsc: src/a.ts(1,1): error TS2304: Cannot find name 'foo'.",
      ],
      diagnostic_subsystems: [
        {
          subsystem: "module-symbol-resolution",
          codes: ["TS2304"],
          count: 1,
          examples: ["tsc: src/a.ts(1,1): error TS2304: Cannot find name 'foo'."],
        },
      ],
    },
  ];
  fs.writeFileSync(jsonl, `${rows.map((row) => JSON.stringify(row)).join("\n")}\n`, "utf8");

  const result = runProjectCompatibility(["summary"], {
    SUMMARY_JSONL_FILE: jsonl,
    SUMMARY_OUTPUT_FILE: summary,
    SUMMARY_PROJECT_SET: "required",
  });
  assert.equal(result.status, 0, result.stderr);

  const payload = JSON.parse(fs.readFileSync(summary, "utf8"));
  assert.deepEqual(payload.by_oracle_classification, {
    "both-pass": 1,
    "tsc-fails-only": 1,
    "tsz-fails-only": 1,
  });
  assert.equal(payload.first_diagnostic_deltas.length, 3);
  assert.equal(payload.first_diagnostic_deltas[0].project, "gamma");
  assert.equal(payload.first_diagnostic_deltas[0].oracle_classification, "tsc-fails-only");
  assert.equal(payload.first_diagnostic_deltas[0].subsystem, "module-symbol-resolution");
  assert.equal(payload.first_diagnostic_deltas[0].code, "TS2304");
  assert.equal(payload.first_diagnostic_deltas[1].project, "beta");
  assert.equal(payload.first_diagnostic_deltas[1].subsystem, "relations-assignability");
  assert.equal(payload.first_diagnostic_deltas[1].code, "TS2322");
  assert.equal(payload.first_diagnostic_deltas[2].project, "beta");
});

// format-step-summary renders a self-contained Markdown block (artifact
// link, classification counts, delta table).
withTempDir((dir) => {
  const summary = path.join(dir, "summary.json");
  fs.writeFileSync(
    summary,
    `${JSON.stringify({
      by_state: { green: 1, yellow: 1, red: 1 },
      by_oracle_classification: {
        "both-pass": 1,
        "tsz-fails-only": 1,
        "tsc-fails-only": 1,
      },
      first_diagnostic_deltas: [
        {
          project: "gamma",
          oracle_classification: "tsc-fails-only",
          state: "red",
          code: "TS2304",
          path: "src/a.ts",
          subsystem: "module-symbol-resolution",
          delta: "tsc: src/a.ts(1,1): error TS2304: Cannot find name 'foo'.",
        },
      ],
    }, null, 2)}\n`,
    "utf8",
  );

  const result = runProjectCompatibility(["format-step-summary"], {
    SUMMARY_INPUT_FILE: summary,
    SUMMARY_TITLE: "Project compatibility artifact",
    SUMMARY_ARTIFACT_NAME: "project-compile-compatibility",
    SUMMARY_ARTIFACT_URL: "https://example.invalid/artifact",
    SUMMARY_JSONL_PATH: ".target/project-compile-guard/project-compatibility.jsonl",
    SUMMARY_SUMMARY_PATH: ".target/project-compile-guard/project-compatibility-summary.json",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /### Project compatibility artifact/);
  assert.match(result.stdout, /\[project-compile-compatibility\]\(https:\/\/example\.invalid\/artifact\)/);
  assert.match(result.stdout, /Rows by state: green=1, yellow=1, red=1/);
  assert.match(result.stdout, /Oracle classification: both-pass=1, tsc-fails-only=1, tsz-fails-only=1/);
  assert.match(result.stdout, /\| Project \| Oracle \| Subsystem \| Code \| Delta \|/);
  assert.match(result.stdout, /\| gamma \| tsc-fails-only \| module-symbol-resolution \| TS2304 \|/);
  assert.match(result.stdout, /See artifact for the remaining diagnostic deltas\./);
});

// Header-only block when no diagnostic deltas were captured.
withTempDir((dir) => {
  const summary = path.join(dir, "summary.json");
  fs.writeFileSync(
    summary,
    `${JSON.stringify({
      by_state: { green: 7 },
      by_oracle_classification: { "both-pass": 7 },
      first_diagnostic_deltas: [],
    }, null, 2)}\n`,
    "utf8",
  );

  const result = runProjectCompatibility(["format-step-summary"], {
    SUMMARY_INPUT_FILE: summary,
    SUMMARY_TITLE: "Project compatibility artifact",
    SUMMARY_ARTIFACT_NAME: "project-compile-compatibility",
    SUMMARY_JSONL_PATH: ".target/project-compile-guard/project-compatibility.jsonl",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /### Project compatibility artifact/);
  assert.match(result.stdout, /Rows by state: green=7/);
  assert.doesNotMatch(result.stdout, /\| Project \|/);
});

// Missing summary file → header-only block annotated "(not produced)" so the
// CI step never errors out and always links the artifact.
withTempDir((dir) => {
  const missing = path.join(dir, "does-not-exist.json");
  const result = runProjectCompatibility(["format-step-summary"], {
    SUMMARY_INPUT_FILE: missing,
    SUMMARY_TITLE: "Project compatibility artifact",
    SUMMARY_ARTIFACT_NAME: "project-compile-compatibility",
    SUMMARY_ARTIFACT_URL: "https://example.invalid/artifact",
    SUMMARY_JSONL_PATH: ".target/project-compile-guard/project-compatibility.jsonl",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /### Project compatibility artifact/);
  assert.match(result.stdout, /\(not produced\)/);
  assert.doesNotMatch(result.stdout, /Rows by state/);
  assert.doesNotMatch(result.stdout, /\| Project \|/);
});

withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  fs.writeFileSync(jsonl, `${JSON.stringify({ name: "a", state: "green" })}\n`, "utf8");

  const clobberResult = runProjectCompatibility(["summary"], {
    SUMMARY_JSONL_FILE: jsonl,
    SUMMARY_OUTPUT_FILE: jsonl,
    SUMMARY_OUTPUT_ROOT: dir,
  });
  assert.equal(clobberResult.status, 1, clobberResult.stderr);
  assert.match(
    clobberResult.stderr,
    /project compatibility summary must not overwrite an input artifact/,
  );

  const outside = path.join(path.dirname(dir), `${path.basename(dir)}-summary.json`);
  const outsideResult = runProjectCompatibility(["summary"], {
    SUMMARY_JSONL_FILE: jsonl,
    SUMMARY_OUTPUT_FILE: outside,
    SUMMARY_OUTPUT_ROOT: dir,
  });
  assert.equal(outsideResult.status, 1, outsideResult.stderr);
  assert.match(outsideResult.stderr, /project compatibility summary must stay inside output root/);
  assert.equal(fs.existsSync(outside), false);

  const directoryOutput = path.join(dir, "summary.json");
  fs.mkdirSync(directoryOutput);
  const directoryResult = runProjectCompatibility(["summary"], {
    SUMMARY_JSONL_FILE: jsonl,
    SUMMARY_OUTPUT_FILE: directoryOutput,
    SUMMARY_OUTPUT_ROOT: dir,
  });
  assert.equal(directoryResult.status, 1, directoryResult.stderr);
  assert.match(directoryResult.stderr, /project compatibility summary path is not a file/);
});
