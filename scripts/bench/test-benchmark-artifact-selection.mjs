#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { selectLatestBenchmarkArtifact } from "./benchmark-artifact-selection.mjs";
import { PROJECT_ROW_DEFINITIONS } from "./project-rows.mjs";
import { GREEN_COMPAT } from "./row-utils.mjs";

const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-bench-artifacts-"));

function writeArtifact(name, generatedAt, results = [{ name: "row", tsz_ms: 1, tsgo_ms: 2 }]) {
  const file = path.join(tempDir, name);
  fs.writeFileSync(file, `${JSON.stringify({ generated_at: generatedAt, results })}\n`);
  return file;
}

function projectRow(name, state = "green") {
  const ok = state === "green";
  return {
    name,
    tsz_ms: ok ? 10 : null,
    tsgo_ms: ok ? 20 : null,
    winner: ok ? "tsz" : "error",
    status: ok ? null : "tsz error; tsc ok",
    compatibility: {
      ...(ok ? GREEN_COMPAT : {}),
      state,
      phase: "check",
      last_successful_phase: ok ? "check" : null,
      exit_class: ok ? "exit success" : "crash",
      diagnostic_status: ok ? "none" : "compiler crashed",
    },
  };
}

function applicationCompatibilityRows() {
  return PROJECT_ROW_DEFINITIONS
    .filter((row) => row.category === "application")
    .map((row, index) => ({
      name: row.name,
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
      status: "compile canary tracked in CI; not timed by vs-tsgo benchmarks",
      compatibility: {
        ...(index === 0 ? GREEN_COMPAT : {}),
        state: index === 0 ? "green" : index === 1 ? "yellow" : "red",
        phase: "check",
        last_successful_phase: index === 0 ? "check" : null,
        exit_class: index === 0 ? "exit success" : index === 1 ? "nonzero exit" : "timeout",
        diagnostic_status: index === 0 ? "none" : index === 1 ? "diagnostic mismatch" : "compiler timed out",
      },
    }));
}

function applicationTimedRows() {
  return PROJECT_ROW_DEFINITIONS
    .filter((row) => row.category === "application")
    .map((row) => projectRow(row.name, "green"));
}

try {
  const snapshot = writeArtifact("bench-snapshot.json", "2026-05-17T01:23:02.991Z");
  const github = writeArtifact("bench-vs-tsgo-github-latest.json", "2026-05-28T02:14:24.444Z");
  const local = writeArtifact("bench-vs-tsgo-local-latest.json", "2026-05-29T02:14:24.444Z");
  const empty = writeArtifact("empty.json", "2026-06-01T00:00:00.000Z", []);

  assert.equal(
    selectLatestBenchmarkArtifact([snapshot, github, local])?.file,
    local,
    "newer benchmark artifact should beat older GitHub and snapshot files",
  );
  assert.equal(
    selectLatestBenchmarkArtifact([local, github, snapshot])?.file,
    local,
    "candidate order should not override generated_at freshness",
  );
  assert.equal(
    selectLatestBenchmarkArtifact([empty, github])?.file,
    github,
    "empty benchmark JSON should not mask the latest usable artifact",
  );
  const goodProject = writeArtifact("good-project.json", "2026-05-30T00:00:00.000Z", [
    projectRow("utility-types-project"),
    { name: "micro", tsz_ms: 1, tsgo_ms: 2, winner: "tsz" },
  ]);
  const badProject = writeArtifact("bad-project.json", "2026-06-01T00:00:00.000Z", [
    projectRow("utility-types-project", "red"),
    { name: "micro", tsz_ms: 1, tsgo_ms: 2, winner: "tsz" },
  ]);
  assert.equal(
    selectLatestBenchmarkArtifact([goodProject, badProject], { minimumProjectTimingPairs: 1 })?.file,
    goodProject,
    "a newer artifact with no successful project timings should not mask older public project timing data",
  );
  const forgedStubbedProject = writeArtifact("forged-stubbed-project.json", "2026-06-01T01:00:00.000Z", [
    projectRow("msw-project"),
  ]);
  assert.equal(
    selectLatestBenchmarkArtifact(
      [goodProject, forgedStubbedProject],
      { minimumProjectTimingPairs: 1 },
    )?.file,
    goodProject,
    "forged zero stub fields cannot make an MSW timing artifact selectable",
  );
  const olderWithApplications = writeArtifact("older-with-applications.json", "2026-06-02T00:00:00.000Z", [
    projectRow("utility-types-project"),
    ...applicationCompatibilityRows(),
  ]);
  const newerWithoutApplications = writeArtifact("newer-without-applications.json", "2026-06-03T00:00:00.000Z", [
    projectRow("utility-types-project"),
  ]);
  assert.equal(
    selectLatestBenchmarkArtifact(
      [newerWithoutApplications, olderWithApplications],
      { minimumProjectTimingPairs: 1, requireApplicationCompat: true },
    )?.file,
    olderWithApplications,
    "a newer artifact without application compatibility should not mask older app-compatible benchmark data",
  );
  const olderWithApplicationTimings = writeArtifact("older-with-application-timings.json", "2026-06-04T00:00:00.000Z", [
    projectRow("utility-types-project"),
    ...applicationTimedRows(),
  ]);
  const newerWithGreenCompileOnlyApplications = writeArtifact(
    "newer-with-green-compile-only-applications.json",
    "2026-06-05T00:00:00.000Z",
    [
      projectRow("utility-types-project"),
      ...applicationCompatibilityRows(),
    ],
  );
  assert.equal(
    selectLatestBenchmarkArtifact(
      [olderWithApplicationTimings, newerWithGreenCompileOnlyApplications],
      {
        minimumProjectTimingPairs: 1,
        requireApplicationCompat: true,
        requireGreenProjectTimingPairs: true,
      },
    )?.file,
    olderWithApplicationTimings,
    "a newer artifact with green compile-only app rows should not mask older app chart data",
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
