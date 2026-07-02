#!/usr/bin/env node
import assert from "node:assert/strict";
import {
  mainCiHealth,
  classifyJobsFailure,
  extractRegressedTests,
  formatIssueBody,
  formatReport,
  reconcileIssue,
} from "./check-main-red.mjs";

const NOW = "2026-06-13T00:00:00Z";

function run(overrides = {}) {
  return {
    id: 100,
    status: "completed",
    name: "CI",
    event: "push",
    conclusion: "success",
    head_sha: "aaaaaaaaaaaa1111",
    display_title: "perf(checker): something",
    html_url: "https://gh.example/runs/100",
    created_at: "2026-06-12T20:00:00Z",
    ...overrides,
  };
}

let passed = 0;
function test(name, fn) {
  fn();
  passed += 1;
  console.log(`ok - ${name}`);
}

test("green when newest conclusive main run succeeded", () => {
  const v = mainCiHealth([
    run({ id: 1, conclusion: "success", created_at: "2026-06-12T21:00:00Z" }),
    run({ id: 2, conclusion: "failure", created_at: "2026-06-12T19:00:00Z" }),
  ]);
  assert.equal(v.red, false);
  assert.equal(v.status, "green");
  assert.equal(v.run.id, 1);
});

test("red when newest conclusive main run failed", () => {
  const v = mainCiHealth([
    run({ id: 2, conclusion: "failure", created_at: "2026-06-12T21:00:00Z", head_sha: "deadbeefcafe0000" }),
    run({ id: 1, conclusion: "success", created_at: "2026-06-12T19:00:00Z", head_sha: "feedface0001" }),
  ]);
  assert.equal(v.red, true);
  assert.equal(v.run.id, 2);
  assert.equal(v.lastGreen.sha, "feedface0001");
});

test("ignores skipped/cancelled push runs (gate-skip), uses merge_group verdict", () => {
  const v = mainCiHealth([
    run({ id: 3, event: "push", conclusion: "skipped", created_at: "2026-06-12T21:30:00Z" }),
    run({ id: 2, event: "merge_group", conclusion: "success", created_at: "2026-06-12T21:00:00Z" }),
  ]);
  assert.equal(v.red, false);
  assert.equal(v.run.id, 2);
});

test("ignores non-CI workflows and non-main events", () => {
  const v = mainCiHealth([
    run({ id: 9, name: "Deploy Website", conclusion: "failure", created_at: "2026-06-12T22:00:00Z" }),
    run({ id: 8, name: "CI", event: "pull_request", conclusion: "failure", created_at: "2026-06-12T21:59:00Z" }),
    run({ id: 7, name: "CI", event: "push", conclusion: "success", created_at: "2026-06-12T21:00:00Z" }),
  ]);
  assert.equal(v.red, false, "Deploy/PR failures must not mark main red");
  assert.equal(v.run.id, 7);
});

test("unknown when no conclusive main run exists", () => {
  const v = mainCiHealth([
    run({ id: 1, status: "in_progress", conclusion: null }),
    run({ id: 2, conclusion: "cancelled" }),
  ]);
  assert.equal(v.status, "unknown");
  assert.equal(v.red, false);
});

test("timed_out and startup_failure count as red", () => {
  for (const c of ["timed_out", "startup_failure"]) {
    const v = mainCiHealth([run({ id: 1, conclusion: c, created_at: "2026-06-12T21:00:00Z" })]);
    assert.equal(v.red, true, `${c} should be red`);
  }
});

// --- infra-interruption classification (issue #14688) ---

// The witnessed false-red signature: the `unit` job concluded `failure` but its
// work step ended with `conclusion: null` (runner interruption) while every
// other step succeeded.
const INTERRUPTED_UNIT_JOBS = {
  jobs: [
    {
      name: "unit",
      conclusion: "failure",
      steps: [
        { name: "Set up job", conclusion: "success" },
        { name: "Checkout", conclusion: "success" },
        { name: "Run unit suite", conclusion: null },
        { name: "Complete job", conclusion: null },
      ],
    },
    // Cancelled siblings (GitHub cancels the rest once one job fails).
    { name: "conformance-0", conclusion: "cancelled", steps: [] },
  ],
};

test("classifyJobsFailure: interrupted/null work step with no failing step => infra", () => {
  assert.equal(classifyJobsFailure(INTERRUPTED_UNIT_JOBS), "infra");
});

