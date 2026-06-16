#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  collectRunsWithJobs,
  formatReport,
  jobTimingFindings,
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
