#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  collectRunsWithJobs,
  flaggedFindings,
  flakyFindings,
  formatReport,
  parseArgs,
  readMainRuns,
} from "./check-flaky-rate.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(ROOT, "scripts", "ci", "check-flaky-rate.mjs");

// Build a run with one named job at a given conclusion. created drives ordering.
function run(id, createdIso, jobs) {
  return { id, createdMs: Date.parse(createdIso), runAttempt: 1, jobs };
}
function jb(name, conclusion, extra = {}) {
  return { name, conclusion, ...extra };
}

// --- flakyFindings: flip counting + ordering -------------------------------

{
  // unit: pass, fail, pass, fail across 4 runs -> 3 flips over 3 transitions.
  const runs = [
    run(1, "2026-06-17T12:00:00Z", [jb("unit", "success")]),
    run(2, "2026-06-17T12:15:00Z", [jb("unit", "failure")]),
    run(3, "2026-06-17T12:30:00Z", [jb("unit", "success")]),
    run(4, "2026-06-17T12:45:00Z", [jb("unit", "failure")]),
  ];
  const [unit] = flakyFindings(runs, { minSamples: 4 });
  assert.equal(unit.name, "unit");
  assert.equal(unit.samples, 4);
  assert.equal(unit.passes, 2);
  assert.equal(unit.fails, 2);
  assert.equal(unit.flips, 3);
  assert.equal(unit.flipRate, 1);
  assert.equal(unit.enoughSamples, true);
}

// Order is by run creation time, not array order: an out-of-order all-rising
// series has zero flips once sorted.
{
  const runs = [
    run(3, "2026-06-17T12:30:00Z", [jb("lint", "success")]),
    run(1, "2026-06-17T12:00:00Z", [jb("lint", "failure")]),
    run(2, "2026-06-17T12:15:00Z", [jb("lint", "failure")]),
  ];
  const [lint] = flakyFindings(runs);
  // sorted: fail(1), fail(2), success(3) -> one flip at the end.
  assert.equal(lint.flips, 1);
  assert.equal(lint.samples, 3);
}

// Inconclusive conclusions (skipped/cancelled/null) are ignored entirely.
{
  const runs = [
    run(1, "2026-06-17T12:00:00Z", [jb("emit", "success"), jb("emit", "cancelled")]),
    run(2, "2026-06-17T12:15:00Z", [jb("emit", "skipped")]),
    run(3, "2026-06-17T12:30:00Z", [jb("emit", "success")]),
  ];
  const [emit] = flakyFindings(runs);
  assert.equal(emit.samples, 2); // two success, cancelled/skipped dropped
  assert.equal(emit.flips, 0);
}

// timed_out / startup_failure count as a fail verdict (flip vs success).
{
  const runs = [
    run(1, "2026-06-17T12:00:00Z", [jb("dist", "success")]),
    run(2, "2026-06-17T12:15:00Z", [jb("dist", "timed_out")]),
    run(3, "2026-06-17T12:30:00Z", [jb("dist", "startup_failure")]),
  ];
  const [dist] = flakyFindings(runs);
  assert.equal(dist.passes, 1);
  assert.equal(dist.fails, 2);
  assert.equal(dist.flips, 1); // success -> fail, then fail -> fail (no flip)
}

// Sort: flakiest (highest flip rate) first.
{
  const runs = [
    run(1, "2026-06-17T12:00:00Z", [jb("steady", "success"), jb("flaky", "success")]),
    run(2, "2026-06-17T12:15:00Z", [jb("steady", "success"), jb("flaky", "failure")]),
    run(3, "2026-06-17T12:30:00Z", [jb("steady", "success"), jb("flaky", "success")]),
  ];
  const findings = flakyFindings(runs);
  assert.deepEqual(findings.map((f) => f.name), ["flaky", "steady"]);
}

// --- flaggedFindings / threshold + min-samples -----------------------------

{
  const findings = [
    { name: "a", flipRate: 0.5, enoughSamples: true },
    { name: "b", flipRate: 0.9, enoughSamples: false }, // too few samples
    { name: "c", flipRate: 0.1, enoughSamples: true }, // below threshold
  ];
  const flagged = flaggedFindings(findings, { threshold: 0.15 });
  assert.deepEqual(flagged.map((f) => f.name), ["a"]);
}

// --- formatReport ----------------------------------------------------------

{
  const runs = [
    run(1, "2026-06-17T12:00:00Z", [jb("unit", "success")]),
    run(2, "2026-06-17T12:15:00Z", [jb("unit", "failure")]),
    run(3, "2026-06-17T12:30:00Z", [jb("unit", "success")]),
    run(4, "2026-06-17T12:45:00Z", [jb("unit", "failure")]),
  ];
  const report = formatReport(flakyFindings(runs, { minSamples: 4 }), {
    workflow: "CI", branch: "main", runCount: 4, threshold: 0.15,
  });
  assert.match(report, /CI Flaky-Rate Probe/);
  assert.match(report, /4 runs sampled/);
  assert.match(report, /unit ⚠️/); // flagged
  assert.match(report, /100%/);
}

