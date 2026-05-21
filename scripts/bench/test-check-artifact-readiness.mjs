#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  REQUIRED_PROJECT_ROWS,
} from "./project-rows.mjs";

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

function makeCompatibility(state) {
  return {
    generated_at: "2026-05-19T01:02:03.000Z",
    source_commit: "abcdef1234567890",
    workflow_name: "Bench",
    workflow_run_id: "12345",
    workflow_run_url: "https://github.com/mohsen1/tsz/actions/runs/12345",
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
    tsz_ms: opts.tsz_ms ?? 50,
    tsgo_ms: opts.tsgo_ms ?? 40,
    winner: opts.winner ?? "tsgo",
    ratio: 1.25,
    ...(opts.errorStatus ? { status: opts.errorStatus } : {}),
    compatibility: makeCompatibility(state),
  };
}

function makeArtifact(rows, extraMeta = {}) {
  return {
    generated_at: "2026-05-19T01:02:03.000Z",
    source_commit: "abcdef1234567890abcd",
    workflow_name: "Bench",
    workflow_run_id: "99999",
    workflow_run_url: "https://github.com/mohsen1/tsz/actions/runs/99999",
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
  const result = run(file);
  assert.equal(result.status, 0, `all-green artifact should exit 0, got:\n${result.stderr}`);
  assert.match(result.stdout, new RegExp(`green.*\\| ${REQUIRED_PROJECT_ROWS.length}`), "should show all green count");
  assert.match(result.stdout, /Measurement profile.*release-pgo/, "should show measurement profile mode");
  assert.match(result.stdout, /PGO profile.*abcdef123456/, "should show PGO profile fingerprint");
  assert.match(result.stdout, /PGO training.*123456abcdef/, "should show PGO training fingerprint");
});
console.log("✅ complete all-green artifact exits 0");

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
});
console.log("✅ missing measurement profile is reported without failing readiness");

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

console.log("\nAll tests passed.");
