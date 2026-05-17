#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(ROOT, "scripts", "ci", "project-compatibility.mjs");

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-project-compat-"));
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function runProjectCompatibility(args, env) {
  return spawnSync(process.execPath, [SCRIPT, ...args], {
    cwd: ROOT,
    env: { ...process.env, ...env },
    encoding: "utf8",
  });
}

withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const tsconfig = path.join(dir, "fixture", "tsconfig.json");
  const sourceRoot = path.join(dir, "fixture", "src");
  fs.mkdirSync(sourceRoot, { recursive: true });

  const result = runProjectCompatibility(["record"], {
    COMPAT_JSONL_FILE: jsonl,
    COMPAT_NAME: "type-fest-project",
    COMPAT_EXIT_CLASS: "nonzero exit",
    COMPAT_PHASE: "check",
    COMPAT_DIAGNOSTIC_STATUS: "diagnostic mismatch or compiler error",
    COMPAT_DIAGNOSTIC_DELTA: [
      `${path.join(sourceRoot, "index.ts")}(10,4): error TS2344: Type 'false' does not satisfy the constraint 'true'.`,
      "tsgo: internal runner note without a diagnostic code",
    ].join("\n"),
    COMPAT_FILES_REACHED: "42",
    COMPAT_PEAK_MEMORY_BYTES: "1048576",
    COMPAT_TSC_EXIT_CODES: "0",
    COMPAT_TSZ_EXIT_CODES: "2 2",
    COMPAT_TSGO_EXIT_CODES: "1",
    COMPAT_TSCONFIG_PATH: tsconfig,
    COMPAT_SOURCE_ROOT: sourceRoot,
    COMPAT_FIXTURE_ROOT: path.join(dir, "fixture"),
  });

  assert.equal(result.status, 0, result.stderr);
  const rows = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map((line) => JSON.parse(line));
  assert.equal(rows.length, 1);
  const [row] = rows;

  assert.equal(row.name, "type-fest-project");
  assert.equal(row.state, "red");
  assert.equal(row.first_failure_class, "evaluation-inference-instantiation");
  assert.equal(row.owner_track, "Track 2/3 conditional, mapped, inference, instantiation");
  assert.equal(row.phase, "check");
  assert.equal(row.last_successful_phase, null);
  assert.deepEqual(row.diagnostic_codes, ["TS2344"]);
  assert.deepEqual(row.exit_codes, { tsc: [0], tsz: [2], tsgo: [1] });
  assert.equal(row.files_reached, 42);
  assert.equal(row.peak_memory_bytes, 1048576);
  assert.equal(row.repro.tsconfig_path, "tsconfig.json");
  assert.equal(row.repro.source_root, "src");
  assert.equal(row.repro.first_failure_path, "src/index.ts");
  assert.equal(row.repro.first_failure_code, "TS2344");
  assert.equal(row.diagnostic_subsystems[0].subsystem, "evaluation-inference-instantiation");
  assert.equal(row.diagnostic_subsystems[1].subsystem, "uncoded diagnostic");
});

withTempDir((dir) => {
  const jsonl = path.join(dir, "compat.jsonl");
  const summary = path.join(dir, "summary.json");
  fs.writeFileSync(
    jsonl,
    [
      JSON.stringify({ name: "a", state: "green" }),
      JSON.stringify({
        name: "b",
        exit_class: "timeout",
        diagnostic_status: "compiler timed out",
      }),
      "not-json",
    ].join("\n"),
    "utf8",
  );

  const result = runProjectCompatibility(["summary"], {
    SUMMARY_JSONL_FILE: jsonl,
    SUMMARY_OUTPUT_FILE: summary,
    SUMMARY_PROJECT_SET: "canary",
    SUMMARY_PROJECT_FILTER: "type",
    SUMMARY_ALLOW_FAILURES: "1",
    SUMMARY_FAILURES: "1",
  });

  assert.equal(result.status, 0, result.stderr);
  const payload = JSON.parse(fs.readFileSync(summary, "utf8"));
  assert.equal(payload.project_set, "canary");
  assert.equal(payload.project_filter, "type");
  assert.equal(payload.allow_failures, true);
  assert.equal(payload.failures, 1);
  assert.equal(payload.row_count, 2);
  assert.equal(payload.malformed_jsonl_lines, 1);
  assert.equal(payload.by_state.green, 1);
  assert.equal(payload.by_state.yellow, 1);
});
