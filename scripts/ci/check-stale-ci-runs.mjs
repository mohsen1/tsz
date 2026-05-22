#!/usr/bin/env node
import fs from "node:fs";
import { spawnSync } from "node:child_process";

const DEFAULT_STALE_MINUTES = 45;
const DEFAULT_MAX_RUNS = 100;
const DEFAULT_GH_MAX_BUFFER_BYTES = 16 * 1024 * 1024;
const ACTIVE_STATUSES = ["in_progress", "queued"];

function usage() {
  return [
    "usage: check-stale-ci-runs.mjs [--fixture path] [--repository owner/repo] [--stale-minutes n] [--max-runs n] [--now iso] [--advisory|--enforce]",
    "",
    "Reports queued or in-progress workflow runs that have been active or",
    "unchanged long enough to look stale.",
  ].join("\n");
}

function parseArgs(argv) {
  const options = {
    advisory: true,
    enforce: false,
    fixture: null,
    maxRuns: DEFAULT_MAX_RUNS,
    now: null,
    repository: process.env.REPOSITORY || process.env.GITHUB_REPOSITORY || null,
    staleMinutes: DEFAULT_STALE_MINUTES,
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
    if (arg === "--stale-minutes") {
      const value = Number.parseInt(argv[++i], 10);
      if (!Number.isInteger(value) || value <= 0) {
        throw new Error("--stale-minutes requires a positive integer");
      }
      options.staleMinutes = value;
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
    if (arg === "--now") {
      options.now = argv[++i];
      if (!options.now || !Number.isFinite(Date.parse(options.now))) {
        throw new Error("--now requires an ISO timestamp");
      }
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

function runGhJson(args) {
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
  return JSON.parse(result.stdout);
}

function normalizeRuns(payload) {
  const runs = Array.isArray(payload) ? payload : payload.workflow_runs;
  if (!Array.isArray(runs)) {
    throw new Error("fixture or API response must be an array or contain workflow_runs");
  }
  return runs;
}

function readFixture(path) {
  return normalizeRuns(JSON.parse(fs.readFileSync(path, "utf8")));
}

function readRunsForStatus(repository, status, maxRuns, fetchJson) {
  const runs = [];
  for (let page = 1; runs.length < maxRuns && page <= 10; page += 1) {
    const payload = fetchJson([
      "api",
      "-H",
      "Accept: application/vnd.github+json",
      `repos/${repository}/actions/runs?status=${status}&per_page=100&page=${page}`,
    ]);
    const pageRuns = normalizeRuns(payload);
    runs.push(...pageRuns.slice(0, Math.max(0, maxRuns - runs.length)));
    if (pageRuns.length < 100) break;
  }

  return runs;
}

export function readActiveRuns(repository, maxRuns, fetchJson = runGhJson) {
  if (!repository) {
    throw new Error("REPOSITORY or GITHUB_REPOSITORY is required");
  }

  const runsByStatus = ACTIVE_STATUSES.map((status) =>
    readRunsForStatus(repository, status, maxRuns, fetchJson),
  );
  const runs = [];
  for (let index = 0; runs.length < maxRuns; index += 1) {
    let addedRun = false;
    for (const statusRuns of runsByStatus) {
      if (index < statusRuns.length) {
        runs.push(statusRuns[index]);
        addedRun = true;
        if (runs.length >= maxRuns) break;
      }
    }
    if (!addedRun) break;
  }

  return runs;
}

function timestamp(run, keys) {
  for (const key of keys) {
    const value = run[key];
    if (typeof value === "string" && Number.isFinite(Date.parse(value))) {
      return value;
    }
  }
  return null;
}

function minutesSince(nowMs, iso) {
  if (!iso) return null;
  const thenMs = Date.parse(iso);
  if (!Number.isFinite(thenMs)) return null;
  return Math.max(0, Math.floor((nowMs - thenMs) / 60000));
}

function runTitle(run) {
  return run.display_title || run.name || run.workflow_name || "(untitled)";
}

function runBranch(run) {
  return run.head_branch || run.headBranch || "";
}

function runUrl(run) {
  return run.html_url || run.url || "";
}

export function staleRunFindings(runs, options = {}) {
  const staleMinutes = options.staleMinutes ?? DEFAULT_STALE_MINUTES;
  const nowMs = Date.parse(options.now || new Date().toISOString());
  if (!Number.isFinite(nowMs)) throw new Error("now must be a valid ISO timestamp");

  return runs
    .filter((run) => ACTIVE_STATUSES.includes(run.status))
    .map((run) => {
      const startedAt = timestamp(run, ["run_started_at", "started_at", "created_at", "createdAt"]);
      const updatedAt = timestamp(run, ["updated_at", "updatedAt", "created_at", "createdAt"]);
      const ageMinutes = minutesSince(nowMs, startedAt);
      const updatedMinutes = minutesSince(nowMs, updatedAt);
      const staleByAge = run.status === "queued" && ageMinutes !== null && ageMinutes >= staleMinutes;
      const staleByUpdate = updatedMinutes !== null && updatedMinutes >= staleMinutes;
      const staleByMissingUpdate = updatedMinutes === null && ageMinutes !== null && ageMinutes >= staleMinutes;
      if (!staleByAge && !staleByUpdate && !staleByMissingUpdate) return null;

      return {
        id: run.id || run.databaseId || "",
        status: run.status,
        title: runTitle(run),
        branch: runBranch(run),
        url: runUrl(run),
        ageMinutes,
        updatedMinutes,
        reason: staleByUpdate || staleByMissingUpdate ? "no recent update" : "old queued run",
      };
    })
    .filter(Boolean)
    .sort((a, b) => (b.updatedMinutes ?? 0) - (a.updatedMinutes ?? 0));
}

function escapeCell(value) {
  return String(value ?? "").replace(/\|/g, "\\|").replace(/\n/g, " ");
}

function minuteLabel(value) {
  return value === null || value === undefined ? "unknown" : `${value}m`;
}

export function formatReport(findings, options = {}) {
  const staleMinutes = options.staleMinutes ?? DEFAULT_STALE_MINUTES;
  const lines = [
    "## Stale CI Run Advisory",
    "",
  ];

  if (findings.length === 0) {
    lines.push(`No queued or in-progress workflow runs are stale at ${staleMinutes} minutes.`);
    return lines.join("\n");
  }

  lines.push(`Found ${findings.length} queued or in-progress workflow run(s) stale at ${staleMinutes} minutes.`);
  lines.push("");
  lines.push("| Run | Status | Age | Last update | Branch | Reason | Title |");
  lines.push("|-----|--------|-----|-------------|--------|--------|-------|");
  for (const finding of findings) {
    const runLabel = finding.url && finding.id
      ? `[#${finding.id}](${finding.url})`
      : escapeCell(finding.id || "unknown");
    lines.push([
      runLabel,
      escapeCell(finding.status),
      minuteLabel(finding.ageMinutes),
      minuteLabel(finding.updatedMinutes),
      escapeCell(finding.branch),
      escapeCell(finding.reason),
      escapeCell(finding.title),
    ].join(" | ").replace(/^/, "| ").replace(/$/, " |"));
  }

  return lines.join("\n");
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const runs = options.fixture
    ? readFixture(options.fixture)
    : readActiveRuns(options.repository, options.maxRuns);
  const findings = staleRunFindings(runs, options);
  console.log(formatReport(findings, options));
  if (findings.length > 0 && options.enforce) {
    process.exit(1);
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
