#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  REQUIRED_PROJECT_ROWS as ALL_REQUIRED_PROJECT_ROWS,
  PROJECT_ROW_DEFINITIONS,
  PROJECT_ROWS_BY_NAME,
} from "./project-rows.mjs";
import { BENCH_RUNNER_EXCLUDED_ROWS } from "./project-row-summary.mjs";
import { fixtureStubEvidenceFor } from "./lib/fixture-stub-inventory.mjs";

// check-artifact-readiness.mjs cannot be imported directly (it runs its CLI,
// including process.exit(), at module scope), so its RUNTIME_GATED_REQUIRED_ROWS
// is mirrored here by literal rather than imported. Keep this in sync with
// that constant (#17561): rows present in bench-vs-tsgo.sh but only measured
// behind a runtime kill-switch, so a default scheduled run never produces a
// result for them.
const RUNTIME_GATED_REQUIRED_ROWS = new Set(["nextjs"]);

// The readiness gate checks only required rows the bench runner actually
// measures: it subtracts BENCH_RUNNER_EXCLUDED_ROWS, RUNTIME_GATED_REQUIRED_ROWS,
// and category:"application" rows. Mirror that here so the synthesized
// artifacts and row counts match.
const REQUIRED_PROJECT_ROWS = ALL_REQUIRED_PROJECT_ROWS.filter(
  (name) =>
    !BENCH_RUNNER_EXCLUDED_ROWS.has(name) &&
    !RUNTIME_GATED_REQUIRED_ROWS.has(name) &&
    PROJECT_ROWS_BY_NAME[name]?.category !== "application",
);
const APPLICATION_PROJECT_ROWS = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.category === "application")
  .map((row) => row.name);
// The --require-green-project-timing-pairs gate only flags rows that are
// perf_timed. A green-compat application row that is *not* perf_timed (its
// vs-tsgo perf benchmark legitimately errors, e.g. infisical) must not be
// counted as a missing timing-pair gap.
const PERF_TIMED_APPLICATION_ROWS = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.category === "application" && row.perf_timed === true)
  .map((row) => row.name);
const NON_PERF_TIMED_APPLICATION_ROWS = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.category === "application" && row.perf_timed !== true)
  .map((row) => row.name);
// --require-application-compat only HARD-BLOCKS on benchmark_set:"required"
// application rows. Canary application rows (every category:"application" row
// today) are real apps installed by the optional best-effort bench-applications
// shard, so a missing/incomplete one is advisory, never publish-blocking.
const CANARY_APPLICATION_ROWS = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.category === "application" && row.benchmark_set === "canary")
  .map((row) => row.name);
const REQUIRED_APPLICATION_ROWS = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.category === "application" && row.benchmark_set === "required")
  .map((row) => row.name);

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const CHECK_SCRIPT = path.join(ROOT, "scripts", "bench", "check-artifact-readiness.mjs");

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-artifact-readiness-"));
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function writeJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

function makeCompatibility(state, projectName) {
  const stubEvidence = fixtureStubEvidenceFor(ROOT, projectName);
  return {
    generated_at: "2026-05-19T01:02:03.000Z",
    source_commit: "abcdef1234567890",
    workflow_name: "Bench",
    workflow_run_id: "12345",
    workflow_run_url: "https://github.com/tsz-org/tsz/actions/runs/12345",
    workflow_run_attempt: "1",
    run_status: "completed",
    state,
    exit_class: state === "green" ? "exit success" : state === "red" ? "nonzero exit" : "exit success",
    first_failure_class: state === "green" ? null : "some failure",
    owner_track: null,
    semantic_owner_family: "recursive type evaluation pressure",
    phase: "check",
    last_successful_phase: "check",
    diagnostic_status: state === "green" ? "none" : state === "yellow" ? "diagnostic mismatch" : "none",
    evidence_schema: 2,
    semantic_completion: "complete",
    root_files: 1,
    source_files: 1,
    root_file_fingerprint: "a".repeat(64),
    source_file_fingerprint: "b".repeat(64),
    oracle_root_files: 1,
    oracle_source_files: 1,
    oracle_root_file_fingerprint: "a".repeat(64),
    oracle_source_file_fingerprint: "b".repeat(64),
    diagnostic_records: 0,
    diagnostic_fingerprint: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    oracle_diagnostic_records: 0,
    oracle_diagnostic_fingerprint: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    stub_inventory_schema: stubEvidence.stubInventorySchema,
    stubbed_modules: stubEvidence.stubbedModules,
    stubbed_any_members: stubEvidence.stubbedAnyMembers,
    stub_inventory_fingerprint: stubEvidence.stubInventoryFingerprint,
    oracle_classification: "both-pass",
    diagnostic_deltas: [],
    diagnostic_subsystems: [],
    known_blockers: state === "green" ? [] : ["recursive alias instantiation"],
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
    exit_codes: { tsc: [0], tsz: [0] },
    files_reached: 1,
    peak_memory_bytes: 1024,
    fixture_sources: [{ name: "fixture", repository: "https://example.invalid/repo.git", ref: "abc123" }],
    emit_status: "not in scope (noEmit project check)",
    dts_status: "not in scope (noEmit project check)",
  };
}

function makeRow(name, state = "green", opts = {}) {
  return {
    name,
    lines: 100,
    kb: 10,
    tsz_ms: Object.hasOwn(opts, "tsz_ms") ? opts.tsz_ms : 50,
    tsgo_ms: Object.hasOwn(opts, "tsgo_ms") ? opts.tsgo_ms : 40,
    winner: Object.hasOwn(opts, "winner") ? opts.winner : "tsgo",
    ratio: 1.25,
    ...(opts.errorStatus ? { status: opts.errorStatus } : {}),
    compatibility: makeCompatibility(state, name),
  };
}

function makeArtifact(rows, extraMeta = {}) {
  return {
    generated_at: "2026-05-19T01:02:03.000Z",
    source_commit: "abcdef1234567890abcd",
    workflow_name: "Bench",
    workflow_run_id: "99999",
    workflow_run_url: "https://github.com/tsz-org/tsz/actions/runs/99999",
    workflow_run_attempt: "1",
    run_status: "completed",
    benchmark_runner: "scripts/bench/bench-vs-tsgo.sh",
    quick_mode: false,
    totals: { benchmarks_run: rows.length, rows: rows.length },
    results: rows,
    ...extraMeta,
  };
}

const SAMPLE_MEASUREMENT_PROFILE = {
  mode: "release-pgo",
  tsz_binary_source: "bench-dist",
  rust_target_cpu: "x86-64-v3",
  profile_guided_optimization: {
    requested: true,
    required: true,
    optimized: true,
    marker_path: "/tmp/tsz/.target-bench/dist/.bench-pgo-optimized",
    marker_found: true,
    profile_use: "/tmp/tsz/.target-bench/pgo-data/merged.profdata",
    profile_fingerprint: "abcdef1234567890",
    training_fingerprint: "123456abcdef7890",
    profile_data_source: "fresh",
    built_at: "2026-05-20T01:02:03Z",
    llvm_profdata: "/toolchain/bin/llvm-profdata",
    training_metadata_available: true,
    training_input_count: 17,
    training_failure_count: 0,
    training_inputs: ["stdin:scalar", "synthetic:mapped_type.ts"],
    training_failed_inputs: [],
  },
};

