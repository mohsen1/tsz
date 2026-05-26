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
      labels: [{ name: "WIP" }, { name: "checker" }, { name: "agent:alpha" }],
      body: "AgentName: alpha\n\nRefs #42\n",
    },
    {
      number: 11,
      title: "[WIP] fix(checker): preserve mapped access (#42)",
      isDraft: true,
      baseRefName: "main",
      headRefName: "agent/mapped-b",
      labels: ["WIP", "agent:omega"],
      body: "AgentName: beta\n",
    },
    {
      number: 12,
      title: "refactor(solver): stage relation policy",
      isDraft: false,
      baseRefName: "agent/mapped-a",
      headRefName: "agent/relation-child",
      labels: ["agent:gamma"],
      body: "AgentName: gamma\nDepends on #10\n",
    },
    {
      number: 42,
      title: "fix(checker): preserve mapped access (#42)",
      isDraft: true,
      baseRefName: "main",
      headRefName: "agent/self-ref",
      labels: [],
      body: "AgentName: delta\nSelf-references PR #42 in coordination notes.\n",
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
    {
      number: 15,
      title: "fix(solver): strip optional tuple undefined",
      isDraft: true,
      baseRefName: "main",
      headRefName: "agent/optional-tuple",
      labels: ["agent:delta"],
      body: "AgentName: delta\n\nFixes #9712\n\nCoordination Notes: PR #9826 (Issue #9694) touches the same helper.\n",
    },
    {
      number: 16,
      title: "fix(solver): preserve variadic tuple shape",
      isDraft: true,
      baseRefName: "main",
      headRefName: "agent/variadic-tuple",
      labels: ["agent:zeta"],
      body: "AgentName: zeta\n\nAddresses #9694\n",
    },
    {
      number: 17,
      title: "fix(checker): ready but blocked",
      isDraft: false,
      baseRefName: "main",
      headRefName: "agent/blocked-ready",
      mergeStateStatus: "BLOCKED",
      mergeable: "MERGEABLE",
      autoMergeRequest: null,
      labels: ["agent:epsilon"],
      body: "AgentName: epsilon\n",
    },
  ]);

  const result = spawnSync(process.execPath, [SCRIPT, "--fixture", fixture, "--json", output], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Open PR Ownership Report/);
  assert.match(result.stdout, /AgentName\/label mismatches: 1/);
  assert.match(result.stdout, /agent\/mapped-a: root #10; children #12/);
  assert.match(result.stdout, /unknown-base: unknown root; children #13/);
  assert.match(
    result.stdout,
    /fix\(checker\): preserve mapped access: #10 \(draft, WIP, alpha, stack root\), #11 \(draft, WIP, beta\), #42 \(draft, delta\)/,
  );
  assert.match(
    result.stdout,
    /#42 \(mixed stacked\/unstacked drafts\): PR #10 \(draft, WIP, alpha, stack root\), PR #11 \(draft, WIP, beta\)/,
  );
  assert.match(
    result.stdout,
    /Duplicate Draft Cleanup Targets[\s\S]*#42 \(mixed stacked\/unstacked drafts; unstacked drafts: 1\): PR #10 \(draft, WIP, alpha, stack root\), PR #11 \(draft, WIP, beta\)/,
  );
  assert.doesNotMatch(result.stdout, /#42: PR #10 .*PR #11 .*PR #42/);
  assert.match(result.stdout, /#11: AgentName beta; label agent:omega/);
  assert.match(
    result.stdout,
    /Blocked Ready Main PRs[\s\S]*Owner counts:[\s\S]*agent:epsilon: 1[\s\S]*PRs:[\s\S]*#17: agent:epsilon; MERGEABLE; auto-merge off; fix\(checker\): ready but blocked/,
  );

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.deepEqual(report.counts, {
    open: 9,
    draft: 7,
    ready: 2,
    stacked: 2,
    missingAgentName: 2,
    agentLabelMismatches: 1,
  });
  assert.deepEqual(report.byBase, [
    { base: "agent/mapped-a", prs: [12] },
    { base: "main", prs: [10, 11, 14, 15, 16, 17, 42] },
    { base: "unknown-base", prs: [13] },
  ]);
  assert.deepEqual(report.stacks, [
    { base: "agent/mapped-a", root: 10, children: [12] },
    { base: "unknown-base", root: null, children: [13] },
  ]);
  assert.deepEqual(report.duplicateTitleScopes, [
    { scope: "fix(checker): preserve mapped access", prs: [10, 11, 42] },
  ]);
  assert.deepEqual(report.duplicateIssueRefs, [
    {
      issue: 42,
      prs: [10, 11],
      draftCount: 2,
      stackedDraftCount: 1,
      unstackedDraftCount: 1,
      draftStackState: "mixed stacked/unstacked drafts",
    },
  ]);
  assert.deepEqual(report.duplicateDraftCleanupTargets, [
    {
      issue: 42,
      prs: [10, 11],
      draftCount: 2,
      stackedDraftCount: 1,
      unstackedDraftCount: 1,
      draftStackState: "mixed stacked/unstacked drafts",
    },
  ]);
  assert.deepEqual(report.agentLabelMismatches, [{ number: 11, agentName: "beta", label: "agent:omega" }]);
  assert.deepEqual(report.blockedReadyMainPrs, [
    {
      number: 17,
      agentName: "epsilon",
      agentLabel: "agent:epsilon",
      autoMergeArmed: false,
      mergeable: "MERGEABLE",
      title: "fix(checker): ready but blocked",
    },
  ]);
  assert.deepEqual(report.blockedReadyMainOwnerCounts, [{ owner: "agent:epsilon", count: 1 }]);
  assert.deepEqual(report.prs.find((pr) => pr.number === 42).issueRefs, []);
  assert.deepEqual(report.prs.find((pr) => pr.number === 15).issueRefs, [9694, 9712, 9826]);
  assert.deepEqual(report.prs.find((pr) => pr.number === 15).claimedIssueRefs, [9712]);
  assert.deepEqual(report.prs.find((pr) => pr.number === 16).issueRefs, [9694]);
  assert.deepEqual(report.prs.find((pr) => pr.number === 16).claimedIssueRefs, [9694]);
  assert.deepEqual(report.prs.find((pr) => pr.number === 10).agentLabels, ["alpha"]);
  assert.equal(report.prs.find((pr) => pr.number === 13).agentName, null);
  assert.equal(report.prs.find((pr) => pr.number === 14).agentName, null);
  assert.equal(report.prs.find((pr) => pr.number === 10).stackRole, "stack root");
  assert.equal(report.prs.find((pr) => pr.number === 12).stackRole, "stack child");
  assert.equal(report.prs.find((pr) => pr.number === 11).stackRole, null);
});
