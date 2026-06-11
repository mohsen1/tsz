#!/usr/bin/env node
// End-to-end tests for scripts/bench/measure-tsz.sh (issue #13174): the
// protocol runner must snapshot the binary before measuring, report wall and
// CPU time per run, and classify wall timeouts by CPU share so contended runs
// surface as unmeasured (exit 125) instead of as a phantom regression.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const dirname = path.dirname(fileURLToPath(import.meta.url));
const measure = path.join(dirname, "measure-tsz.sh");

const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-measure-tsz-"));
const snapDir = path.join(tmpDir, "snapshots");

function writeFakeBin(name, body) {
  const p = path.join(tmpDir, name);
  fs.writeFileSync(p, `#!/bin/sh\n${body}\n`, { mode: 0o755 });
  return p;
}

function runMeasure(args) {
  return spawnSync("bash", [measure, "--snapshot-dir", snapDir, ...args], {
    encoding: "utf8",
  });
}

try {
  // --- measured run: JSON artifact, snapshot isolation, child exit preserved as data ---
  {
    const bin = writeFakeBin("fake-ok", "echo diagnostics; exit 3");
    const jsonFile = path.join(tmpDir, "ok.json");
    const result = runMeasure([
      "--bin", bin,
      "--timeout", "30",
      "--runs", "2",
      "--json-file", jsonFile,
      "--label", "ok-case",
      "--", "--noEmit", "-p", "x.json",
    ]);
    assert.equal(result.status, 0, `measured run should exit 0: ${result.stderr}\n${result.stdout}`);

    const artifact = JSON.parse(fs.readFileSync(jsonFile, "utf8"));
    assert.equal(artifact.protocol, "snapshot+cpu-share/v1");
    assert.equal(artifact.label, "ok-case");
    assert.match(artifact.binary.sha256, /^[0-9a-f]{64}$/);
    assert.notEqual(artifact.binary.snapshot, bin, "must measure the snapshot, not the live path");
    assert.deepEqual(artifact.command, ["--noEmit", "-p", "x.json"]);
    assert.equal(artifact.runs.length, 2);
    for (const run of artifact.runs) {
      assert.equal(run.classification, "measured");
      assert.equal(run.exit_code, 3, "the child's own exit code is data, not failure");
      assert.equal(typeof run.wall_s, "number");
      assert.equal(typeof run.cpu_s, "number");
    }
    assert.deepEqual(artifact.summary, { measured: 2, timeout_cpu_bound: 0, unmeasured: 0 });

    // The hazard from the issue: the live binary is overwritten after the
    // snapshot. The snapshot recorded in the artifact must still be the
    // original content.
    fs.writeFileSync(bin, "#!/bin/sh\nexit 99\n", { mode: 0o755 });
    const snapshotRun = spawnSync(artifact.binary.snapshot, [], { encoding: "utf8" });
    assert.equal(snapshotRun.status, 3, "snapshot must be immune to live-path overwrites");
  }

  // --- idle timeout: contended, unmeasured, exit 125 ---
  {
    const bin = writeFakeBin("fake-idle", "sleep 30");
    const jsonFile = path.join(tmpDir, "idle.json");
    const result = runMeasure(["--bin", bin, "--timeout", "1", "--json-file", jsonFile, "--", "x"]);
    assert.equal(result.status, 125, `contended timeout should exit 125: ${result.stdout}`);
    assert.match(result.stderr, /UNMEASURED/, result.stderr);

    const artifact = JSON.parse(fs.readFileSync(jsonFile, "utf8"));
    assert.match(artifact.runs[0].classification, /^unmeasured-/);
    assert.equal(artifact.summary.unmeasured, 1);
  }

  // --- busy timeout: CPU-bound, genuine slowness, exit 124 ---
  {
    const bin = writeFakeBin("fake-busy", "while :; do :; done");
    const jsonFile = path.join(tmpDir, "busy.json");
    const result = runMeasure(["--bin", bin, "--timeout", "3", "--json-file", jsonFile, "--", "x"]);
    assert.equal(result.status, 124, `cpu-bound timeout should exit 124: ${result.stdout}`);

    const artifact = JSON.parse(fs.readFileSync(jsonFile, "utf8"));
    assert.equal(artifact.runs[0].classification, "timeout-cpu-bound");
    assert.equal(artifact.runs[0].exit_code, 124);
    assert.equal(artifact.summary.timeout_cpu_bound, 1);
  }

  // --- usage errors ---
  {
    const noCommand = runMeasure(["--bin", "/bin/sh"]);
    assert.equal(noCommand.status, 2, "missing -- command must be a usage error");
    const missingBin = runMeasure(["--bin", path.join(tmpDir, "absent"), "--", "x"]);
    assert.equal(missingBin.status, 2, "missing binary must be a setup error");
  }
} finally {
  fs.rmSync(tmpDir, { recursive: true, force: true });
}

console.log("measure-tsz tests passed");