function run(artifactFile, extraArgs = []) {
  return spawnSync(process.execPath, [CHECK_SCRIPT, ...(artifactFile ? [artifactFile] : []), ...extraArgs], {
    cwd: ROOT,
    encoding: "utf8",
    env: { ...process.env, GITHUB_STEP_SUMMARY: "" },
  });
}

// ---------------------------------------------------------------------------
// Test: missing artifact file → exit 2
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const result = run(path.join(dir, "nonexistent.json"));
  assert.equal(result.status, 2, "missing artifact file should exit 2");
  assert.match(result.stdout, /Artifact missing/i, "should report artifact missing");
});
console.log("✅ missing artifact file exits 2");

// ---------------------------------------------------------------------------
// Test: malformed artifact → exit 2
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bad.json");
  fs.writeFileSync(file, "not json {{{");
  const result = run(file);
  assert.equal(result.status, 2, "malformed artifact should exit 2");
  assert.match(result.stdout, /could not be parsed/i, "should report parse error");
});
console.log("✅ malformed artifact exits 2");

// ---------------------------------------------------------------------------
// Test: complete artifact with all required rows green → exit 0
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(rows, { measurement_profile: SAMPLE_MEASUREMENT_PROFILE }));
  const result = run(file, ["--json", "--require-green"]);
  assert.equal(result.status, 0, `all-green artifact should exit 0, got:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.all_required_rows_green, true, "all-green JSON should mark release gate ready");
  assert.equal(parsed.corpus_health.collapsed, false, "all-green corpus is not collapsed");
  assert.equal(parsed.corpus_health.green, REQUIRED_PROJECT_ROWS.length, "corpus_health counts every green row");
  assert.equal(parsed.corpus_health.errored, 0, "all-green corpus reports zero errored rows");
  assert.equal(parsed.metadata_clean, true, "all-green artifact should be metadata-clean");
  assert.equal(parsed.metadata_warnings_total, 0, "all-green artifact should not report metadata warnings");
  assert.deepEqual(parsed.non_green_required_rows, [], "all-green JSON should not report non-green rows");
  const zeroStubRow = parsed.rows.find((row) => row.name === "utility-types-project");
  const zeroStubEvidence = fixtureStubEvidenceFor(ROOT, "utility-types-project");
  assert.equal(zeroStubRow.stub_inventory_schema, zeroStubEvidence.stubInventorySchema);
  assert.equal(zeroStubRow.stubbed_modules, 0);
  assert.equal(zeroStubRow.stubbed_any_members, 0);
  assert.equal(
    zeroStubRow.stub_inventory_fingerprint,
    zeroStubEvidence.stubInventoryFingerprint,
    "readiness must preserve the source-verified zero-stub fingerprint",
  );
  assert.match(result.stderr, new RegExp(`green.*\\| ${REQUIRED_PROJECT_ROWS.length}`), "should show all green count");
  assert.match(result.stderr, /Measurement profile.*release-pgo/, "should show measurement profile mode");
  assert.match(result.stderr, /PGO profile.*abcdef123456/, "should show PGO profile fingerprint");
  assert.match(result.stderr, /PGO training.*123456abcdef/, "should show PGO training fingerprint");
  assert.match(result.stderr, /Binary target CPU.*x86-64-v3/, "should show the binary codegen target CPU");
});
console.log("✅ complete all-green artifact exits 0");

// A legacy phase-only green label is not project evidence. Readiness
// independently rechecks schema-v2 graph, diagnostic, and exit parity instead
// of trusting the producer's state string.
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  delete rows[0].compatibility.evidence_schema;
  delete rows[0].compatibility.root_file_fingerprint;
  writeJson(file, makeArtifact(rows, { measurement_profile: SAMPLE_MEASUREMENT_PROFILE }));
  const result = run(file, ["--json", "--require-green"]);
  assert.equal(result.status, 1, "phase-only green metadata must fail readiness");
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.gray, 1);
  assert.deepEqual(parsed.non_green_required_rows, [
    { name: REQUIRED_PROJECT_ROWS[0], state: "gray" },
  ]);
});
console.log("✅ readiness rejects green rows without exact schema-v2 evidence");

withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  rows[0].compatibility.oracle_source_file_fingerprint = "c".repeat(64);
  writeJson(file, makeArtifact(rows, { measurement_profile: SAMPLE_MEASUREMENT_PROFILE }));
  const result = run(file, ["--json", "--require-green"]);
  assert.equal(result.status, 1, "equal counts with different source paths must fail readiness");
  assert.equal(JSON.parse(result.stdout.trim()).gray, 1);
});
console.log("✅ readiness rechecks graph fingerprints, not counts alone");

withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  delete rows[0].compatibility.stub_inventory_schema;
  delete rows[0].compatibility.stubbed_modules;
  delete rows[0].compatibility.stubbed_any_members;
  delete rows[0].compatibility.stub_inventory_fingerprint;
  writeJson(file, makeArtifact(rows, { measurement_profile: SAMPLE_MEASUREMENT_PROFILE }));
  const result = run(file, ["--json"]);
  assert.equal(result.status, 1, "omitted stub evidence must fail readiness unconditionally");
  assert.deepEqual(JSON.parse(result.stdout.trim()).invalid_project_evidence_rows, [rows[0].name]);
});
console.log("✅ readiness rejects green rows that omit fixture-stub evidence");

withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  const zero = fixtureStubEvidenceFor(ROOT, "utility-types-project");
  const forgedNames = ["msw-project", "effect-project", "drizzle-orm-project"];
  for (const name of forgedNames) {
    const forged = makeRow(name, "green");
    forged.compatibility.stubbed_modules = 0;
    forged.compatibility.stubbed_any_members = 0;
    forged.compatibility.stub_inventory_fingerprint = zero.stubInventoryFingerprint;
    rows.push(forged);
  }
  writeJson(file, makeArtifact(rows, { measurement_profile: SAMPLE_MEASUREMENT_PROFILE }));
  const result = run(file, ["--json"]);
  assert.equal(result.status, 1, "forged zero-stub canary rows must fail readiness");
  assert.deepEqual(JSON.parse(result.stdout.trim()).invalid_project_evidence_rows, forgedNames);
});
console.log("✅ readiness recomputes MSW/effect/drizzle stub truth and rejects forged zeros");

// ---------------------------------------------------------------------------
// Test: --require-project-timing-pairs accepts a present artifact with at least
// one successful project timing row.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(rows, { measurement_profile: SAMPLE_MEASUREMENT_PROFILE }));
  const result = run(file, ["--json", "--require-project-timing-pairs=1"]);
  assert.equal(result.status, 0, `project timing pair gate should pass on green timed rows:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(
    parsed.successful_project_timing_pairs,
    REQUIRED_PROJECT_ROWS.length,
    "JSON should count successful project timing pairs",
  );
});
console.log("✅ --require-project-timing-pairs passes with project timing data");

