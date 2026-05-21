#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { wipStateFindings } from "./check-wip-state-comments.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(ROOT, "scripts", "ci", "check-wip-state-comments.mjs");

function pr(overrides = {}) {
  return {
    number: 123,
    title: "fix(checker): sample",
    draft: false,
    labels: ["WIP"],
    timeline: [
      {
        event: "labeled",
        label: { name: "WIP" },
        created_at: "2026-05-19T00:00:00Z",
        actor: { login: "mohsen1" },
      },
    ],
    comments: [],
    ...overrides,
  };
}

function signedComment(overrides = {}) {
  return {
    created_at: "2026-05-19T01:00:00Z",
    user: { login: "mohsen1" },
    body: [
      "AgentName: TestAgent",
      "Reason: conformance aggregate is red.",
      "Current work: isolate the failing shard.",
      "Next action: push a focused fix.",
    ].join("\n"),
    ...overrides,
  };
}

function withFixture(pullRequests, fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-wip-state-comments-"));
  try {
    const fixture = path.join(dir, "pulls.json");
    fs.writeFileSync(fixture, `${JSON.stringify({ pullRequests })}\n`);
    return fn(fixture);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function runFixture(pullRequests, args = []) {
  return withFixture(pullRequests, (fixture) => spawnSync(process.execPath, [
    SCRIPT,
    "--fixture",
    fixture,
    ...args,
  ], {
    cwd: ROOT,
    encoding: "utf8",
  }));
}

assert.deepEqual(wipStateFindings([pr({ comments: [signedComment()] })]), []);

assert.deepEqual(wipStateFindings([{
  number: 456,
  title: "fix(checker): graph shape",
  isDraft: true,
  labels: { nodes: [] },
  timelineItems: {
    nodes: [{
      __typename: "ConvertToDraftEvent",
      createdAt: "2026-05-19T00:00:00Z",
      actor: { login: "mohsen1" },
    }],
  },
  comments: {
    nodes: [signedComment()],
  },
}]), []);

assert.deepEqual(
  wipStateFindings([pr()]),
  [{
    number: 123,
    title: "fix(checker): sample",
    event: "WIP label",
    eventTime: "2026-05-19T00:00:00Z",
    actor: "mohsen1",
    agentNamePresent: false,
    commentStatus: "missing signed WIP-state comment",
  }],
);

assert.deepEqual(
  wipStateFindings([pr({
    comments: [signedComment({ body: "AgentName: TestAgent\nReason: still investigating." })],
  })]),
  [{
    number: 123,
    title: "fix(checker): sample",
    event: "WIP label",
    eventTime: "2026-05-19T00:00:00Z",
    actor: "mohsen1",
    agentNamePresent: true,
    commentStatus: "signed comment missing reason/blocker/next action",
  }],
);

assert.deepEqual(
  wipStateFindings([pr({
    draft: true,
    labels: [],
    timeline: [],
  })]),
  [],
);

assert.deepEqual(
  wipStateFindings([pr({
    draft: true,
    labels: ["WIP"],
    timeline: [
      {
        event: "labeled",
        label: { name: "WIP" },
        created_at: "2026-05-19T00:00:00Z",
      },
      {
        event: "converted_to_draft",
        created_at: "2026-05-19T02:00:00Z",
      },
    ],
    comments: [signedComment({ created_at: "2026-05-19T01:00:00Z" })],
  })]),
  [{
    number: 123,
    title: "fix(checker): sample",
    event: "converted to draft",
    eventTime: "2026-05-19T02:00:00Z",
    actor: "",
    agentNamePresent: false,
    commentStatus: "missing signed WIP-state comment",
  }],
);

assert.deepEqual(
  wipStateFindings([pr({
    comments: [signedComment({ created_at: "2026-05-20T02:00:00Z" })],
  })], { windowHours: 24 }),
  [{
    number: 123,
    title: "fix(checker): sample",
    event: "WIP label",
    eventTime: "2026-05-19T00:00:00Z",
    actor: "mohsen1",
    agentNamePresent: false,
    commentStatus: "missing signed WIP-state comment",
  }],
);

assert.deepEqual(
  wipStateFindings([pr({
    labels: ["WIP"],
    timeline: [],
  })]),
  [{
    number: 123,
    title: "fix(checker): sample",
    event: "WIP label",
    eventTime: "not found in latest timeline page",
    actor: "",
    agentNamePresent: false,
    commentStatus: "missing timeline event",
  }],
);

const advisory = runFixture([pr()]);
assert.equal(advisory.status, 0, advisory.stderr);
assert.match(advisory.stdout, /WIP State Comment Advisory/);
assert.match(advisory.stdout, /missing signed WIP-state comment/);
assert.match(advisory.stdout, /^\| #123 \| fix\(checker\): sample \| WIP label \|/m);

const enforce = runFixture([pr()], ["--enforce"]);
assert.equal(enforce.status, 1, enforce.stdout);
assert.match(enforce.stdout, /missing signed WIP-state comment/);

const clean = runFixture([pr({ comments: [signedComment()] })], ["--enforce"]);
assert.equal(clean.status, 0, clean.stderr);
assert.match(clean.stdout, /No WIP-state comment gaps found/);

const fakeGhDir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-wip-state-fake-gh-"));
try {
  const fakeGh = path.join(fakeGhDir, "gh");
  fs.writeFileSync(fakeGh, "#!/usr/bin/env bash\nsleep 5\n");
  fs.chmodSync(fakeGh, 0o755);
  const timeoutResult = spawnSync(process.execPath, [
    SCRIPT,
    "--repository",
    "owner/repo",
    "--max-prs",
    "1",
    "--gh-timeout-ms",
    "50",
  ], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${fakeGhDir}${path.delimiter}${process.env.PATH || ""}`,
    },
  });
  assert.equal(timeoutResult.status, 1, timeoutResult.stdout);
  assert.match(timeoutResult.stderr, /timed out after 50ms/);
} finally {
  fs.rmSync(fakeGhDir, { recursive: true, force: true });
}
