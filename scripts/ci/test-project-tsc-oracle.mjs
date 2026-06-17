#!/usr/bin/env node
// Unit tests for the per-row tsc oracle helpers in
// scripts/ci/lib/project-tsc-oracle.sh.
//
// The gate's core contract: tsz_only_delta_lines subtracts tsc's own
// diagnostics from tsz's by (basename, line, column, code) identity, so a row
// passes when tsz MATCHES tsc (empty delta), and a tsc-clean row keeps every
// tsz diagnostic (the gate is unchanged for the required rows).
//
// We drive the real, sourced library from bash so the contract is tested, not
// a mirror of it. The awk programs must stay POSIX-portable (BSD awk on macOS,
// gawk in CI); the multi-line key path in particular regresses under `awk -v`
// ("newline in string"), which is why the delta uses a two-file FILENAME idiom.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const LIB = path.join(ROOT, "scripts", "ci", "lib", "project-tsc-oracle.sh");

assert.ok(fs.existsSync(LIB), `tsc oracle library missing: ${LIB}`);

// Run a delta+count for one (tsz, tsc) log pair. Returns the tsz-only count and
// the raw delta body. tsz/tsc are arrays of diagnostic-log lines.
function runDelta(tszLines, tscLines) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-oracle-"));
  try {
    const tszLog = path.join(dir, "tsz.log");
    const tscLog = path.join(dir, "tsc.log");
    fs.writeFileSync(tszLog, tszLines.join("\n") + (tszLines.length ? "\n" : ""));
    fs.writeFileSync(tscLog, tscLines.join("\n") + (tscLines.length ? "\n" : ""));
    const harness = `
      set -Eeuo pipefail
      source "${LIB}"
      tsz_only_delta_lines "${tszLog}" "${tscLog}" > "${dir}/delta.out"
      tsz_only_delta_lines "${tszLog}" "${tscLog}" | tsz_count_diagnostic_lines
    `;
    const result = spawnSync("bash", ["-c", harness], { encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    const count = Number(String(result.stdout).trim());
    const delta = fs.readFileSync(path.join(dir, "delta.out"), "utf8");
    return { count, delta };
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

const D = (p, l, c, code) => `${p}(${l},${c}): error ${code}: message text`;
const Dpretty = (p, l, c, code) => `${p}:${l}:${c} - error ${code}: message text`;

// 1. tsz reproduces tsc's errors AND adds one of its own -> tsz-only is the one.
{
  const { count } = runDelta(
    [D("src/r.ts", 27, 5, "TS2365"), D("src/a.ts", 50, 9, "TS2322"), D("src/x.ts", 3, 1, "TS2339"), "Found 3 errors."],
    [D("src/r.ts", 27, 5, "TS2365"), D("src/a.ts", 50, 9, "TS2322"), "Found 2 errors."],
  );
  assert.equal(count, 1, "tsz-superset: only the extra tsz diagnostic remains");
}

// 2. tsz matches tsc exactly -> empty tsz-only delta (the row passes the gate).
{
  const { count } = runDelta(
    [D("src/r.ts", 27, 5, "TS2365"), "Found 1 error."],
    [D("src/r.ts", 27, 5, "TS2365"), "Found 1 error."],
  );
  assert.equal(count, 0, "exact match: tsz-only delta is empty");
}

// 3. tsc-clean fixture -> the gate is a no-op: every tsz diagnostic survives.
{
  const tsz = [D("src/a.ts", 1, 1, "TS2322"), D("src/b.ts", 2, 2, "TS2345")];
  const { count, delta } = runDelta(tsz, []);
  assert.equal(count, 2, "tsc-clean row keeps all tsz diagnostics");
  for (const line of tsz) {
    assert.ok(delta.includes(line), `tsc-clean delta must preserve: ${line}`);
  }
}

// 4. tsz is a subset of tsc (tsc reports more) -> tsz-only is empty.
{
  const { count } = runDelta(
    [D("src/r.ts", 1, 1, "TS2365")],
    [D("src/r.ts", 1, 1, "TS2365"), D("src/o.ts", 9, 9, "TS2322"), "Found 2 errors."],
  );
  assert.equal(count, 0, "tsz-subset: no tsz-only diagnostics");
}

// 5. Same location, DIFFERENT code -> a genuine divergence is NOT subtracted.
{
  const { count } = runDelta(
    [D("src/r.ts", 1, 1, "TS2345")],
    [D("src/r.ts", 1, 1, "TS2365")],
  );
  assert.equal(count, 1, "same-location/different-code is a real tsz-only divergence");
}

// 6. Formatter independence: tsc pretty form, tsz paren form, same identity.
{
  const { count } = runDelta(
    [D("src/r.ts", 1, 1, "TS2365")],
    [Dpretty("src/r.ts", 1, 1, "TS2365")],
  );
  assert.equal(count, 0, "diagnostic identity is formatter-independent");
}

// 7. Path normalization: absolute vs relative path with the same basename.
{
  const { count } = runDelta(
    [D("/abs/proj/src/r.ts", 1, 1, "TS2365")],
    [D("src/r.ts", 1, 1, "TS2365")],
  );
  assert.equal(count, 0, "absolute vs relative path with same basename matches");
}

// 8. Portability regression: many tsc keys must not break the subtraction. A
//    multi-line key set passed via `awk -v` fails under BSD awk; the FILENAME
//    two-file idiom must keep all keys. Build 50 matched + 1 extra tsz line.
{
  const tsc = [];
  const tsz = [];
  for (let i = 1; i <= 50; i += 1) {
    const line = D(`src/f${i}.ts`, i, 1, "TS2322");
    tsc.push(line);
    tsz.push(line);
  }
  tsz.push(D("src/only.ts", 999, 1, "TS2345"));
  const { count } = runDelta(tsz, tsc);
  assert.equal(count, 1, "large tsc key set: only the one genuinely-extra tsz line remains");
}

// 9. Identity-key extraction parses both formatter shapes, drops banners.
{
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-oracle-keys-"));
  try {
    const log = path.join(dir, "log");
    fs.writeFileSync(log, [
      D("src/a.ts", 1, 2, "TS2322"),
      Dpretty("src/b.ts", 3, 4, "TS2345"),
      "Found 2 errors.",
      "",
    ].join("\n"));
    const harness = `set -Eeuo pipefail; source "${LIB}"; tsz_diagnostic_identity_keys < "${log}"`;
    const result = spawnSync("bash", ["-c", harness], { encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    const keys = String(result.stdout).trim().split("\n").sort();
    assert.deepEqual(keys, ["a.ts\t1\t2\tTS2322", "b.ts\t3\t4\tTS2345"].sort());
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

console.log("test-project-tsc-oracle: all tests passed");
