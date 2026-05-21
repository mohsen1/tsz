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
      "number,title,isDraft,baseRefName,headRefName,labels,body",
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

function issueRefsFrom(text) {
  return [...String(text).matchAll(/#(\d+)/g)].map((match) => Number(match[1]));
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

function makeReport(pulls) {
  const normalized = pulls.map((pr) => ({
    number: pr.number,
    title: pr.title,
    draft: pr.isDraft,
    base: pr.baseRefName,
    head: pr.headRefName,
    labels: pr.labels.sort(),
    agentName: agentNameFrom(pr.body),
    issueRefs: [...new Set(issueRefsFrom(`${pr.title}\n${pr.body}`).filter((issue) => issue !== pr.number))].sort(
      (a, b) => a - b,
    ),
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

    for (const issue of pr.issueRefs) {
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

  const duplicateTitleScopes = [...byScope.entries()]
    .filter(([, prs]) => prs.length > 1)
    .map(([scope, prs]) => ({ scope, prs: prs.sort((a, b) => a - b) }))
    .sort((a, b) => a.scope.localeCompare(b.scope));

  const duplicateIssueRefs = [...byIssue.entries()]
    .filter(([, prs]) => prs.length > 1)
    .map(([issue, prs]) => ({ issue, prs: prs.sort((a, b) => a - b) }))
    .sort((a, b) => a.issue - b.issue);

  return {
    generatedAt: new Date().toISOString(),
    counts: {
      open: normalized.length,
      draft: normalized.filter((pr) => pr.draft).length,
      ready: normalized.filter((pr) => !pr.draft).length,
      stacked: stacks.reduce((sum, stack) => sum + stack.children.length, 0),
      missingAgentName: normalized.filter((pr) => pr.agentName === null).length,
    },
    byBase: [...byBase.entries()]
      .map(([base, prs]) => ({ base, prs: prs.sort((a, b) => a - b) }))
      .sort((a, b) => a.base.localeCompare(b.base)),
    stacks,
    duplicateTitleScopes,
    duplicateIssueRefs,
    prs: normalized.sort((a, b) => a.number - b.number),
  };
}

function printMarkdown(report) {
  console.log("# Open PR Ownership Report");
  console.log("");
  console.log(
    `Open: ${report.counts.open}; draft: ${report.counts.draft}; ready: ${report.counts.ready}; stacked children: ${report.counts.stacked}; missing AgentName: ${report.counts.missingAgentName}`,
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
      console.log(`- ${duplicate.scope}: ${duplicate.prs.map((pr) => `#${pr}`).join(", ")}`);
    }
  }
  console.log("");
  console.log("## Multiple Drafts Against Same Issue");
  const issueDuplicates = report.duplicateIssueRefs.filter((entry) => {
    const prs = entry.prs
      .map((number) => report.prs.find((pr) => pr.number === number))
      .filter(Boolean);
    return prs.filter((pr) => pr.draft).length > 1;
  });
  if (issueDuplicates.length === 0) {
    console.log("- none");
  } else {
    for (const duplicate of issueDuplicates) {
      console.log(`- #${duplicate.issue}: ${duplicate.prs.map((pr) => `PR #${pr}`).join(", ")}`);
    }
  }
}

const args = parseArgs(process.argv.slice(2));
const report = makeReport(loadPulls(args.fixture));
if (args.outputJson) {
  fs.writeFileSync(args.outputJson, `${JSON.stringify(report, null, 2)}\n`);
}
printMarkdown(report);
