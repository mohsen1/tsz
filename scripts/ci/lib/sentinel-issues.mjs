#!/usr/bin/env node
// Shared dedup-sentinel issue lookup for the ci-health monitors
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
// This module makes the sentinel lookup self-healing:
// - it collects ALL open matches, not the first;
// - it matches on the marker OR the exact bot-owned title, so a
//   marker-stripped body edit cannot orphan the issue;
// - it skips pull requests (the `/issues` endpoint interleaves them, and a PR
//   body quoting the marker must never be edited/closed as the sentinel);
// - callers treat the oldest match as canonical and close the rest as
//   duplicates via `closeDuplicateSentinels`.

export const SENTINEL_PER_PAGE = 100;
// The `/issues` endpoint returns open issues *and* open PRs, so on an active
// repo the combined list routinely exceeds one 100-item page. A persistent
// tracking issue (a standing condition can live for hours/days) is sorted
// created-desc and steadily sinks past page 1 as new PRs/issues land. Page
// through until the list is exhausted; the bound caps the walk on a
// pathological backlog without masking a reachable sentinel.
export const SENTINEL_MAX_PAGES = 20;

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
 * @param {{ marker: string, title?: string }} keys
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
 * Close redundant sentinel issues, pointing each at the canonical one.
 *
 * Used whenever the lookup finds more than one open sentinel (a healed
 * duplicate from a past lookup miss). Returns the closed issue numbers.
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
