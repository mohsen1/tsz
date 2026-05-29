#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { selectLatestBenchmarkArtifact } from "./benchmark-artifact-selection.mjs";

const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-bench-artifacts-"));

function writeArtifact(name, generatedAt, results = [{ name: "row", tsz_ms: 1, tsgo_ms: 2 }]) {
  const file = path.join(tempDir, name);
  fs.writeFileSync(file, `${JSON.stringify({ generated_at: generatedAt, results })}\n`);
  return file;
}

try {
  const snapshot = writeArtifact("bench-snapshot.json", "2026-05-17T01:23:02.991Z");
  const github = writeArtifact("bench-vs-tsgo-github-latest.json", "2026-05-28T02:14:24.444Z");
  const gcs = writeArtifact("bench-vs-tsgo-gcs-latest.json", "2026-05-29T02:14:24.444Z");
  const empty = writeArtifact("empty.json", "2026-06-01T00:00:00.000Z", []);

  assert.equal(
    selectLatestBenchmarkArtifact([snapshot, github, gcs])?.file,
    gcs,
    "newer GCS benchmark truth should beat older GitHub and snapshot files",
  );
  assert.equal(
    selectLatestBenchmarkArtifact([gcs, github, snapshot])?.file,
    gcs,
    "candidate order should not override generated_at freshness",
  );
  assert.equal(
    selectLatestBenchmarkArtifact([empty, github])?.file,
    github,
    "empty benchmark JSON should not mask the latest usable artifact",
  );
  assert.equal(
    selectLatestBenchmarkArtifact([path.join(tempDir, "missing.json")]),
    null,
    "missing candidates should produce no selected artifact",
  );
} finally {
  fs.rmSync(tempDir, { recursive: true, force: true });
}

console.log("benchmark artifact selection tests passed");
