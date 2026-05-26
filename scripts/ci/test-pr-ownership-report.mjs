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
    {
      number: 18,
      title: "fix(solver): conflicting ready branch",
      isDraft: false,
      baseRefName: "main",
      headRefName: "agent/conflicting-ready",
      mergeStateStatus: "DIRTY",
      mergeable: "CONFLICTING",
      autoMergeRequest: null,
      labels: ["agent:zeta"],
      body: "AgentName: zeta\n",
    },
    {
      number: 19,
      title: "fix(checker): conflicting draft branch",
      isDraft: true,
      baseRefName: "main",
      headRefName: "agent/conflicting-draft",
      mergeStateStatus: "DIRTY",
      mergeable: "CONFLICTING",
      autoMergeRequest: null,
      labels: ["agent:delta"],
      body: "AgentName: delta\n",
    },
  ]);

  const result = spawnSync(process.execPath, [SCRIPT, "--fixture", fixture, "--json", output], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Open PR Ownership Report/);
  assert.match(result.stdout, /AgentName\/label mismatches: 1/);
  assert.match(
    result.stdout,
    /Owner Summary[\s\S]*\| agent:delta \| 2 \| 0 \| 2 \| 0 \| 0 \| 0 \| 0 \| 1 \| 0 \|[\s\S]*\| agent:zeta \| 2 \| 1 \| 1 \| 0 \| 0 \| 0 \| 1 \| 1 \| 0 \|[\s\S]*\| unowned \| 2 \| 0 \| 2 \| 0 \| 1 \| 0 \| 0 \| 0 \| 0 \|[\s\S]*\| delta \| 1 \| 0 \| 1 \| 0 \| 0 \| 0 \| 0 \| 0 \| 0 \|/,
  );
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
  assert.match(
    result.stdout,
    /Conflicting Main PRs[\s\S]*Owner counts:[\s\S]*agent:delta: 1[\s\S]*agent:zeta: 1[\s\S]*PRs:[\s\S]*#19: agent:delta; draft; DIRTY; CONFLICTING; auto-merge off; fix\(checker\): conflicting draft branch[\s\S]*#18: agent:zeta; ready; DIRTY; CONFLICTING; auto-merge off; fix\(solver\): conflicting ready branch/,
  );
  assert.match(
    result.stdout,
    /Conflicting Ready Main PRs[\s\S]*Owner counts:[\s\S]*agent:zeta: 1[\s\S]*PRs:[\s\S]*#18: agent:zeta; DIRTY; CONFLICTING; auto-merge off; fix\(solver\): conflicting ready branch/,
  );
  assert.match(
    result.stdout,
    /WIP PRs[\s\S]*Owner counts:[\s\S]*agent:alpha: 1[\s\S]*agent:omega: 1[\s\S]*PRs:[\s\S]*#10: agent:alpha; draft; label; stack root; fix\(checker\): preserve mapped access \(#42\)[\s\S]*#11: agent:omega; draft; label\+title; \[WIP\] fix\(checker\): preserve mapped access \(#42\)/,
  );

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.deepEqual(report.counts, {
    open: 11,
    draft: 8,
    ready: 3,
    stacked: 2,
    missingAgentName: 2,
    agentLabelMismatches: 1,
  });
  assert.deepEqual(report.byBase, [
    { base: "agent/mapped-a", prs: [12] },
    { base: "main", prs: [10, 11, 14, 15, 16, 17, 18, 19, 42] },
    { base: "unknown-base", prs: [13] },
  ]);
  assert.deepEqual(report.ownerSummaries, [
    {
      owner: "agent:delta",
      open: 2,
      ready: 0,
      draft: 2,
      wip: 0,
      stackedChildren: 0,
      blockedReadyMain: 0,
      conflictingReadyMain: 0,
      conflictingMain: 1,
      autoMergeArmed: 0,
    },
    {
      owner: "agent:zeta",
      open: 2,
      ready: 1,
      draft: 1,
      wip: 0,
      stackedChildren: 0,
      blockedReadyMain: 0,
      conflictingReadyMain: 1,
      conflictingMain: 1,
      autoMergeArmed: 0,
    },
    {
      owner: "unowned",
      open: 2,
      ready: 0,
      draft: 2,
      wip: 0,
      stackedChildren: 1,
      blockedReadyMain: 0,
      conflictingReadyMain: 0,
      conflictingMain: 0,
      autoMergeArmed: 0,
    },
    {
      owner: "agent:alpha",
      open: 1,
      ready: 0,
      draft: 1,
      wip: 1,
      stackedChildren: 0,
      blockedReadyMain: 0,
      conflictingReadyMain: 0,
      conflictingMain: 0,
      autoMergeArmed: 0,
    },
    {
      owner: "agent:epsilon",
      open: 1,
      ready: 1,
      draft: 0,
      wip: 0,
      stackedChildren: 0,
      blockedReadyMain: 1,
      conflictingReadyMain: 0,
      conflictingMain: 0,
      autoMergeArmed: 0,
    },
    {
      owner: "agent:gamma",
      open: 1,
      ready: 1,
      draft: 0,
      wip: 0,
      stackedChildren: 1,
      blockedReadyMain: 0,
      conflictingReadyMain: 0,
      conflictingMain: 0,
      autoMergeArmed: 0,
    },
    {
      owner: "agent:omega",
      open: 1,
      ready: 0,
      draft: 1,
      wip: 1,
      stackedChildren: 0,
      blockedReadyMain: 0,
      conflictingReadyMain: 0,
      conflictingMain: 0,
      autoMergeArmed: 0,
    },
    {
      owner: "delta",
      open: 1,
      ready: 0,
      draft: 1,
      wip: 0,
      stackedChildren: 0,
      blockedReadyMain: 0,
      conflictingReadyMain: 0,
      conflictingMain: 0,
      autoMergeArmed: 0,
    },
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
  assert.deepEqual(report.conflictingMainPrs, [
    {
      number: 19,
      draft: true,
      agentName: "delta",
      agentLabel: "agent:delta",
      autoMergeArmed: false,
      mergeStateStatus: "DIRTY",
      mergeable: "CONFLICTING",
      title: "fix(checker): conflicting draft branch",
    },
    {
      number: 18,
      draft: false,
      agentName: "zeta",
      agentLabel: "agent:zeta",
      autoMergeArmed: false,
      mergeStateStatus: "DIRTY",
      mergeable: "CONFLICTING",
      title: "fix(solver): conflicting ready branch",
    },
  ]);
  assert.deepEqual(report.conflictingMainOwnerCounts, [
    { owner: "agent:delta", count: 1 },
    { owner: "agent:zeta", count: 1 },
  ]);
  assert.deepEqual(report.conflictingReadyMainPrs, [
    {
      number: 18,
      draft: false,
      agentName: "zeta",
      agentLabel: "agent:zeta",
      autoMergeArmed: false,
      mergeStateStatus: "DIRTY",
      mergeable: "CONFLICTING",
      title: "fix(solver): conflicting ready branch",
    },
  ]);
  assert.deepEqual(report.conflictingReadyMainOwnerCounts, [{ owner: "agent:zeta", count: 1 }]);
  assert.deepEqual(report.wipPrs, [
    {
      number: 10,
      draft: true,
      agentName: "alpha",
      agentLabel: "agent:alpha",
      base: "main",
      stackRole: "stack root",
      markers: ["label"],
      title: "fix(checker): preserve mapped access (#42)",
    },
    {
      number: 11,
      draft: true,
      agentName: "beta",
      agentLabel: "agent:omega",
      base: "main",
      stackRole: null,
      markers: ["label", "title"],
      title: "[WIP] fix(checker): preserve mapped access (#42)",
    },
  ]);
  assert.deepEqual(report.wipOwnerCounts, [
    { owner: "agent:alpha", count: 1 },
    { owner: "agent:omega", count: 1 },
  ]);
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
