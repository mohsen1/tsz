#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { readyStateFailures } from "./check-pr-ready-state.mjs";

const DEFAULT_BASE = "main";
const DEFAULT_MAX_PRS = 200;
const DEFAULT_QUEUE_BRANCH_PREFIX = "automation/merge-queue";
const DEFAULT_STATUS_CONTEXT = "Queue Tested";
const DEFAULT_PR_REQUIRED_CHECKS = ["CI Summary", "GitGuardian Security Checks"];
const DEFAULT_MERGE_REQUIRED_CHECKS = ["CI Summary"];
const DEFAULT_WAIT_ATTEMPTS = 90;
const DEFAULT_WAIT_INTERVAL_MS = 20_000;
const GH_MAX_BUFFER_BYTES = 24 * 1024 * 1024;
const SUCCESSFUL_CHECK_STATES = new Set(["SUCCESS", "NEUTRAL", "SKIPPED"]);
const SUCCESSFUL_STATUS_STATES = new Set(["SUCCESS"]);

function usage() {
  return [
    "usage: poor-mans-merge-queue.mjs --repository owner/repo [options]",
    "",
    "Serially tests one auto-merge PR on a synthetic latest-main merge branch,",
    "posts a required status to the PR head, and merges only if main/head did not move.",
    "",
    "Options:",
    "  --base <branch>                 Base branch (default: main)",
    "  --max-prs <n>                   Max open PRs to inspect",
    "  --status-context <name>         Required status context to post",
    "  --queue-branch-prefix <prefix>  Temporary branch namespace",
    "  --agent-name <name>             AgentName for queue failure comments",
    "  --pr-required-check <name>      PR-head check required before queueing",
    "  --merge-required-check <name>   Synthetic merge check required before merge",
    "  --no-default-pr-required-checks",
    "  --no-default-merge-required-checks",
    "  --invalidate-open               Mark open PR heads pending and exit",
    "  --invalidate-pr <number>        Mark one PR head pending and exit",
    "  --cleanup-queue-branches        Delete stale queue branches for closed PRs",
    "  --cleanup-superseded-open-queue-branches",
    "                                  Also delete inactive suffixed open-PR queue branches from older bases",
    "  --dry-run                       Report without pushing or merging",
    "  --verbose",
  ].join("\n");
}

function parsePositiveInt(flag, value) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(`${flag} requires a positive integer`);
  }
  return parsed;
}

export function parseArgs(argv) {
  const options = {
    agentName: process.env.AGENT_NAME || "M1-A",
    base: process.env.BASE_BRANCH || DEFAULT_BASE,
    cleanupQueueBranches: false,
    cleanupSupersededOpenQueueBranches: false,
    dryRun: false,
    invalidateOpen: false,
    invalidatePr: null,
    maxPrs: DEFAULT_MAX_PRS,
    mergeRequiredChecks: [...DEFAULT_MERGE_REQUIRED_CHECKS],
    prRequiredChecks: [...DEFAULT_PR_REQUIRED_CHECKS],
    queueBranchPrefix: DEFAULT_QUEUE_BRANCH_PREFIX,
    repository: process.env.REPOSITORY || process.env.GITHUB_REPOSITORY || null,
    statusContext: DEFAULT_STATUS_CONTEXT,
    verbose: false,
    waitAttempts: DEFAULT_WAIT_ATTEMPTS,
    waitIntervalMs: DEFAULT_WAIT_INTERVAL_MS,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--repository") {
      options.repository = argv[++index];
      if (!options.repository) throw new Error("--repository requires owner/repo");
    } else if (arg === "--base") {
      options.base = argv[++index];
      if (!options.base) throw new Error("--base requires a branch name");
    } else if (arg === "--max-prs") {
      options.maxPrs = parsePositiveInt("--max-prs", argv[++index]);
    } else if (arg === "--status-context") {
      options.statusContext = argv[++index];
      if (!options.statusContext) throw new Error("--status-context requires a name");
    } else if (arg === "--queue-branch-prefix") {
      options.queueBranchPrefix = argv[++index];
      if (!options.queueBranchPrefix) throw new Error("--queue-branch-prefix requires a branch prefix");
    } else if (arg === "--agent-name") {
      options.agentName = argv[++index];
      if (!options.agentName) throw new Error("--agent-name requires an AgentName");
    } else if (arg === "--pr-required-check") {
      options.prRequiredChecks.push(argv[++index]);
    } else if (arg === "--merge-required-check") {
      options.mergeRequiredChecks.push(argv[++index]);
    } else if (arg === "--no-default-pr-required-checks") {
      options.prRequiredChecks = [];
    } else if (arg === "--no-default-merge-required-checks") {
      options.mergeRequiredChecks = [];
    } else if (arg === "--invalidate-open") {
      options.invalidateOpen = true;
    } else if (arg === "--invalidate-pr") {
      options.invalidatePr = parsePositiveInt("--invalidate-pr", argv[++index]);
    } else if (arg === "--cleanup-queue-branches") {
      options.cleanupQueueBranches = true;
    } else if (arg === "--cleanup-superseded-open-queue-branches") {
      options.cleanupQueueBranches = true;
      options.cleanupSupersededOpenQueueBranches = true;
    } else if (arg === "--wait-attempts") {
      options.waitAttempts = parsePositiveInt("--wait-attempts", argv[++index]);
    } else if (arg === "--wait-interval-ms") {
      options.waitIntervalMs = parsePositiveInt("--wait-interval-ms", argv[++index]);
    } else if (arg === "--dry-run") {
      options.dryRun = true;
    } else if (arg === "--verbose") {
      options.verbose = true;
    } else if (arg === "--help" || arg === "-h") {
      console.log(usage());
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }

  return options;
}