// ---------------------------------------------------------------------------
// Test: bare --require-project-timing-pairs defaults to one timing pair without
// consuming the artifact path as a value.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(rows));
  const result = run(file, ["--json", "--require-project-timing-pairs"]);
  assert.equal(result.status, 0, `bare project timing pair gate should pass:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.required_project_timing_pairs, 1);
});
console.log("✅ bare --require-project-timing-pairs defaults to one pair");

// ---------------------------------------------------------------------------
// Test: expected source commit marks an artifact current when it matches.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(rows, { measurement_profile: SAMPLE_MEASUREMENT_PROFILE }));
  const result = run(file, ["--json", "--expect-source-commit=abcdef1234567890"]);
  assert.equal(result.status, 0, `current source artifact should exit 0:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.source_freshness.current, true, "JSON should mark matching source current");
  assert.equal(parsed.source_freshness.warning, null, "matching source should not warn");
  assert.match(result.stderr, /Source freshness.*current for abcdef123456/);
});
console.log("✅ source freshness reports current artifact source");

// ---------------------------------------------------------------------------
// Test: --require-source-current fails when the artifact source is stale.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(rows, { measurement_profile: SAMPLE_MEASUREMENT_PROFILE }));
  const result = run(file, [
    "--json",
    "--expect-source-commit",
    "1111111111111111111111111111111111111111",
    "--require-source-current",
  ]);
  assert.equal(result.status, 1, "stale source should fail the source-current gate");
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.source_freshness.current, false, "JSON should mark stale source not current");
  assert.match(parsed.source_freshness.warning, /differs from expected 111111111111/);
  assert.match(result.stderr, /source freshness failed/);
});
console.log("✅ --require-source-current fails on stale artifact source");

// ---------------------------------------------------------------------------
// Test: --require-source-current infers HEAD when no explicit expected commit
// is supplied, so local release-truth checks cannot silently skip freshness.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(rows, {
    measurement_profile: SAMPLE_MEASUREMENT_PROFILE,
    source_commit: "1111111111111111111111111111111111111111",
  }));
  const result = run(file, ["--json", "--require-source-current"]);
  assert.equal(result.status, 1, "source-current gate should infer HEAD and fail stale artifacts");
  const parsed = JSON.parse(result.stdout.trim());
  const head = spawnSync("git", ["rev-parse", "HEAD"], {
    cwd: ROOT,
    encoding: "utf8",
  }).stdout.trim().toLowerCase();
  assert.equal(parsed.source_freshness.expected_source_commit, head, "JSON should record inferred HEAD");
  assert.equal(parsed.source_freshness.current, false, "JSON should mark inferred stale source not current");
  assert.match(result.stderr, /source freshness failed/);
});
console.log("✅ --require-source-current infers HEAD when no expected source is passed");

// ---------------------------------------------------------------------------
// Test: modern artifact without measurement_profile still exits 0 but reports
// the missing profile so dashboards can surface the metadata gap.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(rows));
  const result = run(file);
  assert.equal(result.status, 0, `missing measurement profile should warn, not fail:\n${result.stderr}`);
  assert.match(result.stdout, /Measurement profile.*measurement_profile missing/);
  assert.match(result.stdout, /Measurement profile warnings.*\| 1 \|/);
  assert.match(result.stdout, /artifact measurement_profile.*measurement_profile missing/);
});
console.log("✅ missing measurement profile is reported without failing readiness");

// ---------------------------------------------------------------------------
// Test: --require-clean-metadata fails when measurement_profile is missing.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(rows));
  const result = run(file, ["--json", "--require-clean-metadata"]);
  assert.equal(result.status, 1, "missing measurement profile should fail clean metadata gate");
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.metadata_clean, false, "JSON should mark missing measurement profile as unclean");
  assert.equal(parsed.metadata_warnings_total, 1, "JSON should count missing measurement profile warning");
  assert.match(result.stderr, /measurement profile artifact measurement_profile: measurement_profile missing/);
});
console.log("✅ --require-clean-metadata fails on missing measurement profile");

