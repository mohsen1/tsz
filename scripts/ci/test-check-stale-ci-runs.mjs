#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { formatReport, readActiveRuns, staleRunFindings } from "./check-stale-ci-runs.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(ROOT, "scripts", "ci", "check-stale-ci-runs.mjs");
const NOW = "2026-05-20T13:00:00Z";

function run(overrides = {}) {
  return {
    id: 12345,
    status: "in_progress",
    name: "CI",
    display_title: "fix(setup): sample",
    head_branch: "codex/sample",
    html_url: "https://github.example/runs/12345",
    created_at: "2026-05-20T12:00:00Z",
    run_started_at: "2026-05-20T12:02:00Z",
    updated_at: "2026-05-20T12:10:00Z",
    ...overrides,
  };
}

function withFixture(runs, fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-stale-ci-runs-"));
  try {
    const fixture = path.join(dir, "runs.json");
    fs.writeFileSync(fixture, `${JSON.stringify({ workflow_runs: runs })}\n`);
    return fn(fixture);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function runFixture(runs, args = []) {
  return withFixture(runs, (fixture) => spawnSync(process.execPath, [
    SCRIPT,
    "--fixture",
    fixture,
    "--now",
    NOW,
    ...args,
  ], {
    cwd: ROOT,
    encoding: "utf8",
  }));
}

assert.deepEqual(
  staleRunFindings([run()], { now: NOW, staleMinutes: 45 }),
  [{
    id: 12345,
    status: "in_progress",
    title: "fix(setup): sample",
    branch: "codex/sample",
    url: "https://github.example/runs/12345",
    ageMinutes: 58,
    updatedMinutes: 50,
    reason: "no recent update",
  }],
);

assert.deepEqual(
  staleRunFindings([run({
    id: 2,
    status: "queued",
    created_at: "2026-05-20T12:20:00Z",
    run_started_at: null,
    updated_at: "2026-05-20T12:55:00Z",
  })], { now: NOW, staleMinutes: 45 }),
  [],
);

assert.deepEqual(
  staleRunFindings([run({
    id: 4,
    run_started_at: "2026-05-20T11:00:00Z",
    updated_at: "2026-05-20T12:59:00Z",
  })], { now: NOW, staleMinutes: 45 }),
  [],
);

assert.deepEqual(
  staleRunFindings([run({
    id: 5,
    status: "queued",
    created_at: "2026-05-20T11:00:00Z",
    run_started_at: null,
    updated_at: "2026-05-20T12:59:00Z",
  })], { now: NOW, staleMinutes: 45 }),
  [{
    id: 5,
    status: "queued",
    title: "fix(setup): sample",
    branch: "codex/sample",
    url: "https://github.example/runs/12345",
    ageMinutes: 120,
    updatedMinutes: 1,
    reason: "old queued run",
  }],
);

assert.deepEqual(
  staleRunFindings([run({
    id: 3,
    status: "completed",
    created_at: "2026-05-20T10:00:00Z",
    updated_at: "2026-05-20T10:00:00Z",
  })], { now: NOW, staleMinutes: 45 }),
  [],
);

{
  const fetchedStatuses = [];
  const activeRuns = readActiveRuns("owner/repo", 3, (args) => {
    const endpoint = args[args.length - 1];
    const status = /[?&]status=([^&]+)/.exec(endpoint)?.[1];
    fetchedStatuses.push(status);
    return {
      workflow_runs: status === "queued"
        ? [run({
          id: 20,
          status: "queued",
          created_at: "2026-05-20T11:00:00Z",
          run_started_at: null,
          updated_at: "2026-05-20T11:30:00Z",
        })]
        : [
          run({ id: 10, status: "in_progress" }),
          run({ id: 11, status: "in_progress" }),
          run({ id: 12, status: "in_progress" }),
          run({ id: 13, status: "in_progress" }),
        ],
    };
  });

  assert.deepEqual(fetchedStatuses, ["in_progress", "queued"]);
  assert.deepEqual(activeRuns.map((activeRun) => activeRun.id), [10, 20, 11]);
  assert.ok(
    staleRunFindings(activeRuns, { now: NOW, staleMinutes: 45 })
      .some((finding) => finding.id === 20),
  );
}

const report = formatReport(staleRunFindings([run()], { now: NOW, staleMinutes: 45 }), {
  staleMinutes: 45,
});
assert.match(report, /Stale CI Run Advisory/);
assert.match(report, /\[#12345\]\(https:\/\/github.example\/runs\/12345\)/);
assert.match(report, /no recent update/);

const advisory = runFixture([run()], ["--stale-minutes", "45"]);
assert.equal(advisory.status, 0, advisory.stderr);
assert.match(advisory.stdout, /Found 1 queued or in-progress workflow run/);

const enforce = runFixture([run()], ["--stale-minutes", "45", "--enforce"]);
assert.equal(enforce.status, 1, enforce.stdout);
assert.match(enforce.stdout, /no recent update/);

const clean = runFixture([run({
  run_started_at: "2026-05-20T12:58:00Z",
  updated_at: "2026-05-20T12:59:00Z",
})], ["--stale-minutes", "45"]);
assert.equal(clean.status, 0, clean.stderr);
assert.match(clean.stdout, /No queued or in-progress workflow runs are stale/);

{
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-stale-ci-runs-gh-"));
  try {
    const fakeGh = path.join(dir, "gh");
    fs.writeFileSync(fakeGh, `#!/usr/bin/env node
const endpoint = process.argv.find((arg) => arg.includes("/actions/runs?")) || "";
const status = /[?&]status=([^&]+)/.exec(endpoint)?.[1] || "in_progress";
const run = {
  id: status === "queued" ? 22222 : 11111,
  status,
  display_title: "large queued payload",
  head_branch: "codex/large-payload",
  html_url: "https://github.example/runs/large",
  created_at: "2026-05-20T11:00:00Z",
  run_started_at: status === "queued" ? null : "2026-05-20T11:01:00Z",
  updated_at: "2026-05-20T11:05:00Z",
  padding: "x".repeat(2 * 1024 * 1024),
};
console.log(JSON.stringify({ workflow_runs: [run] }));
`);
    fs.chmodSync(fakeGh, 0o755);

    const result = spawnSync(process.execPath, [
      SCRIPT,
      "--repository",
      "owner/repo",
      "--max-runs",
      "1",
      "--now",
      NOW,
    ], {
      cwd: ROOT,
      encoding: "utf8",
      env: { ...process.env, PATH: `${dir}${path.delimiter}${process.env.PATH || ""}` },
    });

    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /Found 1 queued or in-progress workflow run/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}