function cleanAgentName(agentName) {
  const trimmed = String(agentName || "").trim();
  if (!trimmed) throw new Error("AgentName is required");
  if (/[\r\n]/.test(trimmed)) throw new Error("AgentName must be a single line");
  return trimmed;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    maxBuffer: GH_MAX_BUFFER_BYTES,
    stdio: options.stdio || ["ignore", "pipe", "pipe"],
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error([
      `${command} ${args.join(" ")} failed`,
      result.stdout?.trim(),
      result.stderr?.trim(),
    ].filter(Boolean).join("\n"));
  }
  return result.stdout || "";
}

function runGh(args) {
  return run("gh", args);
}

function runGhJson(args) {
  return JSON.parse(runGh(args));
}

function git(args, options = {}) {
  return run("git", args, options);
}

function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function labelNames(labels) {
  return Array.isArray(labels)
    ? labels.map((label) => typeof label === "string" ? label : label?.name).filter(Boolean)
    : [];
}

function agentLabel(labels) {
  return labelNames(labels).find((label) => label.startsWith("agent:")) || "";
}

function normalize(value) {
  return String(value || "").toUpperCase();
}

export function requiredCheckState(checks, requiredNames) {
  const byName = new Map();
  for (const check of checks || []) {
    const name = check.name || check.context;
    if (!name) continue;
    byName.set(name, check);
  }

  const missing = [];
  const pending = [];
  const failed = [];
  for (const name of requiredNames) {
    const check = byName.get(name);
    if (!check) {
      missing.push(name);
      continue;
    }
    if ("state" in check && !("status" in check)) {
      const state = normalize(check.state);
      if (SUCCESSFUL_STATUS_STATES.has(state)) continue;
      if (["PENDING", "EXPECTED"].includes(state)) pending.push(`${name} (${state.toLowerCase()})`);
      else failed.push(`${name} (${state.toLowerCase() || "unknown"})`);
      continue;
    }
    const status = normalize(check.status);
    if (status && status !== "COMPLETED") {
      pending.push(`${name} (${status.toLowerCase()})`);
      continue;
    }
    const conclusion = normalize(check.conclusion);
    if (SUCCESSFUL_CHECK_STATES.has(conclusion)) continue;
    if (!conclusion) pending.push(`${name} (missing conclusion)`);
    else failed.push(`${name} (${conclusion.toLowerCase()})`);
  }

  if (failed.length) return { kind: "failed", reason: `failing required check(s): ${failed.join(", ")}` };
  if (pending.length) return { kind: "pending", reason: `pending required check(s): ${pending.join(", ")}` };
  if (missing.length) return { kind: "missing", reason: `missing required check(s): ${missing.join(", ")}` };
  return { kind: "passed", reason: "required checks passed" };
}

function actionsRunIdFromUrl(url) {
  const match = String(url || "").match(/\/actions\/runs\/(\d+)(?:\D|$)/);
  return match ? match[1] : null;
}

export function pendingQueueRun(pr, statusContext) {
  const status = (pr.statusCheckRollup || []).find((check) => (
    check.__typename === "StatusContext"
      && check.context === statusContext
      && normalize(check.state) === "PENDING"
  ));
  if (!status?.targetUrl) return null;
  const runId = actionsRunIdFromUrl(status.targetUrl);
  return runId ? { runId, targetUrl: status.targetUrl } : null;
}

export function hasPendingPlaceholderQueueStatus(pr, statusContext) {
  return (pr.statusCheckRollup || []).some((check) => (
    check.__typename === "StatusContext"
      && check.context === statusContext
      && normalize(check.state) === "PENDING"
      && !check.targetUrl
  ));
}

export function queueRunIsActive(run) {
  return normalize(run?.status) !== "COMPLETED";
}

function shortDateTime(value) {
  return value ? `${String(value).slice(0, 16).replace("T", " ")}Z` : "unknown";
}

function shortDate(value) {
  return value ? String(value).slice(0, 10) : "unknown";
}

