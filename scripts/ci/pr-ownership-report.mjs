#!/usr/bin/env node
import fs from "node:fs";
import { spawnSync } from "node:child_process";

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
  const labels = Array.isArray(raw.labels)
    ? raw.labels.map((label) => (typeof label === "string" ? label : label?.name)).filter(Boolean)
    : [];
  return {
    number: Number(raw.number),
    title: String(raw.title ?? ""),
    isDraft: Boolean(raw.isDraft),
    baseRefName: String(raw.baseRefName || "main"),
    headRefName: String(raw.headRefName || ""),
    mergeStateStatus: String(raw.mergeStateStatus || "UNKNOWN"),
    mergeable: String(raw.mergeable || "UNKNOWN"),
    autoMergeArmed: Boolean(raw.autoMergeRequest),
    labels,
    body: String(raw.body ?? ""),
  };
}

function loadPulls(fixture) {
  if (fixture) {
    return JSON.parse(fs.readFileSync(fixture, "utf8")).map(normalizePr);
  }

  const result = spawnSync(
    "gh",
    [
      "pr",
      "list",
      "--state",
      "open",
      "--limit",
      "500",
      "--json",
      "number,title,isDraft,baseRefName,headRefName,labels,body,mergeStateStatus,mergeable,autoMergeRequest",
    ],
    { encoding: "utf8" },
  );
  if (result.status !== 0) {
    fail(result.stderr.trim() || "gh pr list failed");
  }
  return JSON.parse(result.stdout).map(normalizePr);
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
  const wip = pr.labels.includes("WIP") || /^\[wip\]/i.test(pr.title) ? ", WIP" : "";
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

function makeReport(pulls) {
  const normalized = pulls.map((pr) => ({
    number: pr.number,
    title: pr.title,
    draft: pr.isDraft,
    base: pr.baseRefName,
    head: pr.headRefName,
    mergeStateStatus: pr.mergeStateStatus,
    mergeable: pr.mergeable,
    autoMergeArmed: pr.autoMergeArmed,
    labels: pr.labels.sort(),
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
      mergeable: pr.mergeable,
      title: pr.title,
    }))
    .sort((a, b) => (a.agentName || "").localeCompare(b.agentName || "") || a.number - b.number);
  const blockedReadyMainOwnerCounts = [...blockedReadyMainPrs
    .reduce((counts, pr) => {
      const owner = pr.agentLabel || pr.agentName || "unowned";
      counts.set(owner, (counts.get(owner) || 0) + 1);
      return counts;
    }, new Map())
    .entries()]
    .map(([owner, count]) => ({ owner, count }))
    .sort((a, b) => b.count - a.count || a.owner.localeCompare(b.owner));

  return {
    generatedAt: new Date().toISOString(),
    counts: {
      open: normalized.length,
      draft: normalized.filter((pr) => pr.draft).length,
      ready: normalized.filter((pr) => !pr.draft).length,
      stacked: stacks.reduce((sum, stack) => sum + stack.children.length, 0),
      missingAgentName: normalized.filter((pr) => pr.agentName === null).length,
      agentLabelMismatches: agentLabelMismatches.length,
    },
    byBase: [...byBase.entries()]
      .map(([base, prs]) => ({ base, prs: prs.sort((a, b) => a - b) }))
      .sort((a, b) => a.base.localeCompare(b.base)),
    stacks,
    duplicateTitleScopes,
    duplicateIssueRefs,
    duplicateDraftCleanupTargets,
    blockedReadyMainPrs,
    blockedReadyMainOwnerCounts,
    agentLabelMismatches,
    prs: normalized.sort((a, b) => a.number - b.number),
  };
}

function printMarkdown(report) {
  console.log("# Open PR Ownership Report");
  console.log("");
  console.log(
    `Open: ${report.counts.open}; draft: ${report.counts.draft}; ready: ${report.counts.ready}; stacked children: ${report.counts.stacked}; missing AgentName: ${report.counts.missingAgentName}; AgentName/label mismatches: ${report.counts.agentLabelMismatches}`,
  );
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
      console.log(`- ${entry.owner}: ${entry.count}`);
    }
    console.log("");
    console.log("PRs:");
    for (const pr of report.blockedReadyMainPrs) {
      const owner = pr.agentLabel || pr.agentName || "unowned";
      const autoMerge = pr.autoMergeArmed ? "auto-merge armed" : "auto-merge off";
      console.log(`- #${pr.number}: ${owner}; ${pr.mergeable}; ${autoMerge}; ${pr.title}`);
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
const report = makeReport(loadPulls(args.fixture));
if (args.outputJson) {
  fs.writeFileSync(args.outputJson, `${JSON.stringify(report, null, 2)}\n`);
}
printMarkdown(report);
