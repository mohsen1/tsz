#!/usr/bin/env node
import fs from "node:fs";
import { spawnSync } from "node:child_process";

const WIP_LABEL = "WIP";
const DEFAULT_WINDOW_HOURS = 24;
const DEFAULT_MAX_PULL_REQUESTS = 80;
const DEFAULT_GH_TIMEOUT_MS = 30_000;

function usage() {
  return [
    "usage: check-wip-state-comments.mjs [--fixture path] [--repository owner/repo] [--window-hours n] [--max-prs n] [--gh-timeout-ms n] [--advisory|--enforce]",
    "",
    "Reports open PRs whose latest WIP-state event does not have a signed",
    "explanatory comment within the required window.",
  ].join("\n");
}

function parseArgs(argv) {
  const options = {
    advisory: true,
    enforce: false,
    fixture: null,
    ghTimeoutMs: Number.parseInt(process.env.GH_TIMEOUT_MS || "", 10)
      || DEFAULT_GH_TIMEOUT_MS,
    maxPullRequests: DEFAULT_MAX_PULL_REQUESTS,
    repository: process.env.REPOSITORY || process.env.GITHUB_REPOSITORY || null,
    windowHours: DEFAULT_WINDOW_HOURS,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--fixture") {
      options.fixture = argv[++i];
      if (!options.fixture) throw new Error("--fixture requires a path");
      continue;
    }
    if (arg === "--repository") {
      options.repository = argv[++i];
      if (!options.repository) throw new Error("--repository requires owner/repo");
      continue;
    }
    if (arg === "--window-hours") {
      const value = Number(argv[++i]);
      if (!Number.isFinite(value) || value <= 0) {
        throw new Error("--window-hours requires a positive number");
      }
      options.windowHours = value;
      continue;
    }
    if (arg === "--max-prs") {
      const value = Number.parseInt(argv[++i], 10);
      if (!Number.isInteger(value) || value <= 0) {
        throw new Error("--max-prs requires a positive integer");
      }
      options.maxPullRequests = value;
      continue;
    }
    if (arg === "--gh-timeout-ms") {
      const value = Number.parseInt(argv[++i], 10);
      if (!Number.isInteger(value) || value <= 0) {
        throw new Error("--gh-timeout-ms requires a positive integer");
      }
      options.ghTimeoutMs = value;
      continue;
    }
    if (arg === "--advisory") {
      options.advisory = true;
      options.enforce = false;
      continue;
    }
    if (arg === "--enforce") {
      options.enforce = true;
      options.advisory = false;
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

function runGhJson(args, options = {}) {
  const timeout = options.ghTimeoutMs || DEFAULT_GH_TIMEOUT_MS;
  const result = spawnSync("gh", args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    timeout,
  });
  if (result.error) {
    const command = `gh ${args.join(" ")}`;
    if (result.error.code === "ETIMEDOUT") {
      throw new Error(`${command} timed out after ${timeout}ms`);
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
  return JSON.parse(result.stdout);
}

function parseRepository(repository) {
  const [owner, name, ...extra] = repository.split("/");
  if (!owner || !name || extra.length > 0) {
    throw new Error(`repository must use owner/repo form: ${repository}`);
  }
  return { owner, name };
}

function connectionNodes(value) {
  if (Array.isArray(value)) return value;
  if (Array.isArray(value?.nodes)) return value.nodes;
  return [];
}

function normalizeLabel(label) {
  return typeof label === "string" ? label : label?.name;
}

function normalizeTimelineEvent(event) {
  if (event?.__typename === "ConvertToDraftEvent") {
    return {
      event: "converted_to_draft",
      createdAt: event.createdAt,
      actor: event.actor,
    };
  }
  if (event?.__typename === "LabeledEvent") {
    return {
      event: "labeled",
      label: event.label,
      createdAt: event.createdAt,
      actor: event.actor,
    };
  }
  return event;
}

function normalizePr(pr) {
  const labels = Array.isArray(pr.labels)
    ? pr.labels
    : connectionNodes(pr.labels);
  const timeline = Array.isArray(pr.timeline)
    ? pr.timeline
    : connectionNodes(pr.timelineItems);
  const comments = Array.isArray(pr.comments)
    ? pr.comments
    : connectionNodes(pr.comments);
  return {
    number: pr.number,
    title: pr.title || "",
    draft: pr.draft === true || pr.isDraft === true,
    labels: labels.map(normalizeLabel).filter(Boolean),
    timeline: timeline.map(normalizeTimelineEvent),
    comments,
  };
}

function readFixture(path) {
  const fixture = JSON.parse(fs.readFileSync(path, "utf8"));
  const pullRequests = Array.isArray(fixture) ? fixture : fixture.pullRequests;
  if (!Array.isArray(pullRequests)) {
    throw new Error("fixture must be an array or contain pullRequests");
  }
  return pullRequests.map(normalizePr);
}

function readOpenPullRequests(repository, options) {
  if (!repository) {
    throw new Error("REPOSITORY or GITHUB_REPOSITORY is required");
  }

  const { owner, name } = parseRepository(repository);
  const first = Math.min(options.maxPullRequests, 100);
  const response = runGhJson([
    "api",
    "graphql",
    "-f",
    `owner=${owner}`,
    "-f",
    `name=${name}`,
    "-F",
    `first=${first}`,
    "-f",
    "query=query($owner: String!, $name: String!, $first: Int!) { repository(owner: $owner, name: $name) { pullRequests(states: OPEN, first: $first, orderBy: { field: UPDATED_AT, direction: DESC }) { nodes { number title isDraft labels(first: 20) { nodes { name } } timelineItems(last: 30, itemTypes: [CONVERT_TO_DRAFT_EVENT, LABELED_EVENT]) { nodes { __typename ... on ConvertToDraftEvent { createdAt actor { login } } ... on LabeledEvent { createdAt actor { login } label { name } } } } comments(last: 100) { nodes { createdAt body author { login } } } } } } }",
  ], options);
  const pullRequests = (response.data || response).repository?.pullRequests?.nodes;
  if (!Array.isArray(pullRequests)) {
    throw new Error("expected repository.pullRequests.nodes array in GraphQL response");
  }
  return pullRequests.map(normalizePr);
}

function isCandidate(pr) {
  return pr.labels.includes(WIP_LABEL) || pr.draft;
}

function eventTime(event) {
  return event.created_at || event.createdAt || null;
}

function eventActor(event) {
  const actor = event.actor || event.user || null;
  return typeof actor === "string" ? actor : actor?.login || "";
}

function eventLabel(event) {
  const label = event.label || null;
  return typeof label === "string" ? label : label?.name || "";
}

function isWipStateEvent(event) {
  if (event.event === "converted_to_draft") return true;
  return event.event === "labeled" && eventLabel(event) === WIP_LABEL;
}

function latestWipStateEvent(pr) {
  const events = pr.timeline
    .filter(isWipStateEvent)
    .filter((event) => eventTime(event))
    .sort((a, b) => eventTime(a).localeCompare(eventTime(b)));
  return events.at(-1) || null;
}

function commentTime(comment) {
  return comment.created_at || comment.createdAt || null;
}

function commentBody(comment) {
  return comment.body || "";
}

function commentHasAgentName(comment) {
  return /^[ \t>*-]*AgentName:[ \t]*\S+/im.test(commentBody(comment));
}

function commentIsExplanatory(comment) {
  const body = commentBody(comment);
  return commentHasAgentName(comment)
    && /\b(reason|why)\b/i.test(body)
    && /\b(blocker|blocked|current work|currently|next work)\b/i.test(body)
    && /\b(next owner|next action|next step|owner|action)\b/i.test(body);
}

function commentsInWindow(comments, sinceIso, windowHours) {
  const since = Date.parse(sinceIso);
  const until = since + windowHours * 60 * 60 * 1000;
  return comments.filter((comment) => {
    const time = Date.parse(commentTime(comment) || "");
    return Number.isFinite(time) && time >= since && time <= until;
  });
}

export function wipStateFindings(pullRequests, options = {}) {
  const windowHours = options.windowHours || DEFAULT_WINDOW_HOURS;
  const findings = [];

  for (const pr of pullRequests.map(normalizePr)) {
    if (!isCandidate(pr)) continue;

    const latestEvent = latestWipStateEvent(pr);
    if (!latestEvent) {
      if (pr.labels.includes(WIP_LABEL)) {
        findings.push({
          number: pr.number,
          title: pr.title,
          event: "WIP label",
          eventTime: "not found in latest timeline page",
          actor: "",
          agentNamePresent: false,
          commentStatus: "missing timeline event",
        });
      }
      continue;
    }

    const nearbyComments = commentsInWindow(pr.comments, eventTime(latestEvent), windowHours);
    const signedComments = nearbyComments.filter(commentHasAgentName);
    if (nearbyComments.some(commentIsExplanatory)) continue;

    findings.push({
      number: pr.number,
      title: pr.title,
      event: latestEvent.event === "labeled" ? "WIP label" : "converted to draft",
      eventTime: eventTime(latestEvent),
      actor: eventActor(latestEvent),
      agentNamePresent: signedComments.length > 0,
      commentStatus: signedComments.length > 0
        ? "signed comment missing reason/blocker/next action"
        : "missing signed WIP-state comment",
    });
  }

  return findings;
}

function escapeCell(value) {
  return String(value ?? "").replaceAll("|", "\\|").replace(/\s+/g, " ").trim();
}

export function formatMarkdownReport(findings, options = {}) {
  const windowHours = options.windowHours || DEFAULT_WINDOW_HOURS;
  const lines = [
    "## WIP State Comment Advisory",
    "",
    `Window: ${windowHours} hour(s) after the latest WIP-state event.`,
    "",
  ];

  if (findings.length === 0) {
    lines.push("No WIP-state comment gaps found.");
    return lines.join("\n");
  }

  lines.push("| PR | Title | Event | Event Time | Actor | AgentName Present | Comment Status |");
  lines.push("|----|-------|-------|------------|-------|-------------------|----------------|");
  for (const finding of findings) {
    lines.push(`| ${[
      `#${finding.number}`,
      escapeCell(finding.title),
      escapeCell(finding.event),
      escapeCell(finding.eventTime),
      escapeCell(finding.actor),
      finding.agentNamePresent ? "yes" : "no",
      escapeCell(finding.commentStatus),
    ].join(" | ")} |`);
  }
  lines.push("");
  lines.push("Repair: add a PR comment with `AgentName:`, the reason WIP state changed, the current blocker/work, and the next owner/action.");
  return lines.join("\n");
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const pullRequests = options.fixture
    ? readFixture(options.fixture)
    : readOpenPullRequests(options.repository, options);
  const findings = wipStateFindings(pullRequests, options);
  console.log(formatMarkdownReport(findings, options));
  if (options.enforce && findings.length > 0) process.exit(1);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