function elapsedAge(startedAt, now) {
  const startedMs = Date.parse(startedAt || "");
  const nowMs = Date.parse(now || "");
  if (!Number.isFinite(startedMs) || !Number.isFinite(nowMs)) return "unknown";

  const totalMinutes = Math.max(0, Math.floor((nowMs - startedMs) / 60_000));
  if (totalMinutes < 60) return `${totalMinutes}m`;

  const totalHours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (totalHours < 24) {
    return minutes ? `${totalHours}h ${minutes}m` : `${totalHours}h`;
  }

  const days = Math.floor(totalHours / 24);
  const hours = totalHours % 24;
  return hours ? `${days}d ${hours}h` : `${days}d`;
}

export function activeBranchQueueRun(runs) {
  return (runs || []).find((run) => queueRunIsActive(run)) || null;
}

export function activeSyntheticQueueRun(runs, headSha) {
  return (runs || []).find((run) => (
    queueRunIsActive(run) && (!headSha || run.headSha === headSha)
  )) || null;
}

function activePendingQueueRun(repository, pr, options) {
  const pendingRun = pendingQueueRun(pr, options.statusContext);
  if (pendingRun) {
    const run = readWorkflowRun(repository, pendingRun.runId);
    if (!queueRunIsActive(run)) return null;
    return {
      ...pendingRun,
      url: run.url || pendingRun.targetUrl,
      status: run.status || "",
      startedAt: run.startedAt || run.createdAt || "",
    };
  }
  if (!hasPendingPlaceholderQueueStatus(pr, options.statusContext)) return null;
  const run = activeBranchQueueRun(readBranchWorkflowRuns(repository, queueBranch(options, pr)));
  if (!run) return null;
  return {
    runId: String(run.databaseId || ""),
    url: run.url,
    status: run.status || "",
    startedAt: run.startedAt || run.createdAt || "",
  };
}

export function queueSkipReason(pr, requiredState, base) {
  if (pr.baseRefName !== base) return `base is ${pr.baseRefName || "(unknown)"}, not ${base}`;
  if (pr.isDraft) return "draft PR";
  if (pr.isCrossRepository) return "cross-repository PR";
  if (!pr.autoMergeRequest) return "auto-merge is not armed";
  const readinessFailures = readyStateFailures({
    number: pr.number,
    title: pr.title,
    body: pr.body || "",
    draft: pr.isDraft,
    labels: labelNames(pr.labels),
  });
  if (readinessFailures.length) return `ready-state WIP marker: ${readinessFailures.join(", ")}`;
  if (requiredState.kind !== "passed") return requiredState.reason;
  return null;
}

function readPullRequests(repository, base, maxPrs) {
  return runGhJson([
    "pr", "list",
    "--repo", repository,
    "--state", "open",
    "--base", base,
    "--limit", String(maxPrs),
    "--json", [
      "autoMergeRequest",
      "baseRefName",
      "body",
      "headRefName",
      "headRefOid",
      "isCrossRepository",
      "isDraft",
      "labels",
      "number",
      "title",
      "updatedAt",
      "url",
    ].join(","),
  ]);
}

function readPullRequest(repository, number) {
  return runGhJson([
    "pr", "view", String(number),
    "--repo", repository,
    "--json", [
      "autoMergeRequest",
      "baseRefName",
      "body",
      "headRefName",
      "headRefOid",
      "isCrossRepository",
      "isDraft",
      "labels",
      "number",
      "statusCheckRollup",
      "title",
      "updatedAt",
      "url",
    ].join(","),
  ]);
}

function readWorkflowRun(repository, runId) {
  return runGhJson([
    "run", "view", runId,
    "--repo", repository,
    "--json", "databaseId,status,conclusion,url,createdAt,startedAt,updatedAt",
  ]);
}

function readBranchWorkflowRuns(repository, branch) {
  return runGhJson([
    "run", "list",
    "--repo", repository,
    "--branch", branch,
    "--limit", "20",
    "--json", "databaseId,status,conclusion,headSha,url,createdAt,startedAt,updatedAt",
  ]);
}

function readRemoteQueueBranches(options) {
  const output = git(["ls-remote", "--heads", "origin", `${options.queueBranchPrefix}/pr-*`]);
  return output.split("\n").flatMap((line) => {
    const trimmed = line.trim();
    if (!trimmed) return [];
    const [oid, ref] = trimmed.split(/\s+/);
    const prefix = "refs/heads/";
    if (!oid || !ref?.startsWith(prefix)) return [];
    return [{ oid, branch: ref.slice(prefix.length) }];
  });
}

export function queueBranchPrNumber(branch, queueBranchPrefix = DEFAULT_QUEUE_BRANCH_PREFIX) {
  return queueBranchMetadata(branch, queueBranchPrefix)?.number ?? null;
}

export function queueBranchMetadata(branch, queueBranchPrefix = DEFAULT_QUEUE_BRANCH_PREFIX) {
  const escapedPrefix = queueBranchPrefix.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = String(branch || "").match(new RegExp(`^${escapedPrefix}/pr-(\\d+)(?:-([^/]+))?$`));
  return match ? {
    number: Number.parseInt(match[1], 10),
    suffix: match[2] || "",
  } : null;
}

