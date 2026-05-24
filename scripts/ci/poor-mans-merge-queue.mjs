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
    "  --pr-required-check <name>      PR-head check required before queueing",
    "  --merge-required-check <name>   Synthetic merge check required before merge",
    "  --no-default-pr-required-checks",
    "  --no-default-merge-required-checks",
    "  --invalidate-open               Mark open PR heads pending and exit",
    "  --invalidate-pr <number>        Mark one PR head pending and exit",
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
    base: process.env.BASE_BRANCH || DEFAULT_BASE,
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
      "url",
    ].join(","),
  ]);
}

function readWorkflowRun(repository, runId) {
  return runGhJson([
    "run", "view", runId,
    "--repo", repository,
    "--json", "status,conclusion,url",
  ]);
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

function invalidatePullRequest(repository, pr, options) {
  if (options.dryRun) return;
  postStatus(
    repository,
    pr.headRefOid,
    "pending",
    options.statusContext,
    `Waiting for ${options.base} synthetic merge test`,
  );
}

function invalidateOpen(repository, options) {
  const prs = readPullRequests(repository, options.base, options.maxPrs);
  for (const pr of prs) invalidatePullRequest(repository, pr, options);
  return { invalidated: prs.length };
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
      const pendingRun = pendingQueueRun(detailed, options.statusContext);
      if (pendingRun) {
        const run = readWorkflowRun(repository, pendingRun.runId);
        if (normalize(run.status) !== "COMPLETED") {
          return {
            pr: detailed,
            skipReason: `queue test already running (${run.url || pendingRun.targetUrl})`,
          };
        }
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

function waitForSyntheticChecks(repository, mergeOid, options) {
  for (let attempt = 0; attempt < options.waitAttempts; attempt += 1) {
    const state = requiredCheckState(
      commitStatusRollup(repository, mergeOid),
      options.mergeRequiredChecks,
    );
    if (state.kind === "passed") return state;
    if (state.kind === "failed") throw new Error(state.reason);
    if (attempt + 1 < options.waitAttempts) sleep(options.waitIntervalMs);
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
      if (options.verbose) skips.push({ number: pr.number, reason: skipReason, url: pr.url });
      continue;
    }

    if (options.dryRun) {
      return { dryRun: true, selected: pr, baseOid, skips };
    }

    invalidatePullRequest(repository, pr, options);
    try {
      const synthetic = prepareSyntheticMerge(repository, pr, baseOid, options);
      waitForSyntheticChecks(repository, synthetic.mergeOid, options);
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
      postComment(repository, pr.number, [
        "AgentName: GPT-5.5",
        "",
        "Poor man's merge queue could not land this PR.",
        "",
        `Reason: ${error instanceof Error ? error.message : String(error)}`,
      ].join("\n"));
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
  } else if (result.failed) {
    lines.push(`Failed #${result.selected.number}: ${result.reason}`);
  }
  if (options.verbose && result.skips?.length) {
    lines.push("", "### Skips", "", "| PR | Reason |", "|----|--------|");
    for (const skip of result.skips.slice(0, 25)) {
      lines.push(`| #${skip.number} | ${skip.reason.replace(/\|/g, "\\|")} |`);
    }
  }
  return `${lines.join("\n")}\n`;
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (!options.repository) throw new Error("--repository or GITHUB_REPOSITORY is required");
  let result;
  if (options.invalidatePr !== null) {
    const pr = readPullRequest(options.repository, options.invalidatePr);
    invalidatePullRequest(options.repository, pr, options);
    result = { invalidated: 1 };
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
