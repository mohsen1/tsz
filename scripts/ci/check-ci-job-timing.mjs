#!/usr/bin/env node
// Per-job wall-time baseline for the self-hosted CI fleet.
//
// Pulls the most recent successful runs of a workflow (default ci.yml on main),
// joins each run with its jobs, and reports per-job-name timing distributions:
//   - queue_wait seconds = job.started_at - job.created_at
//     (time a job spent eligible-but-waiting for a runner; the fleet-capacity
//     signal — high queue_wait means the runner pool is the bottleneck, not the
//     work)
//   - run seconds = job.completed_at - job.started_at
//     (the actual work; the wall-time signal for "which job defines run end")
//
// Advisory only: this changes no wall-clock behavior. It is the measurement
// baseline that every other CI speed claim on this fleet is checked against
// (issue #13605 item 2), so percentiles are computed deterministically
// (nearest-rank) and the output is a stable markdown table.
import fs from "node:fs";
import { spawnSync } from "node:child_process";

const DEFAULT_WORKFLOW = "ci.yml";
const DEFAULT_BRANCH = "main";
const DEFAULT_MAX_RUNS = 15;
// Larger than the sibling probes' 16MB: this script reads the jobs payload for
// up to DEFAULT_MAX_RUNS runs, each carrying every job's step list.
const DEFAULT_GH_MAX_BUFFER_BYTES = 32 * 1024 * 1024;

function usage() {
  return [
    "usage: check-ci-job-timing.mjs [--fixture path] [--repository owner/repo] [--workflow file] [--branch name] [--max-runs n]",
    "",
    "Reports per-job queue_wait and run-time distributions over recent",
    "successful workflow runs. Advisory: it never fails the workflow.",
  ].join("\n");
}