{
  const empty = formatReport([], { workflow: "CI", branch: "main" });
  assert.match(empty, /No conclusive `CI` jobs on `main`/);
}

// --- parseArgs -------------------------------------------------------------

{
  const options = parseArgs(["--workflow", "CI", "--branch", "main", "--max-runs", "20", "--threshold", "0.25", "--min-samples", "6"]);
  assert.equal(options.workflow, "CI");
  assert.equal(options.maxRuns, 20);
  assert.equal(options.threshold, 0.25);
  assert.equal(options.minSamples, 6);
}

assert.throws(() => parseArgs(["--max-runs", "0"]), /positive integer/);
assert.throws(() => parseArgs(["--threshold", "2"]), /\[0, 1\]/);
assert.throws(() => parseArgs(["--min-samples", "0"]), /positive integer/);
assert.throws(() => parseArgs(["--bogus"]), /unknown argument/);

// --- readMainRuns: URL shape against an injected fetcher --------------------

{
  const calls = [];
  const runs = readMainRuns("owner/repo", { branch: "main", maxRuns: 5 }, (args) => {
    calls.push(args[args.length - 1]);
    return { workflow_runs: [{ id: 1 }, { id: 2 }] };
  });
  assert.equal(calls.length, 1);
  assert.match(calls[0], /repos\/owner\/repo\/actions\/runs\?branch=main&per_page=5/);
  assert.deepEqual(runs.map((r) => r.id), [1, 2]);
}

// --- collectRunsWithJobs: filters to CI push/merge_group + joins jobs -------

{
  const fetchJson = (args) => {
    const endpoint = args[args.length - 1];
    if (endpoint.includes("/jobs")) {
      const runId = /runs\/(\d+)\/jobs/.exec(endpoint)?.[1];
      return { jobs: [jb("unit", "success", { run_attempt: 1 })] };
    }
    return {
      workflow_runs: [
        { id: 10, name: "CI", event: "push", created_at: "2026-06-17T12:00:00Z" },
        { id: 11, name: "CI", event: "merge_group", created_at: "2026-06-17T12:15:00Z" },
        { id: 12, name: "CI", event: "pull_request", created_at: "2026-06-17T12:30:00Z" }, // dropped
        { id: 13, name: "bench", event: "push", created_at: "2026-06-17T12:45:00Z" }, // dropped
      ],
    };
  };
  const joined = collectRunsWithJobs("owner/repo", { workflow: "CI", branch: "main", maxRuns: 10 }, fetchJson);
  assert.deepEqual(joined.map((r) => r.id), [10, 11]);
}

// --- CLI: fixture path ------------------------------------------------------

function runFixture(runs, args = []) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-flaky-"));
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
  const runs = [
    run(1, "2026-06-17T12:00:00Z", [jb("unit", "success")]),
    run(2, "2026-06-17T12:15:00Z", [jb("unit", "failure")]),
    run(3, "2026-06-17T12:30:00Z", [jb("unit", "success")]),
    run(4, "2026-06-17T12:45:00Z", [jb("unit", "failure")]),
  ];
  const result = runFixture(runs, ["--min-samples", "4", "--threshold", "0.15"]);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /CI Flaky-Rate Probe/);
  assert.match(result.stdout, /::warning::check-flaky-rate: job "unit"/);
}

{
  const result = runFixture([]);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /No conclusive `CI` jobs/);
}

// --- CLI: live path through a fake gh on PATH -------------------------------

{
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-flaky-gh-"));
  try {
    const fakeGh = path.join(dir, "gh");
    fs.writeFileSync(fakeGh, `#!/bin/sh
case "$*" in
  *"/actions/runs?branch="*)
    printf '%s\\n' '{"workflow_runs":[{"id":1,"name":"CI","event":"push","created_at":"2026-06-17T12:00:00Z"},{"id":2,"name":"CI","event":"push","created_at":"2026-06-17T12:15:00Z"}]}'
    ;;
  *"/runs/1/jobs"*)
    printf '%s\\n' '{"jobs":[{"name":"unit","conclusion":"success","run_attempt":1}]}'
    ;;
  *"/runs/2/jobs"*)
    printf '%s\\n' '{"jobs":[{"name":"unit","conclusion":"failure","run_attempt":1}]}'
    ;;
  *)
    printf 'unexpected gh args %s\\n' "$*" >&2
    exit 1
    ;;
esac
`);
    fs.chmodSync(fakeGh, 0o755);

    const result = spawnSync(process.execPath, [
      SCRIPT, "--repository", "owner/repo", "--max-runs", "2", "--min-samples", "2", "--threshold", "0.15",
    ], {
      cwd: ROOT,
      encoding: "utf8",
      env: { ...process.env, PATH: `${dir}${path.delimiter}${process.env.PATH || ""}` },
    });

    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /unit/);
    assert.match(result.stdout, /::warning::check-flaky-rate: job "unit"/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

console.log("check-flaky-rate: all assertions passed");
