#!/usr/bin/env node
import assert from "node:assert/strict";
import {
  activeBranchQueueRun,
  activeSyntheticQueueRun,
  failureCommentBody,
  formatResult,
  hasPendingPlaceholderQueueStatus,
  pendingQueueRun,
  parseArgs,
  queueBranchMetadata,
  queueBranchPrNumber,
  queueRunIsActive,
  queueSkipReason,
  requiredCheckState,
  supersededOpenQueueBranchReason,
} from "./poor-mans-merge-queue.mjs";

function check(overrides = {}) {
  return {
    __typename: "CheckRun",
    name: "CI Summary",
    status: "COMPLETED",
    conclusion: "SUCCESS",
    ...overrides,
  };
}

function pr(overrides = {}) {
  return {
    autoMergeRequest: { mergeMethod: "SQUASH" },
    baseRefName: "main",
    body: "AgentName: TestAgent\n\nReady.",
    headRefOid: "a".repeat(40),
    isCrossRepository: false,
    isDraft: false,
    labels: [],
    number: 42,
    statusCheckRollup: [check(), check({ name: "GitGuardian Security Checks" })],
    title: "fix(ci): sample",
    url: "https://github.example/pull/42",
    ...overrides,
  };
}

const originalAgentName = process.env.AGENT_NAME;
delete process.env.AGENT_NAME;
assert.equal(parseArgs(["--repository", "owner/repo"]).repository, "owner/repo");
assert.equal(parseArgs(["--repository", "owner/repo"]).agentName, "M1-A");
if (originalAgentName === undefined) {
  delete process.env.AGENT_NAME;
} else {
  process.env.AGENT_NAME = originalAgentName;
}
assert.equal(parseArgs(["--repository", "owner/repo", "--agent-name", "M4-B"]).agentName, "M4-B");
assert.equal(parseArgs(["--repository", "owner/repo", "--cleanup-queue-branches"]).cleanupQueueBranches, true);
assert.equal(
  parseArgs(["--repository", "owner/repo", "--cleanup-superseded-open-queue-branches"]).cleanupSupersededOpenQueueBranches,
  true,
);
assert.deepEqual(
  parseArgs(["--no-default-pr-required-checks", "--pr-required-check", "lint"]).prRequiredChecks,
  ["lint"],
);
assert.deepEqual(
  parseArgs(["--no-default-merge-required-checks", "--merge-required-check", "CI Summary"]).mergeRequiredChecks,
  ["CI Summary"],
);
assert.equal(parseArgs(["--invalidate-pr", "123"]).invalidatePr, 123);
assert.equal(queueBranchPrNumber("automation/merge-queue/pr-123"), 123);
assert.equal(queueBranchPrNumber("automation/merge-queue/pr-123-extra"), 123);
assert.equal(queueBranchPrNumber("automation/merge-queue/pr-123-a56115a-m4c"), 123);
assert.equal(queueBranchPrNumber("automation/merge-queue/pr-123/extra"), null);
assert.equal(queueBranchPrNumber("automation/merge-queue/not-pr-123"), null);
assert.equal(queueBranchPrNumber("custom/queue/pr-456", "custom/queue"), 456);
assert.deepEqual(
  queueBranchMetadata("automation/merge-queue/pr-123-a56115a-m4c"),
  { number: 123, suffix: "a56115a-m4c" },
);
assert.equal(queueBranchMetadata("automation/merge-queue/pr-123").suffix, "");
assert.equal(
  supersededOpenQueueBranchReason("automation/merge-queue/pr-123-a56115a-m4c", "a56115afffffffffffffffffffffffffffffffffff"),
  null,
);
assert.match(
  supersededOpenQueueBranchReason("automation/merge-queue/pr-123-deadbee-m4c", "a56115afffffffffffffffffffffffffffffffffff"),
  /superseded open PR queue branch/,
);
assert.equal(
  supersededOpenQueueBranchReason("automation/merge-queue/pr-123", "a56115afffffffffffffffffffffffffffffffffff"),
  null,
);

