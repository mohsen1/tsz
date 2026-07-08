#!/usr/bin/env node
/**
 * Detects a stale *published* benchmark dataset.
 *
 * Fetches the public benchmark JSON (default
 * `https://tsz.dev/benchmark-data/latest.json`), parses `generated_at`, and
 * alerts when it is older than `--max-age-hours` (default 6) — or when the
 * payload cannot be parsed into a dated artifact at all.
 *
 * Why this exists: nothing else watches the *published* dataset's freshness.
 * `check-artifact-readiness.mjs` only inspects a freshly merged GitHub Actions
 * artifact and gates on source-commit currency, not wall-clock staleness, so the
 * advisory readiness probe stays green while `latest.json` silently stops
 * advancing. A 46-commit / half-day publish freeze went unnoticed precisely
 * because no monitor compared the live `generated_at` to now. This is that
 * monitor.
 *
 * Exit codes:
 *   0 — fresh (or advisory mode, the default, regardless of status)
 *   3 — stale or unparseable artifact AND `--strict`
 *   4 — fetch/transport failure AND `--strict` (advisory "unknown" by default)
 *
 * With `--file-issue`: opens / updates / closes ONE deduplicated tracking issue
 * keyed on a hidden marker, mirroring `scripts/ci/check-main-red.mjs`, so a
 * standing freeze surfaces as a single tracking issue (non-spammy) instead of a
 * red advisory workflow nobody reads.
 *
 * Usage:
 *   node scripts/bench/check-latest-freshness.mjs [--url <url>] [--max-age-hours <n>]
 *     [--fixture <file>] [--file-issue] [--strict] [--json]
 */

import fs from "node:fs";
import https from "node:https";
import { pathToFileURL } from "node:url";

import { runGh, runGhJson } from "../ci/lib/gh.mjs";
import {
  collectSentinelIssues,
  splitSentinels,
  createSentinelIssue,
  closeSentinelIssue,
  closeDuplicateSentinels,
} from "../ci/lib/sentinel-issues.mjs";

export const DEFAULT_LATEST_URL = "https://tsz.dev/benchmark-data/latest.json";
export const DEFAULT_MAX_AGE_HOURS = 6;
const ISSUE_MARKER = "<!-- bench-latest-freshness-sentinel -->";
const ISSUE_TITLE =
  "🟡 benchmark site data is stale — latest.json generated_at not advancing";
const MS_PER_HOUR = 3_600_000;

/**
 * Pure classifier: decide freshness from a response body + a clock.
 *
 * Returns `{ status, alert, reason, generatedAt, ageHours, maxAgeHours }` where
 * `status` is one of `"ok" | "stale" | "unparseable"` and `alert` is true for
 * anything that should raise attention.
 *
 * @param {string|object|null|undefined} body
 * @param {{ now?: number, maxAgeHours?: number }} [options]
 */
export function classifyFreshness(body, options = {}) {
  const now = Number.isFinite(options.now) ? options.now : Date.now();
  const maxAgeHours = Number.isFinite(options.maxAgeHours)
    ? options.maxAgeHours
    : DEFAULT_MAX_AGE_HOURS;

  let parsed;
  if (body && typeof body === "object") {
    parsed = body;
  } else if (typeof body === "string") {
    try {
      parsed = JSON.parse(body);
    } catch {
      return {
        status: "unparseable",
        alert: true,
        reason: "published benchmark payload is not valid JSON",
        generatedAt: null,
        ageHours: null,
        maxAgeHours,
      };
    }
  } else {
    return {
      status: "unparseable",
      alert: true,
      reason: "published benchmark payload is empty",
      generatedAt: null,
      ageHours: null,
      maxAgeHours,
    };
  }

  const generatedAt =
    parsed && typeof parsed === "object" ? parsed.generated_at : undefined;
  const generatedMs = generatedAt ? Date.parse(generatedAt) : Number.NaN;
  if (!generatedAt || Number.isNaN(generatedMs)) {
    return {
      status: "unparseable",
      alert: true,
      reason: generatedAt
        ? `generated_at ${JSON.stringify(generatedAt)} is not a parseable date`
        : "generated_at field is missing from the published benchmark payload",
      generatedAt: generatedAt ?? null,
      ageHours: null,
      maxAgeHours,
    };
  }

  const ageHours = (now - generatedMs) / MS_PER_HOUR;
  // A future generated_at (clock skew) is not stale; clamp the display age.
  const displayAge = Math.max(0, ageHours);
  if (ageHours > maxAgeHours) {
    return {
      status: "stale",
      alert: true,
      reason: `published benchmark data is ${ageHours.toFixed(
        1,
      )}h old (threshold ${maxAgeHours}h)`,
      generatedAt,
      ageHours,
      maxAgeHours,
    };
  }
  return {
    status: "ok",
    alert: false,
    reason: `published benchmark data is ${displayAge.toFixed(
      1,
    )}h old (threshold ${maxAgeHours}h)`,
    generatedAt,
    ageHours,
    maxAgeHours,
  };
}

