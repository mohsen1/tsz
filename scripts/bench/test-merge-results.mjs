#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { COMPILE_CANARY_PROJECT_ROWS, REQUIRED_PROJECT_ROWS } from "./project-rows.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const MERGE_SCRIPT = path.join(ROOT, "scripts", "bench", "merge-results.mjs");

const REQUIRED_COMPATIBILITY_FIELDS = {
  state: "green",
  exit_class: "exit success",
  first_failure_class: null,
  owner_track: null,
  phase: "check",
  last_successful_phase: "check",
  diagnostic_status: "none",
  diagnostic_deltas: [],
  diagnostic_subsystems: [],
  known_blockers: [],
  exit_codes: { tsc: [0], tsz: [0], tsgo: [0] },
  files_reached: 1,
  peak_memory_bytes: 1024,
  emit_status: "not in scope (noEmit project check)",
  dts_status: "not in scope (noEmit project check)",
  reduced_repro_path: null,
  repro: {
    tsconfig_path: null,
    source_root: null,
    first_failure_path: null,
    first_failure_line: null,
    first_failure_column: null,
    first_failure_code: null,
    reduced_repro_path: null,
    command: null,
  },
};

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-merge-results-"));
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function writePayload(file, results, extraPayload = {}) {
  fs.writeFileSync(
    file,
    `${JSON.stringify({
      quick_mode: false,
      validation: { hyperfine_exit_codes_required: true },
      totals: { benchmarks_run: results.length },
      results,
      ...extraPayload,
    })}\n`,
    "utf8",
  );
}

function writeInput(dir, name, results, extraPayload = {}) {
  const input = path.join(dir, name);
  writePayload(input, results, extraPayload);
  return input;
}

function runMerge(dir, results, extraPayload = {}) {
  const input = writeInput(dir, "input.json", results, extraPayload);
  return runMergeInputs(dir, [input]);
}

function runMergeInputs(dir, inputs) {
  const output = path.join(dir, "merged.json");
  const result = spawnSync(process.execPath, [MERGE_SCRIPT, output, ...inputs], {
    cwd: ROOT,
    encoding: "utf8",
  });
  return { ...result, output };
}

function projectRow(name, compatibility = REQUIRED_COMPATIBILITY_FIELDS) {
  return {
    name,
    lines: 1,
    kb: 1,
    tsz_ms: 1,
    tsgo_ms: 1,
    winner: "tsz",
    ratio: 1,
    compatibility,
  };
}

withTempDir((dir) => {
  const result = runMerge(dir, REQUIRED_PROJECT_ROWS.map((name) => projectRow(name)));
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.validation.project_compatibility_required_fields, true);
});

withTempDir((dir) => {
  const rows = REQUIRED_PROJECT_ROWS.filter((name) => name !== "utility-types-project")
    .map((name) => projectRow(name));
  const result = runMerge(dir, rows);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /utility-types-project: missing project row/);
});

withTempDir((dir) => {
  const rows = REQUIRED_PROJECT_ROWS.map((name) => {
    if (name !== "rxjs-project") return projectRow(name);
    const { peak_memory_bytes: _peakMemoryBytes, ...compatibility } = REQUIRED_COMPATIBILITY_FIELDS;
    return projectRow(name, compatibility);
  });
  const result = runMerge(dir, rows);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /rxjs-project: missing compatibility\.peak_memory_bytes/);
});

withTempDir((dir) => {
  const rows = [
    ...REQUIRED_PROJECT_ROWS.map((name) => projectRow(name)),
    projectRow("utility-types-project"),
  ];
  const result = runMerge(dir, rows);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /utility-types-project: duplicate project row/);
});

withTempDir((dir) => {
  const result = runMerge(dir, [
    projectRow(COMPILE_CANARY_PROJECT_ROWS[0]),
    projectRow(COMPILE_CANARY_PROJECT_ROWS[0]),
  ]);
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    new RegExp(`${COMPILE_CANARY_PROJECT_ROWS[0]}: duplicate project row`),
  );
});

withTempDir((dir) => {
  const result = runMerge(dir, [projectRow(COMPILE_CANARY_PROJECT_ROWS[0])]);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.validation.project_compatibility_required_fields, true);
});

withTempDir((dir) => {
  const { diagnostic_subsystems: _diagnosticSubsystems, ...compatibility } = REQUIRED_COMPATIBILITY_FIELDS;
  const result = runMerge(dir, [projectRow(COMPILE_CANARY_PROJECT_ROWS[0], compatibility)]);
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    new RegExp(`${COMPILE_CANARY_PROJECT_ROWS[0]}: missing compatibility\\.diagnostic_subsystems`),
  );
});

withTempDir((dir) => {
  const { owner_track: _ownerTrack, ...compatibility } = REQUIRED_COMPATIBILITY_FIELDS;
  const result = runMerge(dir, [projectRow(COMPILE_CANARY_PROJECT_ROWS[0], compatibility)]);
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    new RegExp(`${COMPILE_CANARY_PROJECT_ROWS[0]}: missing compatibility\\.owner_track`),
  );
});

withTempDir((dir) => {
  const runner_environment = {
    platform: "linux",
    arch: "x64",
    release: "6.8.0",
    cpu_count: 32,
    cpu_model: "Intel Xeon",
    total_memory_bytes: 137438953472,
    ci: true,
    github_actions: {
      runner_os: "Linux",
      runner_arch: "X64",
    },
    cloud_build: {
      machine_type: "e2-highcpu-32",
    },
  };
  const result = runMerge(dir, [projectRow("standalone")], { runner_environment });
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.deepEqual(merged.runner_environment, runner_environment);
  assert.deepEqual(merged.validation.runner_environment_warnings, []);
});

withTempDir((dir) => {
  const first = writeInput(
    dir,
    "bench-results-a.json",
    [projectRow("first")],
    {
      runner_environment: {
        platform: "linux",
        arch: "x64",
        release: "6.8.0",
        cpu_count: 32,
        cpu_model: "Intel Xeon",
        total_memory_bytes: 137438953472,
        github_actions: {
          runner_os: "Linux",
          runner_arch: "X64",
        },
        cloud_build: {
          machine_type: "e2-highcpu-32",
        },
      },
    },
  );
  const second = writeInput(
    dir,
    "bench-results-b.json",
    [projectRow("second")],
    {
      runner_environment: {
        platform: "linux",
        arch: "x64",
        release: "6.8.0",
        cpu_count: 16,
        cpu_model: "Intel Xeon",
        total_memory_bytes: 68719476736,
        github_actions: {
          runner_os: "Linux",
          runner_arch: "X64",
        },
        cloud_build: {
          machine_type: "e2-highcpu-16",
        },
      },
    },
  );
  const result = runMergeInputs(dir, [first, second]);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.runner_environment.cpu_count, 32);
  assert.equal(merged.validation.runner_environment_warnings.length, 1);
  assert.equal(merged.validation.runner_environment_warnings[0].file, "bench-results-b.json");
  assert.deepEqual(
    merged.validation.runner_environment_warnings[0].mismatched_fields,
    ["cpu_count", "total_memory_bytes", "cloud_build_machine_type"],
  );
});
