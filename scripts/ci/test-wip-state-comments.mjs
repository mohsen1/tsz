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

assert.deepEqual(wipStateFindings([{
  number: 789,
  title: "fix(checker): rest timeline shape",
  draft: true,
  labels: [],
  timeline: [{
    event: "convert_to_draft",
    created_at: "2026-05-19T00:00:00Z",
    actor: { login: "mohsen1" },
  }],
  comments: [signedComment()],
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

const fakeLargeGhDir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-wip-state-large-gh-"));
try {
  const fakeGh = path.join(fakeLargeGhDir, "gh");
  fs.writeFileSync(fakeGh, [
    "#!/usr/bin/env node",
    "const largeBody = 'x'.repeat(1024 * 1024 + 1);",
    "console.log(JSON.stringify({ data: { repository: { pullRequests: { nodes: [{",
    "  number: 321,",
    "  title: 'chore(queue): large comment payload',",
    "  isDraft: false,",
    "  labels: { nodes: [] },",
    "  timelineItems: { nodes: [] },",
    "  comments: { nodes: [{ createdAt: '2026-05-19T00:00:00Z', body: largeBody, author: { login: 'mohsen1' } }] },",
    "}] } } } }));",
    "",
  ].join("\n"));
  fs.chmodSync(fakeGh, 0o755);
  const largeResult = spawnSync(process.execPath, [
    SCRIPT,
    "--repository",
    "owner/repo",
    "--max-prs",
    "1",
  ], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${fakeLargeGhDir}${path.delimiter}${process.env.PATH || ""}`,
    },
  });
  assert.equal(largeResult.status, 0, largeResult.stderr);
  assert.match(largeResult.stdout, /No WIP-state comment gaps found/);
} finally {
  fs.rmSync(fakeLargeGhDir, { recursive: true, force: true });
}

const fakeRestFallbackDir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-wip-state-rest-fallback-"));
try {
  const fakeGh = path.join(fakeRestFallbackDir, "gh");
  fs.writeFileSync(fakeGh, [
    "#!/usr/bin/env node",
    "const args = process.argv.slice(2);",
    "const target = args.at(-1) || '';",
    "if (args.includes('graphql')) {",
    "  console.error('{\"errors\":[{\"type\":\"RATE_LIMIT\",\"code\":\"graphql_rate_limit\",\"message\":\"API rate limit already exceeded\"}]}');",
    "  process.exit(1);",
    "}",
    "if (target.includes('/pulls?')) {",
    "  console.log(JSON.stringify([{",
    "    number: 654,",
    "    title: 'fix(queue): rest fallback',",
    "    draft: true,",
    "    labels: [],",
    "  }]));",
    "  process.exit(0);",
    "}",
    "if (target.includes('/timeline?')) {",
    "  console.log(JSON.stringify([{",
    "    event: 'convert_to_draft',",
    "    created_at: '2026-05-19T00:00:00Z',",
    "    actor: { login: 'mohsen1' },",
    "  }]));",
    "  process.exit(0);",
    "}",
    "if (target.includes('/comments?') && target.includes('since=2026-05-19T00%3A00%3A00Z')) {",
    "  console.log(JSON.stringify([{",
    "    created_at: '2026-05-19T01:00:00Z',",
    "    body: 'AgentName: TestAgent\\nReason: branch is blocked.\\nCurrent work: fixing CI.\\nNext action: mark ready after green.',",
    "    user: { login: 'mohsen1' },",
    "  }]));",
    "  process.exit(0);",
    "}",
    "console.error(`unexpected gh target: ${target}`);",
    "process.exit(1);",
    "",
  ].join("\n"));
  fs.chmodSync(fakeGh, 0o755);
  const fallbackResult = spawnSync(process.execPath, [
    SCRIPT,
    "--repository",
    "owner/repo",
    "--max-prs",
    "1",
  ], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${fakeRestFallbackDir}${path.delimiter}${process.env.PATH || ""}`,
    },
  });
  assert.equal(fallbackResult.status, 0, fallbackResult.stderr);
  assert.match(fallbackResult.stdout, /No WIP-state comment gaps found/);
} finally {
  fs.rmSync(fakeRestFallbackDir, { recursive: true, force: true });
}

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