export function parseArgs(argv) {
  const options = {
    branch: DEFAULT_BRANCH,
    fixture: null,
    maxRuns: DEFAULT_MAX_RUNS,
    repository: process.env.REPOSITORY || process.env.GITHUB_REPOSITORY || null,
    workflow: DEFAULT_WORKFLOW,
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
      if (!options.workflow) throw new Error("--workflow requires a file name");
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
// never a real finding, so this advisory baseline survives a flaky GitHub API
// call instead of reddening the workflow. See issue #13744.
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

function asArray(payload, key) {
  const value = payload?.[key];
  if (!Array.isArray(value)) {
    throw new Error(`API response did not contain an array of ${key}`);
  }
  return value;
}

export function readWorkflowRuns(repository, options, fetchJson = runGhJson) {
  if (!repository) {
    throw new Error("REPOSITORY or GITHUB_REPOSITORY is required");
  }
  const { workflow, branch, maxRuns } = options;
  const payload = fetchJson([
    "api",
    "-H",
    "Accept: application/vnd.github+json",
    `repos/${repository}/actions/workflows/${encodeURIComponent(workflow)}/runs?status=success&branch=${encodeURIComponent(branch)}&per_page=${maxRuns}`,
  ]);
  return asArray(payload, "workflow_runs").slice(0, maxRuns);
}

export function readJobsForRun(repository, runId, fetchJson = runGhJson) {
  const jobs = [];
  for (let page = 1; page <= 10; page += 1) {
    const payload = fetchJson([
      "api",
      "-H",
      "Accept: application/vnd.github+json",
      `repos/${repository}/actions/runs/${runId}/jobs?per_page=100&page=${page}`,
    ]);
    const pageJobs = asArray(payload, "jobs");
    jobs.push(...pageJobs);
    if (pageJobs.length < 100) break;
  }
  return jobs;
}

export function collectRunsWithJobs(repository, options, fetchJson = runGhJson) {
  const runs = readWorkflowRuns(repository, options, fetchJson);
  return runs.map((run) => ({
    id: run.id ?? run.databaseId ?? "",
    jobs: readJobsForRun(repository, run.id ?? run.databaseId, fetchJson),
  }));
}

function secondsBetween(startIso, endIso) {
  if (typeof startIso !== "string" || typeof endIso !== "string") return null;
  const start = Date.parse(startIso);
  const end = Date.parse(endIso);
  if (!Number.isFinite(start) || !Number.isFinite(end)) return null;
  return Math.max(0, Math.round((end - start) / 1000));
}

// Nearest-rank percentile over an unsorted sample. p in [0, 1].
function percentile(values, p) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const rank = Math.ceil(p * sorted.length);
  const index = Math.min(sorted.length - 1, Math.max(0, rank - 1));
  return sorted[index];
}

export function jobTimingFindings(runsWithJobs) {
  const byName = new Map();

  for (const run of runsWithJobs) {
    for (const job of run.jobs || []) {
      const name = job.name || "(unnamed)";
      // Only successful, fully-timed jobs contribute to the baseline; failed or
      // cancelled jobs have truncated or missing completion times that would
      // skew the distribution.
      if (job.conclusion !== "success") continue;
      const runSeconds = secondsBetween(job.started_at, job.completed_at);
      const queueSeconds = secondsBetween(job.created_at, job.started_at);
      if (runSeconds === null) continue;

      let bucket = byName.get(name);
      if (!bucket) {
        bucket = { name, runSamples: [], queueSamples: [] };
        byName.set(name, bucket);
      }
      bucket.runSamples.push(runSeconds);
      if (queueSeconds !== null) bucket.queueSamples.push(queueSeconds);
    }
  }

  const findings = [];
  for (const bucket of byName.values()) {
    findings.push({
      name: bucket.name,
      samples: bucket.runSamples.length,
      runP50: percentile(bucket.runSamples, 0.5),
      runP95: percentile(bucket.runSamples, 0.95),
      runMax: percentile(bucket.runSamples, 1),
      queueP50: percentile(bucket.queueSamples, 0.5),
      queueMax: percentile(bucket.queueSamples, 1),
    });
  }

  // Slowest-by-typical-run first: the job that most often defines run end is the
  // one worth attacking. Tie-break on name for stable output.
  findings.sort((a, b) => (b.runP50 ?? 0) - (a.runP50 ?? 0) || a.name.localeCompare(b.name));
  return findings;
}

function escapeCell(value) {
  return String(value ?? "").replace(/\|/g, "\\|").replace(/\n/g, " ");
}

function secondsLabel(value) {
  if (value === null || value === undefined) return "—";
  if (value < 60) return `${value}s`;
  const minutes = Math.floor(value / 60);
  const seconds = value % 60;
  return `${minutes}m${String(seconds).padStart(2, "0")}s`;
}

export function formatReport(findings, options = {}) {
  const workflow = options.workflow || DEFAULT_WORKFLOW;
  const branch = options.branch || DEFAULT_BRANCH;
  const lines = ["## CI Job Timing Baseline", ""];

  if (findings.length === 0) {
    lines.push(`No successful \`${workflow}\` jobs on \`${branch}\` were found to time.`);
    return lines.join("\n");
  }

  const runs = options.runCount;
  lines.push(
    `Per-job timing over the latest successful \`${workflow}\` runs on \`${branch}\`` +
      (runs ? ` (${runs} run${runs === 1 ? "" : "s"} sampled).` : "."),
  );
  lines.push("");
  lines.push("| Job | Samples | Run p50 | Run p95 | Run max | Queue p50 | Queue max |");
  lines.push("|-----|--------:|--------:|--------:|--------:|----------:|----------:|");
  for (const finding of findings) {
    lines.push([
      escapeCell(finding.name),
      String(finding.samples),
      secondsLabel(finding.runP50),
      secondsLabel(finding.runP95),
      secondsLabel(finding.runMax),
      secondsLabel(finding.queueP50),
      secondsLabel(finding.queueMax),
    ].join(" | ").replace(/^/, "| ").replace(/$/, " |"));
  }
  return lines.join("\n");
}

function readFixture(path) {
  const parsed = JSON.parse(fs.readFileSync(path, "utf8"));
  const runs = Array.isArray(parsed) ? parsed : parsed.runs;
  if (!Array.isArray(runs)) {
    throw new Error("fixture must be an array of runs or an object with a runs array");
  }
  return runs.map((run) => ({ id: run.id ?? "", jobs: run.jobs || [] }));
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const runsWithJobs = options.fixture
    ? readFixture(options.fixture)
    : collectRunsWithJobs(options.repository, options);
  const findings = jobTimingFindings(runsWithJobs);
  console.log(formatReport(findings, { ...options, runCount: runsWithJobs.length }));
}

if (import.meta.url === `file://${process.argv[1]}`) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
