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
const SCRIPT = path.join(ROOT, "scripts", "bench", "project-winner-regression-report.mjs");
const BENCH_WORKFLOW = path.join(ROOT, ".github", "workflows", "bench.yml");

const { createProjectWinnerRegressionReport } = await import(pathToFileURL(SCRIPT));

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-project-winner-regression-"));
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

function artifact(rows, sourceCommit = "abc123") {
  return {
    generated_at: "2026-05-19T01:02:03.000Z",
    source_commit: sourceCommit,
    workflow_run_url: `https://github.com/mohsen1/tsz/actions/runs/${sourceCommit}`,
    results: rows,
  };
}

function row(name, winner, compatibility = GREEN_COMPAT, factor = 2) {
  return {
    name,
    winner,
    factor,
    tsz_ms: winner === "tsz" ? 10 : 20,
    tsgo_ms: winner === "tsz" ? 20 : 10,
    compatibility,
  };
}

const previous = artifact([
  row("vite-vanilla-ts-app", "tsz", GREEN_COMPAT, 2.1),
  row("nextjs-fresh-app", "tsz", YELLOW_COMPAT, 2.0),
  row("ts-essentials-project", "tsz", GREEN_COMPAT, 1.3),
  row("single-file", "tsz", null, 3.0),
]);
const current = artifact([
  row("vite-vanilla-ts-app", "tsgo", GREEN_COMPAT, 1.7),
  row("nextjs-fresh-app", "tsgo", GREEN_COMPAT, 1.5),
  row("ts-essentials-project", "tsz", GREEN_COMPAT, 1.1),
  row("single-file", "tsgo", null, 2.0),
]);

const report = createProjectWinnerRegressionReport(previous, current, "previous.json", "current.json");
assert.equal(report.totals.tsz_to_tsgo_regressions, 1);
assert.equal(report.rows[0].name, "vite-vanilla-ts-app");
assert.equal(report.rows[0].previous.winner, "tsz");
assert.equal(report.rows[0].current.winner, "tsgo");

// Red/yellow/incomplete rows and non-project rows are not green project winner regressions.
const nonGreenReport = createProjectWinnerRegressionReport(
  artifact([
    row("vite-vanilla-ts-app", "tsz", RED_COMPAT, 2),
    row("nextjs-fresh-app", "tsz", GREEN_COMPAT, 2),
    { name: "large-ts-repo", winner: "tsz", factor: 2, tsz_ms: 10, tsgo_ms: 20, artifact_missing: true },
    row("type-fest-project", "tsz", null, 2),
    row("single-file", "tsz", null, 2),
  ]),
  artifact([
    row("vite-vanilla-ts-app", "tsgo", GREEN_COMPAT, 2),
    row("nextjs-fresh-app", "tsgo", YELLOW_COMPAT, 2),
    row("large-ts-repo", "tsgo", GREEN_COMPAT, 2),
    row("type-fest-project", "tsgo", null, 2),
    row("single-file", "tsgo", null, 2),
  ]),
  "previous.json",
  "current.json",
);
assert.equal(nonGreenReport.totals.tsz_to_tsgo_regressions, 0);

withTempDir((dir) => {
  const before = path.join(dir, "before.json");
  const after = path.join(dir, "after.json");
  const out = path.join(dir, "report.json");
  writeJson(before, previous);
  writeJson(after, current);

  const result = spawnSync(process.execPath, [SCRIPT, before, after, out], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 1, "winner regressions should produce an audit failure exit");
  assert.match(result.stdout, /Project Winner Regression Report/);
  assert.match(result.stdout, /generated Vite app/);

  const written = JSON.parse(fs.readFileSync(out, "utf8"));
  assert.equal(written.totals.tsz_to_tsgo_regressions, 1);
});

withTempDir((dir) => {
  const before = path.join(dir, "before.json");
  const after = path.join(dir, "after.json");
  writeJson(before, artifact([row("vite-vanilla-ts-app", "tsz", GREEN_COMPAT, 2)]));
  writeJson(after, artifact([row("vite-vanilla-ts-app", "tsz", GREEN_COMPAT, 1.8)]));

  const result = spawnSync(process.execPath, [SCRIPT, "--json", before, after], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  const parsed = JSON.parse(result.stdout);
  assert.equal(parsed.totals.tsz_to_tsgo_regressions, 0);
});

const benchWorkflow = fs.readFileSync(BENCH_WORKFLOW, "utf8");
assert.match(
  benchWorkflow,
  /project-winner-regression-report\.mjs\s+\\\s*\n\s+"\$previous"\s+\\\s*\n\s+"\$GITHUB_WORKSPACE\/bench-results\.json"\s+\\\s*\n\s+"\$report_json"/,
  "bench workflow should compare the new benchmark artifact against previous latest.json",
);
assert.match(
  benchWorkflow,
  /bench-results-project-winner-regressions\.json/,
  "merged benchmark artifact should include the project winner regression JSON report",
);
assert.match(
  benchWorkflow,
  /bench-results-project-winner-regressions\.md/,
  "merged benchmark artifact should include the project winner regression markdown report",
);

console.log("project winner regression report tests passed");
