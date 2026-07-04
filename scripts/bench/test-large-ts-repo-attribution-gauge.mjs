#!/usr/bin/env node
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const gauge = path.join(scriptDir, "large-ts-repo-attribution-gauge.sh");
const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-large-ts-gauge-"));

function runGauge(args) {
  return spawnSync("bash", [gauge, ...args], {
    cwd: path.resolve(scriptDir, "..", ".."),
    encoding: "utf8",
  });
}

function runBash(script) {
  return spawnSync("bash", ["-lc", script], {
    cwd: path.resolve(scriptDir, "..", ".."),
    encoding: "utf8",
  });
}

try {
  const fixtureDir = path.join(tmpDir, "fixture");
  fs.mkdirSync(path.join(fixtureDir, ".git"), { recursive: true });
  fs.mkdirSync(path.join(fixtureDir, "packages"), { recursive: true });
  const tsconfig = path.join(fixtureDir, "tsconfig.flat.bench.json");
  fs.writeFileSync(tsconfig, "{}\n", "utf8");
  const fakeTsz = path.join(tmpDir, "tsz");
  fs.writeFileSync(fakeTsz, "#!/bin/sh\nexit 0\n", { mode: 0o755 });

  const jsonFile = path.join(tmpDir, "plan.json");
  const result = runGauge([
    "--json-file", jsonFile,
    "--fixture-dir", fixtureDir,
    "--tsz-bin", fakeTsz,
    "--iterations", "3",
    "--top", "7",
    "--timeout", "9",
    "--runs", "2",
  ]);
  assert.equal(result.status, 0, `plan-only gauge failed:\n${result.stderr}\n${result.stdout}`);
  assert.match(result.stdout, /large-ts-repo attribution plan written/);

  const plan = JSON.parse(fs.readFileSync(jsonFile, "utf8"));
  assert.equal(plan.schema_version, 1);
  assert.equal(plan.row, "large-ts-repo");
  assert.equal(plan.fixture.ready, true);
  assert.equal(plan.fixture.tsconfig, tsconfig);
  assert.equal(plan.environment.tsz_bin, fakeTsz);
  assert.equal(plan.environment.tsz_bin_exists, true);
  assert.equal(plan.settings.iterations, 3);
  assert.equal(plan.settings.top, 7);
  assert.equal(plan.settings.timeout_s, 9);
  assert.equal(plan.settings.runs, 2);
  assert.equal(plan.artifacts.bench_json, path.join(tmpDir, "plan.bench.json"));
  assert.equal(plan.artifacts.measure_json, path.join(tmpDir, "plan.measure.json"));
  assert.equal(plan.artifacts.profile_json, path.join(tmpDir, "plan.profile.json"));
  assert.equal(plan.run.measure_requested, false);
  assert.equal(plan.run.profile_requested, false);
  assert.deepEqual(plan.commands.bench_row.slice(0, 4), [
    "scripts/bench/bench-vs-tsgo.sh",
    "--quick",
    "--filter",
    "^large-ts-repo$",
  ]);
  assert.ok(plan.commands.measure.includes("scripts/bench/measure-tsz.sh"));
  assert.ok(plan.commands.measure.includes("--noEmit"));
  assert.ok(plan.commands.profile.includes("scripts/bench/perf-flat-profile.sh"));
  assert.ok(plan.commands.profile.includes("--json-file"));
  assert.ok(plan.commands.profile.includes(path.join(tmpDir, "plan.profile.json")));
  assert.ok(plan.commands.profile.includes("--no-build"));

  const missingFixture = path.join(tmpDir, "missing-fixture");
  const missingJson = path.join(tmpDir, "missing.json");
  const missing = runGauge([
    "--json-file", missingJson,
    "--fixture-dir", missingFixture,
    "--tsz-bin", fakeTsz,
  ]);
  assert.equal(missing.status, 0, `missing-fixture plan should not run heavy setup:\n${missing.stderr}`);
  const missingPlan = JSON.parse(fs.readFileSync(missingJson, "utf8"));
  assert.equal(missingPlan.fixture.ready, false);
  assert.equal(missingPlan.fixture.tsconfig, null);
  assert.equal(missingPlan.artifacts.measure_json, null);
  assert.equal(missingPlan.artifacts.profile_json, null);
  assert.equal(missingPlan.commands.measure, null);
  assert.equal(missingPlan.commands.profile, null);

  const generatedFixture = path.join(tmpDir, "generated");
  fs.mkdirSync(path.join(generatedFixture, "packages"), { recursive: true });
  fs.writeFileSync(path.join(generatedFixture, "tsconfig.base.json"), "{}\n", "utf8");
  const helperResult = runBash(`
set -euo pipefail
source scripts/bench/lib/large-ts-repo-fixture.sh
tsz_large_ts_repo_write_flat_tsconfig "${generatedFixture}"
tsz_large_ts_repo_select_tsconfig "${generatedFixture}"
`);
  assert.equal(helperResult.status, 0, helperResult.stderr);
  assert.equal(
    helperResult.stdout.trim(),
    path.join(generatedFixture, "tsconfig.flat.json"),
    "helper should select generated flat tsconfig when fixture lacks tsconfig.flat.bench.json",
  );
  const generatedConfig = fs.readFileSync(path.join(generatedFixture, "tsconfig.flat.json"), "utf8");
  assert.match(generatedConfig, /"extends": "\.\/tsconfig\.base\.json"/);
  assert.match(generatedConfig, /"include": \["packages\/\*\*\/src\/\*\*\/\*\.ts"\]/);
} finally {
  fs.rmSync(tmpDir, { recursive: true, force: true });
}

console.log("large-ts-repo attribution gauge tests passed");
