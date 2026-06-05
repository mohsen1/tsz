#!/usr/bin/env node
import fs from "node:fs";
import { spawnSync } from "node:child_process";

const REQUIRED_PR_CHECKS = ["CI Summary"];
const DEFAULT_DRAFT_STALE_HOURS = 24;
const DEFAULT_UNSTACKED_DRAFT_BUDGET = 2;

function fail(message) {
  console.error(`error: ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const args = {
    fixture: null,
    outputJson: null,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--fixture") {
      args.fixture = argv[++index] ?? null;
    } else if (arg === "--json") {
      args.outputJson = argv[++index] ?? null;
    } else if (arg === "--help" || arg === "-h") {
      console.log("usage: pr-ownership-report.mjs [--fixture open-prs.json] [--json output.json]");
      process.exit(0);
    } else {
      fail(`unknown argument: ${arg}`);
    }
  }
  return args;
}

function normalizePr(raw) {
  const rawLabels = Array.isArray(raw.labels?.nodes) ? raw.labels.nodes : raw.labels;
  const labels = Array.isArray(rawLabels)
    ? rawLabels.map((label) => (typeof label === "string" ? label : label?.name)).filter(Boolean)
    : [];
  const statusCheckRollup = Array.isArray(raw.statusCheckRollup)
    ? raw.statusCheckRollup.map(normalizeCheck).filter(Boolean)
    : [];
  return {
    number: Number(raw.number),
    title: String(raw.title ?? ""),
    isDraft: Boolean(raw.isDraft),
    updatedAt: raw.updatedAt ? String(raw.updatedAt) : null,
    baseRefName: String(raw.baseRefName || "main"),
    headRefName: String(raw.headRefName || ""),
    mergeStateStatus: String(raw.mergeStateStatus || "UNKNOWN"),
    mergeable: String(raw.mergeable || "UNKNOWN"),
    autoMergeArmed: Boolean(raw.autoMergeRequest),
    inMergeQueue: Boolean(raw.isInMergeQueue),
    mergeQueueEnabled: Boolean(raw.isMergeQueueEnabled),
    labels,
    statusCheckRollup,
    body: String(raw.body ?? ""),
  };
}

function normalizeCheck(raw) {
  if (!raw || typeof raw !== "object") return null;
  return {
    name: String(raw.name || raw.context || ""),
    state: raw.state ? String(raw.state).toUpperCase() : "",
    status: raw.status ? String(raw.status).toUpperCase() : "",
    conclusion: raw.conclusion ? String(raw.conclusion).toUpperCase() : "",
  };
}

function loadPulls(fixture) {
  if (fixture) {
    return JSON.parse(fs.readFileSync(fixture, "utf8")).map(normalizePr);
  }

  const query = `
    query($owner: String!, $name: String!, $after: String) {
      repository(owner: $owner, name: $name) {
        pullRequests(first: 100, after: $after, states: OPEN, orderBy: {field: UPDATED_AT, direction: DESC}) {
          pageInfo {
            hasNextPage
            endCursor
          }
          nodes {
            number
            title
            isDraft
            updatedAt
            baseRefName
            headRefName
            labels(first: 50) { nodes { name } }
            body
            mergeStateStatus
            mergeable
            autoMergeRequest { enabledAt }
            isInMergeQueue
            isMergeQueueEnabled
          }
        }
      }
    }
  `;
  const repository = process.env.GITHUB_REPOSITORY || "tsz-org/tsz";
  const [owner, name] = repository.split("/", 2);
  if (!owner || !name) {
    fail(`invalid GITHUB_REPOSITORY: ${repository}`);
  }

  const pulls = [];
  let after = null;
  for (;;) {
    const args = [
      "api",
      "graphql",
      "-f",
      `owner=${owner}`,
      "-f",
      `name=${name}`,
      "-f",
      `query=${query}`,
    ];
    if (after) {
      args.push("-f", `after=${after}`);
    }
    const result = spawnSync("gh", args, { encoding: "utf8" });
    if (result.status !== 0) {
      fail(result.stderr.trim() || "gh api graphql failed");
    }
    const parsed = JSON.parse(result.stdout);
    const connection = parsed.data?.repository?.pullRequests;
    if (!connection) {
      fail("gh api graphql response did not include pullRequests");
    }
    pulls.push(...connection.nodes);
    if (!connection.pageInfo?.hasNextPage) {
      break;
    }
    after = connection.pageInfo.endCursor;
  }
  return pulls.map(normalizePr);
}

function shouldHydrateRequiredChecks(pr) {
  return (
    !pr.isDraft &&
    pr.baseRefName === "main" &&
    !pr.inMergeQueue &&
    !isWipPr({ labels: pr.labels, title: pr.title })
  );
}

function loadRequiredChecks(number) {
  const result = spawnSync(
    "gh",
    ["pr", "view", String(number), "--json", "statusCheckRollup"],
    { encoding: "utf8" },
  );
  if (result.status !== 0) {
    console.error(
      `warning: could not load status checks for PR #${number}: ${result.stderr.trim() || "gh pr view failed"}`,
    );
    return [];
  }
  try {
    const parsed = JSON.parse(result.stdout);
    return Array.isArray(parsed.statusCheckRollup)
      ? parsed.statusCheckRollup.map(normalizeCheck).filter(Boolean)
      : [];
  } catch (error) {
    console.error(`warning: could not parse status checks for PR #${number}: ${error.message}`);
    return [];
  }
}