test("classifyJobsFailure: a step that concluded failure => real (never masked)", () => {
  const jobs = {
    jobs: [{
      name: "fourslash",
      conclusion: "failure",
      steps: [
        { name: "Checkout", conclusion: "success" },
        { name: "Run fourslash", conclusion: "failure" },
      ],
    }],
  };
  assert.equal(classifyJobsFailure(jobs), "real");
});

test("classifyJobsFailure: job-level timed_out/startup_failure => real", () => {
  for (const c of ["timed_out", "startup_failure"]) {
    const jobs = { jobs: [{ name: "unit", conclusion: c, steps: [{ conclusion: null }] }] };
    assert.equal(classifyJobsFailure(jobs), "real", `${c} job must stay real`);
  }
});

test("classifyJobsFailure: cancelled work step counts as interrupted => infra", () => {
  const jobs = {
    jobs: [{
      name: "unit-checker-integration",
      conclusion: "failure",
      steps: [
        { name: "Checkout", conclusion: "success" },
        { name: "Run checker integration suite", conclusion: "cancelled" },
      ],
    }],
  };
  assert.equal(classifyJobsFailure(jobs), "infra");
});

test("classifyJobsFailure: failing job with no steps is unattributable => real (conservative)", () => {
  assert.equal(classifyJobsFailure({ jobs: [{ name: "x", conclusion: "failure", steps: [] }] }), "real");
});

test("classifyJobsFailure: no failing job => unknown (caller must not suppress)", () => {
  assert.equal(classifyJobsFailure({ jobs: [{ name: "ok", conclusion: "success", steps: [] }] }), "unknown");
  assert.equal(classifyJobsFailure({ jobs: [] }), "unknown");
  assert.equal(classifyJobsFailure(null), "unknown");
});

test("classifyJobsFailure: a real failure alongside an infra one => real", () => {
  const jobs = {
    jobs: [
      INTERRUPTED_UNIT_JOBS.jobs[0],
      { name: "conformance-1", conclusion: "failure", steps: [{ name: "shard", conclusion: "failure" }] },
    ],
  };
  assert.equal(classifyJobsFailure(jobs), "real");
});

test("mainCiHealth: infra-classified newest red is skipped, falls through to green", () => {
  const classifyRun = (r) => (r.id === 2 ? "infra" : "unknown");
  const v = mainCiHealth([
    run({ id: 2, conclusion: "failure", created_at: "2026-06-12T21:00:00Z" }),
    run({ id: 1, conclusion: "success", created_at: "2026-06-12T20:00:00Z" }),
  ], { classifyRun });
  assert.equal(v.red, false);
  assert.equal(v.status, "green");
  assert.equal(v.run.id, 1);
  assert.equal(v.infraSkipped, 1);
});

test("mainCiHealth: a real newest red is NOT suppressed", () => {
  const classifyRun = () => "real";
  const v = mainCiHealth([
    run({ id: 2, conclusion: "failure", created_at: "2026-06-12T21:00:00Z" }),
    run({ id: 1, conclusion: "success", created_at: "2026-06-12T20:00:00Z" }),
  ], { classifyRun });
  assert.equal(v.red, true);
  assert.equal(v.run.id, 2);
  assert.equal(v.infraSkipped, 0);
});

test("mainCiHealth: infra red over a real red still reports the real red", () => {
  const classifyRun = (r) => (r.id === 3 ? "infra" : "real");
  const v = mainCiHealth([
    run({ id: 3, conclusion: "failure", created_at: "2026-06-12T22:00:00Z" }),
    run({ id: 2, conclusion: "failure", created_at: "2026-06-12T21:00:00Z", head_sha: "realred00" }),
    run({ id: 1, conclusion: "success", created_at: "2026-06-12T20:00:00Z", head_sha: "lastgreen0" }),
  ], { classifyRun });
  assert.equal(v.red, true);
  assert.equal(v.run.id, 2);
  assert.equal(v.infraSkipped, 1);
  assert.equal(v.lastGreen.sha, "lastgreen0");
});

test("mainCiHealth: every conclusive run infra-red => unknown, not a breach", () => {
  const classifyRun = () => "infra";
  const v = mainCiHealth([
    run({ id: 2, conclusion: "failure", created_at: "2026-06-12T21:00:00Z" }),
    run({ id: 1, conclusion: "failure", created_at: "2026-06-12T20:00:00Z" }),
  ], { classifyRun });
  assert.equal(v.red, false);
  assert.equal(v.status, "unknown");
  assert.equal(v.infraSkipped, 2);
});