/**
 * Render a short markdown report (stdout / step summary).
 * @param {ReturnType<typeof classifyFreshness>} verdict
 * @param {{ url: string }} ctx
 */
export function formatReport(verdict, ctx) {
  const icon =
    verdict.status === "ok" ? "✅" : verdict.status === "stale" ? "🟡" : "🔴";
  const lines = [
    `### ${icon} Published benchmark freshness`,
    "",
    `- Source: \`${ctx.url}\``,
    `- generated_at: \`${verdict.generatedAt ?? "—"}\``,
    `- Age: ${
      verdict.ageHours == null ? "unknown" : `${verdict.ageHours.toFixed(1)}h`
    } (threshold ${verdict.maxAgeHours}h)`,
    `- Status: **${verdict.status}** — ${verdict.reason}`,
  ];
  return lines.join("\n");
}

/**
 * Issue body for the dedup tracking issue. Embeds the marker (for dedup) and the
 * tracked generated_at (so a still-stale heartbeat only comments when the
 * generated_at actually changed, never every firing).
 */
export function formatIssueBody(verdict, ctx, nowIso) {
  return [
    ISSUE_MARKER,
    "",
    `**The public benchmark dataset has stopped advancing.**`,
    "",
    `- Source: \`${ctx.url}\``,
    `- generated_at: \`${verdict.generatedAt ?? "—"}\``,
    `- Age: ${
      verdict.ageHours == null ? "unknown" : `${verdict.ageHours.toFixed(1)}h`
    } (threshold ${verdict.maxAgeHours}h)`,
    `- Status: \`${verdict.status}\` — ${verdict.reason}`,
    `- Last checked: ${nowIso}`,
    "",
    "Likely causes: the Bench `bench-publish` job stopped publishing `latest.json`",
    "(readiness gate, required-shard completeness, runner-signature, or",
    "`pgo-compile-canaries` failure), or the gh-pages site stopped redeploying.",
    "",
    "Quick fix without a ~2.5h matrix rerun: re-publish a recent COMPLETE run's",
    "artifact with `gh workflow run bench-republish.yml -f run_id=<run id>`",
    "(re-gates against current main, preserves the monotonic guard).",
    "",
    "_Filed automatically by `scripts/bench/check-latest-freshness.mjs`",
    "(ci-health bench-latest-freshness). Do not edit the marker line._",
  ].join("\n");
}

const TRACKED_GENERATED_RE = /generated_at:\s*`([^`]+)`/;

/**
 * Open / update / close the single dedup tracking issue. The oldest open
 * sentinel is canonical; younger matches from a past lookup miss are closed as
 * duplicates (see `../ci/lib/sentinel-issues.mjs` for the healing rules).
 *
 * @param {ReturnType<typeof classifyFreshness>} verdict
 */
export function reconcileIssue(verdict, ctx, nowIso, gh = {}) {
  const { repository } = ctx;
  const fetchJson = gh.fetchJson || runGhJson;
  const runCommand = gh.runCommand || runGh;
  const matches = collectSentinelIssues(repository, fetchJson, {
    marker: ISSUE_MARKER,
    title: ISSUE_TITLE,
  });
  const { canonical: existing, duplicates } = splitSentinels(matches);
  const closeDupes = () =>
    closeDuplicateSentinels(duplicates, existing.number, repository, runCommand, nowIso);

  if (verdict.alert) {
    const body = formatIssueBody(verdict, ctx, nowIso);
    if (existing) {
      const previous = (existing.body?.match(TRACKED_GENERATED_RE) || [])[1] || "";
      const current = verdict.generatedAt ?? "";
      runCommand(["issue", "edit", String(existing.number), "--repo", repository, "--body", body]);
      // Heartbeat-comment only when the tracked generated_at changed, so a
      // standing freeze does not comment on every 15-minute firing.
      const changed = current !== "" && current !== previous;
      if (changed) {
        runCommand([
          "issue",
          "comment",
          String(existing.number),
          "--repo",
          repository,
          "--body",
          `Still stale as of ${nowIso}: generated_at \`${current}\` (${verdict.reason}).`,
        ]);
      }
      return { action: "updated", number: existing.number, commented: changed, closedDuplicates: closeDupes() };
    }
    const created = createSentinelIssue(repository, runCommand, { title: ISSUE_TITLE, body });
    return { action: "created", detail: created.stdout || created.stderr };
  }

  if (existing) {
    closeSentinelIssue(
      existing.number,
      repository,
      runCommand,
      `✅ Published benchmark data is fresh again as of ${nowIso} (${verdict.reason}). Closing.`,
    );
    return { action: "closed", number: existing.number, closedDuplicates: closeDupes() };
  }
  return { action: "noop" };
}