export function supersededOpenQueueBranchReason(queueBranch, currentBaseOid, queueBranchPrefix = DEFAULT_QUEUE_BRANCH_PREFIX) {
  const metadata = queueBranchMetadata(queueBranch, queueBranchPrefix);
  if (!metadata) return null;
  if (!metadata.suffix) return null;
  const basePrefix = String(currentBaseOid || "").slice(0, 7);
  if (basePrefix && metadata.suffix.startsWith(basePrefix)) return null;
  return `superseded open PR queue branch for older base (current ${basePrefix || "unknown"})`;
}

function readPullRequestState(repository, number) {
  return runGhJson([
    "api",
    `repos/${repository}/pulls/${number}`,
    "--jq",
    "{number: .number, state: .state, merged: (.merged_at != null), updatedAt: .updated_at}",
  ]);
}

function readPullRequestOwner(repository, number) {
  const pr = runGhJson([
    "pr", "view", String(number),
    "--repo", repository,
    "--json", "labels",
  ]);
  return agentLabel(pr.labels);
}

function deleteRemoteBranch(branch) {
  git(["push", "origin", `:refs/heads/${branch}`], { stdio: "inherit" });
}

function readBranchOid(repository, branch) {
  return runGhJson([
    "api",
    `repos/${repository}/branches/${branch}`,
    "--jq",
    "{oid: .commit.sha}",
  ]).oid;
}

function postStatus(repository, sha, state, context, description, targetUrl = "") {
  const args = [
    "api", "-X", "POST",
    `repos/${repository}/statuses/${sha}`,
    "-f", `state=${state}`,
    "-f", `context=${context}`,
    "-f", `description=${description.slice(0, 140)}`,
  ];
  if (targetUrl) args.push("-f", `target_url=${targetUrl}`);
  runGh(args);
}

function postComment(repository, number, body) {
  runGh([
    "api", "-X", "POST",
    `repos/${repository}/issues/${number}/comments`,
    "-f", `body=${body}`,
  ]);
}

export function failureCommentBody(agentName, reason) {
  return [
    `AgentName: ${cleanAgentName(agentName)}`,
    "",
    "Poor man's merge queue could not land this PR.",
    "",
    `Reason: ${reason}`,
  ].join("\n");
}

export function skipReasonCounts(skips) {
  const counts = new Map();
  for (const skip of skips || []) {
    const reason = String(skip.summaryReason || skip.reason || "(unknown)");
    counts.set(reason, (counts.get(reason) || 0) + 1);
  }
  return [...counts.entries()]
    .map(([reason, count]) => ({ reason, count }))
    .sort((a, b) => b.count - a.count || a.reason.localeCompare(b.reason));
}

export function skipOwnerCounts(skips) {
  const counts = new Map();
  for (const skip of skips || []) {
    const owner = String(skip.owner || "(unknown)");
    const current = counts.get(owner) || { count: 0, oldestUpdatedAt: null };
    current.count += 1;
    if (skip.updatedAt && (!current.oldestUpdatedAt || skip.updatedAt < current.oldestUpdatedAt)) {
      current.oldestUpdatedAt = skip.updatedAt;
    }
    counts.set(owner, current);
  }
  return [...counts.entries()]
    .map(([owner, data]) => {
      const entry = { owner, count: data.count };
      if (data.oldestUpdatedAt) entry.oldestUpdatedAt = data.oldestUpdatedAt;
      return entry;
    })
    .sort((a, b) => b.count - a.count || a.owner.localeCompare(b.owner));
}

export function skipOwnerReasonCounts(skips) {
  const counts = new Map();
  for (const skip of skips || []) {
    const owner = String(skip.owner || "(unknown)");
    const reason = String(skip.summaryReason || skip.reason || "(unknown)");
    const key = `${owner}\0${reason}`;
    const current = counts.get(key) || { owner, reason, count: 0 };
    current.count += 1;
    counts.set(key, current);
  }
  return [...counts.values()]
    .sort((a, b) => b.count - a.count || a.owner.localeCompare(b.owner) || a.reason.localeCompare(b.reason));
}

function pushSkipOwnerCounts(lines, skips) {
  const ownerSummary = skipOwnerCounts(skips);
  const hasOldestUpdated = ownerSummary.some((entry) => entry.oldestUpdatedAt);
  lines.push(
    "",
    "### Skip Owner Counts",
    "",
    hasOldestUpdated ? "| Count | Owner | Oldest updated |" : "| Count | Owner |",
    hasOldestUpdated ? "|-------|-------|----------------|" : "|-------|-------|",
  );
  for (const entry of ownerSummary) {
    const owner = entry.owner.replace(/\|/g, "\\|");
    if (hasOldestUpdated) {
      lines.push(`| ${entry.count} | ${owner} | ${shortDate(entry.oldestUpdatedAt)} |`);
    } else {
      lines.push(`| ${entry.count} | ${owner} |`);
    }
  }
}

