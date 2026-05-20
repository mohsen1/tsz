#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { COMPILE_CANARY_PROJECT_ROWS, REQUIRED_PROJECT_ROWS } from "./project-rows.mjs";
import { GREEN_COMPAT, YELLOW_COMPAT, RED_COMPAT } from "./row-utils.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const MERGE_SCRIPT = path.join(ROOT, "scripts", "bench", "merge-results.mjs");
const COMPILE_ONLY_CANARY_PROJECT_ROWS = COMPILE_CANARY_PROJECT_ROWS.filter(
  (name) => !REQUIRED_PROJECT_ROWS.includes(name),
);
assert.ok(
  COMPILE_ONLY_CANARY_PROJECT_ROWS.length > 0,
  "test fixture expects at least one compile-canary row outside REQUIRED_PROJECT_ROWS",
);

const SAMPLE_COMPATIBILITY = {
  generated_at: "2026-05-19T01:02:03.000Z",
  source_commit: "abcdef1234567890",
  workflow_name: "Bench",
  workflow_run_id: "12345",
  workflow_run_url: "https://github.com/mohsen1/tsz/actions/runs/12345",
  workflow_run_attempt: "1",
  run_status: "completed",
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
  files_reached_reason: null,
  peak_memory_bytes: 1024,
  peak_memory_bytes_reason: null,
  fixture_sources: [{ name: "fixture", repository: "https://example.invalid/repo.git", ref: "abc123" }],
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

const SAMPLE_MEASUREMENT_PROFILE = {
  mode: "release-pgo",
  tsz_binary_source: "bench-dist",
  profile_guided_optimization: {
    requested: true,
    required: true,
    optimized: true,
    marker_path: "/tmp/tsz/.target-bench/dist/.bench-pgo-optimized",
    marker_found: true,
    profile_use: "/tmp/tsz/.target-bench/pgo-data/merged.profdata",
    profile_fingerprint: "abcdef123456",
    training_fingerprint: "123456abcdef",
    profile_data_source: "fresh",
    built_at: "2026-05-20T01:02:03Z",
    llvm_profdata: "/toolchain/bin/llvm-profdata",
    training_metadata_available: true,
    training_input_count: 2,
    training_failure_count: 0,
    training_inputs: ["stdin:scalar", "synthetic:mapped_type.ts"],
    training_failed_inputs: [],
    config: {
      synthetic: true,
      fetch_utility_types: true,
      fetch_core_projects: false,
      panic_unwind: false,
      extra_inputs: null,
      training_timeout_seconds: 900,
      cache_enabled: true,
    },
  },
};

const SAMPLE_RUNNER_ENVIRONMENT = {
  platform: "linux",
  arch: "x64",
  release: "6.8.0",
  cpu_count: 32,
  cpu_model: "Intel Xeon",
  total_memory_bytes: 137438953472,
  ci: true,
  github_actions: {
    run_id: "12345",
    run_attempt: "1",
    runner_os: "Linux",
    runner_arch: "X64",
    workflow: "Bench",
    job: "bench",
    ref: "refs/heads/main",
    sha: "abcdef1234567890",
  },
  cloud_build: {
    machine_type: "e2-highcpu-32",
  },
};

const SAMPLE_RUN_METADATA = {
  generated_at: "2026-05-19T01:02:03.000Z",
  source_commit: "abcdef1234567890",
  workflow_name: "Bench",
  workflow_run_id: "12345",
  workflow_run_url: "https://github.com/mohsen1/tsz/actions/runs/12345",
  workflow_run_attempt: "1",
  run_status: "completed",
};

function cloneJson(value) {
  return JSON.parse(JSON.stringify(value));
}

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-merge-results-"));
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function writeInput(dir, name, results, extraPayload = {}) {
  const input = path.join(dir, name);
  const payload = {
    benchmark_runner: "scripts/bench/bench-vs-tsgo.sh",
    quick_mode: false,
    validation: { hyperfine_exit_codes_required: true },
    totals: { benchmarks_run: results.length },
    results,
    ...extraPayload,
  };
  fs.writeFileSync(input, `${JSON.stringify(payload)}\n`, "utf8");
  return input;
}

function runMergeInputs(dir, inputs, mergeArgs = []) {
  const output = path.join(dir, "merged.json");
  const result = spawnSync(process.execPath, [MERGE_SCRIPT, output, ...mergeArgs, ...inputs], {
    cwd: ROOT,
    env: {
      ...process.env,
      BENCH_TARGET_SHA: "",
      GITHUB_ACTIONS: "",
      GITHUB_REPOSITORY: "",
      GITHUB_RUN_ATTEMPT: "",
      GITHUB_RUN_ID: "",
      GITHUB_SERVER_URL: "",
      GITHUB_SHA: "",
      GITHUB_WORKFLOW: "",
    },
    encoding: "utf8",
  });
  return { ...result, output };
}

function runMerge(dir, results, extraPayload = {}, mergeArgs = []) {
  const input = writeInput(dir, "input.json", results, extraPayload);
  return runMergeInputs(dir, [input], mergeArgs);
}

function projectRow(name, compatibility = SAMPLE_COMPATIBILITY) {
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
  assert.equal(merged.source_commit, "local");
  assert.equal(merged.workflow_run_id, "local");
  assert.equal(merged.run_status, "local");
  assert.equal(merged.validation.project_compatibility_required_fields, true);
});

