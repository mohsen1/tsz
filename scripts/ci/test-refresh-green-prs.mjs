#!/usr/bin/env node
import assert from "node:assert/strict";
import {
  autoMergeInFlightReason,
  candidateFromCompare,
  cheapSkipReason,
  checkRollupState,
  compareBehindState,
  findInFlightPullRequest,
  formatResultReport,
  parseArgs,
  selectSortedPullRequests,
} from "./refresh-green-prs.mjs";

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
    autoMergeRequest: null,
    baseRefName: "main",
    baseRefOid: "base-sha",
    body: "AgentName: TestAgent\n\nReady for review.\n",
    headRefOid: "head-sha",
    isCrossRepository: false,
    isDraft: false,
    labels: [],
    number: 100,
    statusCheckRollup: [check()],
    title: "fix(ci): sample",
    url: "https://github.example/pull/100",
    ...overrides,
  };
}

assert.deepEqual(parseArgs(["--repository", "owner/repo"]).repository, "owner/repo");
assert.deepEqual(parseArgs(["--no-default-required-check"]).requiredChecks, []);
assert.deepEqual(
  parseArgs(["--no-default-required-check", "--required-check", "merge-queue"]).requiredChecks,
  ["merge-queue"],
);

assert.equal(checkRollupState([check()]).kind, "passed");
assert.equal(checkRollupState([check({ name: "lint", conclusion: "SKIPPED" })], []).kind, "passed");
assert.equal(checkRollupState([check({ status: "IN_PROGRESS", conclusion: "" })]).kind, "pending");
assert.equal(checkRollupState([check({ conclusion: "FAILURE" })]).kind, "failed");
assert.equal(checkRollupState([check({ name: "lint" })]).kind, "missing");
assert.match(checkRollupState([]).reason, /no status checks/);

assert.equal(cheapSkipReason(pr({ isDraft: true }), { base: "main" }), "draft PR");
assert.equal(
  cheapSkipReason(pr({ labels: ["WIP"] }), { base: "main" }),
  "ready-state WIP marker: WIP label",
);
assert.equal(cheapSkipReason(pr({ baseRefName: "release" }), { base: "main" }), "base is release, not main");
assert.equal(cheapSkipReason(pr({ isCrossRepository: true }), { base: "main" }), "cross-repository PR");
assert.equal(
  cheapSkipReason(pr({ statusCheckRollup: [check({ conclusion: "FAILURE" })] }), { base: "main" }),
  "failing checks: CI Summary (failure)",
);
assert.equal(cheapSkipReason(pr(), { base: "main" }), null);

assert.deepEqual(compareBehindState({ status: "diverged", ahead_by: 2, behind_by: 3 }), {
  aheadBy: 2,
  behindBy: 3,
  status: "diverged",
});
assert.equal(candidateFromCompare(pr(), { ahead_by: 2, behind_by: 3 }).eligible, true);
assert.equal(candidateFromCompare(pr(), { ahead_by: 2, behind_by: 0 }).eligible, false);

assert.equal(
  findInFlightPullRequest([
    pr({ number: 101, autoMergeRequest: { mergeMethod: "SQUASH" }, statusCheckRollup: [check()] }),
    pr({
      number: 99,
      autoMergeRequest: { mergeMethod: "SQUASH" },
      statusCheckRollup: [check({ status: "IN_PROGRESS", conclusion: "" })],
    }),
  ], { base: "main" })?.number,
  99,
);
assert.equal(
  findInFlightPullRequest([
    pr({
      number: 99,
      autoMergeRequest: { mergeMethod: "SQUASH" },
      statusCheckRollup: [check({ status: "IN_PROGRESS", conclusion: "" })],
    }),
  ], { base: "main", ignoreInFlight: true }),
  null,
);

assert.equal(
  autoMergeInFlightReason(
    pr({ autoMergeRequest: { mergeMethod: "SQUASH" } }),
    { ahead_by: 2, behind_by: 0 },
    { base: "main" },
  ),
  "auto-merge is armed on a current head",
);
assert.equal(
  autoMergeInFlightReason(
    pr({
      autoMergeRequest: { mergeMethod: "SQUASH" },
      statusCheckRollup: [check({ status: "IN_PROGRESS", conclusion: "" })],
    }),
    null,
    { base: "main" },
  ),
  "checks are still pending",
);
assert.equal(
  autoMergeInFlightReason(
    pr({
      autoMergeRequest: { mergeMethod: "SQUASH" },
      statusCheckRollup: [check({ name: "GitGuardian Security Checks" })],
    }),
    { ahead_by: 2, behind_by: 0 },
    { base: "main" },
  ),
  "required checks are not reported yet for the current auto-merge head",
);
assert.equal(
  autoMergeInFlightReason(
    pr({
      autoMergeRequest: { mergeMethod: "SQUASH" },
      statusCheckRollup: [check({ name: "GitGuardian Security Checks" })],
    }),
    { ahead_by: 2, behind_by: 1 },
    { base: "main" },
  ),
  null,
);
assert.equal(
  autoMergeInFlightReason(
    pr({ autoMergeRequest: { mergeMethod: "SQUASH" } }),
    { ahead_by: 2, behind_by: 1 },
    { base: "main" },
  ),
  null,
);

assert.deepEqual(selectSortedPullRequests([pr({ number: 3 }), pr({ number: 1 })]).map((item) => item.number), [1, 3]);

const report = formatResultReport({
  base: "main",
  dryRun: false,
  failures: [],
  inFlight: null,
  refreshed: [{
    number: 123,
    oldHead: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    newHead: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    url: "https://github.example/pull/123",
  }],
  verboseSkips: [],
});
assert.match(report, /armed auto-merge/);
assert.doesNotMatch(report, /dispatch/);
assert.match(report, /aaaaaaaaaaaa/);
assert.match(report, /bbbbbbbbbbbb/);

const inFlightReport = formatResultReport({
  base: "main",
  dryRun: true,
  failures: [],
  inFlight: pr({ number: 321 }),
  refreshed: [],
  verboseSkips: [],
});
assert.match(inFlightReport, /One-by-one guard/);