function pushSkipOwnerReasonCounts(lines, skips) {
  const ownerReasonSummary = skipOwnerReasonCounts(skips);
  lines.push(
    "",
    "### Skip Owner Reason Counts",
    "",
    "| Count | Owner | Reason |",
    "|-------|-------|--------|",
  );
  for (const entry of ownerReasonSummary) {
    const owner = entry.owner.replace(/\|/g, "\\|");
    const reason = entry.reason.replace(/\|/g, "\\|");
    lines.push(`| ${entry.count} | ${owner} | ${reason} |`);
  }
}

function pushCleanupSkipRows(lines, skips) {
  const hasUpdatedAt = (skips || []).some((skip) => skip.updatedAt);
  lines.push(
    "",
    "### Skips",
    "",
    hasUpdatedAt ? "| Branch | Owner | Updated | Reason |" : "| Branch | Owner | Reason |",
    hasUpdatedAt ? "|--------|-------|---------|--------|" : "|--------|-------|--------|",
  );
  for (const skip of skips.slice(0, 50)) {
    const reason = skip.reason.replace(/\|/g, "\\|");
    if (hasUpdatedAt) {
      lines.push(`| \`${skip.branch}\` | ${skip.owner || "(unknown)"} | ${shortDate(skip.updatedAt)} | ${reason} |`);
    } else {
      lines.push(`| \`${skip.branch}\` | ${skip.owner || "(unknown)"} | ${reason} |`);
    }
  }
  if (skips.length > 50) {
    lines.push(hasUpdatedAt
      ? `| ... |  |  | ${skips.length - 50} more skipped branch(es) omitted |`
      : `| ... |  | ${skips.length - 50} more skipped branch(es) omitted |`);
  }
}

function pushQueueSkipRows(lines, skips) {
  const hasUpdatedAt = (skips || []).some((skip) => skip.updatedAt);
  lines.push(
    "",
    "### Skips",
    "",
    hasUpdatedAt ? "| PR | Owner | Updated | Reason |" : "| PR | Owner | Reason |",
    hasUpdatedAt ? "|----|-------|---------|--------|" : "|----|-------|--------|",
  );
  for (const skip of skips.slice(0, 25)) {
    const reason = skip.reason.replace(/\|/g, "\\|");
    if (hasUpdatedAt) {
      lines.push(`| #${skip.number} | ${skip.owner || "(none)"} | ${shortDate(skip.updatedAt)} | ${reason} |`);
    } else {
      lines.push(`| #${skip.number} | ${skip.owner || "(none)"} | ${reason} |`);
    }
  }
  if (skips.length > 25) {
    lines.push(hasUpdatedAt
      ? `| ... |  |  | ${skips.length - 25} more skipped PR(s) omitted |`
      : `| ... |  | ${skips.length - 25} more skipped PR(s) omitted |`);
  }
}

function invalidatePullRequest(repository, pr, options) {
  if (options.dryRun) return { invalidated: false, skipped: false };
  const detailed = pr.statusCheckRollup ? pr : readPullRequest(repository, pr.number);
  const activeRun = activePendingQueueRun(repository, detailed, options);
  if (activeRun) return { invalidated: false, skipped: true, activeRun };
  postStatus(
    repository,
    pr.headRefOid,
    "pending",
    options.statusContext,
    `Waiting for ${options.base} synthetic merge test`,
    pr.url || "",
  );
  return { invalidated: true, skipped: false };
}

function invalidateOpen(repository, options) {
  const prs = readPullRequests(repository, options.base, options.maxPrs);
  let invalidated = 0;
  let skippedActiveRuns = 0;
  for (const pr of prs) {
    const result = invalidatePullRequest(repository, pr, options);
    if (result?.invalidated) invalidated += 1;
    if (result?.skipped) skippedActiveRuns += 1;
  }
  return { invalidated, skippedActiveRuns };
}

