#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  COMPILE_CANARY_PROJECT_ROWS,
  PROJECT_ROW_DEFINITIONS,
  REQUIRED_PROJECT_ROWS,
} from "./project-rows.mjs";
import { GREEN_COMPAT, YELLOW_COMPAT, RED_COMPAT, isGreen } from "./row-utils.mjs";

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
  ...GREEN_COMPAT,
  generated_at: "2026-05-19T01:02:03.000Z",
  source_commit: "abcdef1234567890abcdef1234567890abcdef12",
  workflow_name: "Bench",
  workflow_run_id: "12345",
  workflow_run_url: "https://github.com/tsz-org/tsz/actions/runs/12345",
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
  workflow_run_url: "https://github.com/tsz-org/tsz/actions/runs/12345",
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

function runMergeInputs(dir, inputs, mergeArgs = [], envOverrides = {}) {
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
      ...envOverrides,
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
  const input = writeInput(dir, "bench-results-local-shard.json", [projectRow("standalone")], {
    generated_at: "2026-05-19T01:02:03.000Z",
    source_commit: "local",
    workflow_name: "local",
    workflow_run_id: "local",
    workflow_run_url: null,
    run_status: "local",
  });
  const result = runMergeInputs(dir, [input], [], {
    BENCH_TARGET_SHA: "feedface1234567890",
    GITHUB_ACTIONS: "true",
    GITHUB_REPOSITORY: "tsz-org/tsz",
    GITHUB_RUN_ATTEMPT: "2",
    GITHUB_RUN_ID: "67890",
    GITHUB_SERVER_URL: "https://github.com",
    GITHUB_SHA: "badcafe1234567890",
    GITHUB_WORKFLOW: "Bench",
  });
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.source_commit, "feedface1234567890");
  assert.equal(merged.workflow_name, "Bench");
  assert.equal(merged.workflow_run_id, "67890");
  assert.equal(merged.workflow_run_url, "https://github.com/tsz-org/tsz/actions/runs/67890");
  assert.equal(merged.run_status, "completed");
});

// Issue #13607: a missing project row is an advisory compatibility gap, not a
// blocking one. The merge must publish the benchmark timing data anyway and
// surface the gap as a ::warning:: (the missing-required-TIMING-row floor is
// owned independently by check-artifact-readiness.mjs).
withTempDir((dir) => {
  const missingRow = REQUIRED_PROJECT_ROWS[0];
  const rows = REQUIRED_PROJECT_ROWS.filter((name) => name !== missingRow)
    .map((name) => projectRow(name));
  const result = runMerge(dir, rows);
  assert.equal(result.status, 0, result.stderr);
  assert.ok(fs.existsSync(result.output), "advisory gap must still write bench-results.json");
  assert.match(
    result.stderr,
    new RegExp(`::warning::[\\s\\S]*${missingRow}: missing project row`),
  );
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.ok(
    merged.validation.project_compatibility_advisory.includes(`${missingRow}: missing project row`),
    "advisory gap must be recorded in merged.validation.project_compatibility_advisory",
  );
});

// Issue #17025: required-set coverage. A CI run with the full declared
// benchmark_set:"required" set present stays `completed` and records an empty
// missing_required_rows list.
const CI_ENV_OVERRIDES = {
  BENCH_TARGET_SHA: "feedface1234567890",
  GITHUB_ACTIONS: "true",
  GITHUB_REPOSITORY: "tsz-org/tsz",
  GITHUB_RUN_ATTEMPT: "1",
  GITHUB_RUN_ID: "67890",
  GITHUB_SERVER_URL: "https://github.com",
  GITHUB_SHA: "badcafe1234567890",
  GITHUB_WORKFLOW: "Bench",
};

withTempDir((dir) => {
  const input = writeInput(
    dir,
    "input.json",
    REQUIRED_PROJECT_ROWS.map((name) => projectRow(name)),
    { run_status: "completed" },
  );
  const result = runMergeInputs(dir, [input], [], CI_ENV_OVERRIDES);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.run_status, "completed");
  assert.deepEqual(merged.validation.missing_required_rows, []);
});

