#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(ROOT, "scripts", "bench", "tsgo-winner-report.mjs");

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-tsgo-winner-report-"));
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

withTempDir((dir) => {
  const input = path.join(dir, "bench.json");
  const output = path.join(dir, "report.json");
  writeJson(input, {
    benchmark_runner: "scripts/bench/bench-vs-tsgo.sh",
    quick_mode: true,
    filter: "project|single",
    results: [
      {
        name: "ts-toolbelt-project",
        lines: 8044,
        kb: 216,
        project_files: 242,
        tsz_ms: 873.92,
        tsgo_ms: 106.15,
        winner: "tsgo",
        factor: 8.23,
        status: null,
        compatibility: {
          exit_class: "exit success",
          diagnostic_status: "none",
          semantic_owner_family: "recursive type evaluation pressure",
        },
      },
      {
        name: "vite-vanilla-ts-app",
        lines: 100,
        kb: 20,
        project_files: 12,
        tsz_ms: 165.15,
        tsgo_ms: 54.51,
        winner: "tsgo",
        factor: 3.03,
        status: null,
        compatibility: {
          exit_class: "exit success",
          diagnostic_status: "none",
          semantic_owner_family: "generated Vite dependency graph",
        },
      },
      {
        name: "single-file-loss",
        lines: 50,
        kb: 2,
        tsz_ms: 20,
        tsgo_ms: 10,
        winner: "tsgo",
        factor: 2,
        status: null,
      },
      {
        name: "tsz-wins",
        tsz_ms: 5,
        tsgo_ms: 10,
        winner: "tsz",
        factor: 2,
      },
      {
        name: "red-project",
        tsz_ms: null,
        tsgo_ms: 10,
        winner: "error",
        factor: 0,
        status: "tsz error",
        compatibility: {
          exit_class: "nonzero exit",
          diagnostic_status: "compiler error",
          semantic_owner_family: "not counted",
        },
      },
      {
        name: "yellow-project",
        tsz_ms: 40,
        tsgo_ms: 20,
        winner: "tsgo",
        factor: 2,
        status: null,
        compatibility: {
          exit_class: "exit success",
          diagnostic_status: "diagnostic mismatch",
          semantic_owner_family: "not counted",
        },
      },
    ],
  });

  const result = spawnSync(process.execPath, [SCRIPT, input, output], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.equal(report.source.quick_mode, true);
  assert.equal(report.totals.rows, 6);
  assert.equal(report.totals.green_tsgo_winners, 3);
  assert.equal(report.totals.project_green_tsgo_winners, 2);
  assert.equal(report.worst.name, "ts-toolbelt-project");
  assert.deepEqual(
    report.rows.map((row) => row.name),
    ["ts-toolbelt-project", "vite-vanilla-ts-app", "single-file-loss"],
  );
  assert.deepEqual(report.by_owner_family, [
    {
      family: "recursive type evaluation pressure",
      rows: 1,
      worst_factor: 8.23,
      worst_row: "ts-toolbelt-project",
    },
    {
      family: "generated Vite dependency graph",
      rows: 1,
      worst_factor: 3.03,
      worst_row: "vite-vanilla-ts-app",
    },
  ]);
});
