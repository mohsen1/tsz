#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { readyStateFailures } from "./check-pr-ready-state.mjs";

const DEFAULT_BASE = "main";
const DEFAULT_MAX_PRS = 200;
const DEFAULT_MAX_REFRESHES = 1;
const DEFAULT_GH_MAX_BUFFER_BYTES = 24 * 1024 * 1024;
const DEFAULT_POLL_ATTEMPTS = 12;
const DEFAULT_POLL_INTERVAL_MS = 5000;
const SUCCESSFUL_CHECK_RUN_CONCLUSIONS = new Set(["SUCCESS", "NEUTRAL", "SKIPPED"]);
const SUCCESSFUL_STATUS_STATES = new Set(["SUCCESS"]);
const REQUIRED_CHECKS = ["CI Summary"];

function usage() {
  return [
    "usage: refresh-green-prs.mjs [--repository owner/repo] [--base main] [--max-prs n] [--max-refreshes n] [--required-check name] [--dry-run] [--ignore-in-flight]",
    "",
    "Refreshes at most one ready, green PR that is behind the base branch, then",
    "enables squash auto-merge for the refreshed head. The branch update must",
    "trigger the normal pull_request CI so required checks attach to the PR.",
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
    ignoreInFlight: false,
    maxPrs: DEFAULT_MAX_PRS,
    maxRefreshes: DEFAULT_MAX_REFRESHES,
    pollAttempts: DEFAULT_POLL_ATTEMPTS,
    pollIntervalMs: DEFAULT_POLL_INTERVAL_MS,
    repository: process.env.REPOSITORY || process.env.GITHUB_REPOSITORY || null,
    requiredChecks: [...REQUIRED_CHECKS],
    verbose: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--repository") {
      options.repository = argv[++index];
      if (!options.repository) throw new Error("--repository requires owner/repo");
      continue;
    }
    if (arg === "--base") {
      options.base = argv[++index];
      if (!options.base) throw new Error("--base requires a branch name");
      continue;
    }
    if (arg === "--max-prs") {
      options.maxPrs = parsePositiveInt("--max-prs", argv[++index]);
      continue;
    }
    if (arg === "--max-refreshes") {
      options.maxRefreshes = parsePositiveInt("--max-refreshes", argv[++index]);
      continue;
    }
    if (arg === "--required-check") {
      const requiredCheck = argv[++index];
      if (!requiredCheck) throw new Error("--required-check requires a check name");
      options.requiredChecks.push(requiredCheck);
      continue;
    }
    if (arg === "--no-default-required-check") {
      options.requiredChecks = [];
      continue;
    }
    if (arg === "--poll-attempts") {
      options.pollAttempts = parsePositiveInt("--poll-attempts", argv[++index]);
      continue;
    }
    if (arg === "--poll-interval-ms") {
      options.pollIntervalMs = parsePositiveInt("--poll-interval-ms", argv[++index]);
      continue;
    }
    if (arg === "--dry-run") {
      options.dryRun = true;
      continue;
    }
    if (arg === "--ignore-in-flight") {
      options.ignoreInFlight = true;
      continue;
    }
    if (arg === "--verbose") {
      options.verbose = true;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      console.log(usage());
      process.exit(0);
    }
    throw new Error(`unknown argument: ${arg}`);
  }

  return options;
}