// A CI run missing a declared required row downgrades run_status to `partial`,
// records the absent row in validation.missing_required_rows, still publishes
// (exit 0, artifact written), and surfaces a ::warning::. This closes the
// reported gap: a `completed` run with a silently smaller results array.
withTempDir((dir) => {
  const missingRow = REQUIRED_PROJECT_ROWS[0];
  const rows = REQUIRED_PROJECT_ROWS.filter((name) => name !== missingRow).map((name) =>
    projectRow(name),
  );
  const input = writeInput(dir, "input.json", rows, { run_status: "completed" });
  const result = runMergeInputs(dir, [input], [], CI_ENV_OVERRIDES);
  assert.equal(result.status, 0, result.stderr);
  assert.ok(fs.existsSync(result.output), "a partial required set must still publish");
  assert.match(
    result.stderr,
    new RegExp(`::warning::[\\s\\S]*${missingRow}: missing required row`),
  );
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.run_status, "partial");
  assert.ok(
    merged.validation.missing_required_rows.includes(missingRow),
    "the absent required row must be recorded in validation.missing_required_rows",
  );
});

// A standalone/timing-only shard carrying NO required rows must not be flagged
// against the whole required set (the coverage check is gated on the artifact
// actually carrying at least one required row), so its run_status is untouched.
withTempDir((dir) => {
  const input = writeInput(dir, "input.json", [projectRow("standalone")], {
    run_status: "completed",
  });
  const result = runMergeInputs(dir, [input], [], CI_ENV_OVERRIDES);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.run_status, "completed");
  assert.deepEqual(merged.validation.missing_required_rows, []);
});

// Issue #16310: the missing-project-row advisory must span the FULL defined
// compatibility corpus, not just REQUIRED_PROJECT_ROWS. A `category:"application"`
// row (guard_set:"canary", never in REQUIRED) that is absent from every shard
// must be advised, otherwise unmeasured application coverage shrinks silently.
withTempDir((dir) => {
  const applicationRow = PROJECT_ROW_DEFINITIONS.find(
    (row) => row.category === "application" && !REQUIRED_PROJECT_ROWS.includes(row.name),
  );
  assert.ok(applicationRow, "test fixture expects at least one application corpus row");
  // A complete REQUIRED set (so no required row is itself missing) plus the
  // application row absent from the artifact entirely.
  const rows = REQUIRED_PROJECT_ROWS.map((name) => projectRow(name));
  const result = runMerge(dir, rows);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.ok(
    merged.validation.project_compatibility_advisory.includes(`${applicationRow.name}: missing project row`),
    `an unmeasured application corpus row must be advised (${applicationRow.name})`,
  );
  assert.match(
    result.stderr,
    new RegExp(`::warning::[\\s\\S]*${applicationRow.name}: missing project row`),
  );
});

