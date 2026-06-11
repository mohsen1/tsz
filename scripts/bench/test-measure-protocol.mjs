#!/usr/bin/env node
// Unit tests for scripts/bench/lib/measure-protocol.sh (issue #13174):
// per-sha binary snapshotting and CPU-share timeout classification. These
// primitives are what keep perf measurements sound on shared boxes, so each
// contract is pinned here:
//   - ps TIME parsing across Linux/macOS formats,
//   - snapshot immutability against source overwrites,
//   - contention classification thresholds.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const dirname = path.dirname(fileURLToPath(import.meta.url));
const lib = path.join(dirname, "lib", "measure-protocol.sh");

function runLib(script) {
  const result = spawnSync("bash", ["-c", `set -euo pipefail; source '${lib}'; ${script}`], {
    encoding: "utf8",
  });
  return result;
}

function libStdout(script) {
  const result = runLib(script);
  assert.equal(result.status, 0, `lib call failed: ${script}\nstderr: ${result.stderr}`);
  return result.stdout.trim();
}

// --- tsz_cputime_to_seconds: every ps TIME shape seen on Linux and macOS ---
const parseCases = [
  ["0:00.05", "0.05"], // macOS mm:ss.cc
  ["12:34.56", "754.56"], // macOS mm:ss.cc with minutes
  ["00:00:05", "5.00"], // Linux hh:mm:ss
  ["1:02:03", "3723.00"], // Linux h:mm:ss
  ["2-00:00:10", "172810.00"], // Linux dd-hh:mm:ss
];
for (const [input, expected] of parseCases) {
  assert.equal(
    libStdout(`tsz_cputime_to_seconds '${input}'`),
    expected,
    `tsz_cputime_to_seconds(${input})`,
  );
}

// --- tsz_process_tree_cpu_seconds: returns a number for a live process ---
{
  const out = libStdout("tsz_process_tree_cpu_seconds $$");
  assert.match(out, /^\d+\.\d{2}$/, `tree cpu sample should be numeric, got: ${out}`);
}

// --- tsz_cpu_share_pct ---
assert.equal(libStdout("tsz_cpu_share_pct 14 420"), "3");
assert.equal(libStdout("tsz_cpu_share_pct 415 420"), "99");
assert.equal(libStdout("tsz_cpu_share_pct 840 420"), "200"); // multi-threaded > 100%
assert.equal(libStdout("tsz_cpu_share_pct '' 420"), ""); // unknown cpu -> no share
assert.equal(libStdout("tsz_cpu_share_pct 14 0"), ""); // degenerate wall -> no share

// --- tsz_timeout_is_contended ---
function contended(args) {
  return runLib(`tsz_timeout_is_contended ${args}`).status === 0;
}
assert.equal(contended("420 14 25"), true, "3% share is contended");
assert.equal(contended("420 415 25"), false, "99% share is not contended");
assert.equal(contended("420 '' 25"), false, "unknown cpu must NOT be contention-confirmed");
assert.equal(contended("420 100 25"), true, "24% share is below the 25% threshold");
assert.equal(contended("420 105 25"), false, "25% share is at the threshold");

// --- tsz_timeout_is_cpu_bound: NOT the negation of is_contended ---
function cpuBound(args) {
  return runLib(`tsz_timeout_is_cpu_bound ${args}`).status === 0;
}
assert.equal(cpuBound("420 415 25"), true, "99% share is CPU-bound");
assert.equal(cpuBound("420 14 25"), false, "3% share is not CPU-bound");
assert.equal(
  cpuBound("420 '' 25"),
  false,
  "no CPU sample is neither contended nor CPU-bound -- unmeasured",
);

// --- tsz_timeout_contention_note ---
{
  const note = libStdout("tsz_timeout_contention_note 420 14 25");
  assert.match(note, /likely CPU contention/, note);
  assert.match(note, /unmeasured, not slow/, note);
}
{
  const note = libStdout("tsz_timeout_contention_note 420 415 25");
  assert.match(note, /CPU-bound timeout/, note);
  assert.doesNotMatch(note, /contention/, note);
}
{
  const note = libStdout("tsz_timeout_contention_note 420 '' 25");
  assert.match(note, /CPU time unavailable/, note);
  assert.match(note, /unmeasured/, note);
}

// --- tsz_snapshot_binary: immutable, content-addressed, mid-run-safe ---
const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-measure-protocol-"));
try {
  const src = path.join(tmpDir, "fake-tsz");
  const snapDir = path.join(tmpDir, "snapshots");
  fs.writeFileSync(src, "#!/bin/sh\necho v1\n", { mode: 0o755 });

  const first = libStdout(`tsz_snapshot_binary '${src}' '${snapDir}'`).split(/\s+/);
  assert.equal(first.length, 2, "prints '<path> <sha256>'");
  const [snapPath, snapHash] = first;
  assert.notEqual(snapPath, src, "snapshot must not be the live path");
  assert.match(snapHash, /^[0-9a-f]{64}$/);
  assert.ok(snapPath.includes(snapHash.slice(0, 16)), "snapshot path is content-addressed");
  const v1Content = fs.readFileSync(snapPath, "utf8");

  // Same content -> same snapshot path (idempotent, concurrent-session safe).
  const again = libStdout(`tsz_snapshot_binary '${src}' '${snapDir}'`).split(/\s+/);
  assert.deepEqual(again, first, "re-snapshot of identical content reuses the path");

  // Overwriting the live binary (the sibling-session hazard) must not touch
  // the existing snapshot, and a new snapshot lands at a new path.
  fs.writeFileSync(src, "#!/bin/sh\necho v2\n", { mode: 0o755 });
  const second = libStdout(`tsz_snapshot_binary '${src}' '${snapDir}'`).split(/\s+/);
  assert.notEqual(second[0], snapPath, "new content gets a new snapshot path");
  assert.notEqual(second[1], snapHash, "new content gets a new hash");
  assert.equal(
    fs.readFileSync(snapPath, "utf8"),
    v1Content,
    "the v1 snapshot is immutable after the source was overwritten",
  );

  // Pruning keeps only the snapshot being measured.
  libStdout(`tsz_prune_binary_snapshots '${src}' '${snapDir}' '${second[0]}'`);
  const remaining = fs.readdirSync(snapDir);
  assert.deepEqual(remaining, [path.basename(second[0])], "prune keeps exactly the live snapshot");

  // Missing source fails loudly.
  const missing = runLib(`tsz_snapshot_binary '${tmpDir}/does-not-exist' '${snapDir}'`);
  assert.notEqual(missing.status, 0, "missing source must fail");
  assert.match(missing.stderr, /not found/);
} finally {
  fs.rmSync(tmpDir, { recursive: true, force: true });
}

console.log("measure-protocol tests passed");