// ---------------------------------------------------------------------------
// Test: merged artifact validation warnings are surfaced in readiness output so
// runner and measurement metadata problems are visible to dashboards.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(rows, {
    measurement_profile: SAMPLE_MEASUREMENT_PROFILE,
    validation: {
      runner_environment_warnings: [
        {
          file: "bench-results-b.json",
          mismatched_fields: ["cpu_count", "cloud_build_machine_type"],
          expected: { cpu_count: 32 },
          actual: { cpu_count: 16 },
        },
      ],
      measurement_profile_warnings: [
        {
          file: "bench-results-pgo-b.json",
          mismatched_fields: ["profile_guided_optimization.profile_fingerprint"],
          expected: { profile_guided_optimization: { profile_fingerprint: "aaa" } },
          actual: { profile_guided_optimization: { profile_fingerprint: "bbb" } },
        },
      ],
    },
  }));
  const result = run(file, ["--json"]);
  assert.equal(result.status, 0, `validation warnings should not fail readiness:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.metadata_clean, false, "JSON should mark validation warnings as unclean");
  assert.equal(parsed.metadata_warnings_total, 2, "JSON should count all metadata warnings");
  assert.equal(parsed.validation_warnings.total, 2, "JSON should count validation warnings");
  assert.deepEqual(
    parsed.validation_warnings.runner_environment[0].mismatched_fields,
    ["cpu_count", "cloud_build_machine_type"],
    "JSON should preserve runner metadata warning fields",
  );
  assert.deepEqual(
    parsed.validation_warnings.measurement_profile[0].mismatched_fields,
    ["profile_guided_optimization.profile_fingerprint"],
    "JSON should preserve measurement profile warning fields",
  );
  assert.match(result.stderr, /Runner metadata warnings \(1\)/);
  assert.match(result.stderr, /Measurement profile warnings \(1\)/);
});
console.log("✅ validation warnings are surfaced in readiness output");

// ---------------------------------------------------------------------------
// Test: --require-clean-metadata fails when merged validation warnings exist.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(rows, {
    measurement_profile: SAMPLE_MEASUREMENT_PROFILE,
    validation: {
      runner_environment_warnings: [
        {
          file: "bench-results-b.json",
          mismatched_fields: ["cpu_model"],
          expected: { cpu_model: "Intel Xeon" },
          actual: { cpu_model: "AMD EPYC" },
        },
      ],
    },
  }));
  const result = run(file, ["--json", "--require-clean-metadata"]);
  assert.equal(result.status, 1, "runner metadata warnings should fail clean metadata gate");
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.metadata_clean, false, "JSON should mark runner warning artifact as unclean");
  assert.equal(parsed.metadata_warnings_total, 1, "JSON should count the runner warning");
  assert.match(result.stderr, /runner metadata bench-results-b\.json: cpu_model/);
});
console.log("✅ --require-clean-metadata fails on runner metadata warnings");

// ---------------------------------------------------------------------------
// Test: artifact missing one required row → exit 1
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const incompleteRows = REQUIRED_PROJECT_ROWS.slice(1).map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(incompleteRows));
  const result = run(file);
  assert.equal(result.status, 1, "missing required row should exit 1");
  assert.match(result.stderr, /missing/, "should mention missing in stderr");
  assert.match(result.stdout, /missing required rows/i, "should mention missing rows in report");
  assert.match(result.stdout, new RegExp(REQUIRED_PROJECT_ROWS[0]), "should name the missing row");
});
console.log("✅ missing required row exits 1");

// ---------------------------------------------------------------------------
// Test: row with error status → state is red → exit 0 (red != missing)
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name, i) =>
    i === 0
      ? makeRow(name, "red", { errorStatus: "tsz crashed" })
      : makeRow(name, "green"),
  );
  writeJson(file, makeArtifact(rows));
  const result = run(file);
  // Red rows are present but not missing — script exits 0 (all rows present)
  assert.equal(result.status, 0, `red row present in artifact should still exit 0, got:\n${result.stderr}`);
  assert.match(result.stdout, /❌.*red.*\| 1/i, "should show 1 red row");
  assert.match(result.stdout, /Phase.*Blocker family/, "should include phase and blocker family columns");
  assert.match(result.stdout, /Last phase.*Files.*Peak RSS/, "should include residency metadata columns");
  assert.match(result.stdout, /some failure/, "should name the first failure class for red rows");
  assert.match(result.stdout, /0\.0 MiB/, "should show peak RSS in MiB");
  assert.match(
    result.stdout,
    /recursive alias instantiation/,
    "should name the first known blocker for red rows",
  );
});
console.log("✅ red row present in artifact exits 0 (not missing)");

// ---------------------------------------------------------------------------
// Test: --require-project-timing-pairs fails an otherwise green artifact when
// no green row recorded a tsz/tsgo timing pair. The rows stay green-compat (so
// the corpus-collapse floor does not fire — that is exercised separately below)
// but carry no timing, isolating the timing-pair gate.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) =>
    makeRow(name, "green", { tsz_ms: null, tsgo_ms: null, winner: "error" }),
  );
  writeJson(file, makeArtifact(rows));
  const result = run(file, ["--json", "--require-project-timing-pairs=1"]);
  assert.equal(result.status, 1, "publish gate should fail when no project timing pairs succeeded");
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.successful_project_timing_pairs, 0, "JSON should show zero successful project timings");
  assert.equal(parsed.required_project_timing_pairs, 1, "JSON should include the requested timing-pair floor");
  assert.equal(parsed.corpus_health.collapsed, false, "green-untimed corpus is not collapsed");
  assert.match(result.stderr, /0 successful project timing pair\(s\); required 1/);
});
console.log("✅ --require-project-timing-pairs fails green-but-untimed project artifacts");

// ---------------------------------------------------------------------------
// Test: corpus collapse (#17561, point 4) — a required corpus that is fully
// PRESENT but carries zero green rows must fail unconditionally, even with no
// gate flags. This is the all-`error` run that read "ok" on 2026-08-15, where
// the gate only counted missing rows and every present-but-errored row slipped
// through.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) =>
    makeRow(name, "red", { errorStatus: "tsz error; tsc ok", tsz_ms: null, tsgo_ms: null, winner: "error" }),
  );
  writeJson(file, makeArtifact(rows));
  const result = run(file, ["--json"]);
  assert.equal(result.status, 1, `all-errored corpus must fail even with no gate flags, got:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.corpus_health.collapsed, true, "all-errored corpus is collapsed");
  assert.equal(parsed.corpus_health.green, 0, "collapsed corpus has zero green rows");
  assert.equal(
    parsed.corpus_health.errored,
    REQUIRED_PROJECT_ROWS.length,
    "collapsed corpus reports every required-measured row as errored",
  );
  assert.equal(
    parsed.corpus_health.measured,
    REQUIRED_PROJECT_ROWS.length,
    "corpus_health.measured counts every required-measured row",
  );
  assert.match(result.stderr, /required corpus health below floor — 0\//);
  // In --json mode the markdown report is written to stderr, not stdout.
  assert.match(result.stderr, /Required corpus collapsed/, "markdown surfaces the collapse section");
});
console.log("✅ corpus collapse fails an all-errored present corpus unconditionally");

// ---------------------------------------------------------------------------
// Test: steady state preserved — a corpus with some rows erroring behind open
// compiler issues (e.g. zod #16055, large-ts-repo #14101) but a green majority
// is NOT collapsed and still publishes. The collapse floor must never freeze a
// partially-degraded but healthy dataset.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name, i) =>
    i < 2
      ? makeRow(name, "red", { errorStatus: "tsz error; tsc ok", tsz_ms: null, tsgo_ms: null, winner: "error" })
      : makeRow(name, "green"),
  );
  writeJson(file, makeArtifact(rows));
  const result = run(file, ["--json"]);
  assert.equal(result.status, 0, `degraded-but-green-majority corpus should publish, got:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.corpus_health.collapsed, false, "green-majority corpus is not collapsed");
  assert.equal(parsed.corpus_health.errored, 2, "corpus_health reports the two errored rows");
  assert.equal(
    parsed.corpus_health.green,
    REQUIRED_PROJECT_ROWS.length - 2,
    "corpus_health reports the green majority",
  );
});
console.log("✅ corpus collapse tolerates a degraded but green-majority corpus");

// ---------------------------------------------------------------------------
// Test: --require-corpus-health=<n> raises the floor above the default 1, so a
// partial collapse (green present, but fewer than the floor) blocks; a corpus
// meeting the floor passes. This is the opt-in policy knob; the default gate
// only enforces the freeze-proof floor of 1.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  // Exactly two green rows, the rest errored.
  const rows = REQUIRED_PROJECT_ROWS.map((name, i) =>
    i < 2
      ? makeRow(name, "green")
      : makeRow(name, "red", { errorStatus: "tsz error; tsc ok", tsz_ms: null, tsgo_ms: null, winner: "error" }),
  );
  writeJson(file, makeArtifact(rows));

  const below = run(file, ["--json", "--require-corpus-health=3"]);
  assert.equal(below.status, 1, "two green rows must fail a floor of three");
  assert.match(below.stderr, /required corpus health below floor — 2\/.*floor 3/);
  const belowParsed = JSON.parse(below.stdout.trim());
  assert.equal(belowParsed.corpus_health.collapsed, false, "two green rows are not a zero-green collapse");
  assert.equal(belowParsed.corpus_health.green, 2, "JSON reports two green rows");

  const meets = run(file, ["--json", "--require-corpus-health=2"]);
  assert.equal(meets.status, 0, `two green rows must meet a floor of two, got:\n${meets.stderr}`);
});
console.log("✅ --require-corpus-health raises the green floor above the default");

// ---------------------------------------------------------------------------
// Test: --require-application-compat surfaces absent application compatibility
// rows but, because every application row today is benchmark_set:"canary", does
// NOT block the publish — they are advisory. The compat for these real apps
// comes from the optional best-effort bench-applications shard / matching-CI
// artifact, either of which can legitimately be absent. (When a future
// benchmark_set:"required" application row is added, its gap WOULD block; see
// blocking_application_compatibility_gaps below.)
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(rows));
  const result = run(file, ["--json", "--require-application-compat"]);
  const expectBlocking = REQUIRED_APPLICATION_ROWS.length > 0;
  assert.equal(
    result.status,
    expectBlocking ? 1 : 0,
    `all-canary application rows absent must be advisory (exit 0), not blocking:\n${result.stderr}`,
  );
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.application_compatibility.required, true);
  assert.equal(parsed.application_compatibility.row_count, APPLICATION_PROJECT_ROWS.length);
  assert.equal(parsed.application_compatibility.present, 0);
  assert.equal(parsed.application_compatibility.missing, APPLICATION_PROJECT_ROWS.length);
  assert.equal(parsed.application_compatibility.advisory_gaps, CANARY_APPLICATION_ROWS.length);
  assert.equal(parsed.application_compatibility.blocking_gaps, REQUIRED_APPLICATION_ROWS.length);
  assert.equal(parsed.advisory_application_compatibility_gaps, CANARY_APPLICATION_ROWS.length);
  assert.equal(parsed.blocking_application_compatibility_gaps, REQUIRED_APPLICATION_ROWS.length);
  assert.match(
    result.stderr,
    /canary application compatibility gap\(s\) \(advisory, not blocking publish\)/,
  );
});
console.log("✅ --require-application-compat treats absent canary application rows as advisory");

// ---------------------------------------------------------------------------
// Test: compile-only application rows with complete compatibility metadata pass
// the application gate even though they are not timed by bench-vs-tsgo.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const requiredRows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  const applicationRows = APPLICATION_PROJECT_ROWS.map((name) =>
    makeRow(name, "green", {
      errorStatus: "compile canary tracked in CI; not timed by vs-tsgo benchmarks",
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
    }),
  );
  writeJson(file, makeArtifact([...requiredRows, ...applicationRows]));
  const result = run(file, ["--json", "--require-application-compat"]);
  assert.equal(result.status, 0, `complete application compatibility rows should pass:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.application_compatibility.present, APPLICATION_PROJECT_ROWS.length);
  assert.equal(parsed.application_compatibility.complete, APPLICATION_PROJECT_ROWS.length);
  assert.equal(parsed.application_compatibility.missing, 0);
  assert.equal(parsed.application_compatibility.incomplete, 0);
});
console.log("✅ --require-application-compat accepts complete compile-only app rows");

