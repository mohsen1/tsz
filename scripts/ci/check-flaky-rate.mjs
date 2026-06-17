#!/usr/bin/env node
// Per-job flaky-rate probe for the self-hosted CI fleet.
//
// Rationale: a flaky job wedges the merge queue in both directions — a flaky
// *pass* at merge_group time lands a real regression, and a flaky *fail* ejects
// a good PR. The fleet has determinism work in flight (#13368/#13255) but no
// standing rate signal to prioritize it. This probe lists recent `main` CI runs
// (push/merge_group), groups each job by name across runs and `run_attempt`
// re-runs, and counts conclusion *flips* (success<->failure) between
// consecutive conclusive results. A high flip rate means a job is
// non-deterministic on an unchanging-ish tree.
//
// Advisory only: it emits a markdown table and `::warning::` lines, and never
// fails the workflow. Pure functions (flakyFindings/formatReport) are unit
// tested; gh I/O lives behind injectable fetchers, mirroring the sibling
// ci-health probes.
import fs from "node:fs";
import { spawnSync } from "node:child_process";

const DEFAULT_WORKFLOW = "CI";
const DEFAULT_BRANCH = "main";
const DEFAULT_MAX_RUNS = 40;
// Flip rate above this fraction (flips / conclusive-transitions) warns.
const DEFAULT_THRESHOLD = 0.15;
// Need at least this many conclusive results for a job before its flip rate is
// meaningful — a 1-of-2 flip is not a trend.
const DEFAULT_MIN_SAMPLES = 4;
const DEFAULT_GH_MAX_BUFFER_BYTES = 32 * 1024 * 1024;
const MAIN_EVENTS = new Set(["push", "merge_group"]);
// Conclusions that represent a real verdict on the tree (others are ignored).
const CONCLUSIVE = new Set(["success", "failure", "timed_out", "startup_failure"]);

function usage() {
  return [
    "usage: check-flaky-rate.mjs [--fixture path] [--repository owner/repo] [--workflow name]",
    "                           [--branch name] [--max-runs n] [--threshold f] [--min-samples n]",
    "",
    "Counts per-job conclusion flips across recent main CI runs and run_attempt",
    "re-runs, and reports a flaky rate. Advisory: it never fails the workflow.",
  ].join("\n");
}

