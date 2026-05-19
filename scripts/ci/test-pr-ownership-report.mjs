#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(ROOT, "scripts", "ci", "pr-ownership-report.mjs");

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-pr-ownership-"));
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function writeJson(file, value) {
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

withTempDir((dir) => {
  const fixture = path.join(dir, "prs.json");
  const output = path.join(dir, "report.json");
  writeJson(fixture, [
    {
      number: 10,
      title: "fix(checker): preserve mapped access (#42)",
      isDraft: true,
      baseRefName: "main",
      headRefName: "agent/mapped-a",
      labels: [{ name: "WIP" }, { name: "checker" }],
      body: "AgentName: alpha\n\nRefs #42\n",
    },
    {
      number: 11,
      title: "[WIP] fix(checker): preserve mapped access (#42)",
      isDraft: true,
      baseRefName: "main",
      headRefName: "agent/mapped-b",
      labels: ["WIP"],
      body: "AgentName: beta\n",
    },
    {
      number: 12,
      title: "refactor(solver): stage relation policy",
      isDraft: false,
      baseRefName: "agent/mapped-a",
      headRefName: "agent/relation-child",
      labels: [],
      body: "AgentName: gamma\nDepends on #10\n",
    },
    {
      number: 13,
      title: "docs: update note",
      isDraft: true,
      baseRefName: "unknown-base",
      headRefName: "agent/docs",
      labels: [],
      body: "",
    },
    {
      number: 14,
      title: "chore(ci): blank agent metadata",
      isDraft: true,
      baseRefName: "main",
      headRefName: "agent/blank-agent",
      labels: [],
      body: "AgentName:\n\n## Track\ncoordination\n",
    },
  ]);

  const result = spawnSync(process.execPath, [SCRIPT, "--fixture", fixture, "--json", output], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Open PR Ownership Report/);
  assert.match(result.stdout, /agent\/mapped-a: root #10; children #12/);
  assert.match(result.stdout, /unknown-base: unknown root; children #13/);
  assert.match(result.stdout, /fix\(checker\): preserve mapped access: #10, #11/);
  assert.match(result.stdout, /#42: PR #10, PR #11/);

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.deepEqual(report.counts, {
    open: 5,
    draft: 4,
    ready: 1,
    stacked: 2,
    missingAgentName: 2,
  });
  assert.deepEqual(report.byBase, [
    { base: "agent/mapped-a", prs: [12] },
    { base: "main", prs: [10, 11, 14] },
    { base: "unknown-base", prs: [13] },
  ]);
  assert.deepEqual(report.stacks, [
    { base: "agent/mapped-a", root: 10, children: [12] },
    { base: "unknown-base", root: null, children: [13] },
  ]);
  assert.deepEqual(report.duplicateTitleScopes, [
    { scope: "fix(checker): preserve mapped access", prs: [10, 11] },
  ]);
  assert.deepEqual(report.duplicateIssueRefs, [
    { issue: 42, prs: [10, 11] },
  ]);
  assert.equal(report.prs.find((pr) => pr.number === 13).agentName, null);
  assert.equal(report.prs.find((pr) => pr.number === 14).agentName, null);
});