// ---------------------------------------------------------------------------
// Test: --require-green-project-timing-pairs surfaces green perf-timed rows that
// only have compile compatibility, but treats them as ADVISORY (non-blocking)
// because every perf_timed row today is a canary/advisory shard. A green-compat
// canary whose vs-tsgo perf benchmark errors must not freeze the publish — it is
// simply omitted from the chart. Regression guard for the half-day benchmark
// site freeze where one such canary (infisical) hard-blocked ~70% of Bench
// publishes until it was demoted to perf_timed:false (#15004); this generalizes
// the rule so any flaky canary timing pair is advisory.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const requiredRows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  const applicationRows = APPLICATION_PROJECT_ROWS.map((name) =>
    makeRow(name, "green", {
      errorStatus: "compile canary tracked in CI; not timed by vs-tsgo benchmarks",
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
    }),
  );
  writeJson(file, makeArtifact([...requiredRows, ...applicationRows]));
  const result = run(file, [
    "--json",
    "--require-application-compat",
    "--require-green-project-timing-pairs",
  ]);
  assert.equal(
    result.status,
    0,
    `canary perf-timed rows missing a chart timing pair must not block publish:\n${result.stderr}`,
  );
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.require_green_project_timing_pairs, true);
  // The full advisory set still surfaces the missing canary charts...
  assert.equal(parsed.green_project_timing_pair_gaps, PERF_TIMED_APPLICATION_ROWS.length);
  assert.equal(parsed.advisory_project_timing_pair_gaps, PERF_TIMED_APPLICATION_ROWS.length);
  // ...but none of them is publish-blocking (no required row opts into perf timing).
  assert.equal(parsed.blocking_project_timing_pair_gaps, 0);
  assert.deepEqual(parsed.blocking_project_timing_pair_gap_rows, []);
  const gapRowNames = parsed.green_project_timing_pair_gap_rows.map((r) => r.name);
  for (const name of NON_PERF_TIMED_APPLICATION_ROWS) {
    assert.ok(
      !gapRowNames.includes(name),
      `non-perf-timed canary app ${name} must not be flagged as a missing timing-pair gap`,
    );
  }
  for (const row of parsed.green_project_timing_pair_gap_rows) {
    assert.equal(row.blocking, false, `canary timing gap ${row.name} must be marked non-blocking`);
  }
  // Advisory warning is surfaced, but not as a publish-blocking failure line.
  assert.match(
    result.stderr,
    /canary perf-timed project row\(s\) missing tsz\/tsgo timing pairs \(advisory/,
  );
  assert.doesNotMatch(
    result.stderr,
    /required perf-timed project row\(s\) missing tsz\/tsgo timing pairs/,
  );
});
console.log("✅ --require-green-project-timing-pairs treats canary perf-timed gaps as advisory, not blocking");

// ---------------------------------------------------------------------------
// Test: --require-green-project-timing-pairs accepts green perf-timed rows once
// the benchmark artifact includes real tsz/tsgo timings for them.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const requiredRows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  const applicationRows = APPLICATION_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact([...requiredRows, ...applicationRows]));
  const result = run(file, [
    "--json",
    "--require-application-compat",
    "--require-green-project-timing-pairs",
  ]);
  assert.equal(result.status, 0, `green timed application rows should pass:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.green_project_timing_pair_gaps, 0);
});
console.log("✅ --require-green-project-timing-pairs accepts green timed app rows");

// ---------------------------------------------------------------------------
// Regression: a green-compat canary application row that is NOT perf_timed
// (its vs-tsgo perf benchmark errors, e.g. infisical) must not block
// --require-green-project-timing-pairs. The compat row is still authoritative;
// only perf-timed rows owe a tsz/tsgo timing pair. This pins the fix for the
// Bench publish gate failing ~70% of runs on "1 green perf-timed project
// row(s) missing tsz/tsgo timing pairs: infisical-project".
// ---------------------------------------------------------------------------
if (NON_PERF_TIMED_APPLICATION_ROWS.length > 0) {
  withTempDir((dir) => {
    const file = path.join(dir, "bench.json");
    const requiredRows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
    const perfTimedAppRows = PERF_TIMED_APPLICATION_ROWS.map((name) => makeRow(name, "green"));
    // Green compatibility, but the perf benchmark errored: no tsz/tsgo timing.
    const nonPerfTimedAppRows = NON_PERF_TIMED_APPLICATION_ROWS.map((name) =>
      makeRow(name, "green", {
        errorStatus: "compile canary tracked in CI; perf benchmark errored",
        tsz_ms: null,
        tsgo_ms: null,
        winner: "error",
      }),
    );
    writeJson(file, makeArtifact([...requiredRows, ...perfTimedAppRows, ...nonPerfTimedAppRows]));
    const result = run(file, [
      "--json",
      "--require-application-compat",
      "--require-green-project-timing-pairs",
    ]);
    assert.equal(
      result.status,
      0,
      `green non-perf-timed canary app rows must not block the timing gate:\n${result.stderr}`,
    );
    const parsed = JSON.parse(result.stdout.trim());
    assert.equal(parsed.green_project_timing_pair_gaps, 0);
    const gapRowNames = parsed.green_project_timing_pair_gap_rows.map((r) => r.name);
    for (const name of NON_PERF_TIMED_APPLICATION_ROWS) {
      assert.ok(
        !gapRowNames.includes(name),
        `non-perf-timed canary app ${name} must not be a timing-pair gap`,
      );
    }
  });
  console.log("✅ --require-green-project-timing-pairs ignores green non-perf-timed canary app rows");
}

// ---------------------------------------------------------------------------
// Test: complete gray application compatibility is still authoritative data.
// Fixture-invalid/reference-failed app rows should not block publishing green
// timed app rows; only missing or partial compatibility metadata should.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const requiredRows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  const applicationRows = APPLICATION_PROJECT_ROWS.map((name) =>
    makeRow(name, "gray", {
      errorStatus: "tsc fixture error",
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
    }),
  );
  for (const row of applicationRows) {
    row.compatibility.exit_class = "fixture invalid";
    row.compatibility.phase = "fixture setup";
    row.compatibility.last_successful_phase = null;
    row.compatibility.diagnostic_status = "tsc fixture failed";
  }
  writeJson(file, makeArtifact([...requiredRows, ...applicationRows]));
  const result = run(file, ["--json", "--require-application-compat"]);
  assert.equal(result.status, 0, `complete gray application compatibility rows should pass:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.application_compatibility.present, APPLICATION_PROJECT_ROWS.length);
  assert.equal(parsed.application_compatibility.complete, APPLICATION_PROJECT_ROWS.length);
  assert.equal(parsed.application_compatibility.missing, 0);
  assert.equal(parsed.application_compatibility.incomplete, 0);
});
console.log("✅ --require-application-compat accepts complete gray app rows");