function cleanupQueueBranches(repository, options) {
  let deleted = 0;
  let wouldDelete = 0;
  let skippedOpen = 0;
  let skippedActiveRuns = 0;
  let skippedUnrecognized = 0;
  let supersededOpen = 0;
  const activeRuns = [];
  const deletions = [];
  const skips = [];
  const currentBaseOid = options.cleanupSupersededOpenQueueBranches
    ? readBranchOid(repository, options.base)
    : "";

  for (const queueBranchInfo of readRemoteQueueBranches(options)) {
    const metadata = queueBranchMetadata(queueBranchInfo.branch, options.queueBranchPrefix);
    if (!metadata) {
      skippedUnrecognized += 1;
      if (options.verbose) {
        skips.push({
          branch: queueBranchInfo.branch,
          reason: "unrecognized queue branch name",
          summaryReason: "unrecognized queue branch name",
        });
      }
      continue;
    }
    const { number } = metadata;

    const pullRequest = readPullRequestState(repository, number);
    if (String(pullRequest.state || "").toUpperCase() === "OPEN") {
      const owner = readPullRequestOwner(repository, number);
      const supersededReason = options.cleanupSupersededOpenQueueBranches
        ? supersededOpenQueueBranchReason(queueBranchInfo.branch, currentBaseOid, options.queueBranchPrefix)
        : null;
      if (!supersededReason) {
        const activeRun = activeBranchQueueRun(readBranchWorkflowRuns(repository, queueBranchInfo.branch));
        if (activeRun) {
          skippedActiveRuns += 1;
          activeRuns.push({
            branch: queueBranchInfo.branch,
            number,
            owner,
            runId: activeRun.databaseId || null,
            url: activeRun.url || "",
            status: activeRun.status || "",
            startedAt: activeRun.startedAt || activeRun.createdAt || "",
          });
          if (options.verbose) {
            skips.push({
              branch: queueBranchInfo.branch,
              owner,
              reason: `active queue run ${activeRun.databaseId || "(unknown)"}`,
              summaryReason: "active queue run",
              updatedAt: pullRequest.updatedAt || "",
            });
          }
          continue;
        }
        skippedOpen += 1;
        if (options.verbose) {
          skips.push({
            branch: queueBranchInfo.branch,
            owner,
            reason: `PR #${number} is open`,
            summaryReason: "open PR branch",
            updatedAt: pullRequest.updatedAt || "",
          });
        }
        continue;
      }
    }

    const activeRun = activeBranchQueueRun(readBranchWorkflowRuns(repository, queueBranchInfo.branch));
    if (activeRun) {
      skippedActiveRuns += 1;
      activeRuns.push({
        branch: queueBranchInfo.branch,
        number,
        owner: "",
        runId: activeRun.databaseId || null,
        url: activeRun.url || "",
        status: activeRun.status || "",
        startedAt: activeRun.startedAt || activeRun.createdAt || "",
      });
      if (options.verbose) {
        skips.push({
          branch: queueBranchInfo.branch,
          owner: "",
          reason: `active queue run ${activeRun.databaseId || "(unknown)"}`,
          summaryReason: "active queue run",
        });
      }
      continue;
    }

    if (String(pullRequest.state || "").toUpperCase() === "OPEN") {
      supersededOpen += 1;
    }
    deletions.push({
      branch: queueBranchInfo.branch,
      number,
      state: pullRequest.state,
      merged: Boolean(pullRequest.merged),
      supersededOpen: String(pullRequest.state || "").toUpperCase() === "OPEN",
    });
    if (options.dryRun) {
      wouldDelete += 1;
    } else {
      deleteRemoteBranch(queueBranchInfo.branch);
      deleted += 1;
    }
  }

  return {
    cleanupQueueBranches: true,
    activeRuns,
    deleted,
    deletions,
    dryRun: options.dryRun,
    skippedActiveRuns,
    skippedOpen,
    skippedUnrecognized,
    supersededOpen,
    skips,
    wouldDelete,
  };
}

function readQueueCandidates(repository, options) {
  return readPullRequests(repository, options.base, options.maxPrs)
    .sort((a, b) => a.number - b.number)
    .map((pr) => {
      const skipReason = queueSkipReason(pr, { kind: "passed" }, options.base);
      if (skipReason) return { pr, skipReason };
      const detailed = readPullRequest(repository, pr.number);
      const requiredState = requiredCheckState(detailed.statusCheckRollup, options.prRequiredChecks);
      const detailedSkipReason = queueSkipReason(detailed, requiredState, options.base);
      if (detailedSkipReason) return { pr: detailed, skipReason: detailedSkipReason };
      const activeRun = activePendingQueueRun(repository, detailed, options);
      if (activeRun) {
        return {
          pr: detailed,
          skipReason: `queue test already running (${activeRun.url})`,
        };
      }
      return {
        pr: detailed,
        skipReason: null,
      };
    });
}

function queueBranch(options, pr) {
  return `${options.queueBranchPrefix}/pr-${pr.number}`;
}

function prepareSyntheticMerge(repository, pr, baseOid, options) {
  const branch = queueBranch(options, pr);
  git(["fetch", "--no-tags", "origin", options.base, `pull/${pr.number}/head`], { stdio: "inherit" });
  git(["checkout", "-B", branch, baseOid], { stdio: "inherit" });
  git([
    "-c", "user.name=TSZ Merge Queue",
    "-c", "user.email=actions@github.com",
    "merge", "--no-ff", "--no-edit", "FETCH_HEAD",
  ], { stdio: "inherit" });
  const mergeOid = git(["rev-parse", "HEAD"]).trim();
  if (!options.dryRun) {
    git(["push", "--force-with-lease", "origin", `${mergeOid}:refs/heads/${branch}`], { stdio: "inherit" });
    runGh(["workflow", "run", "ci.yml", "--repo", repository, "--ref", branch]);
  }
  return { branch, mergeOid };
}

