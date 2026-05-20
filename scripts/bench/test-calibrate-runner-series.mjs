#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";

import {
  GREEN_COMPAT,
  RED_COMPAT,
  YELLOW_COMPAT,
} from "./row-utils.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(ROOT, "scripts", "bench", "calibrate-runner-series.mjs");

const {
  createRunnerCalibrationReport,
  markdownReport,
} = await import(pathToFileURL(SCRIPT));

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-runner-calibration-"));
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

function runnerEnvironment({ cloudBuild = false } = {}) {
  return {
    platform: "linux",
    arch: "x64",
    release: "6.1.0",
    cpu_count: cloudBuild ? 32 : 4,
    cpu_model: cloudBuild ? "Intel Cloud Build" : "AMD Cloud Run",
    total_memory_bytes: cloudBuild ? 34359738368 : 8589934592,
    github_actions: cloudBuild ? null : { runner_os: "Linux", runner_arch: "X64" },
    cloud_build: cloudBuild ? { machine_type: "e2-highcpu-32" } : null,
  };
}

function artifact(rows, { sourceCommit = "abc123", cloudBuild = false } = {}) {
  return {
    generated_at: "2026-05-20T00:00:00.000Z",
    source_commit: sourceCommit,
    workflow_run_url: `https://github.com/mohsen1/tsz/actions/runs/${sourceCommit}`,
    runner_environment: runnerEnvironment({ cloudBuild }),
    results: rows,
  };
}

function row(name, { tszMs, tsgoMs, compatibility = null, status = null } = {}) {
  return {
    name,
    tsz_ms: tszMs,
    tsgo_ms: tsgoMs,
    winner: tszMs != null && tsgoMs != null && tszMs < tsgoMs ? "tsz" : "tsgo",
    factor: tszMs != null && tsgoMs != null ? Math.max(tszMs, tsgoMs) / Math.min(tszMs, tsgoMs) : null,
    status,
    ...(compatibility ? { compatibility } : {}),
  };
}

const baseline = artifact([
  row("manyConstExports.ts", { tszMs: 100, tsgoMs: 80 }),
  row("200 classes", { tszMs: 300, tsgoMs: 180 }),
  row("Recursive generic depth 10", { tszMs: 200, tsgoMs: 100 }),
  row("utility-types-project", { tszMs: 500, tsgoMs: 250, compatibility: GREEN_COMPAT }),
  row("large-ts-repo", { tszMs: 900, tsgoMs: 600 }),
  row("yellow-project", { tszMs: 500, tsgoMs: 250, compatibility: YELLOW_COMPAT }),
  row("red-project", { tszMs: 500, tsgoMs: 250, compatibility: RED_COMPAT }),
  row("missing-timing", { tszMs: null, tsgoMs: null }),
]);

const candidate = artifact([
  row("manyConstExports.ts", { tszMs: 120, tsgoMs: 88 }),
  row("200 classes", { tszMs: 330, tsgoMs: 198 }),
  row("Recursive generic depth 10", { tszMs: 180, tsgoMs: 120 }),
  row("utility-types-project", { tszMs: 650, tsgoMs: 300, compatibility: GREEN_COMPAT }),
  row("large-ts-repo", { tszMs: 810, tsgoMs: 660 }),
  row("yellow-project", { tszMs: 350, tsgoMs: 200, compatibility: GREEN_COMPAT }),
  row("red-project", { tszMs: 350, tsgoMs: 200, compatibility: GREEN_COMPAT }),
  row("missing-timing", { tszMs: null, tsgoMs: null }),
], { cloudBuild: true });

const report = createRunnerCalibrationReport(baseline, candidate, "baseline.json", "candidate.json");
assert.equal(report.calibration_ready, true);
assert.deepEqual(report.warnings, []);
assert.equal(report.totals.shared_rows, 8);
assert.equal(report.totals.green_rows_compared, 5);
assert.equal(report.totals.excluded_non_green_rows, 2);
assert.equal(report.totals.excluded_missing_timing_rows, 1);
assert.equal(report.candidate.runner_signature.cloud_build_machine_type, "e2-highcpu-32");
assert.deepEqual(
  report.rows.map((entry) => [entry.name, entry.family]),
  [
    ["manyConstExports.ts", "compiler-file"],
    ["large-ts-repo", "large-ts-repo"],
    ["utility-types-project", "project"],
    ["Recursive generic depth 10", "solver-stress"],
    ["200 classes", "synthetic"],
  ],
);
assert.deepEqual(
  report.required_family_coverage.map((entry) => [entry.family, entry.compared]),
  [
    ["compiler-file", true],
    ["synthetic", true],
    ["solver-stress", true],
    ["project", true],
    ["large-ts-repo", true],
  ],
);
assert.equal(report.rows.find((entry) => entry.name === "utility-types-project").ratios.tsz_ms, 1.3);

const md = markdownReport(report);
assert.match(md, /Benchmark Runner Calibration Report/);
assert.match(md, /Runner Signatures/);
assert.match(md, /Required Family Coverage/);
assert.match(md, /AMD Cloud Run/);
assert.match(md, /e2-highcpu-32/);
assert.match(md, /Family Summary/);
assert.match(md, /utility-types-project/);
assert.doesNotMatch(md, /yellow-project/);

const mismatch = createRunnerCalibrationReport(
  baseline,
  artifact([row("manyConstExports.ts", { tszMs: 120, tsgoMs: 90 })], {
    sourceCommit: "def456",
    cloudBuild: true,
  }),
  "baseline.json",
  "candidate.json",
);
assert.equal(mismatch.calibration_ready, false);
assert.match(mismatch.warnings.join("\n"), /source_commit mismatch/);
assert.match(mismatch.warnings.join("\n"), /missing required calibration families/);

const missingFamilyCoverage = createRunnerCalibrationReport(
  artifact([row("manyConstExports.ts", { tszMs: 100, tsgoMs: 80 })]),
  artifact([row("manyConstExports.ts", { tszMs: 120, tsgoMs: 90 })], {
    cloudBuild: true,
  }),
  "baseline.json",
  "candidate.json",
);
assert.equal(missingFamilyCoverage.calibration_ready, false);
assert.match(
  missingFamilyCoverage.warnings.join("\n"),
  /missing required calibration families: synthetic, solver-stress, project, large-ts-repo/,
);

withTempDir((dir) => {
  const before = path.join(dir, "before.json");
  const after = path.join(dir, "after.json");
  const out = path.join(dir, "report.json");
  writeJson(before, baseline);
  writeJson(after, candidate);

  const result = spawnSync(process.execPath, [SCRIPT, before, after, out], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.match(result.stdout, /Benchmark Runner Calibration Report/);

  const written = JSON.parse(fs.readFileSync(out, "utf8"));
  assert.equal(written.calibration_ready, true);
  assert.equal(written.totals.green_rows_compared, 5);
});

withTempDir((dir) => {
  const before = path.join(dir, "before.json");
  const after = path.join(dir, "after.json");
  writeJson(before, baseline);
  writeJson(after, artifact([row("manyConstExports.ts", { tszMs: 120, tsgoMs: 90 })], {
    sourceCommit: "def456",
    cloudBuild: true,
  }));

  const result = spawnSync(process.execPath, [SCRIPT, "--json", before, after], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 1, "source mismatch should fail calibration readiness");
  const parsed = JSON.parse(result.stdout);
  assert.equal(parsed.calibration_ready, false);
});

console.log("runner calibration report tests passed");