// ---------------------------------------------------------------------------
// Regression (#15004 follow-up): a SINGLE canary application row missing its
// compatibility entirely — exactly infisical in Bench run 28322724209, where
// the bench-applications shard no longer benched it and that run had no matching
// main-CI compat — must be advisory, not publish-blocking. The other 19/20 app
// rows are present + complete; the lone missing canary must not freeze the site.
// ---------------------------------------------------------------------------
if (CANARY_APPLICATION_ROWS.length > 1) {
  withTempDir((dir) => {
    const file = path.join(dir, "bench.json");
    const missingName = CANARY_APPLICATION_ROWS.includes("infisical-project")
      ? "infisical-project"
      : CANARY_APPLICATION_ROWS[0];
    const requiredRows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
    const applicationRows = APPLICATION_PROJECT_ROWS
      .filter((name) => name !== missingName)
      .map((name) =>
        makeRow(name, "green", {
          errorStatus: "compile canary tracked in CI; not timed by vs-tsgo benchmarks",
          tsz_ms: null,
          tsgo_ms: null,
          winner: "error",
        }),
      );
    writeJson(file, makeArtifact([...requiredRows, ...applicationRows]));
    const result = run(file, ["--json", "--require-application-compat", "--require-green-project-timing-pairs"]);
    assert.equal(
      result.status,
      0,
      `one missing canary application row must not block the publish:\n${result.stderr}`,
    );
    const parsed = JSON.parse(result.stdout.trim());
    assert.equal(parsed.application_compatibility.missing, 1);
    assert.equal(parsed.application_compatibility.present, APPLICATION_PROJECT_ROWS.length - 1);
    assert.equal(parsed.blocking_application_compatibility_gaps, 0);
    assert.equal(parsed.advisory_application_compatibility_gaps, 1);
    assert.deepEqual(parsed.application_compatibility.advisory_gap_rows, [
      { name: missingName, state: "missing", gap_kind: "missing" },
    ]);
    assert.deepEqual(parsed.application_compatibility.blocking_gap_rows, []);
    assert.match(
      result.stderr,
      new RegExp(`${missingName} \\(missing\\)`),
    );
    assert.doesNotMatch(
      result.stderr,
      /application compatibility incomplete for \d+ required row/,
    );
  });
  console.log("✅ --require-application-compat keeps a single missing canary app row advisory");
}

// ---------------------------------------------------------------------------
// Test: a gray application row with partial metadata is incomplete, but because
// it is a canary row the gap is advisory (reported, not publish-blocking).
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const requiredRows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  const applicationRows = APPLICATION_PROJECT_ROWS.map((name) =>
    makeRow(name, "green", {
      errorStatus: "compile canary tracked in CI; not timed by vs-tsgo benchmarks",
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
    }),
  );
  delete applicationRows[0].compatibility.phase;
  writeJson(file, makeArtifact([...requiredRows, ...applicationRows]));
  const result = run(file, ["--json", "--require-application-compat"]);
  const incompleteName = APPLICATION_PROJECT_ROWS[0];
  const expectBlocking = REQUIRED_APPLICATION_ROWS.includes(incompleteName);
  assert.equal(
    result.status,
    expectBlocking ? 1 : 0,
    `partial canary application compatibility must be advisory, not blocking:\n${result.stderr}`,
  );
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.application_compatibility.present, APPLICATION_PROJECT_ROWS.length);
  assert.equal(parsed.application_compatibility.complete, APPLICATION_PROJECT_ROWS.length - 1);
  assert.equal(parsed.application_compatibility.incomplete, 1);
  assert.deepEqual(parsed.application_compatibility.incomplete_rows, [
    { name: incompleteName, state: "gray" },
  ]);
  if (!expectBlocking) {
    assert.equal(parsed.blocking_application_compatibility_gaps, 0);
    assert.equal(parsed.advisory_application_compatibility_gaps, 1);
    assert.deepEqual(parsed.application_compatibility.advisory_gap_rows, [
      { name: incompleteName, state: "gray", gap_kind: "incomplete" },
    ]);
  }
});
console.log("✅ --require-application-compat keeps a partial canary app row advisory");

// ---------------------------------------------------------------------------
// Test: --require-green fails when any present required row is red.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const redName = REQUIRED_PROJECT_ROWS[0];
  const rows = REQUIRED_PROJECT_ROWS.map((name, i) =>
    i === 0 ? makeRow(name, "red", { errorStatus: "tsz crashed" }) : makeRow(name, "green"),
  );
  writeJson(file, makeArtifact(rows));
  const result = run(file, ["--json", "--require-green"]);
  assert.equal(result.status, 1, "--require-green should fail when a required row is red");
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.all_required_rows_green, false, "JSON should mark release gate not ready");
  assert.deepEqual(
    parsed.non_green_required_rows,
    [{ name: redName, state: "red" }],
    "JSON should name the red required row",
  );
  assert.match(result.stderr, new RegExp(`${redName} \\(red\\)`));
});
console.log("✅ --require-green fails on red required rows");

// ---------------------------------------------------------------------------
// Test: yellow row → exit 0
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name, i) =>
    i === 0 ? makeRow(name, "yellow") : makeRow(name, "green"),
  );
  writeJson(file, makeArtifact(rows));
  const result = run(file);
  assert.equal(result.status, 0, `yellow row should exit 0, got:\n${result.stderr}`);
  assert.match(result.stdout, /⚠️.*yellow.*\| 1/i, "should show 1 yellow row");
  assert.match(result.stdout, /some failure/, "should name the first failure class for yellow rows");
});
console.log("✅ yellow row present exits 0");

