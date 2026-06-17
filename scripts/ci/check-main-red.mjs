#!/usr/bin/env node
// Main-red sentinel: detect when the tip of `main` is failing CI (specifically
// the conformance/parity gate) and surface it as a deduplicated tracking issue.
//
// Rationale: merges land on `main` every few minutes through the native merge
// queue. A flaky-pass at merge_group time can land a real conformance
// regression; once `main` is red, every subsequent merge_group run inherits the
// red base and the queue ejects every PR. Nothing in CI watched `main`'s own
// post-merge conclusion, so the wedge went unnoticed for hours. This sentinel
// runs from `ci-health.yml` (every 15 min) and turns that silent wedge into an
// alert that names the first red commit and the regressed tests.
//
// Pure functions (mainCiHealth/formatIssueBody/formatReport) are unit tested;
// gh I/O lives in main() behind injectable fetchers.
import fs from "node:fs";
import { spawnSync } from "node:child_process";

const DEFAULT_MAX_RUNS = 60;
const DEFAULT_GH_MAX_BUFFER_BYTES = 16 * 1024 * 1024;
const WORKFLOW_NAME = "CI";
const MAIN_EVENTS = new Set(["push", "merge_group"]);
// Conclusions that do not represent a real verdict on the merged tree.
const INCONCLUSIVE = new Set(["skipped", "cancelled", "neutral", "stale", null, undefined, ""]);
const ISSUE_MARKER = "<!-- main-red-sentinel -->";
const ISSUE_TITLE = "🔴 main CI is red — conformance/parity floor breached";
const REGRESSED_RE = /REGRESSED:\s*(\S+)/g;

function usage() {
  return [
    "usage: check-main-red.mjs [--fixture path] [--repository owner/repo] [--now iso]",
    "                         [--max-runs n] [--file-issue] [--enforce]",
    "",
    "Inspects the most recent conclusive CI run on `main` (push or merge_group).",
    "When it failed, reports it and — with --file-issue — opens or updates a single",
    "tracking issue; when `main` is green again, closes that issue.",
  ].join("\n");
}

