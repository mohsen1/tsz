#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const MERGE_SCRIPT = path.join(ROOT, "scripts", "bench", "merge-results.mjs");
const REQUIRED_PROJECT_ROWS = [
  "utility-types-project",
  "ts-essentials-project",
  "rxjs-project",
  "type-fest-project",
  "vite-vanilla-ts-app",
  "nextjs-fresh-app",
  "nextjs",
  "large-ts-repo",
];

const REQUIRED_COMPATIBILITY_FIELDS = {
  exit_class: "exit success",
  phase: "check",
  last_successful_phase: "check",
  diagnostic_status: "none",
  diagnostic_deltas: [],
  diagnostic_subsystems: [],
  known_blockers: [],
  exit_codes: { tsc: [0], tsz: [0], tsgo: [0] },
  files_reached: 1,
  peak_memory_bytes: 1024,
  emit_status: "not in scope (noEmit project check)",
  dts_status: "not in scope (noEmit project check)",
};

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-merge-results-"));
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function writePayload(dir, results) {
  const input = path.join(dir, "input.json");
  fs.writeFileSync(
    input,
    `${JSON.stringify({
      quick_mode: false,
      validation: { hyperfine_exit_codes_required: true },
      totals: { benchmarks_run: results.length },
      results,
    })}\n`,
    "utf8",
  );
  return input;
}

function runMerge(dir, results) {
  const input = writePayload(dir, results);
  const output = path.join(dir, "merged.json");
  const result = spawnSync(process.execPath, [MERGE_SCRIPT, output, input], {
    cwd: ROOT,
    encoding: "utf8",
  });
  return { ...result, output };
}

function projectRow(name, compatibility = REQUIRED_COMPATIBILITY_FIELDS) {
  return {
    name,
    lines: 1,
    kb: 1,
    tsz_ms: 1,
    tsgo_ms: 1,
    winner: "tsz",
    ratio: 1,
    compatibility,
  };
}

withTempDir((dir) => {
  const result = runMerge(dir, REQUIRED_PROJECT_ROWS.map((name) => projectRow(name)));
  assert.equal(result.status, 0, result.stderr);
  const merged = JSON.parse(fs.readFileSync(result.output, "utf8"));
  assert.equal(merged.validation.project_compatibility_required_fields, true);
});

withTempDir((dir) => {
  const rows = REQUIRED_PROJECT_ROWS.filter((name) => name !== "utility-types-project")
    .map((name) => projectRow(name));
  const result = runMerge(dir, rows);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /utility-types-project: missing project row/);
});

withTempDir((dir) => {
  const rows = REQUIRED_PROJECT_ROWS.map((name) => {
    if (name !== "rxjs-project") return projectRow(name);
    const { peak_memory_bytes: _peakMemoryBytes, ...compatibility } = REQUIRED_COMPATIBILITY_FIELDS;
    return projectRow(name, compatibility);
  });
  const result = runMerge(dir, rows);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /rxjs-project: missing compatibility\.peak_memory_bytes/);
});