function commitStatusRollup(repository, sha) {
  const status = runGhJson([
    "api",
    `repos/${repository}/commits/${sha}/status`,
    "--jq",
    "{statuses: .statuses}",
  ]);
  const checks = runGhJson([
    "api",
    `repos/${repository}/commits/${sha}/check-runs?per_page=100`,
    "--jq",
    "{check_runs: .check_runs}",
  ]);
  return [
    ...(status.statuses || []).map((item) => ({
      context: item.context,
      state: item.state,
    })),
    ...(checks.check_runs || []).map((item) => ({
      name: item.name,
      status: item.status,
      conclusion: item.conclusion,
    })),
  ];
}

function waitForSyntheticChecks(repository, synthetic, options) {
  for (let attempt = 0; attempt < options.waitAttempts; attempt += 1) {
    const state = requiredCheckState(
      commitStatusRollup(repository, synthetic.mergeOid),
      options.mergeRequiredChecks,
    );
    if (state.kind === "passed") return state;
    if (state.kind === "failed") throw new Error(state.reason);
    if (attempt + 1 < options.waitAttempts) sleep(options.waitIntervalMs);
  }
  const activeRun = activeSyntheticQueueRun(
    readBranchWorkflowRuns(repository, synthetic.branch),
    synthetic.mergeOid,
  );
  if (activeRun) {
    return {
      kind: "pending",
      reason: `timed out while synthetic run ${activeRun.databaseId || "(unknown)"} is still active`,
      activeRun,
    };
  }
  throw new Error(`timed out waiting for synthetic merge check(s): ${options.mergeRequiredChecks.join(", ")}`);
}

function mergePullRequest(repository, pr) {
  runGh([
    "pr", "merge", String(pr.number),
    "--repo", repository,
    "--squash",
    "--match-head-commit", pr.headRefOid,
  ]);
}

function processOne(repository, options) {
  const candidates = readQueueCandidates(repository, options);
  const baseOid = readBranchOid(repository, options.base);
  const skips = [];

  for (const { pr, skipReason } of candidates) {
    if (skipReason) {
      if (options.verbose) skips.push({
        number: pr.number,
        owner: agentLabel(pr.labels),
        reason: skipReason,
        updatedAt: pr.updatedAt,
        url: pr.url,
      });
      continue;
    }

    if (options.dryRun) {
      return { dryRun: true, selected: pr, baseOid, skips };
    }

    invalidatePullRequest(repository, pr, options);
    try {
      const synthetic = prepareSyntheticMerge(repository, pr, baseOid, options);
      const syntheticState = waitForSyntheticChecks(repository, synthetic, options);
      if (syntheticState.activeRun) {
        postStatus(
          repository,
          pr.headRefOid,
          "pending",
          options.statusContext,
          `Waiting for synthetic run ${syntheticState.activeRun.databaseId || synthetic.mergeOid.slice(0, 12)}`,
          syntheticState.activeRun.url || "",
        );
        return { selected: pr, pendingSynthetic: true, synthetic, activeRun: syntheticState.activeRun, skips };
      }
      const refreshed = readPullRequest(repository, pr.number);
      const currentBase = readBranchOid(repository, options.base);
      if (currentBase !== baseOid) {
        invalidatePullRequest(repository, refreshed, options);
        return {
          selected: refreshed,
          baseMoved: true,
          oldBaseOid: baseOid,
          newBaseOid: currentBase,
          synthetic,
          skips,
        };
      }
      if (refreshed.headRefOid !== pr.headRefOid) {
        invalidatePullRequest(repository, refreshed, options);
        return { selected: refreshed, headMoved: true, synthetic, skips };
      }
      postStatus(
        repository,
        pr.headRefOid,
        "success",
        options.statusContext,
        `Synthetic merge ${synthetic.mergeOid.slice(0, 12)} passed`,
      );
      mergePullRequest(repository, pr);
      return { selected: pr, merged: true, synthetic, skips };
    } catch (error) {
      postStatus(
        repository,
        pr.headRefOid,
        "failure",
        options.statusContext,
        error instanceof Error ? error.message : String(error),
      );
      postComment(
        repository,
        pr.number,
        failureCommentBody(options.agentName, error instanceof Error ? error.message : String(error)),
      );
      return { selected: pr, failed: true, reason: error instanceof Error ? error.message : String(error), skips };
    }
  }

  return { selected: null, skips };
}