function hydrateRequiredChecks(pulls) {
  for (const pr of pulls) {
    if (shouldHydrateRequiredChecks(pr)) {
      pr.statusCheckRollup = loadRequiredChecks(pr.number);
    }
  }
  return pulls;
}

function agentNameFrom(body) {
  const match = /^AgentName:[^\S\r\n]*(\S+)?/m.exec(body);
  return match?.[1] ?? null;
}

function agentLabelsFrom(labels) {
  return labels
    .filter((label) => label.startsWith("agent:"))
    .map((label) => label.slice("agent:".length))
    .sort();
}

function issueRefsFrom(text) {
  return [...String(text).matchAll(/#(\d+)/g)].map((match) => Number(match[1]));
}

function uniqueIssueRefs(refs, prNumber) {
  return [...new Set(refs.filter((issue) => issue !== prNumber))].sort((a, b) => a - b);
}

function claimedIssueRefsFromBody(body) {
  const refs = [];
  for (const match of String(body).matchAll(
    /\b(?:addresses?|closes?|fix(?:es)?|resolves?)\b[^\r\n.]*/gi,
  )) {
    refs.push(...issueRefsFrom(match[0]));
  }
  return refs;
}

function claimedIssueRefsFrom(pr) {
  return uniqueIssueRefs([...issueRefsFrom(pr.title), ...claimedIssueRefsFromBody(pr.body)], pr.number);
}

function titleScope(title) {
  return String(title)
    .toLowerCase()
    .replace(/^\[wip\]\s*/, "")
    .replace(/\(#\d+\)\s*$/g, "")
    .replace(/#\d+/g, "#")
    .replace(/\s+/g, " ")
    .trim();
}

function prSummary(report, number, prefix = "#") {
  const pr = report.prs.find((candidate) => candidate.number === number);
  if (!pr) {
    return `${prefix}${number}`;
  }
  const state = pr.draft ? "draft" : "ready";
  const wip = isWipPr(pr) ? ", WIP" : "";
  const owner = pr.agentName ?? "no AgentName";
  const stack = pr.stackRole ? `, ${pr.stackRole}` : "";
  return `${prefix}${number} (${state}${wip}, ${owner}${stack})`;
}

function draftStackState(draftCount, stackedDraftCount) {
  if (draftCount === 0) {
    return "no draft PRs";
  }
  if (draftCount === 1) {
    return stackedDraftCount === 1 ? "single stacked draft" : "single unstacked draft";
  }
  if (stackedDraftCount === 0) {
    return "unstacked drafts";
  }
  if (stackedDraftCount === draftCount) {
    return "stacked-only drafts";
  }
  return "mixed stacked/unstacked drafts";
}

function ownerCounts(prs) {
  return [...prs
    .reduce((counts, pr) => {
      const owner = ownerOf(pr);
      counts.set(owner, (counts.get(owner) || 0) + 1);
      return counts;
    }, new Map())
    .entries()]
    .map(([owner, count]) => ({ owner, count }))
    .sort((a, b) => b.count - a.count || a.owner.localeCompare(b.owner));
}

function ownerCountsWithOldestUpdated(prs) {
  return [...prs
    .reduce((counts, pr) => {
      const owner = ownerOf(pr);
      const current = counts.get(owner) || { count: 0, oldestUpdatedAt: null };
      current.count += 1;
      if (pr.updatedAt && (!current.oldestUpdatedAt || pr.updatedAt < current.oldestUpdatedAt)) {
        current.oldestUpdatedAt = pr.updatedAt;
      }
      counts.set(owner, current);
      return counts;
    }, new Map())
    .entries()]
    .map(([owner, data]) => ({ owner, count: data.count, oldestUpdatedAt: data.oldestUpdatedAt }))
    .sort((a, b) => b.count - a.count || a.owner.localeCompare(b.owner));
}

function ownerOf(pr) {
  if (pr.agentLabel) {
    return pr.agentLabel;
  }
  if (Array.isArray(pr.agentLabels) && pr.agentLabels.length === 1) {
    return `agent:${pr.agentLabels[0]}`;
  }
  return pr.agentName || "unowned";
}

function isWipPr(pr) {
  return pr.labels.includes("WIP") || /^\[wip\]/i.test(pr.title);
}

function wipMarkers(pr) {
  const markers = [];
  if (pr.labels.includes("WIP")) {
    markers.push("label");
  }
  if (/^\[wip\]/i.test(pr.title)) {
    markers.push("title");
  }
  return markers;
}

function shortDate(value) {
  return value ? value.slice(0, 10) : "unknown";
}

function reportNow() {
  return process.env.TSZ_PR_OWNERSHIP_REPORT_NOW || new Date().toISOString();
}

function elapsedAge(updatedAt, now) {
  const updatedMs = Date.parse(updatedAt || "");
  const nowMs = Date.parse(now || "");
  if (!Number.isFinite(updatedMs) || !Number.isFinite(nowMs)) return "unknown";

  const totalMinutes = Math.max(0, Math.floor((nowMs - updatedMs) / 60_000));
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

function ownerCountSummary(entry, now) {
  const oldest = Object.hasOwn(entry, "oldestUpdatedAt")
    ? ` (oldest updated ${shortDate(entry.oldestUpdatedAt)}, oldest age ${elapsedAge(entry.oldestUpdatedAt, now)})`
    : "";
  return `${entry.owner}: ${entry.count}${oldest}`;
}

function ageHours(updatedAt, now) {
  const updatedMs = Date.parse(updatedAt || "");
  const nowMs = Date.parse(now || "");
  if (!Number.isFinite(updatedMs) || !Number.isFinite(nowMs)) return null;
  return Math.max(0, Math.floor((nowMs - updatedMs) / 3_600_000));
}

function checkBucket(check) {
  if (!check) return "missing";
  if (check.state) {
    if (check.state === "SUCCESS") return "pass";
    if (["PENDING", "EXPECTED"].includes(check.state)) return "pending";
    return "fail";
  }
  if (check.status && check.status !== "COMPLETED") return "pending";
  if (["SUCCESS", "NEUTRAL", "SKIPPED"].includes(check.conclusion)) return "pass";
  if (!check.conclusion) return "pending";
  return "fail";
}

function requiredCheckSummary(pr) {
  const buckets = REQUIRED_PR_CHECKS.map((name) => {
    const check = pr.statusCheckRollup.find((candidate) => candidate.name === name);
    return { name, bucket: checkBucket(check) };
  });
  return {
    buckets,
    allPassed: buckets.every((bucket) => bucket.bucket === "pass"),
    text: buckets.map((bucket) => `${bucket.name}=${bucket.bucket}`).join(", "),
  };
}

function requiredCheckState(checks) {
  if (checks.some((check) => check.bucket === "fail")) return "fail";
  if (checks.some((check) => check.bucket === "pending")) return "pending";
  if (checks.some((check) => check.bucket === "missing")) return "missing";
  return "pass";
}

function makeManagerActions(report) {
  const actions = [];
  const conflictingMainNumbers = new Set(report.conflictingMainPrs.map((pr) => pr.number));
  for (const pr of report.readyMainNotQueuedPrs) {
    if (conflictingMainNumbers.has(pr.number)) {
      continue;
    }
    if (pr.queueCandidate) {
      actions.push({
        priority: "P0",
        kind: "queue",
        number: pr.number,
        owner: ownerOf(pr),
        action: "queue verified ready PR",
        commentPolicy: "none; queue and verify merge result",
        reason: "PR-head required checks passed and branch is mergeable",
      });
      continue;
    }

    const state = requiredCheckState(pr.checks);
    if (state === "fail") {
      actions.push({
        priority: "P0",
        kind: "fix-ci",
        number: pr.number,
        owner: ownerOf(pr),
        action: "route failed required check to owner",
        commentPolicy: "comment only with root cause or handoff",
        reason: pr.checkSummary,
      });
    } else if (state === "pending") {
      actions.push({
        priority: "P1",
        kind: "wait-ci",
        number: pr.number,
        owner: ownerOf(pr),
        action: "wait for active PR-head checks",
        commentPolicy: "none",
        reason: pr.checkSummary,
      });
    } else if (state === "missing") {
      actions.push({
        priority: "P1",
        kind: "refresh-ci",
        number: pr.number,
        owner: ownerOf(pr),
        action: "refresh or request PR-head CI evidence",
        commentPolicy: "none unless rerun/fix is blocked",
        reason: pr.checkSummary,
      });
    }
  }

  for (const pr of report.conflictingReadyMainPrs) {
    actions.push({
      priority: "P0",
      kind: "rebase-ready",
      number: pr.number,
      owner: ownerOf(pr),
      action: "rebase or hand off conflicting ready PR",
      commentPolicy: "comment only if taking over or handing off",
      reason: `${pr.mergeStateStatus}/${pr.mergeable}`,
    });
  }

  for (const pr of report.conflictingMainPrs.filter((candidate) => candidate.draft)) {
    actions.push({
      priority: "P1",
      kind: "rebase-draft",
      number: pr.number,
      owner: ownerOf(pr),
      action: "refresh, supersede, or close conflicting draft",
      commentPolicy: "prefer PR body; comment for handoff/closure evidence",
      reason: `${pr.mergeStateStatus}/${pr.mergeable}`,
    });
  }

  for (const pr of report.staleDraftPrs) {
    if (conflictingMainNumbers.has(pr.number)) {
      continue;
    }
    actions.push({
      priority: "P1",
      kind: "stale-draft",
      number: pr.number,
      owner: ownerOf(pr),
      action: "refresh blocker, hand off, or close stale draft",
      commentPolicy: "one signed blocker/handoff comment if state changes",
      reason: `draft age ${pr.ageHours}h`,
    });
  }

  for (const pr of report.staleWipPrs) {
    actions.push({
      priority: "P1",
      kind: "stale-wip",
      number: pr.number,
      owner: ownerOf(pr),
      action: "refresh WIP blocker or split durable follow-up issue",
      commentPolicy: "one signed blocker/handoff comment if keeping WIP open",
      reason: `WIP age ${pr.ageHours}h`,
    });
  }

  for (const owner of report.draftParkingOwners.filter((entry) => entry.overBudget)) {
    actions.push({
      priority: "P2",
      kind: "draft-budget",
      number: null,
      owner: owner.owner,
      action: "reduce unstacked draft runway",
      commentPolicy: "batch by owner; do not comment on every PR",
      reason: `${owner.unstackedDraft} unstacked drafts`,
    });
  }

  for (const duplicate of report.duplicateDraftCleanupTargets) {
    actions.push({
      priority: "P2",
      kind: "duplicate-drafts",
      number: null,
      owner: "mixed",
      action: "deduplicate draft PRs for one issue",
      commentPolicy: "comment only on PRs closed or handed off",
      reason: `issue #${duplicate.issue}: ${duplicate.prs.map((pr) => `#${pr}`).join(", ")}`,
    });
  }

  return actions.sort((a, b) => {
    const priority = a.priority.localeCompare(b.priority);
    if (priority !== 0) return priority;
    const left = a.number ?? Number.MAX_SAFE_INTEGER;
    const right = b.number ?? Number.MAX_SAFE_INTEGER;
    if (left !== right) return left - right;
    return a.kind.localeCompare(b.kind);
  });
}

function makeReport(pulls) {
  const now = reportNow();
  const staleDraftHours = Number.parseInt(
    process.env.TSZ_PR_DRAFT_STALE_HOURS || String(DEFAULT_DRAFT_STALE_HOURS),
    10,
  );
  const unstackedDraftBudget = Number.parseInt(
    process.env.TSZ_PR_UNSTACKED_DRAFT_BUDGET || String(DEFAULT_UNSTACKED_DRAFT_BUDGET),
    10,
  );
  const normalized = pulls.map((pr) => ({
    number: pr.number,
    title: pr.title,
    draft: pr.isDraft,
    updatedAt: pr.updatedAt,
    base: pr.baseRefName,
    head: pr.headRefName,
    mergeStateStatus: pr.mergeStateStatus,
    mergeable: pr.mergeable,
    autoMergeArmed: pr.autoMergeArmed,
    inMergeQueue: pr.inMergeQueue,
    mergeQueueEnabled: pr.mergeQueueEnabled,
    labels: pr.labels.sort(),
    statusCheckRollup: pr.statusCheckRollup,
    agentName: agentNameFrom(pr.body),
    agentLabels: agentLabelsFrom(pr.labels),
    issueRefs: uniqueIssueRefs(issueRefsFrom(`${pr.title}\n${pr.body}`), pr.number),
    claimedIssueRefs: claimedIssueRefsFrom(pr),
    titleScope: titleScope(pr.title),
  }));

  const byBase = new Map();
  const byScope = new Map();
  const byIssue = new Map();
  for (const pr of normalized) {
    if (!byBase.has(pr.base)) {
      byBase.set(pr.base, []);
    }
    byBase.get(pr.base).push(pr.number);

    if (!byScope.has(pr.titleScope)) {
      byScope.set(pr.titleScope, []);
    }
    byScope.get(pr.titleScope).push(pr.number);

    for (const issue of pr.claimedIssueRefs) {
      if (!byIssue.has(issue)) {
        byIssue.set(issue, []);
      }
      byIssue.get(issue).push(pr.number);
    }
  }

  const stacks = [...byBase.entries()]
    .filter(([base]) => base !== "main")
    .map(([base, children]) => {
      const root = normalized.find((pr) => pr.head === base)?.number ?? null;
      return { base, root, children: children.sort((a, b) => a - b) };
    })
    .sort((a, b) => a.base.localeCompare(b.base));

  const stackRoots = new Set(stacks.map((stack) => stack.root).filter((root) => root !== null));
  const stackChildren = new Set(stacks.flatMap((stack) => stack.children));
  for (const pr of normalized) {
    const root = stackRoots.has(pr.number);
    const child = stackChildren.has(pr.number);
    if (root && child) {
      pr.stackRole = "stack middle";
    } else if (root) {
      pr.stackRole = "stack root";
    } else if (child) {
      pr.stackRole = "stack child";
    } else {
      pr.stackRole = null;
    }
  }

  const duplicateTitleScopes = [...byScope.entries()]
    .filter(([, prs]) => prs.length > 1)
    .map(([scope, prs]) => ({ scope, prs: prs.sort((a, b) => a - b) }))
    .sort((a, b) => a.scope.localeCompare(b.scope));

  const prByNumber = new Map(normalized.map((pr) => [pr.number, pr]));

  const duplicateIssueRefs = [...byIssue.entries()]
    .filter(([, prs]) => prs.length > 1)
    .map(([issue, prs]) => {
      const sortedPrs = prs.sort((a, b) => a - b);
      const draftPrs = sortedPrs.map((number) => prByNumber.get(number)).filter((pr) => pr?.draft);
      const stackedDraftCount = draftPrs.filter((pr) => pr.stackRole !== null).length;
      return {
        issue,
        prs: sortedPrs,
        draftCount: draftPrs.length,
        stackedDraftCount,
        unstackedDraftCount: draftPrs.length - stackedDraftCount,
        draftStackState: draftStackState(draftPrs.length, stackedDraftCount),
      };
    })
    .sort((a, b) => a.issue - b.issue);

  const duplicateDraftCleanupTargets = duplicateIssueRefs.filter(
    (entry) => entry.draftCount > 1 && entry.unstackedDraftCount > 0,
  );

  const agentLabelMismatches = normalized
    .filter((pr) => pr.agentLabels.length === 1 && pr.agentName !== null && pr.agentName !== pr.agentLabels[0])
    .map((pr) => ({
      number: pr.number,
      agentName: pr.agentName,
      label: `agent:${pr.agentLabels[0]}`,
    }))
    .sort((a, b) => a.number - b.number);

  const blockedReadyMainPrs = normalized
    .filter((pr) => !pr.draft && pr.base === "main" && pr.mergeStateStatus === "BLOCKED")
    .map((pr) => ({
      number: pr.number,
      agentName: pr.agentName,
      agentLabel: pr.agentLabels.length === 1 ? `agent:${pr.agentLabels[0]}` : null,
      autoMergeArmed: pr.autoMergeArmed,
      updatedAt: pr.updatedAt,
      mergeable: pr.mergeable,
      title: pr.title,
    }))
    .sort((a, b) => (a.agentName || "").localeCompare(b.agentName || "") || a.number - b.number);
  const blockedReadyMainOwnerCounts = ownerCountsWithOldestUpdated(blockedReadyMainPrs);

  const conflictingMainPrs = normalized
    .filter((pr) => pr.base === "main" && (pr.mergeable === "CONFLICTING" || pr.mergeStateStatus === "DIRTY"))
    .map((pr) => ({
      number: pr.number,
      draft: pr.draft,
      agentName: pr.agentName,
      agentLabel: pr.agentLabels.length === 1 ? `agent:${pr.agentLabels[0]}` : null,
      autoMergeArmed: pr.autoMergeArmed,
      updatedAt: pr.updatedAt,
      mergeStateStatus: pr.mergeStateStatus,
      mergeable: pr.mergeable,
      title: pr.title,
    }))
    .sort((a, b) => (a.agentName || "").localeCompare(b.agentName || "") || a.number - b.number);
  const conflictingMainOwnerCounts = ownerCountsWithOldestUpdated(conflictingMainPrs);
  const conflictingReadyMainPrs = conflictingMainPrs.filter((pr) => !pr.draft);
  const conflictingReadyMainOwnerCounts = ownerCountsWithOldestUpdated(conflictingReadyMainPrs);

  const wipPrs = normalized
    .filter(isWipPr)
    .map((pr) => ({
      number: pr.number,
      draft: pr.draft,
      agentName: pr.agentName,
      agentLabel: pr.agentLabels.length === 1 ? `agent:${pr.agentLabels[0]}` : null,
      base: pr.base,
      stackRole: pr.stackRole,
      markers: wipMarkers(pr),
      title: pr.title,
    }))
    .sort((a, b) => ownerOf(a).localeCompare(ownerOf(b)) || a.number - b.number);
  const wipOwnerCounts = ownerCounts(wipPrs);

  const ownerSummaries = [...normalized
    .reduce((summaries, pr) => {
      const owner = ownerOf(pr);
      if (!summaries.has(owner)) {
        summaries.set(owner, {
          owner,
          open: 0,
          ready: 0,
          draft: 0,
          wip: 0,
          stackedChildren: 0,
          blockedReadyMain: 0,
          conflictingReadyMain: 0,
          conflictingMain: 0,
          autoMergeArmed: 0,
        });
      }
      const summary = summaries.get(owner);
      summary.open += 1;
      if (pr.draft) {
        summary.draft += 1;
      } else {
        summary.ready += 1;
      }
      if (isWipPr(pr)) {
        summary.wip += 1;
      }
      if (pr.stackRole === "stack child" || pr.stackRole === "stack middle") {
        summary.stackedChildren += 1;
      }
      if (!pr.draft && pr.base === "main" && pr.mergeStateStatus === "BLOCKED") {
        summary.blockedReadyMain += 1;
      }
      if (
        !pr.draft &&
        pr.base === "main" &&
        (pr.mergeable === "CONFLICTING" || pr.mergeStateStatus === "DIRTY")
      ) {
        summary.conflictingReadyMain += 1;
      }
      if (pr.base === "main" && (pr.mergeable === "CONFLICTING" || pr.mergeStateStatus === "DIRTY")) {
        summary.conflictingMain += 1;
      }
      if (pr.autoMergeArmed) {
        summary.autoMergeArmed += 1;
      }
      return summaries;
    }, new Map())
    .values()]
    .sort((a, b) => b.open - a.open || a.owner.localeCompare(b.owner));

  const draftRunwayByOwner = [...normalized
    .filter((pr) => pr.draft)
    .reduce((summaries, pr) => {
      const owner = ownerOf(pr);
      if (!summaries.has(owner)) {
        summaries.set(owner, {
          owner,
          draft: 0,
          unstackedDraft: 0,
          staleDraft: 0,
          oldestUpdatedAt: null,
        });
      }
      const summary = summaries.get(owner);
      summary.draft += 1;
      if (pr.stackRole !== "stack child" && pr.stackRole !== "stack middle") {
        summary.unstackedDraft += 1;
      }
      const age = ageHours(pr.updatedAt, now);
      if (age !== null && age >= staleDraftHours) {
        summary.staleDraft += 1;
      }
      if (pr.updatedAt && (!summary.oldestUpdatedAt || pr.updatedAt < summary.oldestUpdatedAt)) {
        summary.oldestUpdatedAt = pr.updatedAt;
      }
      return summaries;
    }, new Map())
    .values()]
    .map((entry) => ({
      ...entry,
      overBudget: entry.unstackedDraft > unstackedDraftBudget,
    }))
    .sort(
      (a, b) =>
        b.unstackedDraft - a.unstackedDraft ||
        b.staleDraft - a.staleDraft ||
        a.owner.localeCompare(b.owner),
    );

  const draftParkingOwners = draftRunwayByOwner.filter((entry) => entry.overBudget || entry.staleDraft > 0);
  const staleDraftPrs = normalized
    .filter((pr) => pr.draft && !isWipPr(pr))
    .map((pr) => ({
      number: pr.number,
      agentName: pr.agentName,
      agentLabel: pr.agentLabels.length === 1 ? `agent:${pr.agentLabels[0]}` : null,
      updatedAt: pr.updatedAt,
      ageHours: ageHours(pr.updatedAt, now),
      stackRole: pr.stackRole,
      title: pr.title,
    }))
    .filter((pr) => pr.ageHours !== null && pr.ageHours >= staleDraftHours)
    .sort((a, b) => (a.agentName || "").localeCompare(b.agentName || "") || a.number - b.number);

  const staleWipPrs = normalized
    .filter((pr) => pr.draft && isWipPr(pr))
    .map((pr) => ({
      number: pr.number,
      agentName: pr.agentName,
      agentLabel: pr.agentLabels.length === 1 ? `agent:${pr.agentLabels[0]}` : null,
      updatedAt: pr.updatedAt,
      ageHours: ageHours(pr.updatedAt, now),
      stackRole: pr.stackRole,
      markers: wipMarkers(pr),
      title: pr.title,
    }))
    .filter((pr) => pr.ageHours !== null && pr.ageHours >= staleDraftHours)
    .sort((a, b) => (a.agentName || "").localeCompare(b.agentName || "") || a.number - b.number);

  const readyMainNotQueuedPrs = normalized
    .filter((pr) => (
      !pr.draft
      && pr.base === "main"
      && !isWipPr(pr)
      && !pr.inMergeQueue
    ))
    .map((pr) => {
      const checks = requiredCheckSummary(pr);
      return {
        number: pr.number,
        agentName: pr.agentName,
        agentLabel: pr.agentLabels.length === 1 ? `agent:${pr.agentLabels[0]}` : null,
        updatedAt: pr.updatedAt,
        mergeStateStatus: pr.mergeStateStatus,
        mergeable: pr.mergeable,
        checks: checks.buckets,
        checkSummary: checks.text,
        queueCandidate: checks.allPassed
          && pr.mergeStateStatus !== "DIRTY"
          && pr.mergeable !== "CONFLICTING",
        title: pr.title,
      };
    })
    .sort((a, b) => Number(b.queueCandidate) - Number(a.queueCandidate) || a.number - b.number);

  const report = {
    generatedAt: new Date().toISOString(),
    counts: {
      open: normalized.length,
      draft: normalized.filter((pr) => pr.draft).length,
      ready: normalized.filter((pr) => !pr.draft).length,
      stacked: stacks.reduce((sum, stack) => sum + stack.children.length, 0),
      missingAgentName: normalized.filter((pr) => pr.agentName === null).length,
      agentLabelMismatches: agentLabelMismatches.length,
      nativeQueued: normalized.filter((pr) => pr.inMergeQueue).length,
      readyNotQueued: readyMainNotQueuedPrs.length,
      queueCandidates: readyMainNotQueuedPrs.filter((pr) => pr.queueCandidate).length,
      draftParkingOwners: draftParkingOwners.length,
      staleDraftPrs: staleDraftPrs.length,
      staleWipPrs: staleWipPrs.length,
    },
    byBase: [...byBase.entries()]
      .map(([base, prs]) => ({ base, prs: prs.sort((a, b) => a - b) }))
      .sort((a, b) => a.base.localeCompare(b.base)),
    ownerSummaries,
    draftRunwayByOwner,
    draftParkingOwners,
    staleDraftPrs,
    staleWipPrs,
    readyMainNotQueuedPrs,
    stacks,
    duplicateTitleScopes,
    duplicateIssueRefs,
    duplicateDraftCleanupTargets,
    blockedReadyMainPrs,
    blockedReadyMainOwnerCounts,
    conflictingReadyMainPrs,
    conflictingReadyMainOwnerCounts,
    conflictingMainPrs,
    conflictingMainOwnerCounts,
    wipPrs,
    wipOwnerCounts,
    agentLabelMismatches,
    prs: normalized.sort((a, b) => a.number - b.number),
  };
  report.managerActions = makeManagerActions(report);
  return report;
}

function printMarkdown(report) {
  const now = reportNow();
  console.log("# Open PR Ownership Report");
  console.log("");
  console.log(
    `Open: ${report.counts.open}; draft: ${report.counts.draft}; ready: ${report.counts.ready}; native queued: ${report.counts.nativeQueued}; ready not queued: ${report.counts.readyNotQueued}; queue candidates: ${report.counts.queueCandidates}; draft parking owners: ${report.counts.draftParkingOwners}; stale drafts: ${report.counts.staleDraftPrs}; stale WIP: ${report.counts.staleWipPrs}; stacked children: ${report.counts.stacked}; missing AgentName: ${report.counts.missingAgentName}; AgentName/label mismatches: ${report.counts.agentLabelMismatches}`,
  );
  console.log("");
  console.log("## Manager Next Actions");
  if (report.managerActions.length === 0) {
    console.log("- none; wait and refresh without commenting");
  } else {
    for (const action of report.managerActions) {
      const target = action.number === null ? action.owner : `#${action.number}`;
      console.log(
        `- ${action.priority} ${action.kind} ${target}: ${action.action}; comment: ${action.commentPolicy}; reason: ${action.reason}`,
      );
    }
  }
  console.log("");
  console.log("## Owner Summary");
  if (report.ownerSummaries.length === 0) {
    console.log("- none");
  } else {
    console.log("");
    console.log(
      "| Owner | Open | Ready | Draft | WIP | Stacked children | Blocked ready main | Conflicting ready | Conflicting main | Auto-merge armed |",
    );
    console.log(
      "|-------|------|-------|-------|-----|------------------|--------------------|-------------------|------------------|------------------|",
    );
    for (const owner of report.ownerSummaries) {
      console.log(
        `| ${owner.owner} | ${owner.open} | ${owner.ready} | ${owner.draft} | ${owner.wip} | ${owner.stackedChildren} | ${owner.blockedReadyMain} | ${owner.conflictingReadyMain} | ${owner.conflictingMain} | ${owner.autoMergeArmed} |`,
      );
    }
  }
  console.log("");
  console.log("## Queue Admission");
  if (report.readyMainNotQueuedPrs.length === 0) {
    console.log("- none");
  } else {
    console.log("");
    console.log("| PR | Owner | Queue candidate | Merge state | Checks | Title |");
    console.log("|----|-------|-----------------|-------------|--------|-------|");
    for (const pr of report.readyMainNotQueuedPrs) {
      const owner = ownerOf(pr);
      const candidate = pr.queueCandidate ? "yes" : "no";
      console.log(
        `| #${pr.number} | ${owner} | ${candidate} | ${pr.mergeStateStatus}/${pr.mergeable} | ${pr.checkSummary.replace(/\|/g, "\\|")} | ${pr.title.replace(/\|/g, "\\|")} |`,
      );
    }
  }
  console.log("");
  console.log("## Draft Parking Risks");
  if (
    report.draftParkingOwners.length === 0
    && report.staleDraftPrs.length === 0
    && report.staleWipPrs.length === 0
  ) {
    console.log("- none");
  } else {
    if (report.draftParkingOwners.length) {
      console.log("");
      console.log("Owners over draft budget or carrying stale drafts:");
      for (const owner of report.draftParkingOwners) {
        const budget = owner.overBudget ? "over budget" : "within budget";
        console.log(
          `- ${owner.owner}: drafts ${owner.draft}; unstacked ${owner.unstackedDraft}; stale ${owner.staleDraft}; ${budget}; oldest updated ${shortDate(owner.oldestUpdatedAt)} (${elapsedAge(owner.oldestUpdatedAt, now)})`,
        );
      }
    }
    if (report.staleDraftPrs.length) {
      console.log("");
      console.log("Stale draft PRs:");
      for (const pr of report.staleDraftPrs) {
        const owner = ownerOf(pr);
        const stack = pr.stackRole ? `; ${pr.stackRole}` : "";
        console.log(
          `- #${pr.number}: ${owner}; updated ${shortDate(pr.updatedAt)}; age ${pr.ageHours}h${stack}; ${pr.title}`,
        );
      }
    }
    if (report.staleWipPrs.length) {
      console.log("");
      console.log("Stale WIP PRs:");
      for (const pr of report.staleWipPrs) {
        const owner = ownerOf(pr);
        const stack = pr.stackRole ? `; ${pr.stackRole}` : "";
        console.log(
          `- #${pr.number}: ${owner}; updated ${shortDate(pr.updatedAt)}; age ${pr.ageHours}h; ${pr.markers.join("+")}${stack}; ${pr.title}`,
        );
      }
    }
  }
  console.log("");
  console.log("## Base Branches");
  for (const entry of report.byBase) {
    console.log(`- ${entry.base}: ${entry.prs.map((pr) => `#${pr}`).join(", ")}`);
  }
  console.log("");
  console.log("## Stack Edges");
  if (report.stacks.length === 0) {
    console.log("- none");
  } else {
    for (const stack of report.stacks) {
      const root = stack.root === null ? "unknown root" : `root #${stack.root}`;
      console.log(`- ${stack.base}: ${root}; children ${stack.children.map((pr) => `#${pr}`).join(", ")}`);
    }
  }
  console.log("");
  console.log("## Duplicate-Looking Title Scopes");
  if (report.duplicateTitleScopes.length === 0) {
    console.log("- none");
  } else {
    for (const duplicate of report.duplicateTitleScopes) {
      console.log(`- ${duplicate.scope}: ${duplicate.prs.map((pr) => prSummary(report, pr)).join(", ")}`);
    }
  }
  console.log("");
  console.log("## Multiple Drafts Against Same Issue");
  const issueDuplicates = report.duplicateIssueRefs.filter((entry) => entry.draftCount > 1);
  if (issueDuplicates.length === 0) {
    console.log("- none");
  } else {
    for (const duplicate of issueDuplicates) {
      console.log(
        `- #${duplicate.issue} (${duplicate.draftStackState}): ${duplicate.prs
          .map((pr) => prSummary(report, pr, "PR #"))
          .join(", ")}`,
      );
    }
  }
  console.log("");
  console.log("## Duplicate Draft Cleanup Targets");
  if (report.duplicateDraftCleanupTargets.length === 0) {
    console.log("- none");
  } else {
    for (const duplicate of report.duplicateDraftCleanupTargets) {
      console.log(
        `- #${duplicate.issue} (${duplicate.draftStackState}; unstacked drafts: ${
          duplicate.unstackedDraftCount
        }): ${duplicate.prs.map((pr) => prSummary(report, pr, "PR #")).join(", ")}`,
      );
    }
  }
  console.log("");
  console.log("## Blocked Ready Main PRs");
  if (report.blockedReadyMainPrs.length === 0) {
    console.log("- none");
  } else {
    console.log("");
    console.log("Owner counts:");
    for (const entry of report.blockedReadyMainOwnerCounts) {
      console.log(`- ${ownerCountSummary(entry, now)}`);
    }
    console.log("");
    console.log("PRs:");
    for (const pr of report.blockedReadyMainPrs) {
      const owner = ownerOf(pr);
      const autoMerge = pr.autoMergeArmed ? "auto-merge armed" : "auto-merge off";
      console.log(`- #${pr.number}: ${owner}; updated ${shortDate(pr.updatedAt)}; ${pr.mergeable}; ${autoMerge}; ${pr.title}`);
    }
  }
  console.log("");
  console.log("## Conflicting Ready Main PRs");
  if (report.conflictingReadyMainPrs.length === 0) {
    console.log("- none");
  } else {
    console.log("");
    console.log("Owner counts:");
    for (const entry of report.conflictingReadyMainOwnerCounts) {
      console.log(`- ${ownerCountSummary(entry, now)}`);
    }
    console.log("");
    console.log("PRs:");
    for (const pr of report.conflictingReadyMainPrs) {
      const owner = ownerOf(pr);
      const autoMerge = pr.autoMergeArmed ? "auto-merge armed" : "auto-merge off";
      console.log(
        `- #${pr.number}: ${owner}; updated ${shortDate(pr.updatedAt)}; ${pr.mergeStateStatus}; ${pr.mergeable}; ${autoMerge}; ${pr.title}`,
      );
    }
  }
  console.log("");
  console.log("## Conflicting Main PRs");
  if (report.conflictingMainPrs.length === 0) {
    console.log("- none");
  } else {
    console.log("");
    console.log("Owner counts:");
    for (const entry of report.conflictingMainOwnerCounts) {
      console.log(`- ${ownerCountSummary(entry, now)}`);
    }
    console.log("");
    console.log("PRs:");
    for (const pr of report.conflictingMainPrs) {
      const owner = ownerOf(pr);
      const state = pr.draft ? "draft" : "ready";
      const autoMerge = pr.autoMergeArmed ? "auto-merge armed" : "auto-merge off";
      console.log(
        `- #${pr.number}: ${owner}; ${state}; ${pr.mergeStateStatus}; ${pr.mergeable}; ${autoMerge}; ${pr.title}`,
      );
    }
  }
  console.log("");
  console.log("## WIP PRs");
  if (report.wipPrs.length === 0) {
    console.log("- none");
  } else {
    console.log("");
    console.log("Owner counts:");
    for (const entry of report.wipOwnerCounts) {
      console.log(`- ${entry.owner}: ${entry.count}`);
    }
    console.log("");
    console.log("PRs:");
    for (const pr of report.wipPrs) {
      const owner = ownerOf(pr);
      const state = pr.draft ? "draft" : "ready";
      const stack = pr.stackRole ? `; ${pr.stackRole}` : "";
      console.log(`- #${pr.number}: ${owner}; ${state}; ${pr.markers.join("+")}${stack}; ${pr.title}`);
    }
  }
  console.log("");
  console.log("## AgentName / Label Mismatches");
  if (report.agentLabelMismatches.length === 0) {
    console.log("- none");
  } else {
    for (const mismatch of report.agentLabelMismatches) {
      console.log(`- #${mismatch.number}: AgentName ${mismatch.agentName}; label ${mismatch.label}`);
    }
  }
}

const args = parseArgs(process.argv.slice(2));
const pulls = loadPulls(args.fixture);
if (!args.fixture) {
  hydrateRequiredChecks(pulls);
}
const report = makeReport(pulls);
if (args.outputJson) {
  fs.writeFileSync(args.outputJson, `${JSON.stringify(report, null, 2)}\n`);
}
printMarkdown(report);
