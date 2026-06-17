#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  baselineMedianP50,
  buildJsonDocument,
  collectRunsWithJobs,
  compareToBaseline,
  formatComparison,
  formatReport,
  jobTimingFindings,
  loadBaselineDocs,
  parseArgs,
  readWorkflowRuns,
} from "./check-ci-job-timing.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(ROOT, "scripts", "ci", "check-ci-job-timing.mjs");

function job(overrides = {}) {
  return {
    name: "conformance-0",
    conclusion: "success",
    // created -> started is 30s queue wait; started -> completed is 300s run.
    created_at: "2026-05-20T12:00:00Z",
    started_at: "2026-05-20T12:00:30Z",
    completed_at: "2026-05-20T12:05:30Z",
    ...overrides,
  };
}

// --- jobTimingFindings: core arithmetic and aggregation -------------------

{
  const findings = jobTimingFindings([{ id: 1, jobs: [job()] }]);
  assert.deepEqual(findings, [{
    name: "conformance-0",
    samples: 1,
    runP50: 300,
    runP95: 300,
    runMax: 300,
    queueP50: 30,
    queueMax: 30,
  }]);
}

// Multiple samples of the same job name aggregate into one row; percentiles use
// nearest-rank so the values are exact members of the sample.
{
  const runs = [60, 120, 180, 240, 300].map((runSecs, i) => ({
    id: i,
    jobs: [job({
      created_at: "2026-05-20T12:00:00Z",
      started_at: "2026-05-20T12:00:00Z",
      completed_at: new Date(Date.parse("2026-05-20T12:00:00Z") + runSecs * 1000).toISOString(),
    })],
  }));
  const [finding] = jobTimingFindings(runs);
  assert.equal(finding.samples, 5);
  assert.equal(finding.runP50, 180);
  assert.equal(finding.runP95, 300);
  assert.equal(finding.runMax, 300);
  assert.equal(finding.queueP50, 0);
}

// Slowest-by-p50 first; tie-break on name. Failed/cancelled jobs are excluded.
{
  const findings = jobTimingFindings([{
    id: 1,
    jobs: [
      job({ name: "fast", completed_at: "2026-05-20T12:00:40Z" }), // 10s run
      job({ name: "slow", completed_at: "2026-05-20T12:10:30Z" }), // 600s run
      job({ name: "failed-row", conclusion: "failure" }),
      job({ name: "cancelled-row", conclusion: "cancelled" }),
    ],
  }]);
  assert.deepEqual(findings.map((f) => f.name), ["slow", "fast"]);
}

// Jobs still missing a completion time are dropped (no run seconds); a missing
// created_at degrades only the queue column, not the run sample.
{
  const findings = jobTimingFindings([{
    id: 1,
    jobs: [
      job({ name: "incomplete", completed_at: null }),
      job({ name: "no-queue", created_at: null }),
    ],
  }]);
  assert.deepEqual(findings.map((f) => f.name), ["no-queue"]);
  const [noQueue] = findings;
  assert.equal(noQueue.runP50, 300);
  assert.equal(noQueue.queueP50, null);
  assert.equal(noQueue.queueMax, null);
}

// --- formatReport ----------------------------------------------------------

{
  const report = formatReport(jobTimingFindings([{ id: 1, jobs: [job()] }]), {
    workflow: "ci.yml",
    branch: "main",
    runCount: 1,
  });
  assert.match(report, /CI Job Timing Baseline/);
  assert.match(report, /1 run sampled/);
  assert.match(report, /conformance-0/);
  assert.match(report, /5m00s/); // 300s run rendered as minutes/seconds
  assert.match(report, /30s/); // 30s queue wait
}

{
  const empty = formatReport([], { workflow: "ci.yml", branch: "main" });
  assert.match(empty, /No successful `ci.yml` jobs on `main`/);
}

// --- parseArgs -------------------------------------------------------------

{
  const options = parseArgs(["--workflow", "bench.yml", "--branch", "release", "--max-runs", "7"]);
  assert.equal(options.workflow, "bench.yml");
  assert.equal(options.branch, "release");
  assert.equal(options.maxRuns, 7);
}

assert.throws(() => parseArgs(["--max-runs", "0"]), /positive integer/);
assert.throws(() => parseArgs(["--bogus"]), /unknown argument/);

{
  const options = parseArgs(["--json", "out.json", "--baseline-dir", "base", "--regress-threshold", "25", "--regress-min-seconds", "90"]);
  assert.equal(options.jsonPath, "out.json");
  assert.equal(options.baselineDir, "base");
  assert.equal(options.regressThreshold, 25);
  assert.equal(options.regressMinSeconds, 90);
}