export function formatResult(result, options) {
  const lines = [
    "## Poor Man's Merge Queue",
    "",
    `Mode: ${options.dryRun ? "dry run" : "apply"}`,
    `Base: \`${options.base}\``,
    `Status: \`${options.statusContext}\``,
    "",
  ];
  if (result.invalidated !== undefined) {
    lines.push(`Invalidated ${result.invalidated} open PR head(s).`);
    if (result.skippedActiveRuns) {
      lines.push(`Preserved ${result.skippedActiveRuns} active queue run status(es).`);
    }
  } else if (result.cleanupQueueBranches) {
    if (result.dryRun) {
      lines.push(`Would delete ${result.wouldDelete} stale queue branch(es).`);
    } else {
      lines.push(`Deleted ${result.deleted} stale queue branch(es).`);
    }
    lines.push(`Skipped ${result.skippedOpen} open PR branch(es).`);
    lines.push(`Preserved ${result.skippedActiveRuns} branch(es) with active queue runs.`);
    if (result.supersededOpen) {
      lines.push(`Included ${result.supersededOpen} superseded open PR branch(es).`);
    }
    if (result.skippedUnrecognized) {
      lines.push(`Skipped ${result.skippedUnrecognized} unrecognized queue branch name(s).`);
    }
    if (options.verbose && result.deletions?.length) {
      lines.push("", "### Queue Branches", "", "| Branch | PR | State |", "|--------|----|-------|");
      for (const deletion of result.deletions.slice(0, 50)) {
        const state = deletion.supersededOpen
          ? "open superseded"
          : deletion.merged ? "merged" : String(deletion.state || "closed").toLowerCase();
        lines.push(`| \`${deletion.branch}\` | #${deletion.number} | ${state} |`);
      }
    }
    if (options.verbose && result.activeRuns?.length) {
      const now = result.now || new Date().toISOString();
      lines.push(
        "",
        "### Active Queue Runs",
        "",
        "| Branch | PR | Owner | Run | Status | Started | Age |",
        "|--------|----|-------|-----|--------|---------|-----|",
      );
      for (const run of result.activeRuns.slice(0, 50)) {
        const runId = run.runId || "(unknown)";
        const runLink = run.url ? `[${runId}](${run.url})` : runId;
        lines.push(`| \`${run.branch}\` | #${run.number} | ${run.owner || "(unknown)"} | ${runLink} | ${String(run.status || "unknown").toLowerCase()} | ${shortDateTime(run.startedAt)} | ${elapsedAge(run.startedAt, now)} |`);
      }
    }
    if (options.verbose && result.skips?.length) {
      const summary = skipReasonCounts(result.skips);
      lines.push("", "### Skip Reason Counts", "", "| Count | Reason |", "|-------|--------|");
      for (const entry of summary) {
        lines.push(`| ${entry.count} | ${entry.reason.replace(/\|/g, "\\|")} |`);
      }
      pushSkipOwnerCounts(lines, result.skips);
      pushSkipOwnerReasonCounts(lines, result.skips);
      pushCleanupSkipRows(lines, result.skips);
    }
  } else if (!result.selected) {
    lines.push("No queue-ready auto-merge PR found.");
  } else if (result.dryRun) {
    lines.push(`Would synthetic-test and merge #${result.selected.number} at \`${result.selected.headRefOid.slice(0, 12)}\`.`);
  } else if (result.merged) {
    lines.push(`Merged #${result.selected.number} after synthetic merge \`${result.synthetic.mergeOid.slice(0, 12)}\` passed.`);
  } else if (result.baseMoved) {
    lines.push(`Retest needed for #${result.selected.number}: base moved from \`${result.oldBaseOid.slice(0, 12)}\` to \`${result.newBaseOid.slice(0, 12)}\`.`);
  } else if (result.headMoved) {
    lines.push(`Retest needed for #${result.selected.number}: PR head moved during queue test.`);
  } else if (result.pendingSynthetic) {
    const runId = result.activeRun?.databaseId || "(unknown)";
    const runUrl = result.activeRun?.url ? ` (${result.activeRun.url})` : "";
    lines.push(`Preserved #${result.selected.number}: synthetic run ${runId}${runUrl} is still active after the queue wait window.`);
  } else if (result.failed) {
    lines.push(`Failed #${result.selected.number}: ${result.reason}`);
  }
  if (!result.cleanupQueueBranches && options.verbose && result.skips?.length) {
    const summary = skipReasonCounts(result.skips);
    lines.push("", "### Skip Reason Counts", "", "| Count | Reason |", "|-------|--------|");
    for (const entry of summary) {
      lines.push(`| ${entry.count} | ${entry.reason.replace(/\|/g, "\\|")} |`);
    }
    pushSkipOwnerCounts(lines, result.skips);
    pushSkipOwnerReasonCounts(lines, result.skips);
    pushQueueSkipRows(lines, result.skips);
  }
  return `${lines.join("\n")}\n`;
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (!options.repository) throw new Error("--repository or GITHUB_REPOSITORY is required");
  let result;
  if (options.invalidatePr !== null) {
    const pr = readPullRequest(options.repository, options.invalidatePr);
    const invalidation = invalidatePullRequest(options.repository, pr, options);
    result = {
      invalidated: invalidation?.invalidated ? 1 : 0,
      skippedActiveRuns: invalidation?.skipped ? 1 : 0,
    };
  } else if (options.cleanupQueueBranches) {
    result = cleanupQueueBranches(options.repository, options);
  } else if (options.invalidateOpen) {
    result = invalidateOpen(options.repository, options);
  } else {
    result = processOne(options.repository, options);
  }
  console.log(formatResult(result, options));
}

if (import.meta.url === `file://${process.argv[1]}`) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
