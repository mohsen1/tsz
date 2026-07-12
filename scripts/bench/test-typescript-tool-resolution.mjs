#!/usr/bin/env node

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const BENCH_SCRIPT = path.join(SCRIPT_DIR, "bench-vs-tsgo.sh");
const PREREQS = path.join(SCRIPT_DIR, "lib", "bench-vs-tsgo-prereqs.sh");

function writeJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(value)}\n`);
}

function writeTypeScriptTool(toolDir, packageVersion, reportedVersion = packageVersion) {
  const packageJson = path.join(toolDir, "node_modules", "typescript", "package.json");
  const entry = path.join(toolDir, "node_modules", "typescript", "bin", "tsc");
  writeJson(packageJson, { name: "typescript", version: packageVersion });
  fs.mkdirSync(path.dirname(entry), { recursive: true });
  fs.writeFileSync(entry, `#!/bin/sh\necho 'Version ${reportedVersion}'\n`, { mode: 0o755 });
  fs.chmodSync(entry, 0o755);
  return entry;
}

function runHarness(body, args) {
  const result = spawnSync("bash", ["-c", `set -Eeuo pipefail\nsource "$1"\n${body}`, "bash", PREREQS, ...args], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return String(result.stdout).trim().split("\n").filter(Boolean);
}

const benchSource = fs.readFileSync(BENCH_SCRIPT, "utf8");
assert.doesNotMatch(benchSource, /node_modules\/\.bin\/tsc/);
assert.match(benchSource, /node_modules\/typescript\/bin\/tsc/);
assert.match(
  benchSource,
  /cleanup_benchmark_temp\(\) \{[\s\S]*?rm -rf -- "\$\{TEMP_DIR:\?\}"[\s\S]*?\}/,
);
assert.match(benchSource, /trap cleanup_benchmark_temp EXIT/);
assert.doesNotMatch(benchSource, /trap ["']export_results_json; rm -rf/);

const temp = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-typescript-tool-resolution-"));
try {
  const toolDir = path.join(temp, "custom tool dir");
  const entry = writeTypeScriptTool(toolDir, "7.0.2");

  const valid = runHarness(
    `printf '%s\\n' "$(typescript_tool_entry_path "$2")"
     if typescript_tool_entry_is_valid "$2" "$3"; then echo valid; else echo invalid; fi`,
    [toolDir, "7.0.2"],
  );
  assert.deepEqual(valid, [entry, "valid"]);
  assert.equal(fs.existsSync(path.join(toolDir, "node_modules", ".bin", "tsc")), false);

  writeTypeScriptTool(toolDir, "7.0.1", "7.0.1");
  assert.deepEqual(
    runHarness(
      `if typescript_tool_entry_is_valid "$2" "$3"; then echo valid; else echo invalid; fi`,
      [toolDir, "7.0.2"],
    ),
    ["invalid"],
  );

  writeTypeScriptTool(toolDir, "7.0.2", "7.0.1");
  assert.deepEqual(
    runHarness(
      `if typescript_tool_entry_is_valid "$2" "$3"; then echo valid; else echo invalid; fi`,
      [toolDir, "7.0.2"],
    ),
    ["invalid"],
  );

  writeTypeScriptTool(toolDir, "7.0.2");
  fs.writeFileSync(path.join(toolDir, ".tsgo-spec"), "typescript@7.0.2\n");
  fs.writeFileSync(path.join(toolDir, ".tsc-spec"), "7.0.2\n");
  assert.deepEqual(
    runHarness(
      `RED='' NC=''
       TSGO=''; TSGO_TOOL_DIR="$2"; TSGO_LOCAL_BIN="$(typescript_tool_entry_path "$2")"; TSGO_NPM_SPEC='typescript@7.0.2'
       TSC=''; TSC_TOOL_DIR="$2"; TSC_LOCAL_BIN="$(typescript_tool_entry_path "$2")"; TSC_NPM_SPEC='7.0.2'
       ensure_tsgo
       ensure_tsc
       printf '%s\\n%s\\n' "$TSGO" "$TSC"`,
      [toolDir],
    ),
    [entry, entry],
  );

  assert.deepEqual(
    runHarness(
      `for version in 7.0.2 7.0.0-dev.20260711 7.0.1-rc; do
         typescript_version_is_exact "$version" || exit 1
       done
       for version in latest next '^7.0.0' ''; do
         if typescript_version_is_exact "$version"; then exit 1; fi
       done
       echo exact-version-validation-ok`,
      [],
    ),
    ["exact-version-validation-ok"],
  );
} finally {
  fs.rmSync(temp, { recursive: true, force: true });
}

console.log("test-typescript-tool-resolution: all tests passed");
