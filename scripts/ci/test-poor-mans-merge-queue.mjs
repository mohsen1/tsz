#!/usr/bin/env node
import assert from "node:assert/strict";
import {
  activeBranchQueueRun,
  formatResult,
  hasPendingPlaceholderQueueStatus,
  pendingQueueRun,
  parseArgs,
  queueRunIsActive,
  queueSkipReason,
  requiredCheckState,
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

assert.equal(parseArgs(["--repository", "owner/repo"]).repository, "owner/repo");
assert.deepEqual(
  parseArgs(["--no-default-pr-required-checks", "--pr-required-check", "lint"]).prRequiredChecks,
  ["lint"],
);
assert.deepEqual(
  parseArgs(["--no-default-merge-required-checks", "--merge-required-check", "CI Summary"]).mergeRequiredChecks,
  ["CI Summary"],
);
assert.equal(parseArgs(["--invalidate-pr", "123"]).invalidatePr, 123);

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

assert.equal(queueSkipReason(pr({ isDraft: true }), { kind: "passed" }, "main"), "draft PR");
assert.equal(queueSkipReason(pr({ autoMergeRequest: null }), { kind: "passed" }, "main"), "auto-merge is not armed");
assert.equal(queueSkipReason(pr({ labels: ["WIP"] }), { kind: "passed" }, "main"), "ready-state WIP marker: WIP label");
assert.equal(queueSkipReason(pr(), { kind: "pending", reason: "pending checks" }, "main"), "pending checks");
assert.equal(queueSkipReason(pr(), { kind: "passed" }, "main"), null);
assert.equal(queueSkipReason({ ...pr(), statusCheckRollup: undefined }, { kind: "passed" }, "main"), null);

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

console.log("poor man's merge queue tests passed");
