#!/usr/bin/env node
import fs from "node:fs";
import { spawnSync } from "node:child_process";

const WIP_LABEL = "WIP";

const BODY_WIP_PATTERNS = [
  {
    kind: "body [WIP] marker",
    pattern: /(^|\n)\s*(?:[-*]\s*)?(?:\*\*)?\[WIP\](?:\*\*)?(?:\s|:|-|$)/i,
  },
  {
    kind: "body WIP status line",
    pattern: /(^|\n)\s*(?:[-*]\s*)?(?:\*\*)?(?:status|state|readiness|merge state|coordination status)(?:\*\*)?\s*:?\s+[^\n]*(?:\bwip\b|\bwork in progress\b|\bnot ready\b|\bdo not merge\b|\bblocked\b|\bblocker\b)/i,
  },
  {
    kind: "body blocker declaration",
    pattern: /(^|\n)\s*(?:[-*]\s*)?(?:\*\*)?(?:blocker|blocked|blocking)(?:\*\*)?\s*:?\s+[^\n]+/i,
  },
  {
    kind: "body WIP declaration",
    pattern: /\b(?:this pr|this branch|current head|merge state|ready state|readiness)\b[^\n]{0,80}\b(?:wip|work in progress|not ready|do not merge|blocked|blocker)\b/i,
  },
];

function usage() {
  return [
    "usage: check-pr-ready-state.mjs [--fixture path]",
    "",
    "Without --fixture, reads PR_NUMBER and REPOSITORY from the environment and",
    "loads pull-request state through gh api.",
  ].join("\n");
}

function parseArgs(argv) {
  const options = { fixture: null };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--fixture") {
      options.fixture = argv[i + 1];
      i += 1;
      if (!options.fixture) throw new Error("--fixture requires a path");
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
    stdio: ["ignore", "pipe", "pipe"],
  });
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

function readPullRequest(options) {
  if (options.fixture) {
    return JSON.parse(fs.readFileSync(options.fixture, "utf8"));
  }

  const repository = process.env.REPOSITORY || process.env.GITHUB_REPOSITORY;
  const prNumber = process.env.PR_NUMBER;
  if (!repository || !prNumber) {
    throw new Error("PR_NUMBER and REPOSITORY or GITHUB_REPOSITORY are required");
  }

  const output = runGh([
    "api",
    `repos/${repository}/pulls/${prNumber}`,
    "--jq",
    "{number,title,body,draft,labels:[.labels[]?.name]}",
  ]);
  return JSON.parse(output);
}

function hasWipTitle(title) {
  return /(^|[\s:])\[WIP\]([\s:]|$)/i.test(title || "");
}

function bodyWipMarker(body) {
  for (const { kind, pattern } of BODY_WIP_PATTERNS) {
    if (pattern.test(body || "")) return kind;
  }
  return null;
}

export function readyStateFailures(pr) {
  if (pr.draft === true) return [];

  const labels = Array.isArray(pr.labels)
    ? pr.labels.map((label) => typeof label === "string" ? label : label?.name)
    : [];
  const failures = [];

  if (labels.some((label) => label === WIP_LABEL)) {
    failures.push("WIP label");
  }
  if (hasWipTitle(pr.title)) {
    failures.push("[WIP] title marker");
  }

  const bodyMarker = bodyWipMarker(pr.body);
  if (bodyMarker) {
    failures.push(bodyMarker);
  }

  return failures;
}

export function formatFailure(pr, failures) {
  const prLabel = pr.number ? `#${pr.number}` : "this PR";
  return [
    `Ready PRs must not carry WIP status (${prLabel}).`,
    `Found: ${failures.join(", ")}.`,
    "Repair: remove WIP labels, [WIP] title text, and body text declaring WIP/blocker state only after implementation and verification are complete.",
  ].join("\n");
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const pr = readPullRequest(options);
  const failures = readyStateFailures(pr);
  if (failures.length > 0) {
    console.error(formatFailure(pr, failures));
    process.exit(1);
  }
  console.log("Ready-state WIP check passed.");
}

if (import.meta.url === `file://${process.argv[1]}`) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
