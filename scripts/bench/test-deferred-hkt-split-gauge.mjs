#!/usr/bin/env node
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(ROOT, "scripts", "bench", "deferred-hkt-split-gauge.mjs");

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-deferred-hkt-split-"));
try {
  fs.mkdirSync(path.join(dir, "perf-counters"), { recursive: true });
  fs.writeFileSync(
    path.join(dir, "deferred-hkt-split-gauge.jsonl"),
    `${JSON.stringify({
      name: "fp-ts-project",
      state: "yellow",
      exit_class: "nonzero exit",
      diagnostic_status: "diagnostic mismatch or compiler error",
      diagnostic_deltas: ["tsz: src/Alt.ts(1,1): error TS2322: mismatch"],
    })}\n${JSON.stringify({
      name: "neverthrow-project",
      state: "green",
      exit_class: "exit success",
      diagnostic_status: "none",
      diagnostic_deltas: [],
    })}\n`,
  );
  fs.writeFileSync(
    path.join(dir, "perf-counters", "fp-ts-project.perf.json"),
    `${JSON.stringify({
      identity: {
        inference_source_placeholder_unknown_fallback_types: 7,
        inference_source_placeholder_unknown_fallback_placeholders: 9,
        inference_source_placeholder_unknown_fallback_index_access_types: 3,
        relation_deferred_index_access_pair_total: 11,
        relation_deferred_index_access_pair_accepted: 2,
      },
    })}\n`,
  );
  fs.writeFileSync(
    path.join(dir, "perf-counters", "neverthrow-project.perf.json"),
    `${JSON.stringify({
      identity: {
        inference_source_placeholder_unknown_fallback_types: 0,
        inference_source_placeholder_unknown_fallback_placeholders: 0,
        inference_source_placeholder_unknown_fallback_index_access_types: 0,
        relation_deferred_index_access_pair_total: 4,
        relation_deferred_index_access_pair_accepted: 4,
      },
    })}\n`,
  );

  const summaryPath = path.join(dir, "summary.json");
  const result = spawnSync(process.execPath, [
    SCRIPT,
    "--from-existing",
    "--fixture-root",
    dir,
    "--rows",
    "fp-ts-project,neverthrow-project,io-ts-project",
    "--summary-json",
    summaryPath,
  ], { cwd: ROOT, encoding: "utf8" });

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Deferred HKT split gauge/);
  const summary = JSON.parse(fs.readFileSync(summaryPath, "utf8"));
  assert.equal(summary.rows.length, 3);
  assert.equal(summary.rows[0].split, "mixed");
  assert.equal(summary.rows[0].diagnostic_delta_count, 1);
  assert.equal(summary.rows[1].split, "relation-deferred");
  assert.equal(summary.rows[2].split, "no-signal");
  assert.equal(
    summary.totals.counters.inference_source_placeholder_unknown_fallback_types,
    7,
  );
  assert.equal(summary.totals.counters.relation_deferred_index_access_pair_total, 15);
} finally {
  fs.rmSync(dir, { recursive: true, force: true });
}

console.log("deferred-hkt-split-gauge: ok");
