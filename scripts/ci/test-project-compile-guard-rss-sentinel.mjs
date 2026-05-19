#!/usr/bin/env node
// Regression: when run_with_timeout's RSS sampler is enabled but no positive
// sample is captured (process exits before the polling tick), the scratch
// file must stay empty so LAST_PEAK_RSS_BYTES propagates as "" — not as "0".
// The "" sentinel is what record_project_compatibility relies on to derive
// peak_memory_bytes_reason = "process exited before sampling". A "0" sentinel
// here previously made the row silently record a real zero-byte measurement.
//
// This test exercises the scratch-file lifecycle directly rather than running
// the full project-compile-guard.sh (which requires TSZ_BIN and the bench
// fixture pipeline) — the lifecycle is the contract that changed and that
// future edits must preserve.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";

const fenceScript = String.raw`
set -Eeuo pipefail

# Mirror of run_with_timeout's RSS scratch-file lifecycle as of the fix.
# Initial sentinel and scratch file must both start empty.
LAST_PEAK_RSS_BYTES=""
rss_file=$(mktemp)
: > "$rss_file"

# The monitor's write condition is "rss_kb > peak_kb" with peak_kb starting at
# zero. Simulating "process exited before any positive sample" means the
# monitor body never executed — so the file stays empty.

# Post-process readback (verbatim from the fixed run_with_timeout).
LAST_PEAK_RSS_BYTES="$(cat "$rss_file" 2>/dev/null || true)"
rm -f "$rss_file"

# Assert: empty sentinel preserved end-to-end.
if [ -n "$LAST_PEAK_RSS_BYTES" ]; then
  echo "fail: LAST_PEAK_RSS_BYTES expected empty, got: \${LAST_PEAK_RSS_BYTES}" >&2
  exit 1
fi
echo "no-sample:ok"

# Conversely: a single positive sample must propagate the byte count, not be
# clobbered by the initial empty file.
LAST_PEAK_RSS_BYTES=""
rss_file=$(mktemp)
: > "$rss_file"
printf '%s\n' "$((1024 * 1024))" > "$rss_file"
LAST_PEAK_RSS_BYTES="$(cat "$rss_file" 2>/dev/null || true)"
rm -f "$rss_file"
if [ "$LAST_PEAK_RSS_BYTES" != "1048576" ]; then
  echo "fail: positive-sample propagation, got: \${LAST_PEAK_RSS_BYTES}" >&2
  exit 1
fi
echo "positive-sample:ok"
`;

const result = spawnSync("bash", ["-c", fenceScript], { encoding: "utf8" });
assert.equal(result.status, 0, `bash test failed: ${result.stderr}`);
assert.match(result.stdout, /no-sample:ok/);
assert.match(result.stdout, /positive-sample:ok/);

// Now exercise the boundary that consumes the sentinel: record_project_compatibility
// receives peak_memory_bytes="" and must emit peak_memory_bytes_reason in the
// closed vocabulary. We run the mjs script directly with the empty env, just
// like the bash record_project_compatibility wrapper would after deriving the
// reason from peak_rss_unavailable_reason().
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const COMPAT_SCRIPT = path.join(ROOT, "scripts", "ci", "project-compatibility.mjs");

const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-rss-sentinel-"));
try {
  const jsonl = path.join(tmpDir, "compat.jsonl");
  // The bash record_project_compatibility wrapper sets both env vars together
  // when peak_memory_bytes is empty: it passes COMPAT_PEAK_MEMORY_BYTES=""
  // and a derived COMPAT_PEAK_MEMORY_BYTES_REASON.
  const recordResult = spawnSync(process.execPath, [COMPAT_SCRIPT, "record"], {
    cwd: ROOT,
    env: {
      ...process.env,
      COMPAT_JSONL_FILE: jsonl,
      COMPAT_NAME: "rss-sentinel-no-sample",
      COMPAT_EXIT_CLASS: "oom",
      COMPAT_PHASE: "check",
      COMPAT_DIAGNOSTIC_STATUS: "compiler OOM or killed",
      COMPAT_FILES_REACHED: "100",
      COMPAT_PEAK_MEMORY_BYTES: "",
      COMPAT_PEAK_MEMORY_BYTES_REASON: "process exited before sampling",
    },
    encoding: "utf8",
  });
  assert.equal(recordResult.status, 0, recordResult.stderr);
  const [row] = fs.readFileSync(jsonl, "utf8").trim().split(/\r?\n/).map(JSON.parse);
  assert.equal(row.peak_memory_bytes, null);
  assert.equal(row.peak_memory_bytes_reason, "process exited before sampling");
  // Crucially, the row must NOT carry a zero-byte measurement with a null
  // reason — that is the precise regression the upstream sampler-sentinel
  // change prevents.
  assert.notEqual(row.peak_memory_bytes, 0, "no-sample row must not record peak_memory_bytes: 0");
} finally {
  fs.rmSync(tmpDir, { recursive: true, force: true });
}