// ---------------------------------------------------------------------------
// Test: --require-green fails when required rows are yellow or gray.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const yellowName = REQUIRED_PROJECT_ROWS[0];
  const grayName = REQUIRED_PROJECT_ROWS[1];
  const partialGreen = makeRow(grayName, "green");
  delete partialGreen.compatibility.phase;
  const rows = REQUIRED_PROJECT_ROWS.map((name, i) => {
    if (i === 0) return makeRow(name, "yellow");
    if (i === 1) return partialGreen;
    return makeRow(name, "green");
  });
  writeJson(file, makeArtifact(rows));
  const result = run(file, ["--json", "--require-green"]);
  assert.equal(result.status, 1, "--require-green should fail on yellow/gray rows");
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.all_required_rows_green, false, "JSON should mark release gate not ready");
  assert.deepEqual(
    parsed.non_green_required_rows,
    [
      { name: yellowName, state: "yellow" },
      { name: grayName, state: "gray" },
    ],
    "JSON should name yellow and gray required rows",
  );
  assert.match(result.stderr, new RegExp(`${yellowName} \\(yellow\\)`));
  assert.match(result.stderr, new RegExp(`${grayName} \\(gray\\)`));
});
console.log("✅ --require-green fails on yellow or gray required rows");

// ---------------------------------------------------------------------------
// Test: partial compatibility metadata cannot be reported as green.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const partialGreen = makeRow(REQUIRED_PROJECT_ROWS[0], "green");
  delete partialGreen.compatibility.phase;
  const rows = REQUIRED_PROJECT_ROWS.map((name, i) =>
    i === 0 ? partialGreen : makeRow(name, "green"),
  );
  writeJson(file, makeArtifact(rows));
  const result = run(file, ["--json"]);
  assert.equal(result.status, 0, `partial green compatibility should not fail readiness:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.green, REQUIRED_PROJECT_ROWS.length - 1, "partial green row must not count as green");
  assert.equal(parsed.gray, 1, "partial green row should count as gray/incomplete");
  assert.equal(parsed.rows[0].state, "gray", "partial green row should render as gray");
});
console.log("✅ partial green compatibility is gray");

// ---------------------------------------------------------------------------
// Test: partial yellow metadata is incomplete, not an authoritative yellow.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const partialYellow = makeRow(REQUIRED_PROJECT_ROWS[0], "yellow");
  delete partialYellow.compatibility.exit_class;
  const rows = REQUIRED_PROJECT_ROWS.map((name, i) =>
    i === 0 ? partialYellow : makeRow(name, "green"),
  );
  writeJson(file, makeArtifact(rows));
  const result = run(file, ["--json"]);
  assert.equal(result.status, 0, `partial yellow compatibility should not fail readiness:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.yellow, 0, "partial yellow row must not count as yellow");
  assert.equal(parsed.gray, 1, "partial yellow row should count as gray/incomplete");
  assert.equal(parsed.rows[0].state, "gray", "partial yellow row should render as gray");
});
console.log("✅ partial yellow compatibility is gray");

// ---------------------------------------------------------------------------
// Test: status text must not turn missing/incomplete project metadata into an
// authoritative red row.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const incompleteStatusRow = makeRow(REQUIRED_PROJECT_ROWS[0], "red", {
    errorStatus: "tsz crashed before compatibility artifact",
    tsz_ms: null,
    tsgo_ms: null,
    winner: "error",
  });
  delete incompleteStatusRow.compatibility;
  incompleteStatusRow.artifact_missing = true;
  const rows = REQUIRED_PROJECT_ROWS.map((name, i) =>
    i === 0 ? incompleteStatusRow : makeRow(name, "green"),
  );
  writeJson(file, makeArtifact(rows));
  const result = run(file, ["--json"]);
  assert.equal(result.status, 0, `status-only incomplete metadata should not fail readiness:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.red, 0, "status-only incomplete row must not count as red");
  assert.equal(parsed.gray, 1, "status-only incomplete row should count as gray/incomplete");
  assert.equal(parsed.rows[0].state, "gray", "status-only incomplete row should render as gray");
  assert.equal(parsed.rows[0].exit_class, null, "status-only incomplete row should not invent exit metadata");
});
console.log("✅ status-only incomplete project metadata is gray");

// ---------------------------------------------------------------------------
// Test: duplicate required rows are ambiguous, not authoritative green rows.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const duplicatedName = REQUIRED_PROJECT_ROWS[0];
  const duplicateRed = makeRow(duplicatedName, "red");
  const duplicateGreen = makeRow(duplicatedName, "green");
  const rows = [
    duplicateRed,
    duplicateGreen,
    ...REQUIRED_PROJECT_ROWS.slice(1).map((name) => makeRow(name, "green")),
  ];
  writeJson(file, makeArtifact(rows));
  const result = run(file, ["--json"]);
  assert.equal(result.status, 1, `duplicate required row should fail readiness:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.green, REQUIRED_PROJECT_ROWS.length - 1, "duplicate row must not count as green");
  assert.equal(parsed.gray, 1, "duplicate row should count as gray/incomplete");
  assert.deepEqual(
    parsed.duplicate_rows,
    [{ name: duplicatedName, count: 2 }],
    "duplicate required row should be named with its count",
  );
  assert.equal(parsed.rows[0].state, "gray", "duplicate row should render as gray");
  assert.equal(parsed.rows[0].exit_class, "duplicate row", "duplicate row should name the metadata ambiguity");
  assert.match(result.stderr, new RegExp(`${duplicatedName} \\(2\\)`));
});
console.log("✅ duplicate required rows fail readiness");