withTempDir((dir) => {
  const rows = REQUIRED_PROJECT_ROWS.map((name) => {
    if (name !== "rxjs-project") return projectRow(name);
    const { peak_memory_bytes: _peakMemoryBytes, ...compatibility } = SAMPLE_COMPATIBILITY;
    return projectRow(name, compatibility);
  });
  const result = runMerge(dir, rows);
  assert.equal(result.status, 0, result.stderr);
  assert.ok(fs.existsSync(result.output));
  assert.match(
    result.stderr,
    /::warning::[\s\S]*rxjs-project: missing compatibility\.peak_memory_bytes/,
  );
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

// Duplicate rows stay BLOCKING even for a canary-only row. This is the floor
// that check-artifact-readiness.mjs does NOT cover (it only inspects REQUIRED
// rows), so the merge step must remain the authoritative duplicate guard.
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

// Issue #13607 witness: a compile-only canary row with a compatibility gap,
// paired with the full REQUIRED set (complete timing), must publish. The merge
// exits 0, writes the artifact, preserves the canary row, and warns.
withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const { peak_memory_bytes: _peakMemoryBytes, ...compatibility } = SAMPLE_COMPATIBILITY;
  const rows = [
    ...REQUIRED_PROJECT_ROWS.map((name) => projectRow(name)),
    projectRow(canaryRow, compatibility),
  ];
  const result = runMerge(dir, rows);
  assert.equal(result.status, 0, result.stderr);
  assert.ok(fs.existsSync(result.output), "compat gap must still write bench-results.json");
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.ok(
    merged.results.some((r) => r.name === canaryRow),
    "canary row must survive into the published artifact",
  );
  assert.match(
    result.stderr,
    new RegExp(`::warning::[\\s\\S]*${canaryRow}: missing compatibility\\.peak_memory_bytes`),
  );
  assert.ok(
    merged.validation.project_compatibility_advisory.includes(
      `${canaryRow}: missing compatibility.peak_memory_bytes`,
    ),
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
  // Pick any row that is benchmark-required yet still a compile canary, rather
  // than hardcoding one: a row graduates out of COMPILE_CANARY_PROJECT_ROWS the
  // moment its guard_set flips to "required", so a literal name breaks on every
  // promotion. The merge behavior under test depends only on the required-canary
  // category, not on which row fills it.
  const requiredCanaryRow = REQUIRED_PROJECT_ROWS.find((name) =>
    COMPILE_CANARY_PROJECT_ROWS.includes(name),
  );
  assert.ok(
    requiredCanaryRow,
    "expected at least one benchmark-required compile-canary row",
  );
  const slowdownCompatibility = {
    ...SAMPLE_COMPATIBILITY,
    state: "red",
    exit_class: "slowdown",
    first_failure_class: "runtime slowdown during project timing",
    owner_track: "Track 10 runtime slowdown triage",
    phase: "timing",
    last_successful_phase: null,
    diagnostic_status: "runtime slowdown",
    diagnostic_deltas: ["timing failure: tsz 356 ms, tsgo 10 ms, ratio 35.61x, threshold 8x"],
    known_blockers: ["runtime slowdown during project timing", "timing phase blocker"],
  };
  const rows = REQUIRED_PROJECT_ROWS.map((name) => {
    if (name !== requiredCanaryRow) return projectRow(name);
    return {
      ...projectRow(name, slowdownCompatibility),
      tsz_ms: null,
      tsgo_ms: null,
      tsz_lps: null,
      tsgo_lps: null,
      winner: "error",
      factor: 0,
      status: "tsz slowdown (35.61x slower than tsgo; threshold 8x)",
    };
  });
  const input = writeInput(dir, "input.json", rows);
  const compatibilityJsonl = path.join(dir, "project-compatibility.jsonl");
  fs.writeFileSync(
    compatibilityJsonl,
    `${JSON.stringify({ ...SAMPLE_COMPATIBILITY, name: requiredCanaryRow })}\n`,
    "utf8",
  );

  const result = runMergeInputs(dir, ["--compat-jsonl", compatibilityJsonl, input]);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  const row = merged.results.find((candidate) => candidate.name === requiredCanaryRow);
  assert.ok(row, "expected required canary benchmark row");
  assert.equal(row.status, "tsz slowdown (35.61x slower than tsgo; threshold 8x)");
  assert.equal(row.compatibility.state, "red");
  assert.equal(row.compatibility.exit_class, "slowdown");
  assert.equal(row.compatibility.first_failure_class, "runtime slowdown during project timing");
  assert.deepEqual(row.compatibility.known_blockers, [
    "runtime slowdown during project timing",
    "timing phase blocker",
  ]);
});

withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const { diagnostic_subsystems: _diagnosticSubsystems, ...compatibility } = SAMPLE_COMPATIBILITY;
  const result = runMerge(dir, [projectRow(canaryRow, compatibility)]);
  assert.equal(result.status, 0, result.stderr);
  assert.ok(fs.existsSync(result.output));
  assert.match(
    result.stderr,
    new RegExp(`::warning::[\\s\\S]*${canaryRow}: missing compatibility\\.diagnostic_subsystems`),
  );
});

withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const compatibility = {
    ...SAMPLE_COMPATIBILITY,
    fixture_sources: [],
  };
  const result = runMerge(dir, [projectRow(canaryRow, compatibility)]);
  assert.equal(result.status, 0, result.stderr);
  assert.ok(fs.existsSync(result.output));
  assert.match(
    result.stderr,
    new RegExp(`::warning::[\\s\\S]*${canaryRow}: compatibility\\.fixture_sources must name at least one source`),
  );
});

withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const compatibility = {
    ...SAMPLE_COMPATIBILITY,
    fixture_sources: [
      { name: "fixture", repository: "https://example.invalid/repo.git", ref: "" },
      { name: "", repository: "", ref: "abc123" },
    ],
  };
  const result = runMerge(dir, [projectRow(canaryRow, compatibility)]);
  assert.equal(result.status, 0, result.stderr);
  assert.ok(fs.existsSync(result.output));
  assert.match(
    result.stderr,
    new RegExp(`::warning::[\\s\\S]*${canaryRow}: compatibility\\.fixture_sources\\[0\\]\\.ref must be a non-empty string`),
  );
  assert.match(
    result.stderr,
    new RegExp(`${canaryRow}: compatibility\\.fixture_sources\\[1\\]\\.name must be a non-empty string`),
  );
  assert.match(
    result.stderr,
    new RegExp(`${canaryRow}: compatibility\\.fixture_sources\\[1\\]\\.repository must be a non-empty string`),
  );
});

withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const { owner_track: _ownerTrack, ...compatibility } = SAMPLE_COMPATIBILITY;
  const result = runMerge(dir, [projectRow(canaryRow, compatibility)]);
  assert.equal(result.status, 0, result.stderr);
  assert.ok(fs.existsSync(result.output));
  assert.match(
    result.stderr,
    new RegExp(`::warning::[\\s\\S]*${canaryRow}: missing compatibility\\.owner_track`),
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
  assert.equal(result.status, 0, result.stderr);
  assert.ok(fs.existsSync(result.output));
  assert.match(
    result.stderr,
    new RegExp(`::warning::[\\s\\S]*${canaryRow}: red/yellow compatibility\\.first_failure_class must name the first blocker`),
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
  assert.equal(result.status, 0, result.stderr);
  assert.ok(fs.existsSync(result.output));
  assert.match(
    result.stderr,
    new RegExp(`::warning::[\\s\\S]*${canaryRow}: red/yellow compatibility\\.known_blockers must name at least one blocker`),
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
  assert.equal(result.status, 0, result.stderr);
  assert.ok(fs.existsSync(result.output));
  assert.match(
    result.stderr,
    new RegExp(`::warning::[\\s\\S]*${canaryRow}: red/yellow compatibility\\.first_failure_class must match the first known blocker`),
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
  const changedRunnerEnvironment = {
    ...SAMPLE_RUNNER_ENVIRONMENT,
    cpu_model: "AMD EPYC 7B12",
    total_memory_bytes: 137438945280,
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
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.validation.runner_signature_required, true);
  assert.deepEqual(merged.validation.runner_environment_warnings, []);
});

withTempDir((dir) => {
  const changedRunnerEnvironment = {
    ...SAMPLE_RUNNER_ENVIRONMENT,
    cpu_model: "AMD EPYC 7B12",
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
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.validation.runner_signature_required, true);
  assert.deepEqual(merged.validation.runner_environment_warnings, []);
});

withTempDir((dir) => {
  const localRunnerEnvironment = { ...SAMPLE_RUNNER_ENVIRONMENT };
  delete localRunnerEnvironment.cloud_build;
  const changedRunnerEnvironment = {
    ...localRunnerEnvironment,
    cpu_model: "AMD EPYC 7B12",
  };
  const first = writeInput(
    dir,
    "bench-results-a.json",
    [projectRow("first")],
    {
      ...SAMPLE_RUN_METADATA,
      runner_environment: localRunnerEnvironment,
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
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.validation.runner_signature_required, true);
  assert.equal(merged.validation.runner_environment_warnings.length, 1);
  assert.equal(merged.validation.runner_environment_warnings[0].file, "bench-results-b.json");
  assert.deepEqual(
    merged.validation.runner_environment_warnings[0].mismatched_fields,
    ["cpu_model"],
  );
});

withTempDir((dir) => {
  const localRunnerEnvironment = { ...SAMPLE_RUNNER_ENVIRONMENT };
  delete localRunnerEnvironment.cloud_build;
  const changedRunnerEnvironment = {
    ...localRunnerEnvironment,
    total_memory_bytes: 137438945280,
  };
  const first = writeInput(
    dir,
    "bench-results-a.json",
    [projectRow("first")],
    {
      ...SAMPLE_RUN_METADATA,
      runner_environment: localRunnerEnvironment,
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
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.validation.runner_signature_required, true);
  assert.equal(merged.validation.runner_environment_warnings.length, 1);
  assert.equal(merged.validation.runner_environment_warnings[0].file, "bench-results-b.json");
  assert.deepEqual(
    merged.validation.runner_environment_warnings[0].mismatched_fields,
    ["total_memory_bytes"],
  );
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

// Optional canary/application shard signature defects are ADVISORY: a broken
// optional shard must never fail the merge step (and freeze latest.json) when
// the required shards are clean. Classify by shard.label.
withTempDir((dir) => {
  const required = writeInput(
    dir,
    "bench-results-projects.json",
    [projectRow("required-row")],
    {
      ...SAMPLE_RUN_METADATA,
      runner_environment: SAMPLE_RUNNER_ENVIRONMENT,
      shard: { label: "projects", filter: "projects" },
      filter: "projects",
    },
  );
  // Optional canary shard missing its entire runner_environment — would block
  // pre-fix; advisory now.
  const canary = writeInput(
    dir,
    "bench-results-bench-canaries.json",
    [projectRow("valibot-project")],
    {
      ...SAMPLE_RUN_METADATA,
      shard: { label: "bench-canaries", filter: "valibot-project" },
      filter: "bench-canaries",
    },
  );
  const result = runMergeInputs(dir, [required, canary], ["--require-runner-signature"]);
  assert.equal(result.status, 0, `optional canary signature defect must not block publish:\n${result.stderr}`);
  assert.match(result.stderr, /advisory gaps on optional shards/);
  assert.match(result.stderr, /bench-results-bench-canaries\.json: missing runner_environment/);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.ok(
    merged.validation.runner_signature_advisory.some((m) => /bench-results-bench-canaries\.json/.test(m)),
    "advisory signature gaps are recorded in the merged validation block",
  );
});

// The same defect on a REQUIRED shard still blocks (the advisory split must not
// leak to required rows).
withTempDir((dir) => {
  const required = writeInput(
    dir,
    "bench-results-projects.json",
    [projectRow("required-row")],
    {
      ...SAMPLE_RUN_METADATA,
      shard: { label: "projects", filter: "projects" },
      filter: "projects",
    },
  );
  const result = runMergeInputs(dir, [required], ["--require-runner-signature"]);
  assert.equal(result.status, 1, "a required shard missing runner_environment still blocks");
  assert.match(result.stderr, /validation failed \(blocking\)/);
  assert.match(result.stderr, /bench-results-projects\.json: missing runner_environment/);
});

// Filename fallback: an optional shard so broken it dropped shard.label is still
// recognised as optional by its `bench-results-bench-applications.json` name.
withTempDir((dir) => {
  const required = writeInput(
    dir,
    "bench-results-projects.json",
    [projectRow("required-row")],
    {
      ...SAMPLE_RUN_METADATA,
      runner_environment: SAMPLE_RUNNER_ENVIRONMENT,
      shard: { label: "projects", filter: "projects" },
      filter: "projects",
    },
  );
  const application = writeInput(
    dir,
    "bench-results-bench-applications.json",
    [projectRow("umami-project")],
    {
      ...SAMPLE_RUN_METADATA,
      runner_environment: SAMPLE_RUNNER_ENVIRONMENT,
      // shard omitted entirely -> missing shard.label/shard.filter, but the
      // filename identifies it as the optional applications shard.
      filter: "bench-applications",
    },
  );
  const result = runMergeInputs(dir, [required, application], ["--require-runner-signature"]);
  assert.equal(result.status, 0, `optional applications shard defect must not block:\n${result.stderr}`);
  assert.match(result.stderr, /bench-results-bench-applications\.json: missing shard\.label/);
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

// Shards measured with binaries built for different target CPUs must be
// flagged: their timings are not comparable (#13248).
withTempDir((dir) => {
  const firstProfile = cloneJson(SAMPLE_MEASUREMENT_PROFILE);
  const secondProfile = cloneJson(SAMPLE_MEASUREMENT_PROFILE);
  firstProfile.rust_target_cpu = "x86-64-v3";
  secondProfile.rust_target_cpu = "x86-64";

  const first = writeInput(
    dir,
    "bench-results-cpu-a.json",
    [projectRow("first")],
    { measurement_profile: firstProfile },
  );
  const second = writeInput(
    dir,
    "bench-results-cpu-b.json",
    [projectRow("second")],
    { measurement_profile: secondProfile },
  );
  const result = runMergeInputs(dir, [first, second]);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.validation.measurement_profile_warnings.length, 1);
  assert.deepEqual(
    merged.validation.measurement_profile_warnings[0].mismatched_fields,
    ["rust_target_cpu"],
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

// Non-artifact_missing rows with a missing required compatibility field are an
// advisory gap (issue #13607): the merge must publish timing data and warn,
// not block. (artifact_missing rows above are exempt entirely.)
withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const { peak_memory_bytes: _peakMemoryBytes, ...compatibility } = SAMPLE_COMPATIBILITY;
  const result = runMerge(dir, [projectRow(canaryRow, compatibility)]);
  assert.equal(result.status, 0, result.stderr);
  assert.ok(fs.existsSync(result.output), "advisory gap must still write bench-results.json");
  assert.match(
    result.stderr,
    new RegExp(`::warning::[\\s\\S]*${canaryRow}: missing compatibility\\.peak_memory_bytes`),
  );
});

// artifact_missing row mixed with required green rows: the artifact_missing
// row must not count as a green win even though its winner is set.
withTempDir((dir) => {
  // Required rows with a declared fixture owner are intentionally ineligible
  // for green timing even when that owner writes no `any` members. Derive the
  // exact baseline instead of assuming every required row is zero-stub.
  const requiredGreenRows = REQUIRED_PROJECT_ROWS.filter((name) => isGreen(projectRow(name)));
  assert.ok(requiredGreenRows.length > 0, "test fixture expects a green required project row");
  const missingRow = requiredGreenRows[0];
  const rows = REQUIRED_PROJECT_ROWS.map((name) =>
    name === missingRow
      ? { name, artifact_missing: true, winner: "tsz", ratio: 1, tsz_ms: 1, tsgo_ms: 2 }
      : projectRow(name),
  );
  const result = runMerge(dir, rows);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.totals.rows, REQUIRED_PROJECT_ROWS.length);
  assert.equal(merged.totals.green_tsz_wins, requiredGreenRows.length - 1, "artifact_missing row must not count as a green win");
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

// Issue #14398: a failed shard writes an error stub ({results:[], error}). It carries no timing data, so a
// --require-runner-signature merge must DROP it (with a warning) and keep
// merging the shards that succeeded — a single transient shard failure must not
// crash the whole benchmark publish on the signature gate.
withTempDir((dir) => {
  const signed = writeInput(dir, "bench-results-projects.json", [projectRow("standalone")], {
    ...SAMPLE_RUN_METADATA,
    runner_environment: SAMPLE_RUNNER_ENVIRONMENT,
    shard: { label: "projects", filter: "projects" },
    filter: "projects",
  });
  const stub = path.join(dir, "bench-results-project-hotspots.json");
  fs.writeFileSync(
    stub,
    `${JSON.stringify({ schema_version: 1, results: [], error: "benchmark shard did not write results" })}\n`,
    "utf8",
  );
  const result = runMergeInputs(dir, [signed, stub], ["--require-runner-signature"]);
  assert.equal(result.status, 0, result.stderr);
  assert.match(
    result.stderr,
    /::warning::[\s\S]*bench-results-project-hotspots\.json: benchmark shard did not write results/,
  );
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.ok(merged.results.some((r) => r.name === "standalone"), "the successful shard must survive the merge");
  assert.deepEqual(
    merged.merged_from,
    ["bench-results-projects.json"],
    "the dropped error stub must not appear in merged_from",
  );
  assert.equal(merged.totals.rows, 1, "the dropped stub must not contribute rows");
  assert.equal(
    merged.validation.hyperfine_exit_codes_required,
    true,
    "a dropped stub must not flip aggregate validation flags",
  );
  assert.ok(
    merged.validation.dropped_empty_shards.some(
      (s) => s.file === "bench-results-project-hotspots.json"
        && s.error === "benchmark shard did not write results",
    ),
    "the dropped stub must be recorded in validation.dropped_empty_shards",
  );
});

// A payload with no result rows is treated as a failed/partial shard and dropped
// even when it carries no `error` and no signature: it contributes nothing, so it
// is not subject to the runner-signature gate.
withTempDir((dir) => {
  const signed = writeInput(dir, "bench-results-a.json", [projectRow("standalone")], {
    ...SAMPLE_RUN_METADATA,
    runner_environment: SAMPLE_RUNNER_ENVIRONMENT,
    shard: { label: "a", filter: "a" },
    filter: "a",
  });
  const empty = path.join(dir, "bench-results-empty.json");
  fs.writeFileSync(empty, `${JSON.stringify({ schema_version: 1, results: [] })}\n`, "utf8");
  const result = runMergeInputs(dir, [signed, empty], ["--require-runner-signature"]);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.deepEqual(merged.merged_from, ["bench-results-a.json"]);
  assert.ok(
    merged.validation.dropped_empty_shards.some((s) => s.file === "bench-results-empty.json" && s.error === null),
  );
});

// Security boundary preserved: a shard that DOES contribute result rows but lacks
// the runner signature must still hard-fail under --require-runner-signature — only
// genuinely-empty shards are exempt (they carry no timing data to forge).
withTempDir((dir) => {
  const unsignedWithResults = writeInput(dir, "bench-results-unsigned.json", [projectRow("standalone")], {
    ...SAMPLE_RUN_METADATA,
    shard: { label: "unsigned", filter: "unsigned" },
    filter: "unsigned",
  });
  const result = runMergeInputs(dir, [unsignedWithResults], ["--require-runner-signature"]);
  assert.equal(result.status, 1, "an unsigned payload that contributes rows must still fail");
  assert.match(result.stderr, /bench-results-unsigned\.json: missing runner_environment/);
});

// A perf-timed canary (the bench-canaries shard already measured it) must keep
// its real tsz_ms/tsgo_ms and must NOT be stamped the "not timed" placeholder
// status when its compile-guard compat is attached — otherwise isGreen() becomes
// false and the (green) canary silently drops off the perf chart. Regression for
// the bench-canaries perf-chart fix.
withTempDir((dir) => {
  const canaryRow = COMPILE_ONLY_CANARY_PROJECT_ROWS[0];
  const timedRow = { name: canaryRow, winner: "tsgo", tsz_ms: 2549, tsgo_ms: 206, ratio: 12.36 };
  const input = writeInput(dir, "input.json", [timedRow]);
  const compatibilityJsonl = path.join(dir, "project-compatibility.jsonl");
  fs.writeFileSync(
    compatibilityJsonl,
    `${JSON.stringify({ ...SAMPLE_COMPATIBILITY, name: canaryRow, files_reached: 50 })}\n`,
    "utf8",
  );
  const result = runMergeInputs(dir, ["--compat-jsonl", compatibilityJsonl, input]);
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  const row = merged.results.find((candidate) => candidate.name === canaryRow);
  assert.ok(row, "expected the perf-timed canary row to survive merge");
  assert.equal(row.tsz_ms, 2549, "perf-timed canary must keep its measured tsz_ms");
  assert.equal(row.tsgo_ms, 206, "perf-timed canary must keep its measured tsgo_ms");
  assert.ok(!row.status, "a perf-timed canary must not be stamped 'not timed' (would drop it off the chart)");
  assert.equal(row.compatibility.state, "green");
});

// The merged artifact's `generated_at` must be a FRESH wall-clock stamp at merge
// time, never inherited from a shard payload. The publish path's monotonic
// latest.json guard refuses to overwrite latest.json when the incoming
// generated_at is <= the published one; if the merge inherited a frozen/stale
// shard timestamp, every fresh run would look "older-or-equal" and the public
// site would stop advancing. Pin that a stale shard generated_at does not leak
// into the merged metadata.
withTempDir((dir) => {
  const staleGeneratedAt = "2020-01-01T00:00:00.000Z";
  const inputs = REQUIRED_PROJECT_ROWS.map((name, index) =>
    writeInput(dir, `bench-results-${index}.json`, [projectRow(name)], {
      generated_at: staleGeneratedAt,
      source_commit: "abcdef1234567890abcd",
    }),
  );
  const before = Date.now();
  const result = runMergeInputs(dir, inputs);
  const after = Date.now();
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.notEqual(
    merged.generated_at,
    staleGeneratedAt,
    "merged generated_at must not inherit a shard's frozen timestamp",
  );
  const mergedMs = Date.parse(merged.generated_at);
  assert.ok(Number.isFinite(mergedMs), "merged generated_at must be a valid ISO 8601 timestamp");
  // Wall-clock stamp at merge time: within the [before, after] window (allowing
  // a small clock skew). Critically it is strictly newer than the stale shards.
  assert.ok(
    mergedMs >= before - 1000 && mergedMs <= after + 1000,
    `merged generated_at (${merged.generated_at}) should reflect merge time, not a stale/inherited value`,
  );
  assert.ok(
    mergedMs > Date.parse(staleGeneratedAt),
    "merged generated_at must advance past stale shard timestamps so the monotonic publish guard can publish",
  );
});