assert.throws(() => parseArgs(["--json"]), /--json requires a path/);
assert.throws(() => parseArgs(["--baseline-dir"]), /--baseline-dir requires a path/);
assert.throws(() => parseArgs(["--regress-threshold", "-1"]), /non-negative/);

// --- buildJsonDocument / baseline / compareToBaseline ----------------------

{
  const findings = jobTimingFindings([{ id: 1, jobs: [job()] }]);
  const doc = buildJsonDocument(findings, { workflow: "ci.yml", branch: "main", runCount: 1, generatedAt: "2026-06-17T00:00:00Z" });
  assert.equal(doc.schemaVersion, 1);
  assert.equal(doc.workflow, "ci.yml");
  assert.equal(doc.runCount, 1);
  assert.equal(doc.findings.length, 1);
  assert.equal(doc.findings[0].name, "conformance-0");
}

// baselineMedianP50: per-job median over prior docs; non-numeric p50 skipped.
{
  const docs = [
    { findings: [{ name: "unit", runP50: 100 }, { name: "lint", runP50: 50 }] },
    { findings: [{ name: "unit", runP50: 120 }, { name: "lint", runP50: null }] },
    { findings: [{ name: "unit", runP50: 140 }] },
  ];
  const baseline = baselineMedianP50(docs);
  assert.equal(baseline.get("unit").median, 120); // median of [100,120,140]
  assert.equal(baseline.get("unit").samples, 3);
  assert.equal(baseline.get("lint").median, 50); // only one numeric sample
  assert.equal(baseline.get("lint").samples, 1);
}

// compareToBaseline: flags only jobs over threshold whose baseline >= minSeconds.
{
  const findings = [
    { name: "unit", runP50: 200 }, // baseline 120 -> +66% -> regression
    { name: "lint", runP50: 70 }, // baseline 50 (< minSeconds 60) -> skipped
    { name: "stable", runP50: 130 }, // baseline 120 -> +8% -> ok
    { name: "novel", runP50: 999 }, // no baseline -> skipped
  ];
  const baseline = baselineMedianP50([
    { findings: [{ name: "unit", runP50: 120 }, { name: "lint", runP50: 50 }, { name: "stable", runP50: 120 }] },
  ]);
  const comparison = compareToBaseline(findings, baseline, { thresholdPct: 30, minSeconds: 60 });
  assert.deepEqual(comparison.regressions.map((r) => r.name), ["unit"]);
  assert.equal(comparison.compared, 2); // unit + stable (lint skipped by minSeconds, novel by no baseline)
  assert.equal(Math.round(comparison.regressions[0].deltaPct), 67);
}