function parseArgs(argv) {
  const options = {
    fixture: null,
    repository: process.env.REPOSITORY || process.env.GITHUB_REPOSITORY || null,
    now: null,
    maxRuns: DEFAULT_MAX_RUNS,
    fileIssue: false,
    enforce: false,
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
    if (arg === "--now") {
      options.now = argv[++i];
      if (!options.now || !Number.isFinite(Date.parse(options.now))) {
        throw new Error("--now requires an ISO timestamp");
      }
      continue;
    }
    if (arg === "--max-runs") {
      const value = Number.parseInt(argv[++i], 10);
      if (!Number.isInteger(value) || value <= 0) {
        throw new Error("--max-runs requires a positive integer");
      }
      options.maxRuns = value;
      continue;
    }
    if (arg === "--file-issue") {
      options.fileIssue = true;
      continue;
    }
    if (arg === "--enforce") {
      options.enforce = true;
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

// Bounded exponential backoff over transient gh transport failures (5xx,
// secondary rate limit, transient network errors). This retries only the
// transport — never a real verdict/finding — so the sentinel's signal is
// unchanged; it just survives a flaky GitHub API call instead of reddening the
// advisory ci-health workflow. See issue #13744.
const GH_RETRY_ATTEMPTS = Math.max(1, Number.parseInt(process.env.GH_RETRY_ATTEMPTS || "", 10) || 4);
const GH_RETRY_BASE_MS = Math.max(0, Number.parseInt(process.env.GH_RETRY_BASE_MS || "", 10) || 500);
const GH_RETRY_MAX_MS = 8000;
const TRANSIENT_NET_CODES = new Set([
  "ETIMEDOUT", "ECONNRESET", "ECONNREFUSED", "EAI_AGAIN", "ENOTFOUND", "EPIPE",
]);

function sleepSync(ms) {
  if (!(ms > 0)) return;
  // Synchronous sleep without busy-waiting; spawnSync gives us no async seam.
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function isTransientGhResult(result) {
  if (result.error) {
    // ENOBUFS is a hard output-size error, not transient.
    if (result.error.code === "ENOBUFS") return false;
    return TRANSIENT_NET_CODES.has(result.error.code);
  }
  if ((result.status ?? 0) === 0) return false;
  const text = `${result.stdout || ""}\n${result.stderr || ""}`;
  return /\bHTTP\s+(?:408|425|429|5\d\d)\b/i.test(text)
    || /secondary rate limit/i.test(text)
    || /\b(?:Bad Gateway|Service Unavailable|Gateway Time-?out|Internal Server Error|Server Error)\b/i.test(text);
}

function spawnGh(args, spawnOptions) {
  let result;
  for (let attempt = 1; attempt <= GH_RETRY_ATTEMPTS; attempt += 1) {
    result = spawnSync("gh", args, spawnOptions);
    if (attempt === GH_RETRY_ATTEMPTS || !isTransientGhResult(result)) break;
    sleepSync(Math.min(GH_RETRY_BASE_MS * 2 ** (attempt - 1), GH_RETRY_MAX_MS));
  }
  return result;
}

function runGhJson(args) {
  const result = spawnGh(args, {
    encoding: "utf8",
    maxBuffer: DEFAULT_GH_MAX_BUFFER_BYTES,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error([`gh ${args.join(" ")} failed`, result.stdout?.trim(), result.stderr?.trim()]
      .filter(Boolean).join("\n"));
  }
  return JSON.parse(result.stdout);
}

function runGh(args) {
  const result = spawnGh(args, {
    encoding: "utf8",
    maxBuffer: DEFAULT_GH_MAX_BUFFER_BYTES,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error) return { status: 1, stdout: "", stderr: result.error.message };
  return {
    status: result.status ?? 1,
    stdout: (result.stdout || "").trim(),
    stderr: (result.stderr || "").trim(),
  };
}

function normalizeRuns(payload) {
  const runs = Array.isArray(payload) ? payload : payload?.workflow_runs;
  if (!Array.isArray(runs)) {
    throw new Error("fixture or API response must be an array or contain workflow_runs");
  }
  return runs;
}

function readFixture(path) {
  return normalizeRuns(JSON.parse(fs.readFileSync(path, "utf8")));
}

export function readMainRuns(repository, maxRuns, fetchJson = runGhJson) {
  if (!repository) throw new Error("REPOSITORY or GITHUB_REPOSITORY is required");
  const payload = fetchJson([
    "api",
    "-H",
    "Accept: application/vnd.github+json",
    `repos/${repository}/actions/runs?branch=main&per_page=${Math.min(100, maxRuns)}`,
  ]);
  return normalizeRuns(payload).slice(0, maxRuns);
}

function runField(run, keys) {
  for (const key of keys) {
    if (run[key] !== undefined && run[key] !== null) return run[key];
  }
  return undefined;
}

function eventOf(run) {
  return runField(run, ["event"]) || "";
}

function workflowOf(run) {
  return runField(run, ["name", "workflow_name", "workflowName"]) || "";
}

function createdMs(run) {
  const iso = runField(run, ["created_at", "createdAt", "run_started_at", "updated_at"]);
  const ms = Date.parse(iso);
  return Number.isFinite(ms) ? ms : 0;
}

// Verdict on `main`'s tip: find the most recent *conclusive* CI run from a
// push/merge_group event and report whether it failed.
export function mainCiHealth(runs, options = {}) {
  const workflow = options.workflow ?? WORKFLOW_NAME;
  const candidates = runs
    .filter((run) => (runField(run, ["status"]) || "completed") === "completed")
    .filter((run) => workflowOf(run) === workflow)
    .filter((run) => MAIN_EVENTS.has(eventOf(run)))
    .filter((run) => !INCONCLUSIVE.has(runField(run, ["conclusion"])))
    .sort((a, b) => createdMs(b) - createdMs(a));

  if (candidates.length === 0) {
    return { red: false, status: "unknown", run: null };
  }

  const newest = candidates[0];
  const conclusion = runField(newest, ["conclusion"]);
  const red = conclusion === "failure" || conclusion === "timed_out" || conclusion === "startup_failure";
  return {
    red,
    status: red ? "red" : "green",
    conclusion,
    run: {
      id: runField(newest, ["id", "databaseId"]),
      sha: runField(newest, ["head_sha", "headSha"]) || "",
      event: eventOf(newest),
      title: runField(newest, ["display_title", "displayTitle"]) || workflowOf(newest),
      url: runField(newest, ["html_url", "url"]) || "",
      createdAt: runField(newest, ["created_at", "createdAt"]) || "",
    },
    // Most recent green run before the failure, if any — helps bound the
    // suspect merge window.
    lastGreen: (() => {
      const g = candidates.find((run) => runField(run, ["conclusion"]) === "success");
      if (!g) return null;
      return {
        sha: runField(g, ["head_sha", "headSha"]) || "",
        url: runField(g, ["html_url", "url"]) || "",
        createdAt: runField(g, ["created_at", "createdAt"]) || "",
      };
    })(),
  };
}

export function extractRegressedTests(log) {
  if (!log) return [];
  const out = new Set();
  let m;
  while ((m = REGRESSED_RE.exec(log)) !== null) out.add(m[1]);
  return [...out];
}

export function formatIssueBody(verdict, regressedTests, nowIso) {
  const run = verdict.run || {};
  const lines = [
    ISSUE_MARKER,
    "",
    "`main` is failing its post-merge CI on the conformance/parity gate. While",
    "this is red, every queued PR inherits the red base through the merge queue",
    "and is ejected, so the queue is effectively wedged until a fix/revert lands.",
    "",
    "| Field | Value |",
    "|-------|-------|",
    `| First red run | [${run.id ?? "?"}](${run.url || ""}) (${run.event || "?"}) |`,
    `| Head commit | \`${(run.sha || "").slice(0, 12)}\` |`,
    `| Title | ${escapeInline(run.title || "")} |`,
    `| Detected | ${nowIso} |`,
  ];
  if (verdict.lastGreen?.sha) {
    lines.push(`| Last green | \`${verdict.lastGreen.sha.slice(0, 12)}\` ([run](${verdict.lastGreen.url})) |`);
  }
  lines.push("");
  if (regressedTests.length > 0) {
    lines.push("### Regressed tests");
    lines.push("");
    for (const t of regressedTests) lines.push(`- \`${t}\``);
    lines.push("");
  }
  lines.push("### What to do");
  lines.push("");
  lines.push("1. Identify the offending merge in the window above (bisect the named test with a narrow `--filter`).");
  lines.push("2. Land a `Goal: hold` fix or revert PR; its merge group (fix on top of red `main`) goes green and drains the queue.");
  lines.push("3. This issue auto-closes once a conclusive `main` CI run is green again.");
  lines.push("");
  lines.push("_Filed automatically by `scripts/ci/check-main-red.mjs` (ci-health). Do not edit the marker line._");
  return lines.join("\n");
}

function escapeInline(value) {
  return String(value ?? "").replace(/\|/g, "\\|").replace(/\n/g, " ");
}

export function formatReport(verdict, regressedTests = []) {
  const lines = ["## Main-Red Sentinel", ""];
  if (verdict.status === "unknown") {
    lines.push("No conclusive CI run found on `main` yet (push/merge_group). Nothing to report.");
    return lines.join("\n");
  }
  if (!verdict.red) {
    lines.push(`✅ \`main\` is green — latest conclusive CI run \`${(verdict.run.sha || "").slice(0, 12)}\` (${verdict.conclusion}).`);
    return lines.join("\n");
  }
  lines.push(`🔴 \`main\` is RED — run [${verdict.run.id}](${verdict.run.url}) (${verdict.run.event}) concluded \`${verdict.conclusion}\` at \`${(verdict.run.sha || "").slice(0, 12)}\`.`);
  if (regressedTests.length > 0) {
    lines.push("");
    lines.push("Regressed tests:");
    for (const t of regressedTests) lines.push(`- \`${t}\``);
  }
  return lines.join("\n");
}

// --- issue lifecycle (gh I/O) ---

// The issue body always carries the tracked head commit in its "Head commit"
// row (see formatIssueBody). Recover the short sha last recorded so we can post
// the heartbeat comment only when the red head actually moved, not every cron
// firing.
const TRACKED_SHA_RE = /Head commit\s*\|\s*`([0-9a-f]+)`/i;

function lastTrackedSha(issue) {
  if (!issue || typeof issue.body !== "string") return "";
  const match = issue.body.match(TRACKED_SHA_RE);
  return match ? match[1] : "";
}

function findSentinelIssue(repository, fetchJson) {
  const issues = fetchJson([
    "api",
    "-H",
    "Accept: application/vnd.github+json",
    `repos/${repository}/issues?state=open&per_page=100`,
  ]);
  if (!Array.isArray(issues)) return null;
  return issues.find((it) => typeof it.body === "string" && it.body.includes(ISSUE_MARKER)) || null;
}

export function reconcileIssue(verdict, regressedTests, nowIso, ctx) {
  const { repository, fetchJson = runGhJson, runCommand = runGh } = ctx;
  const existing = findSentinelIssue(repository, fetchJson);
  const body = formatIssueBody(verdict, regressedTests, nowIso);

  if (verdict.red) {
    if (existing) {
      // Always refresh the body (idempotent: same sha -> same body). Only add a
      // heartbeat comment when the red head sha changed since the last recorded
      // one, so a long-red `main` does not spam a comment every 15-min firing.
      const currentSha = (verdict.run.sha || "").slice(0, 12);
      const previousSha = lastTrackedSha(existing);
      runCommand(["issue", "edit", String(existing.number), "--repo", repository, "--body", body]);
      const headChanged = currentSha !== "" && currentSha !== previousSha;
      if (headChanged) {
        runCommand(["issue", "comment", String(existing.number), "--repo", repository,
          "--body", `Still red as of ${nowIso}: \`${currentSha}\` ([run](${verdict.run.url})).`]);
      }
      return { action: "updated", number: existing.number, commented: headChanged };
    }
    const created = runCommand(["issue", "create", "--repo", repository,
      "--title", ISSUE_TITLE, "--body", body, "--label", "tech-debt"]);
    return { action: "created", detail: created.stdout || created.stderr };
  }

  // Green: close any open sentinel issue.
  if (existing) {
    runCommand(["issue", "comment", String(existing.number), "--repo", repository,
      "--body", `✅ \`main\` is green again as of ${nowIso} (\`${(verdict.run.sha || "").slice(0, 12)}\`). Closing.`]);
    runCommand(["issue", "close", String(existing.number), "--repo", repository,
      "--reason", "completed"]);
    return { action: "closed", number: existing.number };
  }
  return { action: "noop" };
}

function bestEffortRegressedTests(repository, verdict) {
  if (!verdict.red || !verdict.run?.id) return [];
  // Find the failing conformance-aggregate job and scrape REGRESSED lines.
  try {
    const jobs = runGhJson([
      "api",
      "-H",
      "Accept: application/vnd.github+json",
      `repos/${repository}/actions/runs/${verdict.run.id}/jobs?per_page=100`,
    ]);
    const list = Array.isArray(jobs?.jobs) ? jobs.jobs : [];
    const agg = list.find((j) => /conformance-aggregate/i.test(j.name || "") && j.conclusion === "failure");
    if (!agg) return [];
    const log = runGh(["run", "view", "--repo", repository, "--job", String(agg.id), "--log"]);
    if (log.status !== 0) return [];
    return extractRegressedTests(log.stdout);
  } catch {
    return [];
  }
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const nowIso = options.now || new Date().toISOString();
  const runs = options.fixture
    ? readFixture(options.fixture)
    : readMainRuns(options.repository, options.maxRuns);
  const verdict = mainCiHealth(runs);

  const regressed = (!options.fixture && verdict.red && options.repository)
    ? bestEffortRegressedTests(options.repository, verdict)
    : [];

  console.log(formatReport(verdict, regressed));

  if (options.fileIssue && options.repository && !options.fixture) {
    const result = reconcileIssue(verdict, regressed, nowIso, { repository: options.repository });
    console.log(`sentinel issue: ${result.action}${result.number ? ` #${result.number}` : ""}`);
  }

  if (verdict.red && options.enforce) process.exit(1);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
