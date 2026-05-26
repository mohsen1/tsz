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
    updatedAt: raw.updatedAt ? String(raw.updatedAt) : null,
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
      "number,title,isDraft,updatedAt,baseRefName,headRefName,labels,body,mergeStateStatus,mergeable,autoMergeRequest",
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

function makeReport(pulls) {
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
      updatedAt: pr.updatedAt,
      mergeable: pr.mergeable,
      title: pr.title,
    }))
    .sort((a, b) => (a.agentName || "").localeCompare(b.agentName || "") || a.number - b.number);
  const blockedReadyMainOwnerCounts = ownerCounts(blockedReadyMainPrs);

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
  const conflictingMainOwnerCounts = ownerCounts(conflictingMainPrs);
  const conflictingReadyMainPrs = conflictingMainPrs.filter((pr) => !pr.draft);
  const conflictingReadyMainOwnerCounts = ownerCounts(conflictingReadyMainPrs);

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
    ownerSummaries,
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
}

function printMarkdown(report) {
  console.log("# Open PR Ownership Report");
  console.log("");
  console.log(
    `Open: ${report.counts.open}; draft: ${report.counts.draft}; ready: ${report.counts.ready}; stacked children: ${report.counts.stacked}; missing AgentName: ${report.counts.missingAgentName}; AgentName/label mismatches: ${report.counts.agentLabelMismatches}`,
  );
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
      console.log(`- ${entry.owner}: ${entry.count}`);
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
      console.log(`- ${entry.owner}: ${entry.count}`);
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
const report = makeReport(loadPulls(args.fixture));
if (args.outputJson) {
  fs.writeFileSync(args.outputJson, `${JSON.stringify(report, null, 2)}\n`);
}
printMarkdown(report);