withTempDir((dir) => {
  const missingRow = REQUIRED_PROJECT_ROWS[0];
  const rows = REQUIRED_PROJECT_ROWS.filter((name) => name !== missingRow)
    .map((name) => projectRow(name));
  const result = runMerge(dir, rows);
  assert.equal(result.status, 1);
  assert.match(result.stderr, new RegExp(`${missingRow}: missing project row`));
});

withTempDir((dir) => {
  const rows = REQUIRED_PROJECT_ROWS.map((name) => {
    if (name !== "rxjs-project") return projectRow(name);
    const { peak_memory_bytes: _peakMemoryBytes, ...compatibility } = SAMPLE_COMPATIBILITY;
    return projectRow(name, compatibility);
  });
  const result = runMerge(dir, rows);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /rxjs-project: missing compatibility\.peak_memory_bytes/);
});

withTempDir((dir) => {
  const duplicateRow = REQUIRED_PROJECT_ROWS[0];
  const rows = [
    ...REQUIRED_PROJECT_ROWS.map((name) => projectRow(name)),
    projectRow(duplicateRow),
  ];
  const result = runMerge(dir, rows);
  assert.equal(result.status, 1);
  assert.match(result.stderr, new RegExp(`${duplicateRow}: duplicate project row`));
});

withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const result = runMerge(dir, [
    projectRow(canaryRow),
    projectRow(canaryRow),
  ]);
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    new RegExp(`${canaryRow}: duplicate project row`),
  );
});

withTempDir((dir) => {
  const result = runMerge(dir, [projectRow(COMPILE_ONLY_CANARY_PROJECT_ROWS[0])]);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.validation.project_compatibility_required_fields, true);
});

withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const input = writeInput(dir, "input.json", [projectRow("standalone")]);
  const compatibilityJsonl = path.join(dir, "project-compatibility.jsonl");
  fs.writeFileSync(
    compatibilityJsonl,
    `${JSON.stringify({ ...SAMPLE_COMPATIBILITY, name: canaryRow, files_reached: 78 })}\n`,
    "utf8",
  );

  const result = runMergeInputs(dir, ["--compat-jsonl", compatibilityJsonl, input]);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  const row = merged.results.find((candidate) => candidate.name === canaryRow);
  assert.ok(row, "expected merge to add compile-canary compatibility row");
  assert.equal(row.lines, 78);
  assert.match(row.status, /compile canary tracked in CI/);
  assert.equal(row.compatibility.state, "green");
  assert.equal(merged.validation.project_compatibility_required_fields, true);
});

withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const { diagnostic_subsystems: _diagnosticSubsystems, ...compatibility } = SAMPLE_COMPATIBILITY;
  const result = runMerge(dir, [projectRow(canaryRow, compatibility)]);
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    new RegExp(`${canaryRow}: missing compatibility\\.diagnostic_subsystems`),
  );
});

withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const { owner_track: _ownerTrack, ...compatibility } = SAMPLE_COMPATIBILITY;
  const result = runMerge(dir, [projectRow(canaryRow, compatibility)]);
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    new RegExp(`${canaryRow}: missing compatibility\\.owner_track`),
  );
});

withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const compatibility = {
    ...SAMPLE_COMPATIBILITY,
    state: "red",
    exit_class: "nonzero exit",
    first_failure_class: null,
    known_blockers: ["relations-assignability"],
  };
  const result = runMerge(dir, [projectRow(canaryRow, compatibility)]);
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    new RegExp(`${canaryRow}: red/yellow compatibility\\.first_failure_class must name the first blocker`),
  );
});

withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const compatibility = {
    ...SAMPLE_COMPATIBILITY,
    state: "yellow",
    diagnostic_status: "diagnostic mismatch",
    first_failure_class: "relations-assignability",
    known_blockers: [],
  };
  const result = runMerge(dir, [projectRow(canaryRow, compatibility)]);
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    new RegExp(`${canaryRow}: red/yellow compatibility\\.known_blockers must name at least one blocker`),
  );
});

withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const compatibility = {
    ...SAMPLE_COMPATIBILITY,
    state: "yellow",
    diagnostic_status: "diagnostic mismatch",
    first_failure_class: "relations-assignability",
    known_blockers: ["evaluation-inference-instantiation", "relations-assignability"],
  };
  const result = runMerge(dir, [projectRow(canaryRow, compatibility)]);
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    new RegExp(`${canaryRow}: red/yellow compatibility\\.first_failure_class must match the first known blocker`),
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

withTempDir((dir) => {
  const first = writeInput(
    dir,
    "bench-results-a.json",
    [projectRow("first")],
    {
      ...SAMPLE_RUN_METADATA,
      runner_environment: SAMPLE_RUNNER_ENVIRONMENT,
      shard: { label: "compiler-files", filter: "compiler" },
      filter: "compiler",
    },
  );
  const second = writeInput(
    dir,
    "bench-results-b.json",
    [projectRow("second")],
    {
      ...SAMPLE_RUN_METADATA,
      runner_environment: SAMPLE_RUNNER_ENVIRONMENT,
      shard: { label: "synthetic", filter: "synthetic" },
      filter: "synthetic",
    },
  );
  const result = runMergeInputs(dir, [first, second], ["--require-runner-signature"]);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.validation.runner_signature_required, true);
  assert.deepEqual(merged.validation.runner_environment_warnings, []);
});

withTempDir((dir) => {
  const input = writeInput(
    dir,
    "bench-results-missing-env.json",
    [projectRow("standalone")],
    {
      ...SAMPLE_RUN_METADATA,
      shard: { label: "standalone", filter: "standalone" },
      filter: "standalone",
    },
  );
  const result = runMergeInputs(dir, [input], ["--require-runner-signature"]);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /bench-results-missing-env\.json: missing runner_environment/);
});

withTempDir((dir) => {
  const input = writeInput(
    dir,
    "bench-results-missing-shard.json",
    [projectRow("standalone")],
    {
      ...SAMPLE_RUN_METADATA,
      runner_environment: SAMPLE_RUNNER_ENVIRONMENT,
      filter: "standalone",
    },
  );
  const result = runMergeInputs(dir, [input], ["--require-runner-signature"]);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /bench-results-missing-shard\.json: missing shard\.label/);
  assert.match(result.stderr, /bench-results-missing-shard\.json: missing shard\.filter/);
});

withTempDir((dir) => {
  const input = writeInput(
    dir,
    "bench-results-missing-runner.json",
    [projectRow("standalone")],
    {
      ...SAMPLE_RUN_METADATA,
      benchmark_runner: undefined,
      runner_environment: SAMPLE_RUNNER_ENVIRONMENT,
      shard: { label: "standalone", filter: "standalone" },
      filter: "standalone",
    },
  );
  const result = runMergeInputs(dir, [input], ["--require-runner-signature"]);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /bench-results-missing-runner\.json: missing benchmark_runner/);
});