// formatComparison rendering across the three states.
{
  assert.match(formatComparison({ regressions: [], compared: 0, thresholdPct: 30, minSeconds: 60 }, 0), /first baseline/);
  assert.match(formatComparison({ regressions: [], compared: 3, thresholdPct: 30, minSeconds: 60 }, 4), /No job's run p50 exceeded/);
  const warn = formatComparison({
    regressions: [{ name: "unit", current: 200, baseline: 120, deltaPct: 66.7 }],
    compared: 2,
    thresholdPct: 30,
    minSeconds: 60,
  }, 4);
  assert.match(warn, /1 job\(s\) regressed/);
  assert.match(warn, /unit/);
  assert.match(warn, /\+67%/);
}

// loadBaselineDocs: reads *.json, skips unparseable/wrong-shape, missing dir ok.
{
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-ci-timing-baseline-"));
  try {
    fs.writeFileSync(path.join(dir, "a.json"), JSON.stringify({ findings: [{ name: "unit", runP50: 100 }] }));
    fs.writeFileSync(path.join(dir, "b.json"), "{not valid json");
    fs.writeFileSync(path.join(dir, "c.json"), JSON.stringify({ noFindings: true }));
    fs.writeFileSync(path.join(dir, "d.txt"), "ignored");
    const docs = loadBaselineDocs(dir);
    assert.equal(docs.length, 1);
    assert.equal(docs[0].findings[0].name, "unit");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
  assert.deepEqual(loadBaselineDocs(path.join(os.tmpdir(), "tsz-does-not-exist-xyz")), []);
}

// --- CLI: --json writes the document; --baseline-dir warns on regression ----

{
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-ci-timing-json-"));
  try {
    const baselineDir = path.join(dir, "baseline");
    fs.mkdirSync(baselineDir);
    // Prior baseline: conformance-0 ran ~100s typically.
    fs.writeFileSync(
      path.join(baselineDir, "prior.json"),
      JSON.stringify({ findings: [{ name: "conformance-0", runP50: 100 }] }),
    );
    const outJson = path.join(dir, "out.json");
    // Current fixture: conformance-0 took 300s (job() default) -> +200% regression.
    const fixture = path.join(dir, "runs.json");
    fs.writeFileSync(fixture, JSON.stringify({ runs: [{ id: 1, jobs: [job()] }] }));
    const result = spawnSync(process.execPath, [
      SCRIPT, "--fixture", fixture, "--json", outJson, "--baseline-dir", baselineDir,
    ], { cwd: ROOT, encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /::warning::ci-job-timing: job "conformance-0"/);
    assert.match(result.stdout, /Timing Regression Check/);
    const doc = JSON.parse(fs.readFileSync(outJson, "utf8"));
    assert.equal(doc.schemaVersion, 1);
    assert.equal(doc.findings[0].name, "conformance-0");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// --- readWorkflowRuns: URL shape against an injected fetcher ----------------

{
  const calls = [];
  const runs = readWorkflowRuns("owner/repo", { workflow: "ci.yml", branch: "main", maxRuns: 5 }, (args) => {
    calls.push(args[args.length - 1]);
    return { workflow_runs: [{ id: 100 }, { id: 101 }] };
  });
  assert.equal(calls.length, 1);
  assert.match(calls[0], /repos\/owner\/repo\/actions\/workflows\/ci\.yml\/runs\?status=success&branch=main&per_page=5/);
  assert.deepEqual(runs.map((r) => r.id), [100, 101]);
}

// --- collectRunsWithJobs: join runs with their paginated jobs ---------------

{
  const fetchJson = (args) => {
    const endpoint = args[args.length - 1];
    if (endpoint.includes("/workflows/")) {
      return { workflow_runs: [{ id: 100 }, { id: 101 }] };
    }
    const runId = /runs\/(\d+)\/jobs/.exec(endpoint)?.[1];
    return { jobs: [job({ name: `job-for-${runId}` })] };
  };
  const joined = collectRunsWithJobs("owner/repo", { workflow: "ci.yml", branch: "main", maxRuns: 5 }, fetchJson);
  assert.deepEqual(joined.map((r) => r.id), [100, 101]);
  assert.deepEqual(joined.flatMap((r) => r.jobs.map((j) => j.name)), ["job-for-100", "job-for-101"]);
}

// --- CLI: fixture path ------------------------------------------------------

function runFixture(runs, args = []) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-ci-job-timing-"));
  try {
    const fixture = path.join(dir, "runs.json");
    fs.writeFileSync(fixture, `${JSON.stringify({ runs })}\n`);
    return spawnSync(process.execPath, [SCRIPT, "--fixture", fixture, ...args], {
      cwd: ROOT,
      encoding: "utf8",
    });
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

{
  const result = runFixture([{ id: 1, jobs: [job()] }]);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /CI Job Timing Baseline/);
  assert.match(result.stdout, /conformance-0/);
}

{
  const result = runFixture([]);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /No successful `ci.yml` jobs/);
}

// --- CLI: live path through a fake gh on PATH -------------------------------

{
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-ci-job-timing-gh-"));
  try {
    const fakeGh = path.join(dir, "gh");
    fs.writeFileSync(fakeGh, `#!/bin/sh
case "$*" in
  *"/workflows/"*)
    printf '%s\\n' '{"workflow_runs":[{"id":555}]}'
    ;;
  *"/jobs"*)
    case "$*" in
      *"page=1"*)
        printf '%s\\n' '{"jobs":[{"name":"unit","conclusion":"success","created_at":"2026-05-20T12:00:00Z","started_at":"2026-05-20T12:00:10Z","completed_at":"2026-05-20T12:02:10Z"}]}'
        ;;
      *)
        printf '%s\\n' '{"jobs":[]}'
        ;;
    esac
    ;;
  *)
    printf 'unexpected gh args %s\\n' "$*" >&2
    exit 1
    ;;
esac
`);
    fs.chmodSync(fakeGh, 0o755);

    const result = spawnSync(process.execPath, [
      SCRIPT,
      "--repository",
      "owner/repo",
      "--max-runs",
      "1",
    ], {
      cwd: ROOT,
      encoding: "utf8",
      env: { ...process.env, PATH: `${dir}${path.delimiter}${process.env.PATH || ""}` },
    });

    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /unit/);
    assert.match(result.stdout, /2m00s/); // 120s run
    assert.match(result.stdout, /10s/); // 10s queue wait
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

console.log("check-ci-job-timing: all assertions passed");