assert.equal(requiredCheckState([check()], ["CI Summary"]).kind, "passed");
assert.equal(requiredCheckState([check({ status: "IN_PROGRESS", conclusion: "" })], ["CI Summary"]).kind, "pending");
assert.equal(requiredCheckState([check({ conclusion: "FAILURE" })], ["CI Summary"]).kind, "failed");
assert.equal(requiredCheckState([], ["CI Summary"]).kind, "missing");
assert.equal(requiredCheckState([{ context: "Queue Tested", state: "SUCCESS" }], ["Queue Tested"]).kind, "passed");
assert.equal(requiredCheckState([{ context: "Queue Tested", state: "PENDING" }], ["Queue Tested"]).kind, "pending");
assert.deepEqual(
  pendingQueueRun(pr({
    statusCheckRollup: [
      {
        __typename: "StatusContext",
        context: "Queue Tested",
        state: "PENDING",
        targetUrl: "https://github.com/owner/repo/actions/runs/123456789",
      },
    ],
  }), "Queue Tested"),
  {
    runId: "123456789",
    targetUrl: "https://github.com/owner/repo/actions/runs/123456789",
  },
);
assert.equal(
  pendingQueueRun(pr({
    statusCheckRollup: [
      {
        __typename: "StatusContext",
        context: "Queue Tested",
        state: "PENDING",
      },
    ],
  }), "Queue Tested"),
  null,
);
assert.equal(
  hasPendingPlaceholderQueueStatus(pr({
    statusCheckRollup: [
      {
        __typename: "StatusContext",
        context: "Queue Tested",
        state: "PENDING",
      },
    ],
  }), "Queue Tested"),
  true,
);
assert.equal(
  hasPendingPlaceholderQueueStatus(pr({
    statusCheckRollup: [
      {
        __typename: "StatusContext",
        context: "Queue Tested",
        state: "PENDING",
        targetUrl: "https://github.com/owner/repo/actions/runs/123456789",
      },
    ],
  }), "Queue Tested"),
  false,
);
assert.equal(queueRunIsActive({ status: "queued" }), true);
assert.equal(queueRunIsActive({ status: "in_progress" }), true);
assert.equal(queueRunIsActive({ status: "completed", conclusion: "cancelled" }), false);
assert.deepEqual(
  activeBranchQueueRun([
    { databaseId: 1, status: "completed", conclusion: "cancelled", url: "https://github.example/runs/1" },
    { databaseId: 2, status: "queued", conclusion: "", url: "https://github.example/runs/2" },
  ]),
  { databaseId: 2, status: "queued", conclusion: "", url: "https://github.example/runs/2" },
);
assert.equal(
  activeBranchQueueRun([
    { databaseId: 1, status: "completed", conclusion: "cancelled", url: "https://github.example/runs/1" },
  ]),
  null,
);
assert.deepEqual(
  activeSyntheticQueueRun([
    { databaseId: 1, status: "queued", conclusion: "", headSha: "a".repeat(40), url: "https://github.example/runs/1" },
    { databaseId: 2, status: "queued", conclusion: "", headSha: "b".repeat(40), url: "https://github.example/runs/2" },
  ], "b".repeat(40)),
  { databaseId: 2, status: "queued", conclusion: "", headSha: "b".repeat(40), url: "https://github.example/runs/2" },
);
assert.equal(
  activeSyntheticQueueRun([
    { databaseId: 1, status: "completed", conclusion: "success", headSha: "b".repeat(40), url: "https://github.example/runs/1" },
  ], "b".repeat(40)),
  null,
);

assert.equal(queueSkipReason(pr({ isDraft: true }), { kind: "passed" }, "main"), "draft PR");
assert.equal(queueSkipReason(pr({ autoMergeRequest: null }), { kind: "passed" }, "main"), "auto-merge is not armed");
assert.equal(queueSkipReason(pr({ labels: ["WIP"] }), { kind: "passed" }, "main"), "ready-state WIP marker: WIP label");
assert.equal(queueSkipReason(pr(), { kind: "pending", reason: "pending checks" }, "main"), "pending checks");
assert.equal(queueSkipReason(pr(), { kind: "passed" }, "main"), null);
assert.equal(queueSkipReason({ ...pr(), statusCheckRollup: undefined }, { kind: "passed" }, "main"), null);
assert.match(failureCommentBody("M1-A", "CI Summary failed"), /^AgentName: M1-A\n\nPoor man's merge queue/m);
assert.throws(() => failureCommentBody("M1-A\nOther", "CI Summary failed"), /single line/);

assert.match(
  formatResult({
    dryRun: true,
    selected: pr(),
    baseOid: "b".repeat(40),
    skips: [],
  }, parseArgs(["--repository", "owner/repo", "--dry-run"])),
  /Would synthetic-test and merge #42/,
);

assert.match(
  formatResult({
    invalidated: 12,
    skippedActiveRuns: 2,
  }, parseArgs(["--repository", "owner/repo", "--invalidate-open"])),
  /Preserved 2 active queue run status/,
);

const cleanupFormat = formatResult({
  cleanupQueueBranches: true,
  deletions: [
    { branch: "automation/merge-queue/pr-1", number: 1, state: "closed", merged: true },
    { branch: "automation/merge-queue/pr-2-deadbee", number: 2, state: "open", supersededOpen: true },
  ],
  dryRun: true,
  skippedActiveRuns: 1,
  skippedOpen: 2,
  skippedUnrecognized: 1,
  supersededOpen: 1,
  skips: [],
  wouldDelete: 2,
}, parseArgs(["--repository", "owner/repo", "--cleanup-queue-branches", "--dry-run", "--verbose"]));
assert.match(cleanupFormat, /Would delete 2 stale queue branch/);
assert.match(cleanupFormat, /Included 1 superseded open PR branch/);
assert.match(cleanupFormat, /open superseded/);
assert.doesNotMatch(cleanupFormat, /\| #undefined \|/);

assert.match(
  formatResult({
    selected: pr(),
    merged: true,
    synthetic: { mergeOid: "c".repeat(40) },
    skips: [],
  }, parseArgs(["--repository", "owner/repo"])),
  /Merged #42 after synthetic merge/,
);

assert.match(
  formatResult({
    selected: pr(),
    baseMoved: true,
    oldBaseOid: "d".repeat(40),
    newBaseOid: "e".repeat(40),
    synthetic: { mergeOid: "f".repeat(40) },
    skips: [],
  }, parseArgs(["--repository", "owner/repo"])),
  /Retest needed/,
);

assert.match(
  formatResult({
    selected: pr(),
    pendingSynthetic: true,
    synthetic: { mergeOid: "c".repeat(40) },
    activeRun: { databaseId: 123, url: "https://github.example/runs/123" },
    skips: [],
  }, parseArgs(["--repository", "owner/repo"])),
  /synthetic run 123.*still active/,
);

console.log("poor man's merge queue tests passed");
