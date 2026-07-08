#!/usr/bin/env node
// Shared dedup-sentinel issue lifecycle for the ci-health monitors
// (`scripts/ci/check-main-red.mjs`, `scripts/bench/check-latest-freshness.mjs`).
//
// Each monitor tracks a standing condition (red `main`, frozen benchmark
// publish) through ONE open tracking issue, deduplicated on a hidden HTML
// marker in the body. Two failure modes historically broke that invariant:
//
// 1. A lookup miss (single-page scan before #15467, or a human/agent editing
//    the body and dropping the marker despite the "do not edit" note) makes
//    the reconciler create a duplicate on the next firing.
// 2. Once two sentinel issues are open, a first-match lookup only ever sees
//    one of them: the other is never updated and never closed on recovery
//    (#15532 duplicating #15401 is the concrete witness).
//
// This module makes the sentinel lifecycle self-healing:
// - the lookup collects ALL open matches, not the first;
// - it matches on the marker OR the exact bot-owned title, so a
//   marker-stripped body edit cannot orphan the issue;
// - it skips pull requests (the `/issues` endpoint interleaves them, and a PR
//   body quoting the marker must never be edited/closed as the sentinel);
// - `splitSentinels` names the oldest match canonical; callers close the rest
//   as duplicates via `closeDuplicateSentinels`.

// Every monitor's sentinel is created with this label (via
// `createSentinelIssue`) so humans can triage it; the lookup deliberately does
// NOT rely on it — a re-labeled sentinel must still be found.
const SENTINEL_LABEL = "tech-debt";

export const SENTINEL_PER_PAGE = 100;
// Caps the walk on a pathological backlog (the `/issues` endpoint interleaves
// open PRs, so the combined listing routinely exceeds one page) without
// masking a reachable sentinel.
const SENTINEL_MAX_PAGES = 20;

/**
 * Collect every open sentinel issue for a monitor, oldest first.
 *
 * A sentinel matches when its body carries `marker` or its title equals the
 * bot-owned `title` exactly (the fallback that survives marker-stripping body
 * edits). Pull requests are ignored. Returns `[]` when nothing matches or the
 * issue listing is unavailable.
 *
 * @param {string} repository owner/repo
 * @param {(args: string[]) => any} fetchJson gh JSON fetcher seam
 * @param {{ marker: string, title: string }} keys
 */
export function collectSentinelIssues(repository, fetchJson, { marker, title }) {
  const matches = [];
  for (let page = 1; page <= SENTINEL_MAX_PAGES; page += 1) {
    const issues = fetchJson([
      "api",
      "-H",
      "Accept: application/vnd.github+json",
      `repos/${repository}/issues?state=open&per_page=${SENTINEL_PER_PAGE}&page=${page}`,
    ]);
    if (!Array.isArray(issues)) break;
    for (const item of issues) {
      if (!item || typeof item !== "object") continue;
      // The /issues listing marks PRs with a `pull_request` key.
      if (item.pull_request) continue;
      const bodyHit = typeof item.body === "string" && item.body.includes(marker);
      const titleHit =
        typeof title === "string" && title !== "" && item.title === title;
      if (bodyHit || titleHit) matches.push(item);
    }
    // A short (or empty) page is the last one.
    if (issues.length < SENTINEL_PER_PAGE) break;
  }
  matches.sort((a, b) => (a.number ?? 0) - (b.number ?? 0));
  return matches;
}

/**
 * Name the canonical sentinel among the open matches: the oldest (lowest
 * number) survives — it carries the original discussion — and every younger
 * match is a duplicate from a past lookup miss.
 *
 * @param {ReturnType<typeof collectSentinelIssues>} matches
 */
export function splitSentinels(matches) {
  return { canonical: matches[0] ?? null, duplicates: matches.slice(1) };
}

/**
 * Create a monitor's sentinel issue, labeled `tech-debt`.
 *
 * @param {string} repository owner/repo
 * @param {(args: string[]) => { status: number, stdout: string, stderr: string }} runCommand gh command seam
 * @param {{ title: string, body: string }} content
 */
export function createSentinelIssue(repository, runCommand, { title, body }) {
  return runCommand([
    "issue",
    "create",
    "--repo",
    repository,
    "--title",
    title,
    "--body",
    body,
    "--label",
    SENTINEL_LABEL,
  ]);
}

/**
 * Close the canonical sentinel on recovery: a monitor-worded comment, then a
 * close with reason `completed`.
 *
 * @param {number} number the canonical sentinel's issue number
 * @param {string} repository owner/repo
 * @param {(args: string[]) => any} runCommand gh command seam
 * @param {string} message recovery comment body
 */
export function closeSentinelIssue(number, repository, runCommand, message) {
  runCommand(["issue", "comment", String(number), "--repo", repository, "--body", message]);
  runCommand(["issue", "close", String(number), "--repo", repository, "--reason", "completed"]);
}

/**
 * Close redundant sentinel issues, pointing each at the canonical one.
 * Returns the closed issue numbers.
 *
 * @param {Array<{ number?: number }>} duplicates non-canonical matches
 * @param {number} canonicalNumber the issue that stays open (or just closed as completed)
 * @param {string} repository owner/repo
 * @param {(args: string[]) => any} runCommand gh command seam
 * @param {string} nowIso timestamp for the audit comment
 */
export function closeDuplicateSentinels(duplicates, canonicalNumber, repository, runCommand, nowIso) {
  const closed = [];
  for (const duplicate of duplicates) {
    if (duplicate?.number == null) continue;
    runCommand([
      "issue",
      "comment",
      String(duplicate.number),
      "--repo",
      repository,
      "--body",
      `Duplicate tracking issue — consolidated into #${canonicalNumber} as of ${nowIso}. Closing.`,
    ]);
    runCommand([
      "issue",
      "close",
      String(duplicate.number),
      "--repo",
      repository,
      "--reason",
      "not planned",
    ]);
    closed.push(duplicate.number);
  }
  return closed;
}