export function parseArgs(argv) {
  const options = {
    fixture: null,
    repository: process.env.REPOSITORY || process.env.GITHUB_REPOSITORY || null,
    workflow: DEFAULT_WORKFLOW,
    branch: DEFAULT_BRANCH,
    maxRuns: DEFAULT_MAX_RUNS,
    threshold: DEFAULT_THRESHOLD,
    minSamples: DEFAULT_MIN_SAMPLES,
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
    if (arg === "--workflow") {
      options.workflow = argv[++i];
      if (!options.workflow) throw new Error("--workflow requires a name");
      continue;
    }
    if (arg === "--branch") {
      options.branch = argv[++i];
      if (!options.branch) throw new Error("--branch requires a name");
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
    if (arg === "--threshold") {
      const value = Number.parseFloat(argv[++i]);
      if (!Number.isFinite(value) || value < 0 || value > 1) {
        throw new Error("--threshold requires a number in [0, 1]");
      }
      options.threshold = value;
      continue;
    }
    if (arg === "--min-samples") {
      const value = Number.parseInt(argv[++i], 10);
      if (!Number.isInteger(value) || value <= 0) {
        throw new Error("--min-samples requires a positive integer");
      }
      options.minSamples = value;
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
// secondary rate limit, transient network errors). Retries only the transport,
// never a real finding, so this advisory probe survives a flaky GitHub API call
// instead of reddening the workflow. See issue #13744.
const GH_RETRY_ATTEMPTS = Math.max(1, Number.parseInt(process.env.GH_RETRY_ATTEMPTS || "", 10) || 4);
const GH_RETRY_BASE_MS = Math.max(0, Number.parseInt(process.env.GH_RETRY_BASE_MS || "", 10) || 500);
const GH_RETRY_MAX_MS = 8000;
const TRANSIENT_NET_CODES = new Set([
  "ETIMEDOUT", "ECONNRESET", "ECONNREFUSED", "EAI_AGAIN", "ENOTFOUND", "EPIPE",
]);

function sleepSync(ms) {
  if (!(ms > 0)) return;
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function isTransientGhResult(result) {
  if (result.error) {
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
  if (result.error) {
    const command = `gh ${args.join(" ")}`;
    if (result.error.code === "ENOBUFS") {
      throw new Error(`${command} exceeded ${DEFAULT_GH_MAX_BUFFER_BYTES} bytes of output`);
    }
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error([
      `gh ${args.join(" ")} failed`,
      result.stdout?.trim(),
      result.stderr?.trim(),
    ].filter(Boolean).join("\n"));
  }
  return JSON.parse(result.stdout);
}

function normalizeRuns(payload) {
  const runs = Array.isArray(payload) ? payload : payload?.workflow_runs;
  if (!Array.isArray(runs)) {
    throw new Error("fixture or API response must be an array or contain workflow_runs");
  }
  return runs;
}

export function readMainRuns(repository, options, fetchJson = runGhJson) {
  if (!repository) throw new Error("REPOSITORY or GITHUB_REPOSITORY is required");
  const { branch, maxRuns } = options;
  const payload = fetchJson([
    "api",
    "-H",
    "Accept: application/vnd.github+json",
    `repos/${repository}/actions/runs?branch=${encodeURIComponent(branch)}&per_page=${Math.min(100, maxRuns)}`,
  ]);
  return normalizeRuns(payload).slice(0, maxRuns);
}

export function readJobsForRun(repository, runId, fetchJson = runGhJson) {
  const jobs = [];
  for (let page = 1; page <= 10; page += 1) {
    // filter=all captures every run_attempt's jobs, not just the latest, so a
    // re-run-to-green (or re-run-to-red) shows up as a conclusion flip.
    const payload = fetchJson([
      "api",
      "-H",
      "Accept: application/vnd.github+json",
      `repos/${repository}/actions/runs/${runId}/jobs?filter=all&per_page=100&page=${page}`,
    ]);
    const pageJobs = Array.isArray(payload?.jobs) ? payload.jobs : [];
    jobs.push(...pageJobs);
    if (pageJobs.length < 100) break;
  }
  return jobs;
}

function eventOf(run) {
  return run.event ?? "";
}

function workflowOf(run) {
  return run.name ?? run.workflow_name ?? run.workflowName ?? "";
}

function createdMs(run) {
  const iso = run.created_at ?? run.createdAt ?? run.run_started_at ?? run.updated_at;
  const ms = Date.parse(iso);
  return Number.isFinite(ms) ? ms : 0;
}

export function collectRunsWithJobs(repository, options, fetchJson = runGhJson) {
  const runs = readMainRuns(repository, options, fetchJson)
    .filter((run) => workflowOf(run) === options.workflow)
    .filter((run) => MAIN_EVENTS.has(eventOf(run)));
  return runs.map((run) => ({
    id: run.id ?? run.databaseId ?? "",
    createdMs: createdMs(run),
    runAttempt: run.run_attempt ?? run.runAttempt ?? 1,
    jobs: readJobsForRun(repository, run.id ?? run.databaseId, fetchJson),
  }));
}

// A stable ordering key for a (run, job-attempt) data point: oldest run first,
// then earliest attempt. Ties on identical timestamps fall back to id.
function dataPointOrder(a, b) {
  return a.createdMs - b.createdMs || a.runAttempt - b.runAttempt || String(a.id).localeCompare(String(b.id));
}

export function flakyFindings(runsWithJobs, options = {}) {
  const minSamples = options.minSamples ?? DEFAULT_MIN_SAMPLES;

  const byName = new Map();
  for (const run of runsWithJobs) {
    for (const job of run.jobs || []) {
      const conclusion = job.conclusion;
      if (!CONCLUSIVE.has(conclusion)) continue;
      const name = job.name || "(unnamed)";
      let series = byName.get(name);
      if (!series) {
        series = [];
        byName.set(name, series);
      }
      series.push({
        id: `${run.id}#${job.run_attempt ?? job.runAttempt ?? run.runAttempt ?? 1}`,
        createdMs: run.createdMs ?? 0,
        // Attempt-level ordering uses the job's own run_attempt when present.
        runAttempt: job.run_attempt ?? job.runAttempt ?? run.runAttempt ?? 1,
        // Normalize every non-success conclusive verdict to "fail" — what we
        // care about is the pass/fail flip, not which failure flavor.
        verdict: conclusion === "success" ? "pass" : "fail",
      });
    }
  }

  const findings = [];
  for (const [name, series] of byName) {
    series.sort(dataPointOrder);
    const samples = series.length;
    const passes = series.filter((s) => s.verdict === "pass").length;
    const fails = samples - passes;
    let flips = 0;
    for (let i = 1; i < series.length; i += 1) {
      if (series[i].verdict !== series[i - 1].verdict) flips += 1;
    }
    const transitions = Math.max(0, samples - 1);
    const flipRate = transitions > 0 ? flips / transitions : 0;
    findings.push({
      name,
      samples,
      passes,
      fails,
      flips,
      flipRate,
      // A job is only ranked as flaky when it has both outcomes AND enough
      // samples; an all-pass or all-fail job has flipRate 0 anyway.
      enoughSamples: samples >= minSamples,
    });
  }

  // Flakiest first; tie-break on more flips then name for stable output.
  findings.sort((a, b) => b.flipRate - a.flipRate || b.flips - a.flips || a.name.localeCompare(b.name));
  return findings;
}

function escapeCell(value) {
  return String(value ?? "").replace(/\|/g, "\\|").replace(/\n/g, " ");
}

export function formatReport(findings, options = {}) {
  const threshold = options.threshold ?? DEFAULT_THRESHOLD;
  const workflow = options.workflow || DEFAULT_WORKFLOW;
  const branch = options.branch || DEFAULT_BRANCH;
  const lines = ["## CI Flaky-Rate Probe", ""];

  if (findings.length === 0) {
    lines.push(`No conclusive \`${workflow}\` jobs on \`${branch}\` were found to score.`);
    return lines.join("\n");
  }

  const runs = options.runCount;
  lines.push(
    `Per-job conclusion-flip rate over recent \`${workflow}\` runs on \`${branch}\`` +
      (runs ? ` (${runs} run${runs === 1 ? "" : "s"} sampled)` : "") +
      `. Flip rate = pass↔fail transitions / conclusive transitions; warn threshold ${(threshold * 100).toFixed(0)}%.`,
  );
  lines.push("");
  lines.push("| Job | Samples | Pass | Fail | Flips | Flip rate |");
  lines.push("|-----|--------:|-----:|-----:|------:|----------:|");
  for (const f of findings) {
    const flagged = f.enoughSamples && f.flipRate > threshold;
    lines.push([
      escapeCell(f.name) + (flagged ? " ⚠️" : ""),
      String(f.samples),
      String(f.passes),
      String(f.fails),
      String(f.flips),
      `${(f.flipRate * 100).toFixed(0)}%`,
    ].join(" | ").replace(/^/, "| ").replace(/$/, " |"));
  }
  return lines.join("\n");
}

export function flaggedFindings(findings, options = {}) {
  const threshold = options.threshold ?? DEFAULT_THRESHOLD;
  return findings.filter((f) => f.enoughSamples && f.flipRate > threshold);
}

function readFixture(path) {
  const parsed = JSON.parse(fs.readFileSync(path, "utf8"));
  const runs = Array.isArray(parsed) ? parsed : parsed.runs;
  if (!Array.isArray(runs)) {
    throw new Error("fixture must be an array of runs or an object with a runs array");
  }
  return runs.map((run) => ({
    id: run.id ?? "",
    createdMs: run.createdMs ?? createdMs(run),
    runAttempt: run.run_attempt ?? run.runAttempt ?? 1,
    jobs: run.jobs || [],
  }));
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const runsWithJobs = options.fixture
    ? readFixture(options.fixture)
    : collectRunsWithJobs(options.repository, options);
  const findings = flakyFindings(runsWithJobs, options);
  console.log(formatReport(findings, { ...options, runCount: runsWithJobs.length }));

  for (const f of flaggedFindings(findings, options)) {
    console.log(
      `::warning::check-flaky-rate: job "${f.name}" flipped pass<->fail ${f.flips} time(s) ` +
        `over ${f.samples} run(s) (${(f.flipRate * 100).toFixed(0)}% flip rate, ` +
        `threshold ${(options.threshold * 100).toFixed(0)}%)`,
    );
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