withTempDir((dir) => {
  const input = writeInput(
    dir,
    "bench-results-wrong-runner.json",
    [projectRow("standalone")],
    {
      ...SAMPLE_RUN_METADATA,
      benchmark_runner: "scripts/bench/other-runner.sh",
      runner_environment: SAMPLE_RUNNER_ENVIRONMENT,
      shard: { label: "standalone", filter: "standalone" },
      filter: "standalone",
    },
  );
  const result = runMergeInputs(dir, [input], ["--require-runner-signature"]);
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /bench-results-wrong-runner\.json: benchmark_runner "scripts\/bench\/other-runner\.sh" does not match "scripts\/bench\/bench-vs-tsgo\.sh"/,
  );
});

withTempDir((dir) => {
  const first = writeInput(
    dir,
    "bench-results-a.json",
    [projectRow("first")],
    {
      ...SAMPLE_RUN_METADATA,
      runner_environment: SAMPLE_RUNNER_ENVIRONMENT,
      shard: { label: "duplicate", filter: "first" },
      filter: "first",
    },
  );
  const second = writeInput(
    dir,
    "bench-results-b.json",
    [projectRow("second")],
    {
      ...SAMPLE_RUN_METADATA,
      runner_environment: SAMPLE_RUNNER_ENVIRONMENT,
      shard: { label: "duplicate", filter: "second" },
      filter: "second",
    },
  );
  const result = runMergeInputs(dir, [first, second], ["--require-runner-signature"]);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /bench-results-b\.json: duplicate shard\.label "duplicate"/);
});

withTempDir((dir) => {
  const changedRunnerEnvironment = {
    ...SAMPLE_RUNNER_ENVIRONMENT,
    cpu_count: 16,
    total_memory_bytes: 68719476736,
    cloud_build: { machine_type: "e2-highcpu-16" },
  };
  const first = writeInput(
    dir,
    "bench-results-a.json",
    [projectRow("first")],
    {
      ...SAMPLE_RUN_METADATA,
      runner_environment: SAMPLE_RUNNER_ENVIRONMENT,
      shard: { label: "first", filter: "first" },
      filter: "first",
    },
  );
  const second = writeInput(
    dir,
    "bench-results-b.json",
    [projectRow("second")],
    {
      ...SAMPLE_RUN_METADATA,
      runner_environment: changedRunnerEnvironment,
      shard: { label: "second", filter: "second" },
      filter: "second",
    },
  );
  const result = runMergeInputs(dir, [first, second], ["--require-runner-signature"]);
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /bench-results-b\.json: runner_environment mismatch \(cpu_count, total_memory_bytes, cloud_build_machine_type\)/,
  );
});

withTempDir((dir) => {
  const result = runMerge(
    dir,
    [projectRow("standalone")],
    { measurement_profile: SAMPLE_MEASUREMENT_PROFILE },
  );
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.deepEqual(merged.measurement_profile, SAMPLE_MEASUREMENT_PROFILE);
  assert.deepEqual(merged.validation.measurement_profile_warnings, []);
});

withTempDir((dir) => {
  const firstProfile = cloneJson(SAMPLE_MEASUREMENT_PROFILE);
  const secondProfile = cloneJson(SAMPLE_MEASUREMENT_PROFILE);
  secondProfile.profile_guided_optimization.profile_fingerprint = "fedcba654321";
  secondProfile.profile_guided_optimization.training_fingerprint = "654321fedcba";
  secondProfile.profile_guided_optimization.training_inputs.push("utility-types");
  secondProfile.profile_guided_optimization.training_input_count = 3;

  const first = writeInput(
    dir,
    "bench-results-pgo-a.json",
    [projectRow("first")],
    { measurement_profile: firstProfile },
  );
  const second = writeInput(
    dir,
    "bench-results-pgo-b.json",
    [projectRow("second")],
    { measurement_profile: secondProfile },
  );
  const result = runMergeInputs(dir, [first, second]);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.deepEqual(merged.measurement_profile, firstProfile);
  assert.equal(merged.validation.measurement_profile_warnings.length, 1);
  assert.equal(merged.validation.measurement_profile_warnings[0].file, "bench-results-pgo-b.json");
  assert.deepEqual(
    merged.validation.measurement_profile_warnings[0].mismatched_fields,
    [
      "profile_guided_optimization.profile_fingerprint",
      "profile_guided_optimization.training_fingerprint",
      "profile_guided_optimization.training_input_count",
      "profile_guided_optimization.training_inputs",
    ],
  );
});

