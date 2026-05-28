#!/usr/bin/env node
import assert from "node:assert/strict";
import {
  activeBranchQueueRun,
  activeRunOwnerStatusCounts,
  activeSyntheticQueueRun,
  failureCommentBody,
  forceWithLeaseArgForOid,
  formatResult,
  hasPendingPlaceholderQueueStatus,
  normalizePullRequestState,
  normalizeRestPullRequest,
  normalizeRestWorkflowRun,
  pendingQueueRun,
  parseArgs,
  queueBranchMetadata,
  queueBranchPrNumber,
  queueRunIsActive,
  queueSkipReason,
  readPaginatedObjectArray,
  requiredCheckState,
  skipOwnerCounts,
  skipOwnerReasonCounts,
  skipReasonCounts,
  supersededOpenQueueBranchReason,
  syntheticCiDispatchArgs,
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
    labels: ["merge-queue"],
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
assert.equal(parseArgs(["--repository", "owner/repo", "--queue-label", "ready-to-merge"]).queueLabel, "ready-to-merge");
assert.equal(parseArgs(["--repository", "owner/repo", "--ci-workflow", "queue-ci.yml"]).ciWorkflow, "queue-ci.yml");
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

assert.deepEqual(normalizePullRequestState({
  number: 10271,
  state: "closed",
  merged_at: "2026-05-26T20:01:52Z",
  updated_at: "2026-05-26T20:02:00Z",
  labels: [{ name: "agent:Studio-E" }],
}), {
  number: 10271,
  state: "closed",
  merged: true,
  updatedAt: "2026-05-26T20:02:00Z",
  owner: "agent:Studio-E",
});

assert.equal(
  forceWithLeaseArgForOid("automation/merge-queue/pr-123", "a".repeat(40)),
  `--force-with-lease=refs/heads/automation/merge-queue/pr-123:${"a".repeat(40)}`,
);
assert.equal(
  forceWithLeaseArgForOid("automation/merge-queue/pr-123", ""),
  "--force-with-lease=refs/heads/automation/merge-queue/pr-123:",
);
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
assert.deepEqual(
  syntheticCiDispatchArgs("owner/repo", "ci.yml", "automation/merge-queue/pr-123"),
  [
    "api", "-X", "POST",
    "repos/owner/repo/actions/workflows/ci.yml/dispatches",
    "-f", "ref=automation/merge-queue/pr-123",
  ],
);
assert.deepEqual(
  syntheticCiDispatchArgs("owner/repo", ".github/workflows/ci.yml", "automation/merge-queue/pr-123"),
  [
    "api", "-X", "POST",
    "repos/owner/repo/actions/workflows/.github%2Fworkflows%2Fci.yml/dispatches",
    "-f", "ref=automation/merge-queue/pr-123",
  ],
);

const paginatedObjectArrayUrls = [];
assert.deepEqual(
  readPaginatedObjectArray("repos/owner/repo/commits/abc/check-runs", "check_runs", {
    perPage: 2,
    readJson: (url) => {
      paginatedObjectArrayUrls.push(url);
      if (url.endsWith("page=1")) return { check_runs: [{ name: "CI Summary" }, { name: "lint" }] };
      if (url.endsWith("page=2")) return { check_runs: [{ name: "GitGuardian Security Checks" }] };
      return { check_runs: [] };
    },
  }).map((item) => item.name),
  ["CI Summary", "lint", "GitGuardian Security Checks"],
);
assert.deepEqual(paginatedObjectArrayUrls, [
  "repos/owner/repo/commits/abc/check-runs?per_page=2&page=1",
  "repos/owner/repo/commits/abc/check-runs?per_page=2&page=2",
]);
assert.throws(
  () => readPaginatedObjectArray("repos/owner/repo/commits/abc/check-runs", "check_runs", {
    readJson: () => ({ not_check_runs: [] }),
  }),
  /expected GitHub API object array check_runs/,
);

assert.equal(requiredCheckState([check()], ["CI Summary"]).kind, "passed");
assert.equal(requiredCheckState([check({ status: "IN_PROGRESS", conclusion: "" })], ["CI Summary"]).kind, "pending");
assert.equal(requiredCheckState([check({ conclusion: "FAILURE" })], ["CI Summary"]).kind, "failed");
assert.equal(requiredCheckState([], ["CI Summary"]).kind, "missing");
assert.equal(requiredCheckState([{ context: "Queue Tested", state: "SUCCESS" }], ["Queue Tested"]).kind, "passed");
assert.equal(requiredCheckState([{ context: "Queue Tested", state: "PENDING" }], ["Queue Tested"]).kind, "pending");
assert.equal(requiredCheckState([
  check({ conclusion: "SUCCESS", completedAt: "2026-05-26T12:00:00Z" }),
  check({ conclusion: "FAILURE", completedAt: "2026-05-26T11:00:00Z" }),
], ["CI Summary"]).kind, "passed");
assert.equal(requiredCheckState([
  { context: "Queue Tested", state: "SUCCESS", createdAt: "2026-05-26T11:00:00Z" },
  { context: "Queue Tested", state: "PENDING", createdAt: "2026-05-26T12:00:00Z" },
], ["Queue Tested"]).kind, "pending");
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
assert.equal(queueSkipReason(pr({ labels: [] }), { kind: "passed" }, "main"), "missing merge-queue label");
assert.equal(queueSkipReason(pr({ labels: ["ready-to-merge"] }), { kind: "passed" }, "main", "ready-to-merge"), null);
assert.equal(queueSkipReason(pr({ labels: ["WIP"] }), { kind: "passed" }, "main"), "ready-state WIP marker: WIP label");
assert.equal(queueSkipReason(pr(), { kind: "pending", reason: "pending checks" }, "main"), "pending checks");
assert.equal(queueSkipReason(pr(), { kind: "passed" }, "main"), null);
assert.equal(queueSkipReason({ ...pr(), statusCheckRollup: undefined }, { kind: "passed" }, "main"), null);
assert.deepEqual(
  normalizeRestPullRequest({
    auto_merge: { merge_method: "squash" },
    base: { ref: "main", repo: { full_name: "owner/repo" } },
    body: "AgentName: TestAgent",
    draft: false,
    head: { ref: "feature", repo: { full_name: "owner/repo" }, sha: "b".repeat(40) },
    html_url: "https://github.example/owner/repo/pull/42",
    labels: [{ name: "agent:M4-A" }],
    number: 42,
    title: "fix(ci): sample",
    updated_at: "2026-05-26T10:00:00Z",
  }, "owner/repo", [{ context: "CI Summary", state: "success" }]),
  {
    autoMergeRequest: { merge_method: "squash" },
    baseRefName: "main",
    body: "AgentName: TestAgent",
    headRefName: "feature",
    headRefOid: "b".repeat(40),
    isCrossRepository: false,
    isDraft: false,
    labels: [{ name: "agent:M4-A" }],
    number: 42,
    statusCheckRollup: [{ context: "CI Summary", state: "success" }],
    title: "fix(ci): sample",
    updatedAt: "2026-05-26T10:00:00Z",
    url: "https://github.example/owner/repo/pull/42",
  },
);
assert.equal(
  normalizeRestPullRequest({
    base: { ref: "main", repo: { full_name: "owner/repo" } },
    head: { ref: "feature", repo: { full_name: "fork/repo" }, sha: "b".repeat(40) },
  }, "owner/repo").isCrossRepository,
  true,
);
assert.deepEqual(
  normalizeRestWorkflowRun({
    id: 123,
    status: "in_progress",
    conclusion: null,
    head_sha: "c".repeat(40),
    html_url: "https://github.example/runs/123",
    created_at: "2026-05-26T10:00:00Z",
    run_started_at: "2026-05-26T10:01:00Z",
    updated_at: "2026-05-26T10:02:00Z",
  }),
  {
    databaseId: 123,
    status: "in_progress",
    conclusion: "",
    headSha: "c".repeat(40),
    url: "https://github.example/runs/123",
    createdAt: "2026-05-26T10:00:00Z",
    startedAt: "2026-05-26T10:01:00Z",
    updatedAt: "2026-05-26T10:02:00Z",
  },
);
assert.deepEqual(
  skipReasonCounts([
    { number: 1, reason: "draft PR" },
    { number: 2, reason: "missing merge-queue label" },
    { number: 3, reason: "draft PR" },
  ]),
  [
    { reason: "draft PR", count: 2 },
    { reason: "missing merge-queue label", count: 1 },
  ],
);
assert.deepEqual(
  skipReasonCounts([
    { reason: "PR #1 is open", summaryReason: "open PR branch" },
    { reason: "PR #2 is open", summaryReason: "open PR branch" },
    { reason: "active queue run 123", summaryReason: "active queue run" },
  ]),
  [
    { reason: "open PR branch", count: 2 },
    { reason: "active queue run", count: 1 },
  ],
);
assert.deepEqual(
  skipOwnerCounts([
    { owner: "agent:M4-A", reason: "draft PR" },
    { owner: "agent:M4-B", reason: "missing merge-queue label" },
    { owner: "agent:M4-A", reason: "missing merge-queue label" },
  ]),
  [
    { owner: "agent:M4-A", count: 2 },
    { owner: "agent:M4-B", count: 1 },
  ],
);
assert.deepEqual(
  skipOwnerReasonCounts([
    { owner: "agent:M4-A", reason: "draft PR" },
    { owner: "agent:M4-B", reason: "missing merge-queue label" },
    { owner: "agent:M4-A", reason: "missing merge-queue label" },
    { owner: "agent:M4-A", reason: "missing merge-queue label" },
    { owner: "agent:M4-B", reason: "PR #10084 is open", summaryReason: "open PR branch" },
  ]),
  [
    { owner: "agent:M4-A", reason: "missing merge-queue label", count: 2 },
    { owner: "agent:M4-A", reason: "draft PR", count: 1 },
    { owner: "agent:M4-B", reason: "missing merge-queue label", count: 1 },
    { owner: "agent:M4-B", reason: "open PR branch", count: 1 },
  ],
);
assert.deepEqual(
  activeRunOwnerStatusCounts([
    { owner: "agent:M4-A", status: "in_progress", startedAt: "2026-05-26T03:35:21Z" },
    { owner: "agent:M4-A", status: "queued", startedAt: "2026-05-26T03:40:00Z" },
    { owner: "agent:M4-A", status: "queued", startedAt: "2026-05-26T03:20:00Z" },
    { owner: "agent:M1-C", status: "queued", startedAt: "2026-05-26T03:45:00Z" },
  ]),
  [
    { owner: "agent:M4-A", status: "queued", count: 2, oldestStartedAt: "2026-05-26T03:20:00Z" },
    { owner: "agent:M1-C", status: "queued", count: 1, oldestStartedAt: "2026-05-26T03:45:00Z" },
    { owner: "agent:M4-A", status: "in_progress", count: 1, oldestStartedAt: "2026-05-26T03:35:21Z" },
  ],
);
assert.deepEqual(
  skipOwnerCounts([
    { owner: "agent:M4-A", updatedAt: "2026-05-25T10:00:00Z", reason: "draft PR" },
    { owner: "agent:M4-B", updatedAt: "2026-05-24T09:00:00Z", reason: "missing merge-queue label" },
    { owner: "agent:M4-A", updatedAt: "2026-05-23T08:00:00Z", reason: "missing merge-queue label" },
  ]),
  [
    { owner: "agent:M4-A", count: 2, oldestUpdatedAt: "2026-05-23T08:00:00Z" },
    { owner: "agent:M4-B", count: 1, oldestUpdatedAt: "2026-05-24T09:00:00Z" },
  ],
);
assert.match(failureCommentBody("M1-A", "CI Summary failed"), /^AgentName: M1-A\n\nPoor man's merge queue/m);
assert.doesNotMatch(failureCommentBody("M1-A", "CI Summary failed"), /not evidence that the PR head failed CI/);
assert.match(
  failureCommentBody(
    "Studio-F",
    "git checkout -B automation/merge-queue/pr-10521 4e422f332cbe68ca5452299c4f1a144a30eefab0 failed",
  ),
  /infrastructure\/worktree evidence, not evidence that the PR head failed CI/,
);
assert.match(
  failureCommentBody("Studio-F", "git fetch --no-tags origin main pull/10521/head failed"),
  /infrastructure\/worktree evidence, not evidence that the PR head failed CI/,
);
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

const cleanupActiveRunFormat = formatResult({
  cleanupQueueBranches: true,
  activeRuns: [
    {
      branch: "automation/merge-queue/pr-9515",
      number: 9515,
      owner: "agent:M4-A",
      runId: 26423420117,
      url: "https://github.example/runs/26423420117",
      status: "in_progress",
      startedAt: "2026-05-26T03:35:21Z",
    },
    {
      branch: "automation/merge-queue/pr-10084",
      number: 10084,
      owner: "agent:M1-C",
      runId: 26423420118,
      url: "https://github.example/runs/26423420118",
      status: "queued",
      startedAt: "2026-05-26T04:45:00Z",
    },
  ],
  deletions: [],
  dryRun: true,
  skippedActiveRuns: 2,
  skippedOpen: 0,
  skippedUnrecognized: 0,
  supersededOpen: 0,
  now: "2026-05-26T05:05:00Z",
  skips: [
    {
      branch: "automation/merge-queue/pr-9515",
      owner: "agent:M4-A",
      reason: "active queue run 26423420117",
      summaryReason: "active queue run",
    },
    {
      branch: "automation/merge-queue/pr-9632",
      owner: "agent:M4-A",
      reason: "PR #9632 is open",
      summaryReason: "open PR branch",
    },
    {
      branch: "automation/merge-queue/pr-9912",
      owner: "agent:M4-C",
      reason: "PR #9912 is open",
      summaryReason: "open PR branch",
    },
  ],
  wouldDelete: 0,
}, parseArgs(["--repository", "owner/repo", "--cleanup-queue-branches", "--dry-run", "--verbose"]));
assert.match(cleanupActiveRunFormat, /Preserved 2 branch\(es\) with active queue runs/);
assert.match(cleanupActiveRunFormat, /### Active Queue Run Owner Status Counts/);
assert.match(cleanupActiveRunFormat, /\| 1 \| agent:M1-C \| queued \| 2026-05-26 04:45Z \| 20m \|/);
assert.match(cleanupActiveRunFormat, /\| 1 \| agent:M4-A \| in_progress \| 2026-05-26 03:35Z \| 1h 29m \|/);
assert.match(cleanupActiveRunFormat, /### Active Queue Runs/);
assert.match(
  cleanupActiveRunFormat,
  /\| `automation\/merge-queue\/pr-9515` \| #9515 \| agent:M4-A \| \[26423420117\]\(https:\/\/github\.example\/runs\/26423420117\) \| in_progress \| 2026-05-26 03:35Z \| 1h 29m \|/,
);
assert.match(
  cleanupActiveRunFormat,
  /\| `automation\/merge-queue\/pr-10084` \| #10084 \| agent:M1-C \| \[26423420118\]\(https:\/\/github\.example\/runs\/26423420118\) \| queued \| 2026-05-26 04:45Z \| 20m \|/,
);
assert.match(cleanupActiveRunFormat, /### Skip Reason Counts/);
assert.match(cleanupActiveRunFormat, /\| 2 \| open PR branch \|/);
assert.match(cleanupActiveRunFormat, /\| 1 \| active queue run \|/);
assert.match(cleanupActiveRunFormat, /### Skip Owner Counts/);
assert.match(cleanupActiveRunFormat, /\| 2 \| agent:M4-A \|/);
assert.match(cleanupActiveRunFormat, /\| 1 \| agent:M4-C \|/);
assert.match(cleanupActiveRunFormat, /### Skip Owner Reason Counts/);
assert.match(cleanupActiveRunFormat, /\| 1 \| agent:M4-A \| active queue run \|/);
assert.match(cleanupActiveRunFormat, /\| 1 \| agent:M4-A \| open PR branch \|/);
assert.match(cleanupActiveRunFormat, /\| Branch \| Owner \| Reason \|/);
assert.match(cleanupActiveRunFormat, /\| `automation\/merge-queue\/pr-9515` \| agent:M4-A \| active queue run 26423420117 \|/);
assert.match(cleanupActiveRunFormat, /\| `automation\/merge-queue\/pr-9912` \| agent:M4-C \| PR #9912 is open \|/);

const cleanupOwnerDateFormat = formatResult({
  cleanupQueueBranches: true,
  activeRuns: [],
  deletions: [],
  dryRun: true,
  skippedActiveRuns: 0,
  skippedOpen: 3,
  skippedUnrecognized: 0,
  supersededOpen: 0,
  now: "2026-05-26T11:15:00Z",
  skips: [
    {
      branch: "automation/merge-queue/pr-9515",
      owner: "agent:M4-A",
      reason: "PR #9515 is open",
      summaryReason: "open PR branch",
      updatedAt: "2026-05-25T10:00:00Z",
    },
    {
      branch: "automation/merge-queue/pr-9632",
      owner: "agent:M4-A",
      reason: "PR #9632 is open",
      summaryReason: "open PR branch",
      updatedAt: "2026-05-24T09:00:00Z",
    },
    {
      branch: "automation/merge-queue/pr-9912",
      owner: "agent:M4-C",
      reason: "PR #9912 is open",
      summaryReason: "open PR branch",
      updatedAt: "2026-05-23T08:00:00Z",
    },
  ],
  wouldDelete: 0,
}, parseArgs(["--repository", "owner/repo", "--cleanup-queue-branches", "--dry-run", "--verbose"]));
assert.match(cleanupOwnerDateFormat, /\| Count \| Owner \| Oldest updated \| Oldest age \|/);
assert.match(cleanupOwnerDateFormat, /\| 2 \| agent:M4-A \| 2026-05-24 \| 2d 2h \|/);
assert.match(cleanupOwnerDateFormat, /\| 1 \| agent:M4-C \| 2026-05-23 \| 3d 3h \|/);
assert.match(cleanupOwnerDateFormat, /\| Branch \| Owner \| Updated \| Reason \|/);
assert.match(cleanupOwnerDateFormat, /\| `automation\/merge-queue\/pr-9632` \| agent:M4-A \| 2026-05-24 \| PR #9632 is open \|/);

const queueSkipFormat = formatResult({
  selected: null,
  skips: Array.from({ length: 27 }, (_, index) => ({
    number: index + 1,
    owner: index % 2 === 0 ? "agent:M1-A" : "agent:M4-B",
    reason: index % 3 === 0 ? "draft PR" : "missing merge-queue label",
  })),
}, parseArgs(["--repository", "owner/repo", "--dry-run", "--verbose"]));
assert.match(queueSkipFormat, /### Skip Reason Counts/);
assert.match(queueSkipFormat, /\| 18 \| missing merge-queue label \|/);
assert.match(queueSkipFormat, /\| 9 \| draft PR \|/);
assert.match(queueSkipFormat, /### Skip Owner Counts/);
assert.match(queueSkipFormat, /\| 14 \| agent:M1-A \|/);
assert.match(queueSkipFormat, /\| 13 \| agent:M4-B \|/);
assert.match(queueSkipFormat, /### Skip Owner Reason Counts/);
assert.match(queueSkipFormat, /\| 9 \| agent:M4-B \| missing merge-queue label \|/);
assert.match(queueSkipFormat, /\| 5 \| agent:M1-A \| draft PR \|/);
assert.match(queueSkipFormat, /\| PR \| Owner \| Reason \|/);
assert.match(queueSkipFormat, /\| #1 \| agent:M1-A \| draft PR \|/);
assert.match(queueSkipFormat, /\| \.\.\. \|  \| 2 more skipped PR\(s\) omitted \|/);

const queueSkipOwnerDateFormat = formatResult({
  now: "2026-05-26T11:15:00Z",
  selected: null,
  skips: [
    { number: 1, owner: "agent:M1-A", reason: "draft PR", updatedAt: "2026-05-25T10:00:00Z" },
    { number: 2, owner: "agent:M1-A", reason: "missing merge-queue label", updatedAt: "2026-05-23T08:00:00Z" },
    { number: 3, owner: "agent:M4-B", reason: "missing merge-queue label", updatedAt: "2026-05-24T09:00:00Z" },
  ],
}, parseArgs(["--repository", "owner/repo", "--dry-run", "--verbose"]));
assert.match(queueSkipOwnerDateFormat, /\| Count \| Owner \| Oldest updated \| Oldest age \|/);
assert.match(queueSkipOwnerDateFormat, /\| 2 \| agent:M1-A \| 2026-05-23 \| 3d 3h \|/);
assert.match(queueSkipOwnerDateFormat, /\| 1 \| agent:M4-B \| 2026-05-24 \| 2d 2h \|/);
assert.match(queueSkipOwnerDateFormat, /\| PR \| Owner \| Updated \| Reason \|/);
assert.match(queueSkipOwnerDateFormat, /\| #2 \| agent:M1-A \| 2026-05-23 \| missing merge-queue label \|/);

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