// ---------------------------------------------------------------------------
// Test: complete red compatibility still reports as red.
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name, i) =>
    i === 0 ? makeRow(name, "red") : makeRow(name, "green"),
  );
  writeJson(file, makeArtifact(rows));
  const result = run(file, ["--json"]);
  assert.equal(result.status, 0, `complete red compatibility should remain present:\n${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.red, 1, "complete red row should count as red");
  assert.equal(parsed.rows[0].state, "red", "complete red row should render as red");
  assert.equal(parsed.rows[0].phase, "check", "complete red row should preserve phase metadata");
});
console.log("✅ complete red compatibility stays red");

// ---------------------------------------------------------------------------
// Test: --json flag writes ONLY JSON to stdout (markdown goes to stderr)
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(rows, { measurement_profile: SAMPLE_MEASUREMENT_PROFILE }));
  const result = run(file, ["--json"]);
  assert.equal(result.status, 0, `--json flag with full artifact should exit 0`);
  // stdout must be exactly one line of valid JSON
  const trimmed = result.stdout.trim();
  let parsed;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    assert.fail(`--json stdout is not valid JSON: ${trimmed}`);
  }
  assert.equal(parsed.missing, 0, "JSON output should show 0 missing");
  assert.equal(parsed.artifact_absent, false, "JSON should have artifact_absent: false");
  assert.equal(parsed.measurement_profile.present, true, "JSON should report measurement profile presence");
  assert.equal(parsed.measurement_profile.mode, "release-pgo", "JSON should report measurement profile mode");
  assert.equal(
    parsed.measurement_profile.training_fingerprint,
    SAMPLE_MEASUREMENT_PROFILE.profile_guided_optimization.training_fingerprint,
    "JSON should report training fingerprint",
  );
  assert.equal(parsed.green, REQUIRED_PROJECT_ROWS.length, "JSON output should show all green");
  assert.ok(Array.isArray(parsed.rows), "JSON output should have rows array");
  assert.equal(parsed.rows.length, REQUIRED_PROJECT_ROWS.length, "rows array should have all required rows");
  assert.equal(parsed.rows[0].phase, "check", "JSON rows should report phase reached");
  assert.equal(parsed.rows[0].last_successful_phase, "check", "JSON rows should report last successful phase");
  assert.equal(parsed.rows[0].files_reached, 1, "JSON rows should report files reached");
  assert.equal(parsed.rows[0].peak_memory_bytes, 1024, "JSON rows should report peak memory");
  assert.equal(
    parsed.rows[0].owner_family,
    "recursive type evaluation pressure",
    "JSON rows should report semantic owner family",
  );
  assert.deepEqual(parsed.rows[0].known_blockers, [], "green JSON rows should preserve known blocker list");
  // markdown goes to stderr, not stdout
  assert.match(result.stderr, /Benchmark artifact readiness/i, "markdown report should be on stderr with --json");
});
console.log("✅ --json outputs only JSON on stdout, markdown on stderr");

// ---------------------------------------------------------------------------
// Test: --json with missing artifact emits JSON with artifact_absent: true
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const result = run(path.join(dir, "nonexistent.json"), ["--json"]);
  assert.equal(result.status, 2, "missing artifact with --json should exit 2");
  let parsed;
  try {
    parsed = JSON.parse(result.stdout.trim());
  } catch {
    assert.fail(`--json stdout with absent artifact is not valid JSON: ${result.stdout}`);
  }
  assert.equal(parsed.artifact_absent, true, "JSON should have artifact_absent: true");
  assert.equal(parsed.missing, REQUIRED_PROJECT_ROWS.length, "all rows should be missing");
  assert.equal(parsed.all_required_rows_green, false, "absent artifact is not release-ready");
  assert.equal(parsed.non_green_required_rows.length, REQUIRED_PROJECT_ROWS.length, "all missing rows are non-green");
});
console.log("✅ --json with absent artifact emits artifact_absent: true");

// ---------------------------------------------------------------------------
// Test: artifact with no results array → all rows missing → exit 1
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  writeJson(file, makeArtifact([]));
  const result = run(file);
  assert.equal(result.status, 1, "artifact with empty results should exit 1");
  assert.match(result.stdout, /missing required rows/i);
});
console.log("✅ empty results array exits 1");

// ---------------------------------------------------------------------------
// Test: multiple rows missing → all names appear in report
// ---------------------------------------------------------------------------
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const missingNames = REQUIRED_PROJECT_ROWS.slice(0, 3);
  const presentRows = REQUIRED_PROJECT_ROWS.slice(3).map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(presentRows));
  const result = run(file);
  assert.equal(result.status, 1, "multiple missing rows should exit 1");
  for (const name of missingNames) {
    assert.match(result.stdout, new RegExp(name), `missing row ${name} should appear in report`);
  }
});
console.log("✅ multiple missing rows all named in report");

// ---------------------------------------------------------------------------
// Test (#17025): required-set coverage. A declared benchmark_set:"required" row
// that the bench runner never measures (BENCH_RUNNER_EXCLUDED_ROWS, e.g.
// type-challenges-solutions-project) is absent from REQUIRED_MEASURED_ROWS, so
// its absence does NOT trip the default missing-required-row gate — but it IS
// reported in required_coverage, and the artifact's merge-stamped run_status is
// echoed through.
const COVERAGE_EXCLUDED_ROW = "type-challenges-solutions-project";
assert.ok(
  ALL_REQUIRED_PROJECT_ROWS.includes(COVERAGE_EXCLUDED_ROW) &&
    !REQUIRED_PROJECT_ROWS.includes(COVERAGE_EXCLUDED_ROW),
  "test fixture expects a declared-required row that the readiness timing gate excludes",
);
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = ALL_REQUIRED_PROJECT_ROWS.filter((n) => n !== COVERAGE_EXCLUDED_ROW).map((n) =>
    makeRow(n, "green"),
  );
  writeJson(file, makeArtifact(rows, { run_status: "partial" }));
  const jsonRes = run(file, ["--json"]);
  assert.equal(jsonRes.status, 0, `excluded-row absence must not block by default: ${jsonRes.stderr}`);
  const json = JSON.parse(jsonRes.stdout);
  assert.equal(json.required_coverage.declared, ALL_REQUIRED_PROJECT_ROWS.length);
  assert.equal(json.required_coverage.missing, 1);
  assert.deepEqual(json.required_coverage.missing_rows, [COVERAGE_EXCLUDED_ROW]);
  assert.equal(json.required_coverage.run_status, "partial");
  assert.match(jsonRes.stderr, /Required-set coverage gap/);
});
console.log("✅ required-set coverage reports an unmeasured declared-required row (non-blocking)");

// Regression (#17561): `nextjs` is declared benchmark_set:"required" but the
// bench runner gates it off by default (NEXTJS_BENCHMARK_ENABLED=0 unless an
// explicit --filter reaches it), so it is permanently absent from the daily
// scheduled artifact. Before RUNTIME_GATED_REQUIRED_ROWS excluded it from
// REQUIRED_MEASURED_ROWS, that tripped the default missing-row gate on every
// scheduled run, so bench.yml's readiness step never reported `ready=true`
// and the site never auto-republished same-day.
const NEXTJS_ROW = "nextjs";
assert.ok(
  ALL_REQUIRED_PROJECT_ROWS.includes(NEXTJS_ROW) && !REQUIRED_PROJECT_ROWS.includes(NEXTJS_ROW),
  "test fixture expects nextjs to be declared required but excluded from the readiness timing gate",
);
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = REQUIRED_PROJECT_ROWS.map((name) => makeRow(name, "green"));
  writeJson(file, makeArtifact(rows));
  const result = run(file);
  assert.equal(
    result.status,
    0,
    `nextjs's permanent absence must not block the default readiness gate: ${result.stderr}`,
  );
});
console.log("✅ nextjs absence does not block the default readiness gate (#17561)");

// The opt-in --require-required-coverage flag turns an absent declared-required
// row into a blocking failure for callers that have given the full set a
// measurement path.
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = ALL_REQUIRED_PROJECT_ROWS.filter((n) => n !== COVERAGE_EXCLUDED_ROW).map((n) =>
    makeRow(n, "green"),
  );
  writeJson(file, makeArtifact(rows));
  const res = run(file, ["--require-required-coverage"]);
  assert.equal(res.status, 1, "opt-in coverage gate must block on an absent declared-required row");
  assert.match(res.stderr, new RegExp(COVERAGE_EXCLUDED_ROW));
});
console.log("✅ --require-required-coverage blocks on an absent declared-required row");

// A complete declared-required set clears the coverage gate under the opt-in flag.
withTempDir((dir) => {
  const file = path.join(dir, "bench.json");
  const rows = ALL_REQUIRED_PROJECT_ROWS.map((n) => makeRow(n, "green"));
  writeJson(file, makeArtifact(rows));
  const jsonRes = run(file, ["--json", "--require-required-coverage"]);
  assert.equal(jsonRes.status, 0, `a complete required set must pass the coverage gate: ${jsonRes.stderr}`);
  const json = JSON.parse(jsonRes.stdout);
  assert.equal(json.required_coverage.missing, 0);
  assert.deepEqual(json.required_coverage.missing_rows, []);
});
console.log("✅ complete required set passes --require-required-coverage");

console.log("\nAll tests passed.");
