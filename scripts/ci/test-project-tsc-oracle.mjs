#!/usr/bin/env node
// Unit tests for the per-row tsc oracle helpers in
// scripts/ci/lib/project-tsc-oracle.sh.
//
// The gate's core contract: exact project-relative path/span/code/message
// diagnostic multisets and ordinary exit codes must match. The one-sided delta
// remains a reporting helper; an empty delta alone is never pass evidence.
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

function writeJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(value)}\n`);
}

function runOracleCommand(root) {
  const harness = `
    set -Eeuo pipefail
    source "$1"
    ROOT_DIR="$2"
    tsz_project_oracle_tsc_command
  `;
  const result = spawnSync("bash", ["-c", harness, "bash", LIB, root], {
    encoding: "utf8",
    env: Object.fromEntries(
      Object.entries(process.env).filter(([key]) => key !== "TSZ_PROJECT_TSC_ORACLE_BIN"),
    ),
  });
  assert.equal(result.status, 0, result.stderr);
  return String(result.stdout).trim().split("\n").filter(Boolean);
}

function writePinnedOracle(root, { installed = "7.0.2", reported = installed } = {}) {
  const versions = path.join(root, "scripts", "conformance", "typescript-versions.json");
  const packageJson = path.join(root, "scripts", "node_modules", "typescript", "package.json");
  const tsc = path.join(root, "scripts", "node_modules", "typescript", "lib", "tsc.js");
  writeJson(versions, {
    current: "corpus-pin",
    mappings: { "corpus-pin": { npm: "7.0.2" } },
    default: { npm: "7.0.2" },
  });
  writeJson(packageJson, { name: "typescript", version: installed });
  fs.mkdirSync(path.dirname(tsc), { recursive: true });
  fs.writeFileSync(tsc, `console.log("Version ${reported}");\n`);
  return tsc;
}

// Run a delta+count for one (tsz, tsc) log pair. Returns the tsz-only count and
// the raw delta body. tsz/tsc are arrays of diagnostic-log lines.
function runDelta(tszLines, tscLines, { projectRoot = "" } = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-oracle-"));
  try {
    const tszLog = path.join(dir, "tsz.log");
    const tscLog = path.join(dir, "tsc.log");
    fs.writeFileSync(tszLog, tszLines.join("\n") + (tszLines.length ? "\n" : ""));
    fs.writeFileSync(tscLog, tscLines.join("\n") + (tscLines.length ? "\n" : ""));
    const harness = `
      set -Eeuo pipefail
      source "${LIB}"
      tsz_only_delta_lines "${tszLog}" "${tscLog}" "${projectRoot}" > "${dir}/delta.out"
      count="$(tsz_only_delta_lines "${tszLog}" "${tscLog}" "${projectRoot}" | tsz_count_diagnostic_lines "${projectRoot}")"
      agrees=0
      tsz_diagnostic_multisets_agree "${tszLog}" "${tscLog}" "${projectRoot}" && agrees=1
      printf '%s\t%s\n' "$count" "$agrees"
    `;
    const result = spawnSync("bash", ["-c", harness], { encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    const [countText, agreesText] = String(result.stdout).trim().split("\t");
    const count = Number(countText);
    const delta = fs.readFileSync(path.join(dir, "delta.out"), "utf8");
    return { count, delta, agrees: agreesText === "1" };
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

const D = (p, l, c, code) => `${p}(${l},${c}): error ${code}: message text`;
const Dpretty = (p, l, c, code) => `${p}:${l}:${c} - error ${code}: message text`;
const Dglobal = (code, message = "No inputs were found in config file.") =>
  `error ${code}: ${message}`;

// 1. tsz reproduces tsc's errors AND adds one of its own -> tsz-only is the one.
{
  const { count, agrees } = runDelta(
    [D("src/r.ts", 27, 5, "TS2365"), D("src/a.ts", 50, 9, "TS2322"), D("src/x.ts", 3, 1, "TS2339"), "Found 3 errors."],
    [D("src/r.ts", 27, 5, "TS2365"), D("src/a.ts", 50, 9, "TS2322"), "Found 2 errors."],
  );
  assert.equal(count, 1, "tsz-superset: only the extra tsz diagnostic remains");
  assert.equal(agrees, false);
}

// 2. tsz matches tsc exactly -> empty tsz-only delta (the row passes the gate).
{
  const { count, agrees } = runDelta(
    [D("src/r.ts", 27, 5, "TS2365"), "Found 1 error."],
    [D("src/r.ts", 27, 5, "TS2365"), "Found 1 error."],
  );
  assert.equal(count, 0, "exact match: tsz-only delta is empty");
  assert.equal(agrees, true, "exact diagnostic multisets agree");
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

// 4. tsz is a subset of tsc (tsc reports more) -> one-way delta is empty, but
//    symmetric parity must reject the missing diagnostic.
{
  const { count, agrees } = runDelta(
    [D("src/r.ts", 1, 1, "TS2365")],
    [D("src/r.ts", 1, 1, "TS2365"), D("src/o.ts", 9, 9, "TS2322"), "Found 2 errors."],
  );
  assert.equal(count, 0, "tsz-subset: no tsz-only diagnostics");
  assert.equal(agrees, false, "tsz strict subset must never be parity");
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

// 7. Path normalization: absolute vs project-relative spelling.
{
  const projectRoot = path.join(os.tmpdir(), "oracle-project-root");
  const { count, agrees } = runDelta(
    [D(path.join(projectRoot, "src", "r.ts"), 1, 1, "TS2365")],
    [D("src/r.ts", 1, 1, "TS2365")],
    { projectRoot },
  );
  assert.equal(count, 0, "absolute vs project-relative path matches");
  assert.equal(agrees, true);
}

// Same basename in distinct directories is not the same diagnostic.
{
  const { count, agrees } = runDelta(
    [D("a/index.ts", 1, 1, "TS2322")],
    [D("b/index.ts", 1, 1, "TS2322")],
  );
  assert.equal(count, 1);
  assert.equal(agrees, false, "full project-relative paths prevent basename collisions");
}

// Duplicate multiplicity is part of exact parity in either direction.
{
  const line = D("src/a.ts", 1, 1, "TS2322");
  assert.equal(runDelta([line], [line, line]).agrees, false);
  const extra = runDelta([line, line], [line]);
  assert.equal(extra.count, 1);
  assert.equal(extra.agrees, false);
}

// Located diagnostics also compare normalized message text.
{
  const { count, agrees } = runDelta(
    ["src/a.ts(1,1): error TS2322: source says A"],
    ["src/a.ts(1,1): error TS2322: source says B"],
  );
  assert.equal(count, 1);
  assert.equal(agrees, false);
}

// Multiline reason chains are part of parity even when the coded first line is
// identical. The one-way coded delta remains empty, but symmetric agreement
// must reject different continuation text.
{
  const first = "src/a.ts(1,1): error TS2322: Type A is not assignable to type B.";
  const { count, agrees } = runDelta(
    [first, "  Types of property 'value' are incompatible."],
    [first, "  Property 'value' is missing."],
  );
  assert.equal(count, 0);
  assert.equal(agrees, false, "different multiline reason chains cannot become green");
}

// Continuations belong to their primary diagnostic. Sorting primary keys and
// comparing one detached continuation stream would falsely accept this pair:
// the same reason lines occur in the same order, but each is attached to the
// other diagnostic in the tsc log.
{
  const a = "a/index.ts(1,1): error TS2322: first diagnostic";
  const b = "b/index.ts(2,1): error TS2345: second diagnostic";
  assert.equal(
    runDelta(
      [a, "  reason owned by a", b, "  reason owned by b"],
      [b, "  reason owned by a", a, "  reason owned by b"],
    ).agrees,
    false,
    "swapped continuation ownership cannot become parity",
  );
}

// Unparsed nonblank output is never equality evidence, even when both sides
// happen to print the same unknown shape. Known compiler summaries are only
// transport and remain ignorable once the diagnostic body is parsed.
{
  assert.equal(
    runDelta(["unknown diagnostic shape A"], ["unknown diagnostic shape B"]).agrees,
    false,
  );
  assert.equal(
    runDelta(["same unknown diagnostic shape"], ["same unknown diagnostic shape"]).agrees,
    false,
  );
  const line = D("src/a.ts", 1, 1, "TS2322");
  assert.equal(
    runDelta([line, "Found 1 error."], [line, "Found 1 error."]).agrees,
    true,
    "known summary transport does not make an otherwise parsed log uncovered",
  );
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

// 9. Global/config diagnostics carry no path, line, or column. They still
// participate in parity by code and exact message, so a matching TS18003
// cancels while a different pathless code/message remains actionable.
{
  assert.equal(
    runDelta([Dglobal("TS18003")], [Dglobal("TS18003")]).count,
    0,
    "matching pathless TS18003 diagnostics must cancel",
  );
  assert.equal(
    runDelta([Dglobal("TS18003")], [Dglobal("TS18002")]).count,
    1,
    "different pathless diagnostic codes must not cancel",
  );
  assert.equal(
    runDelta(
      [Dglobal("TS18003", "No inputs matched include 'src/**/*.ts'.")],
      [Dglobal("TS18003", "No inputs matched include 'tests/**/*.ts'.")],
    ).count,
    1,
    "same-code global diagnostics with different messages must not cancel",
  );
  assert.equal(
    runDelta(
      [Dglobal("TS18003", "No inputs matched include '[src/  **/*.ts]'.")],
      [Dglobal("TS18003", "No inputs matched include '[src/ **/*.ts]'.")],
    ).count,
    1,
    "double spaces inside a quoted TS18003 path remain meaningful identity",
  );
}

// 10. Identity-key extraction parses both located formatter shapes and the
// global shape, while dropping banners.
{
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-oracle-keys-"));
  try {
    const log = path.join(dir, "log");
    fs.writeFileSync(log, [
      D("src/a.ts", 1, 2, "TS2322"),
      Dpretty("src/b.ts", 3, 4, "TS2345"),
      Dglobal("TS18003"),
      "Found 3 errors.",
      "",
    ].join("\n"));
    const harness = `set -Eeuo pipefail; source "${LIB}"; tsz_diagnostic_identity_keys < "${log}"`;
    const result = spawnSync("bash", ["-c", harness], { encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    const keys = String(result.stdout).trim().split("\n").sort();
    assert.deepEqual(
      keys,
      [
        "src/a.ts\t1\t2\tTS2322\tmessage text",
        "src/b.ts\t3\t4\tTS2345\tmessage text",
        "<global>\t0\t0\tTS18003\tNo inputs were found in config file.",
      ].sort(),
    );
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// 11. The project oracle resolves only the exact pinned scripts/npm compiler.
//     The legacy corpus tree is not a TypeScript 7 compiler source checkout.
{
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-oracle-command-"));
  try {
    const tsc = writePinnedOracle(root);
    assert.deepEqual(runOracleCommand(root), ["node", tsc]);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

// 12. Wrapper metadata and the executable's own version must both match the
//     pin; arbitrary source-tree and top-level shims are never fallbacks.
for (const mismatch of [
  { installed: "7.0.1", reported: "7.0.1" },
  { installed: "7.0.2", reported: "7.0.1" },
]) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-oracle-mismatch-"));
  try {
    writePinnedOracle(root, mismatch);
    const sourceTreeTsc = path.join(root, "typescript", "lib", "tsc.js");
    const topLevelTsc = path.join(root, "node_modules", ".bin", "tsc");
    fs.mkdirSync(path.dirname(sourceTreeTsc), { recursive: true });
    fs.mkdirSync(path.dirname(topLevelTsc), { recursive: true });
    fs.writeFileSync(sourceTreeTsc, "console.log('Version 7.0.2');\n");
    fs.writeFileSync(topLevelTsc, "#!/bin/sh\necho 'Version 7.0.2'\n", { mode: 0o755 });
    assert.deepEqual(runOracleCommand(root), []);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

console.log("test-project-tsc-oracle: all tests passed");
