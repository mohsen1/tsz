#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { readyStateFailures } from "./check-pr-ready-state.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(ROOT, "scripts", "ci", "check-pr-ready-state.mjs");

function readyPr(overrides = {}) {
  return {
    number: 123,
    title: "fix(checker): sample",
    body: "AgentName: TestAgent\n\n## Summary\nReady for review.\n",
    draft: false,
    labels: [],
    ...overrides,
  };
}

function withFixture(pr, fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-pr-ready-state-"));
  try {
    const fixture = path.join(dir, "pr.json");
    fs.writeFileSync(fixture, `${JSON.stringify(pr)}\n`);
    return fn(fixture);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function runFixture(pr) {
  return withFixture(pr, (fixture) => spawnSync(process.execPath, [SCRIPT, "--fixture", fixture], {
    cwd: ROOT,
    encoding: "utf8",
  }));
}

assert.deepEqual(readyStateFailures(readyPr()), []);

assert.deepEqual(
  readyStateFailures(readyPr({ labels: ["WIP"] })),
  ["WIP label"],
);

assert.deepEqual(
  readyStateFailures(readyPr({ title: "[WIP] fix(checker): sample" })),
  ["[WIP] title marker"],
);

assert.deepEqual(
  readyStateFailures(readyPr({ body: "AgentName: TestAgent\n\nStatus: WIP pending verification\n" })),
  ["body WIP status line"],
);

assert.deepEqual(
  readyStateFailures(readyPr({ body: "AgentName: TestAgent\n\nBlocker: conformance aggregate is red\n" })),
  ["body blocker declaration"],
);

assert.deepEqual(
  readyStateFailures(readyPr({
    body: "AgentName: TestAgent\n\nThis PR is blocked on lint while the branch is reviewed.\n",
  })),
  ["body WIP declaration"],
);

assert.deepEqual(
  readyStateFailures(readyPr({
    body: "AgentName: TestAgent\n\nThis PR fixes a bug where ready-state checks were blocked by stale WIP labels.\n",
  })),
  [],
);

assert.deepEqual(
  readyStateFailures(readyPr({
    body: "AgentName: TestAgent\n\nThis branch removes the blocker from the project-corpus gate.\n",
  })),
  [],
);

assert.deepEqual(
  readyStateFailures(readyPr({
    draft: true,
    labels: ["WIP"],
    title: "[WIP] fix(checker): sample",
    body: "AgentName: TestAgent\n\nStatus: WIP pending verification\n",
  })),
  [],
);

const failing = runFixture(readyPr({
  labels: ["WIP"],
  title: "[WIP] fix(checker): sample",
  body: "AgentName: TestAgent\n\nReadiness: blocked on lint\n",
}));
assert.equal(failing.status, 1, failing.stderr);
assert.match(failing.stderr, /Ready PRs must not carry WIP status/);
assert.match(failing.stderr, /WIP label/);
assert.match(failing.stderr, /\[WIP\] title marker/);
assert.match(failing.stderr, /body WIP status line/);
assert.match(failing.stderr, /Repair: remove WIP labels/);

const passingDraft = runFixture(readyPr({
  draft: true,
  labels: ["WIP"],
  title: "[WIP] fix(checker): sample",
}));
assert.equal(passingDraft.status, 0, passingDraft.stderr);
assert.match(passingDraft.stdout, /Ready-state WIP check passed/);