test("formatReport surfaces ignored infra-interrupted runs", () => {
  const classifyRun = () => "infra";
  const v = mainCiHealth([
    run({ id: 2, conclusion: "failure", created_at: "2026-06-12T21:00:00Z" }),
  ], { classifyRun });
  assert.match(formatReport(v), /Ignored 1 infra-interrupted red run/);
  assert.match(formatReport(v), /#14688/);
});

test("extractRegressedTests dedupes and parses aggregate log", () => {
  const log = [
    "error: unlisted conformance regressions:",
    "  REGRESSED: TypeScript/tests/cases/conformance/es2019/globalThisAmbientModules.ts",
    "  REGRESSED: TypeScript/tests/cases/compiler/temporal.ts",
    "  REGRESSED: TypeScript/tests/cases/compiler/temporal.ts",
  ].join("\n");
  assert.deepEqual(extractRegressedTests(log), [
    "TypeScript/tests/cases/conformance/es2019/globalThisAmbientModules.ts",
    "TypeScript/tests/cases/compiler/temporal.ts",
  ]);
  assert.deepEqual(extractRegressedTests(""), []);
});

test("issue body embeds marker, window, and regressed tests", () => {
  const v = mainCiHealth([
    run({ id: 42, conclusion: "failure", head_sha: "deadbeefcafe9999", created_at: "2026-06-12T21:00:00Z" }),
    run({ id: 41, conclusion: "success", head_sha: "00ddff112233", created_at: "2026-06-12T20:00:00Z" }),
  ]);
  const body = formatIssueBody(v, ["TypeScript/tests/cases/compiler/temporal.ts"], NOW);
  assert.match(body, /<!-- main-red-sentinel -->/);
  assert.match(body, /deadbeefcafe/);
  assert.match(body, /00ddff112233/);
  assert.match(body, /temporal\.ts/);
});

test("formatReport summarizes green/red/unknown", () => {
  assert.match(formatReport(mainCiHealth([run({ conclusion: "success", created_at: "2026-06-12T21:00:00Z" })])), /green/);
  assert.match(formatReport(mainCiHealth([run({ conclusion: "failure", created_at: "2026-06-12T21:00:00Z" })])), /RED/);
  assert.match(formatReport(mainCiHealth([])), /No conclusive/);
});

test("reconcileIssue creates when red and no existing issue", () => {
  const calls = [];
  const v = mainCiHealth([run({ id: 5, conclusion: "failure", created_at: "2026-06-12T21:00:00Z" })]);
  const result = reconcileIssue(v, [], NOW, {
    repository: "o/r",
    fetchJson: () => [],
    runCommand: (args) => { calls.push(args); return { status: 0, stdout: "https://gh/issues/77", stderr: "" }; },
  });
  assert.equal(result.action, "created");
  assert.ok(calls.some((a) => a[0] === "issue" && a[1] === "create"));
});

test("reconcileIssue updates existing open issue when still red", () => {
  const calls = [];
  const v = mainCiHealth([run({ id: 5, conclusion: "failure", created_at: "2026-06-12T21:00:00Z" })]);
  const result = reconcileIssue(v, [], NOW, {
    repository: "o/r",
    fetchJson: () => [{ number: 77, body: "x <!-- main-red-sentinel --> y" }],
    runCommand: (args) => { calls.push(args); return { status: 0, stdout: "", stderr: "" }; },
  });
  assert.equal(result.action, "updated");
  assert.equal(result.number, 77);
  assert.ok(calls.some((a) => a[0] === "issue" && a[1] === "edit"));
});

test("reconcileIssue closes existing issue when green", () => {
  const calls = [];
  const v = mainCiHealth([run({ id: 6, conclusion: "success", created_at: "2026-06-12T21:00:00Z" })]);
  const result = reconcileIssue(v, [], NOW, {
    repository: "o/r",
    fetchJson: () => [{ number: 77, body: "<!-- main-red-sentinel -->" }],
    runCommand: (args) => { calls.push(args); return { status: 0, stdout: "", stderr: "" }; },
  });
  assert.equal(result.action, "closed");
  assert.ok(calls.some((a) => a[0] === "issue" && a[1] === "close"));
});

test("reconcileIssue noop when green and no issue", () => {
  const v = mainCiHealth([run({ id: 6, conclusion: "success", created_at: "2026-06-12T21:00:00Z" })]);
  const result = reconcileIssue(v, [], NOW, {
    repository: "o/r",
    fetchJson: () => [],
    runCommand: () => { throw new Error("must not be called"); },
  });
  assert.equal(result.action, "noop");
});

console.log(`\n${passed} tests passed`);