function runGh(args) {
  const result = spawnSync("gh", args, {
    encoding: "utf8",
    maxBuffer: DEFAULT_GH_MAX_BUFFER_BYTES,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error) {
    const command = `gh ${args.join(" ")}`;
    if (result.error.code === "ENOBUFS") {
      throw new Error(`${command} exceeded ${DEFAULT_GH_MAX_BUFFER_BYTES} bytes of output`);
    }
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(
      [
        `gh ${args.join(" ")} failed`,
        result.stdout.trim(),
        result.stderr.trim(),
      ].filter(Boolean).join("\n"),
    );
  }
  return result.stdout;
}

function runGhJson(args) {
  return JSON.parse(runGh(args));
}

function checkName(check) {
  return check.name || check.context || "(unnamed)";
}

function normalizeState(value) {
  return String(value || "").toUpperCase();
}

function checkStatus(check) {
  const typeName = check.__typename || check.type || "";
  if (typeName === "StatusContext" || "state" in check) {
    const state = normalizeState(check.state);
    if (SUCCESSFUL_STATUS_STATES.has(state)) return { kind: "passed" };
    if (["PENDING", "EXPECTED"].includes(state)) {
      return { kind: "pending", detail: state.toLowerCase() };
    }
    return { kind: "failed", detail: state.toLowerCase() || "unknown" };
  }

  const status = normalizeState(check.status);
  if (status && status !== "COMPLETED") {
    return { kind: "pending", detail: status.toLowerCase() };
  }

  const conclusion = normalizeState(check.conclusion);
  if (SUCCESSFUL_CHECK_RUN_CONCLUSIONS.has(conclusion)) {
    return { kind: "passed" };
  }
  if (!conclusion) return { kind: "pending", detail: "missing conclusion" };
  return { kind: "failed", detail: conclusion.toLowerCase() };
}

export function checkRollupState(statusCheckRollup, requiredChecks = REQUIRED_CHECKS) {
  const checks = Array.isArray(statusCheckRollup) ? statusCheckRollup : [];
  if (checks.length === 0) {
    return {
      kind: "missing",
      reason: "no status checks reported for the head commit",
      pending: [],
      failed: [],
      missingRequired: [...requiredChecks],
    };
  }

  const pending = [];
  const failed = [];
  const passedNames = new Set();

  for (const check of checks) {
    const name = checkName(check);
    const state = checkStatus(check);
    if (state.kind === "passed") {
      passedNames.add(name);
    } else if (state.kind === "pending") {
      pending.push(`${name} (${state.detail})`);
    } else {
      failed.push(`${name} (${state.detail})`);
    }
  }

  const missingRequired = requiredChecks.filter((name) => !passedNames.has(name));
  if (failed.length > 0) {
    return { kind: "failed", reason: `failing checks: ${failed.join(", ")}`, pending, failed, missingRequired };
  }
  if (pending.length > 0) {
    return { kind: "pending", reason: `pending checks: ${pending.join(", ")}`, pending, failed, missingRequired };
  }
  if (missingRequired.length > 0) {
    return {
      kind: "missing",
      reason: `missing required green check(s): ${missingRequired.join(", ")}`,
      pending,
      failed,
      missingRequired,
    };
  }

  return { kind: "passed", reason: "all checks passed", pending, failed, missingRequired };
}

function labelNames(labels) {
  return Array.isArray(labels)
    ? labels.map((label) => typeof label === "string" ? label : label?.name).filter(Boolean)
    : [];
}

function hasAutoMerge(pr) {
  return pr.autoMergeRequest !== null && pr.autoMergeRequest !== undefined;
}

function summarySkipReason(pr, options = {}) {
  if (pr.baseRefName !== options.base) {
    return `base is ${pr.baseRefName || "(unknown)"}, not ${options.base}`;
  }
  if (pr.isDraft === true) return "draft PR";
  if (pr.isCrossRepository === true) return "cross-repository PR";

  const readinessFailures = readyStateFailures({
    number: pr.number,
    title: pr.title,
    body: "",
    draft: pr.isDraft,
    labels: labelNames(pr.labels),
  });
  if (readinessFailures.length > 0) {
    return `ready-state WIP marker: ${readinessFailures.join(", ")}`;
  }

  return null;
}

export function findInFlightPullRequest(prs, options = {}) {
  if (options.ignoreInFlight) return null;
  const requiredChecks = options.requiredChecks || REQUIRED_CHECKS;
  return [...prs]
    .sort((a, b) => a.number - b.number)
    .find((pr) =>
      pr.baseRefName === options.base
        && pr.isDraft !== true
        && hasAutoMerge(pr)
        && checkRollupState(pr.statusCheckRollup, requiredChecks).kind === "pending"
    ) || null;
}

export function autoMergeInFlightReason(pr, compare, options = {}) {
  if (options.ignoreInFlight) return null;
  if (pr.baseRefName !== options.base || pr.isDraft === true || !hasAutoMerge(pr)) {
    return null;
  }

  const rollup = checkRollupState(pr.statusCheckRollup, options.requiredChecks || REQUIRED_CHECKS);
  if (rollup.kind === "pending") return "checks are still pending";

  const behind = compareBehindState(compare);
  if (rollup.kind === "missing") {
    if (behind.behindBy <= 0) {
      return "required checks are not reported yet for the current auto-merge head";
    }
    return null;
  }
  if (rollup.kind !== "passed") return null;

  if (behind.behindBy <= 0) {
    return "auto-merge is armed on a current head";
  }

  return null;
}

export function cheapSkipReason(pr, options = {}) {
  if (pr.baseRefName !== options.base) {
    return `base is ${pr.baseRefName || "(unknown)"}, not ${options.base}`;
  }
  if (pr.isDraft === true) return "draft PR";
  if (pr.isCrossRepository === true) return "cross-repository PR";

  const readinessFailures = readyStateFailures({
    number: pr.number,
    title: pr.title,
    body: pr.body,
    draft: pr.isDraft,
    labels: labelNames(pr.labels),
  });
  if (readinessFailures.length > 0) {
    return `ready-state WIP marker: ${readinessFailures.join(", ")}`;
  }

  const rollup = checkRollupState(pr.statusCheckRollup, options.requiredChecks || REQUIRED_CHECKS);
  if (rollup.kind !== "passed") return rollup.reason;

  return null;
}

export function compareBehindState(compare) {
  const behindBy = Number(compare?.behind_by ?? compare?.behindBy ?? 0);
  const aheadBy = Number(compare?.ahead_by ?? compare?.aheadBy ?? 0);
  return {
    aheadBy: Number.isFinite(aheadBy) ? aheadBy : 0,
    behindBy: Number.isFinite(behindBy) ? behindBy : 0,
    status: compare?.status || "unknown",
  };
}

export function candidateFromCompare(pr, compare) {
  const state = compareBehindState(compare);
  if (state.behindBy <= 0) {
    return {
      eligible: false,
      reason: "head already contains the latest base",
      compare: state,
      pr,
    };
  }
  return {
    eligible: true,
    reason: `behind ${state.behindBy} commit(s) and ahead ${state.aheadBy} commit(s)`,
    compare: state,
    pr,
  };
}

function readPullRequests(repository, base, maxPrs) {
  if (!repository) throw new Error("REPOSITORY or GITHUB_REPOSITORY is required");
  return runGhJson([
    "pr",
    "list",
    "--repo",
    repository,
    "--state",
    "open",
    "--base",
    base,
    "--limit",
    String(maxPrs),
    "--json",
    [
      "autoMergeRequest",
      "baseRefName",
      "headRefName",
      "headRefOid",
      "isCrossRepository",
      "isDraft",
      "labels",
      "mergeStateStatus",
      "number",
      "title",
      "url",
    ].join(","),
  ]);
}

function readPullRequest(repository, number) {
  return runGhJson([
    "pr",
    "view",
    String(number),
    "--repo",
    repository,
    "--json",
    [
      "autoMergeRequest",
      "baseRefName",
      "body",
      "headRefName",
      "headRefOid",
      "isDraft",
      "labels",
      "number",
      "statusCheckRollup",
      "title",
      "url",
    ].join(","),
  ]);
}

function readBranchOid(repository, branch) {
  const branchState = runGhJson([
    "api",
    "-H",
    "Accept: application/vnd.github+json",
    `repos/${repository}/branches/${branch}`,
    "--jq",
    "{oid: .commit.sha}",
  ]);
  if (!branchState.oid) {
    throw new Error(`could not resolve ${branch} for ${repository}`);
  }
  return branchState.oid;
}

function readCompare(repository, baseOid, headOid) {
  return runGhJson([
    "api",
    "-H",
    "Accept: application/vnd.github+json",
    `repos/${repository}/compare/${baseOid}...${headOid}`,
    "--jq",
    "{status, ahead_by, behind_by}",
  ]);
}

function updateBranch(repository, pr) {
  return runGhJson([
    "api",
    "-X",
    "PUT",
    "-H",
    "Accept: application/vnd.github+json",
    `repos/${repository}/pulls/${pr.number}/update-branch`,
    "-f",
    `expected_head_sha=${pr.headRefOid}`,
  ]);
}

function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function waitForUpdatedHead(repository, pr, options) {
  for (let attempt = 0; attempt < options.pollAttempts; attempt += 1) {
    const refreshed = readPullRequest(repository, pr.number);
    if (refreshed.headRefOid && refreshed.headRefOid !== pr.headRefOid) return refreshed;
    if (attempt + 1 < options.pollAttempts) sleep(options.pollIntervalMs);
  }
  throw new Error(`PR #${pr.number} branch update did not produce a new head SHA`);
}

function enableAutoMerge(repository, pr) {
  runGh([
    "pr",
    "merge",
    String(pr.number),
    "--repo",
    repository,
    "--squash",
    "--auto",
    "--match-head-commit",
    pr.headRefOid,
  ]);
}

function markdownLink(label, url) {
  return url ? `[${label}](${url})` : label;
}

export function formatResultReport(result) {
  const lines = [
    "## Refresh Green PRs",
    "",
    `Mode: ${result.dryRun ? "dry run" : "apply"}`,
    `Base: \`${result.base}\``,
    "",
  ];

  if (result.inFlight) {
    lines.push(
      `One-by-one guard: waiting for ${markdownLink(`#${result.inFlight.number}`, result.inFlight.url)} because ${result.inFlightReason || "auto-merge is already enabled"}.`,
    );
    return `${lines.join("\n")}\n`;
  }

  if (result.refreshed.length > 0) {
    lines.push("| PR | Action | Old head | New head |");
    lines.push("|----|--------|----------|----------|");
    for (const item of result.refreshed) {
      lines.push([
        markdownLink(`#${item.number}`, item.url),
        result.dryRun ? "would refresh and enable auto-merge" : "refreshed and armed auto-merge",
        `\`${item.oldHead.slice(0, 12)}\``,
        item.newHead ? `\`${item.newHead.slice(0, 12)}\`` : result.dryRun ? "`pending`" : "`unknown`",
      ].join(" | ").replace(/^/, "| ").replace(/$/, " |"));
    }
  } else {
    lines.push("No stale green ready PR was refreshed.");
  }

  if (result.failures.length > 0) {
    lines.push("");
    lines.push("### Skipped After Attempt");
    lines.push("");
    lines.push("| PR | Reason |");
    lines.push("|----|--------|");
    for (const failure of result.failures) {
      lines.push(`| ${markdownLink(`#${failure.number}`, failure.url)} | ${failure.reason.replace(/\|/g, "\\|")} |`);
    }
  }

  if (result.verboseSkips.length > 0) {
    lines.push("");
    lines.push("### Other Skips");
    lines.push("");
    lines.push("| PR | Reason |");
    lines.push("|----|--------|");
    for (const skip of result.verboseSkips.slice(0, 20)) {
      lines.push(`| ${markdownLink(`#${skip.number}`, skip.url)} | ${skip.reason.replace(/\|/g, "\\|")} |`);
    }
  }

  return `${lines.join("\n")}\n`;
}

export function selectSortedPullRequests(prs) {
  return [...prs].sort((a, b) => a.number - b.number);
}

function refreshPullRequests(options) {
  const prs = readPullRequests(options.repository, options.base, options.maxPrs);
  const currentBaseOid = readBranchOid(options.repository, options.base);
  const result = {
    base: options.base,
    dryRun: options.dryRun,
    failures: [],
    inFlight: null,
    inFlightReason: null,
    refreshed: [],
    verboseSkips: [],
  };

  for (const pr of selectSortedPullRequests(prs)) {
    if (options.ignoreInFlight) break;
    if (pr.baseRefName !== options.base || pr.isDraft === true || !hasAutoMerge(pr)) continue;
    const hydrated = readPullRequest(options.repository, pr.number);
    const rollup = checkRollupState(hydrated.statusCheckRollup, options.requiredChecks);
    const compare = rollup.kind === "passed" || rollup.kind === "missing"
      ? readCompare(options.repository, currentBaseOid, hydrated.headRefOid)
      : null;
    const inFlightReason = autoMergeInFlightReason(hydrated, compare, options);
    if (inFlightReason) {
      result.inFlight = hydrated;
      result.inFlightReason = inFlightReason;
      break;
    }
  }
  if (result.inFlight) return result;

  for (const pr of selectSortedPullRequests(prs)) {
    if (result.refreshed.length >= options.maxRefreshes) break;

    const summaryReason = summarySkipReason(pr, options);
    if (summaryReason) {
      if (options.verbose) result.verboseSkips.push({ number: pr.number, reason: summaryReason, url: pr.url });
      continue;
    }

    const hydrated = readPullRequest(options.repository, pr.number);
    const skipReason = cheapSkipReason(hydrated, options);
    if (skipReason) {
      if (options.verbose) result.verboseSkips.push({ number: pr.number, reason: skipReason, url: pr.url });
      continue;
    }

    const candidate = candidateFromCompare(
      hydrated,
      readCompare(options.repository, currentBaseOid, hydrated.headRefOid),
    );
    if (!candidate.eligible) {
      if (options.verbose) result.verboseSkips.push({ number: pr.number, reason: candidate.reason, url: pr.url });
      continue;
    }

    if (options.dryRun) {
      result.refreshed.push({
        number: pr.number,
        oldHead: hydrated.headRefOid,
        newHead: null,
        url: pr.url,
      });
      break;
    }

    let branchUpdateAccepted = false;
    try {
      updateBranch(options.repository, hydrated);
      branchUpdateAccepted = true;
      const refreshed = waitForUpdatedHead(options.repository, hydrated, options);
      if (!hasAutoMerge(refreshed)) {
        enableAutoMerge(options.repository, refreshed);
      }
      result.refreshed.push({
        number: pr.number,
        oldHead: hydrated.headRefOid,
        newHead: refreshed.headRefOid,
        url: pr.url,
      });
    } catch (error) {
      result.failures.push({
        number: pr.number,
        reason: error instanceof Error ? error.message : String(error),
        url: pr.url,
      });
      if (branchUpdateAccepted) break;
    }
  }

  return result;
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const result = refreshPullRequests(options);
  console.log(formatResultReport(result));
}

if (import.meta.url === `file://${process.argv[1]}`) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