// artifact_missing rows: accepted by merge step without a compatibility object
// or without all required compatibility fields. They must not block the merge.
withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const rows = [
    ...REQUIRED_PROJECT_ROWS.map((name) => projectRow(name)),
    { name: canaryRow, lines: 1, kb: 1, tsz_ms: null, tsgo_ms: null, winner: "error", ratio: null, artifact_missing: true },
  ];
  const result = runMerge(dir, rows);
  assert.equal(result.status, 0, result.stderr);
});

withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const rows = [
    ...REQUIRED_PROJECT_ROWS.map((name) => projectRow(name)),
    {
      name: canaryRow,
      lines: 1,
      kb: 1,
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
      ratio: null,
      artifact_missing: true,
      compatibility: { exit_class: "timeout" },
    },
  ];
  const result = runMerge(dir, rows);
  assert.equal(result.status, 0, result.stderr);
});

// Non-artifact_missing rows must still fail if they have missing required fields.
withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const { peak_memory_bytes: _peakMemoryBytes, ...compatibility } = SAMPLE_COMPATIBILITY;
  const result = runMerge(dir, [projectRow(canaryRow, compatibility)]);
  assert.equal(result.status, 1);
  assert.match(result.stderr, new RegExp(`${canaryRow}: missing compatibility\\.peak_memory_bytes`));
});

// artifact_missing row mixed with required green rows: the artifact_missing
// row must not count as a green win even though its winner is set.
withTempDir((dir) => {
  const missingRow = REQUIRED_PROJECT_ROWS[0];
  const rows = REQUIRED_PROJECT_ROWS.map((name) =>
    name === missingRow
      ? { name, artifact_missing: true, winner: "tsz", ratio: 1, tsz_ms: 1, tsgo_ms: 2 }
      : projectRow(name),
  );
  const result = runMerge(dir, rows);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.totals.rows, REQUIRED_PROJECT_ROWS.length);
  assert.equal(merged.totals.green_tsz_wins, REQUIRED_PROJECT_ROWS.length - 1, "artifact_missing row must not count as a green win");
});

// green_tsz_wins / green_tsgo_wins: yellow/red rows with non-green compat do not count
withTempDir((dir) => {
  const greenRow = { name: "green", winner: "tsz", tsz_ms: 1, tsgo_ms: 2, compatibility: GREEN_COMPAT };
  const yellowRow = { name: "yellow", winner: "tsz", tsz_ms: 1, tsgo_ms: 2, compatibility: YELLOW_COMPAT };
  const redRow = { name: "red", winner: "tsz", tsz_ms: 1, tsgo_ms: 2, compatibility: RED_COMPAT };
  const noCompatRow = { name: "no-compat", winner: "tsz", tsz_ms: 1, tsgo_ms: 2 };
  const result = runMerge(dir, [greenRow, yellowRow, redRow, noCompatRow]);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.totals.tsz_wins, 4);
  assert.equal(merged.totals.green_tsz_wins, 2, "yellow/red compat rows must not count as green wins");
  assert.equal(merged.totals.green_tsgo_wins, 0);
});

// artifact_missing row paired with a green tsgo row: only the green row
// contributes to green win totals.
withTempDir((dir) => {
  const artifactMissingRow = { name: "missing", winner: "tsz", tsz_ms: 1, tsgo_ms: 2, artifact_missing: true };
  const greenRow = { name: "green", winner: "tsgo", tsz_ms: 2, tsgo_ms: 1, compatibility: GREEN_COMPAT };
  const result = runMerge(dir, [artifactMissingRow, greenRow]);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.totals.green_tsz_wins, 0, "artifact_missing row must not count as a green win");
  assert.equal(merged.totals.green_tsgo_wins, 1);
});