// ---- HTTP (node:https, redirect-following, works on every Node version) ------

function httpGet(url, { maxRedirects = 4, timeoutMs = 15000 } = {}) {
  return new Promise((resolve, reject) => {
    const request = https.get(url, { headers: { "User-Agent": "tsz-bench-latest-freshness" } }, (response) => {
      const status = response.statusCode || 0;
      if (status >= 300 && status < 400 && response.headers.location) {
        response.resume();
        if (maxRedirects <= 0) {
          reject(new Error(`too many redirects fetching ${url}`));
          return;
        }
        const next = new URL(response.headers.location, url).toString();
        httpGet(next, { maxRedirects: maxRedirects - 1, timeoutMs }).then(resolve, reject);
        return;
      }
      let raw = "";
      response.setEncoding("utf8");
      response.on("data", (chunk) => (raw += chunk));
      response.on("end", () => {
        if (status < 200 || status >= 300) {
          reject(new Error(`GET ${url} returned ${status}`));
          return;
        }
        resolve(raw);
      });
    });
    request.setTimeout(timeoutMs, () => request.destroy(new Error(`GET ${url} timed out after ${timeoutMs}ms`)));
    request.on("error", reject);
  });
}

function parseArgs(argv) {
  const options = {
    url: DEFAULT_LATEST_URL,
    maxAgeHours: DEFAULT_MAX_AGE_HOURS,
    fixture: null,
    fileIssue: false,
    strict: false,
    json: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const valueOf = (inline) => (inline !== undefined ? inline : argv[++i]);
    if (arg === "--url" || arg.startsWith("--url=")) {
      options.url = valueOf(arg.includes("=") ? arg.split("=").slice(1).join("=") : undefined);
    } else if (arg === "--max-age-hours" || arg.startsWith("--max-age-hours=")) {
      options.maxAgeHours = Number(valueOf(arg.includes("=") ? arg.split("=")[1] : undefined));
    } else if (arg === "--fixture" || arg.startsWith("--fixture=")) {
      options.fixture = valueOf(arg.includes("=") ? arg.split("=").slice(1).join("=") : undefined);
    } else if (arg === "--file-issue") {
      options.fileIssue = true;
    } else if (arg === "--strict") {
      options.strict = true;
    } else if (arg === "--json") {
      options.json = true;
    }
  }
  if (!Number.isFinite(options.maxAgeHours) || options.maxAgeHours <= 0) {
    options.maxAgeHours = DEFAULT_MAX_AGE_HOURS;
  }
  return options;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const nowIso = new Date().toISOString();

  let body;
  let fetchError = null;
  if (options.fixture) {
    body = fs.readFileSync(options.fixture, "utf8");
  } else {
    try {
      body = await httpGet(options.url);
    } catch (error) {
      fetchError = error;
    }
  }

  if (fetchError) {
    // A transport failure is "unknown", not proof of staleness: stay advisory by
    // default so a transient network blip never spams the tracking issue.
    const message = `::warning::bench-latest-freshness: could not fetch ${options.url}: ${fetchError.message}`;
    process.stderr.write(`${message}\n`);
    if (options.json) process.stdout.write(`${JSON.stringify({ status: "fetch-error", url: options.url, error: fetchError.message })}\n`);
    process.exit(options.strict ? 4 : 0);
  }

  const verdict = classifyFreshness(body, { maxAgeHours: options.maxAgeHours });
  const report = formatReport(verdict, { url: options.url });

  if (options.json) {
    process.stdout.write(`${JSON.stringify({ url: options.url, ...verdict })}\n`);
  } else {
    process.stdout.write(`${report}\n`);
  }

  if (verdict.alert) {
    process.stderr.write(
      `::warning::bench-latest-freshness: ${verdict.status} — ${verdict.reason} (${options.url})\n`,
    );
  }

  if (options.fileIssue) {
    const repository = process.env.REPOSITORY || process.env.GITHUB_REPOSITORY;
    if (!repository) {
      process.stderr.write("::warning::bench-latest-freshness: --file-issue requires REPOSITORY or GITHUB_REPOSITORY\n");
    } else {
      try {
        const outcome = reconcileIssue(verdict, { url: options.url, repository }, nowIso);
        process.stderr.write(`bench-latest-freshness issue: ${JSON.stringify(outcome)}\n`);
      } catch (error) {
        process.stderr.write(`::warning::bench-latest-freshness: issue reconcile failed: ${error.message}\n`);
      }
    }
  }

  process.exit(options.strict && verdict.alert ? 3 : 0);
}

const invokedDirectly =
  process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (invokedDirectly) {
  main().catch((error) => {
    process.stderr.write(`bench-latest-freshness failed: ${error.stack || error.message}\n`);
    process.exit(2);
  });
}
