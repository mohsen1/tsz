#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import {
  classifyFreshness,
  formatReport,
  formatIssueBody,
  reconcileIssue,
  DEFAULT_MAX_AGE_HOURS,
  DEFAULT_LATEST_URL,
} from "./check-latest-freshness.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const SCRIPT = path.join(SCRIPT_DIR, "check-latest-freshness.mjs");
const NOW = Date.parse("2026-06-28T18:00:00.000Z");

function at(hoursAgo) {
  return new Date(NOW - hoursAgo * 3_600_000).toISOString();
}

// ---- fresh -> ok ------------------------------------------------------------
{
  const verdict = classifyFreshness(JSON.stringify({ generated_at: at(2) }), {
    now: NOW,
    maxAgeHours: 6,
  });
  assert.equal(verdict.status, "ok", "2h-old data under a 6h threshold is fresh");
  assert.equal(verdict.alert, false, "fresh data must not alert");
  assert.ok(Math.abs(verdict.ageHours - 2) < 1e-6, "age is computed in hours");
}

// Exactly at the threshold is still fresh (boundary is inclusive of <=).
{
  const verdict = classifyFreshness(JSON.stringify({ generated_at: at(6) }), {
    now: NOW,
    maxAgeHours: 6,
  });
  assert.equal(verdict.status, "ok", "age == threshold is not yet stale");
  assert.equal(verdict.alert, false);
}

// A future generated_at (clock skew) is not stale.
{
  const verdict = classifyFreshness(JSON.stringify({ generated_at: at(-1) }), {
    now: NOW,
    maxAgeHours: 6,
  });
  assert.equal(verdict.status, "ok", "future generated_at is not stale");
  assert.equal(verdict.alert, false);
}

// Default threshold is applied when none is supplied.
{
  const verdict = classifyFreshness(JSON.stringify({ generated_at: at(1) }), { now: NOW });
  assert.equal(verdict.maxAgeHours, DEFAULT_MAX_AGE_HOURS, "default threshold applied");
  assert.equal(verdict.status, "ok");
}

// ---- stale -> alert ---------------------------------------------------------
{
  const verdict = classifyFreshness(JSON.stringify({ generated_at: at(9) }), {
    now: NOW,
    maxAgeHours: 6,
  });
  assert.equal(verdict.status, "stale", "9h-old data over a 6h threshold is stale");
  assert.equal(verdict.alert, true, "stale data must alert");
  assert.match(verdict.reason, /9\.0h old \(threshold 6h\)/);
}

// The exact 46-commit-freeze witness: 2026-06-27T19:03 frozen, ~23h later.
{
  const frozen = "2026-06-27T19:03:16.801Z";
  const verdict = classifyFreshness(JSON.stringify({ generated_at: frozen }), {
    now: Date.parse("2026-06-28T18:00:00.000Z"),
    maxAgeHours: 6,
  });
  assert.equal(verdict.status, "stale", "the real freeze witness is flagged stale");
  assert.equal(verdict.alert, true);
}

// ---- unparseable -> alert ---------------------------------------------------
{
  const verdict = classifyFreshness("<html>not json</html>", { now: NOW, maxAgeHours: 6 });
  assert.equal(verdict.status, "unparseable", "non-JSON body is unparseable");
  assert.equal(verdict.alert, true, "unparseable payload must alert");
}
{
  const verdict = classifyFreshness(JSON.stringify({ results: [] }), { now: NOW });
  assert.equal(verdict.status, "unparseable", "missing generated_at is unparseable");
  assert.equal(verdict.alert, true);
}
{
  const verdict = classifyFreshness(JSON.stringify({ generated_at: "not-a-date" }), { now: NOW });
  assert.equal(verdict.status, "unparseable", "unparseable generated_at date is flagged");
  assert.equal(verdict.alert, true);
}
{
  const verdict = classifyFreshness("", { now: NOW });
  assert.equal(verdict.status, "unparseable", "empty body is unparseable");
  assert.equal(verdict.alert, true);
}

// ---- report + issue body include the marker / key facts ---------------------
{
  const verdict = classifyFreshness(JSON.stringify({ generated_at: at(9) }), {
    now: NOW,
    maxAgeHours: 6,
  });
  const report = formatReport(verdict, { url: DEFAULT_LATEST_URL });
  assert.match(report, /Status: \*\*stale\*\*/);
  const body = formatIssueBody(verdict, { url: DEFAULT_LATEST_URL, repository: "tsz-org/tsz" }, NOW_ISO());
  assert.match(body, /bench-latest-freshness-sentinel/, "issue body carries the dedup marker");
  assert.match(body, /bench-republish\.yml/, "issue body points at the quick-fix lever");
  assert.match(body, /generated_at: `2026-06-2/, "issue body records the tracked generated_at");
}

function NOW_ISO() {
  return new Date(NOW).toISOString();
}

// ---- reconcileIssue: open / heartbeat / close, deduped on the marker --------
{
  // No existing issue + alert -> create.
  const calls = [];
  const verdict = classifyFreshness(JSON.stringify({ generated_at: at(9) }), { now: NOW, maxAgeHours: 6 });
  const outcome = reconcileIssue(
    verdict,
    { url: DEFAULT_LATEST_URL, repository: "tsz-org/tsz" },
    NOW_ISO(),
    {
      fetchJson: () => [],
      runCommand: (args) => {
        calls.push(args);
        return { status: 0, stdout: "https://github.com/tsz-org/tsz/issues/1", stderr: "" };
      },
    },
  );
  assert.equal(outcome.action, "created");
  assert.ok(calls.some((c) => c[0] === "issue" && c[1] === "create"), "creates a tracking issue");
}
{
  // Existing issue with same generated_at + still alert -> edit, NO heartbeat.
  const calls = [];
  const verdict = classifyFreshness(JSON.stringify({ generated_at: "2026-06-27T19:03:16.801Z" }), {
    now: NOW,
    maxAgeHours: 6,
  });
  const existingBody = formatIssueBody(verdict, { url: DEFAULT_LATEST_URL, repository: "tsz-org/tsz" }, NOW_ISO());
  const outcome = reconcileIssue(
    verdict,
    { url: DEFAULT_LATEST_URL, repository: "tsz-org/tsz" },
    NOW_ISO(),
    {
      fetchJson: () => [{ number: 7, body: existingBody }],
      runCommand: (args) => {
        calls.push(args);
        return { status: 0, stdout: "", stderr: "" };
      },
    },
  );
  assert.equal(outcome.action, "updated");
  assert.equal(outcome.commented, false, "no heartbeat comment when generated_at is unchanged");
  assert.ok(calls.some((c) => c[1] === "edit"), "edits the existing issue body");
  assert.ok(!calls.some((c) => c[1] === "comment"), "does not spam a comment on every firing");
}
{
  // Existing issue + now fresh -> comment + close.
  const calls = [];
  const verdict = classifyFreshness(JSON.stringify({ generated_at: at(1) }), { now: NOW, maxAgeHours: 6 });
  const outcome = reconcileIssue(
    verdict,
    { url: DEFAULT_LATEST_URL, repository: "tsz-org/tsz" },
    NOW_ISO(),
    {
      fetchJson: () => [{ number: 9, body: `${"<!-- bench-latest-freshness-sentinel -->"}\nstale` }],
      runCommand: (args) => {
        calls.push(args);
        return { status: 0, stdout: "", stderr: "" };
      },
    },
  );
  assert.equal(outcome.action, "closed");
  assert.ok(calls.some((c) => c[1] === "close"), "closes the tracking issue on recovery");
}
{
  // Sentinel past page 1: the `/issues` list mixes open PRs, so a persistent
  // tracking issue sinks onto a later page as new PRs/issues land. reconcileIssue
  // must page through and find it instead of re-creating a duplicate every firing.
  const MARKER = "<!-- bench-latest-freshness-sentinel -->";
  const page1 = Array.from({ length: 100 }, (_, i) => ({ number: 2000 + i, body: "unrelated PR body" }));
  const page2 = [{ number: 42, body: `${MARKER}\nstale` }];
  const seenPages = [];
  const calls = [];
  const verdict = classifyFreshness(JSON.stringify({ generated_at: at(9) }), { now: NOW, maxAgeHours: 6 });
  const outcome = reconcileIssue(
    verdict,
    { url: DEFAULT_LATEST_URL, repository: "tsz-org/tsz" },
    NOW_ISO(),
    {
      fetchJson: (args) => {
        const url = String(args[args.length - 1]);
        const page = Number((url.match(/[?&]page=(\d+)/) || [])[1] || 1);
        seenPages.push(page);
        return page === 1 ? page1 : page === 2 ? page2 : [];
      },
      runCommand: (args) => {
        calls.push(args);
        return { status: 0, stdout: "", stderr: "" };
      },
    },
  );
  assert.equal(outcome.action, "updated", "finds the sentinel on page 2 and edits it");
  assert.equal(outcome.number, 42, "locates the correct tracking issue past page 1");
  assert.deepEqual(seenPages, [1, 2], "pages through until the marker is found");
  assert.ok(!calls.some((c) => c[1] === "create"), "never creates a duplicate tracking issue");
}
{
  // Two open sentinels (a healed duplicate from a past lookup miss) + still
  // stale -> the oldest is canonical and updated; the newer one is closed as a
  // duplicate pointing at it. Never a third issue.
  const MARKER = "<!-- bench-latest-freshness-sentinel -->";
  const calls = [];
  const verdict = classifyFreshness(JSON.stringify({ generated_at: "2026-06-27T19:03:16.801Z" }), {
    now: NOW,
    maxAgeHours: 6,
  });
  const existingBody = formatIssueBody(verdict, { url: DEFAULT_LATEST_URL, repository: "tsz-org/tsz" }, NOW_ISO());
  const outcome = reconcileIssue(
    verdict,
    { url: DEFAULT_LATEST_URL, repository: "tsz-org/tsz" },
    NOW_ISO(),
    {
      fetchJson: () => [
        { number: 15532, body: `${MARKER}\nnewer duplicate` },
        { number: 15401, body: existingBody },
      ],
      runCommand: (args) => {
        calls.push(args);
        return { status: 0, stdout: "", stderr: "" };
      },
    },
  );
  assert.equal(outcome.action, "updated");
  assert.equal(outcome.number, 15401, "the oldest sentinel is canonical");
  assert.deepEqual(outcome.closedDuplicates, [15532], "the younger duplicate is closed");
  assert.ok(!calls.some((c) => c[1] === "create"), "never creates a third tracking issue");
  const dupComment = calls.find((c) => c[1] === "comment" && c[2] === "15532");
  assert.match(dupComment[dupComment.length - 1], /#15401/, "duplicate close points at the canonical issue");
}
{
  // A body edit that stripped the marker must not orphan the sentinel: the
  // exact bot title still matches, the issue is updated (re-stamping the
  // marker), and no duplicate is created.
  const calls = [];
  const verdict = classifyFreshness(JSON.stringify({ generated_at: "2026-06-27T19:03:16.801Z" }), {
    now: NOW,
    maxAgeHours: 6,
  });
  const outcome = reconcileIssue(
    verdict,
    { url: DEFAULT_LATEST_URL, repository: "tsz-org/tsz" },
    NOW_ISO(),
    {
      fetchJson: () => [
        {
          number: 61,
          title: "🟡 benchmark site data is stale — latest.json generated_at not advancing",
          body: "claimed; body rewritten without the marker",
        },
      ],
      runCommand: (args) => {
        calls.push(args);
        return { status: 0, stdout: "", stderr: "" };
      },
    },
  );
  assert.equal(outcome.action, "updated", "title fallback finds the marker-stripped sentinel");
  assert.equal(outcome.number, 61);
  assert.ok(!calls.some((c) => c[1] === "create"), "no duplicate for a marker-stripped body");
  const edit = calls.find((c) => c[1] === "edit");
  assert.match(edit[edit.length - 1], /bench-latest-freshness-sentinel/, "edit restores the marker");
}
{
  // Recovery with a lingering duplicate: BOTH sentinels close, not just the
  // first match.
  const MARKER = "<!-- bench-latest-freshness-sentinel -->";
  const calls = [];
  const verdict = classifyFreshness(JSON.stringify({ generated_at: at(1) }), { now: NOW, maxAgeHours: 6 });
  const outcome = reconcileIssue(
    verdict,
    { url: DEFAULT_LATEST_URL, repository: "tsz-org/tsz" },
    NOW_ISO(),
    {
      fetchJson: () => [
        { number: 90, body: `${MARKER}\nnewer duplicate` },
        { number: 80, body: `${MARKER}\nstale` },
      ],
      runCommand: (args) => {
        calls.push(args);
        return { status: 0, stdout: "", stderr: "" };
      },
    },
  );
  assert.equal(outcome.action, "closed");
  assert.equal(outcome.number, 80, "the canonical (oldest) sentinel closes as completed");
  assert.deepEqual(outcome.closedDuplicates, [90], "the duplicate closes too");
  const closed = calls.filter((c) => c[1] === "close").map((c) => c[2]);
  assert.deepEqual(closed.sort(), ["80", "90"], "no zombie sentinel survives recovery");
}

// ---- end-to-end CLI via --fixture (offline, no network) ---------------------
function runCli(args, fixtureValue) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-latest-freshness-"));
  try {
    const fixture = path.join(dir, "latest.json");
    fs.writeFileSync(fixture, fixtureValue, "utf8");
    return spawnSync(process.execPath, [SCRIPT, "--fixture", fixture, ...args], { encoding: "utf8" });
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

{
  // Fresh fixture: advisory exit 0, JSON status ok.
  const result = runCli(["--json", "--max-age-hours", "100000"], JSON.stringify({ generated_at: at(1) }));
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /"status":"ok"/);
}
{
  // Stale fixture in advisory mode: still exit 0, but warns.
  const result = runCli(["--json", "--max-age-hours", "0.001"], JSON.stringify({ generated_at: at(9) }));
  assert.equal(result.status, 0, "advisory mode never fails the job");
  assert.match(result.stdout, /"status":"stale"/);
  assert.match(result.stderr, /::warning::bench-latest-freshness/);
}
{
  // Stale fixture with --strict: exit 3.
  const result = runCli(["--strict", "--max-age-hours", "0.001"], JSON.stringify({ generated_at: at(9) }));
  assert.equal(result.status, 3, "strict mode exits non-zero on staleness");
}
{
  // Unparseable fixture with --strict: exit 3.
  const result = runCli(["--strict"], "not json at all");
  assert.equal(result.status, 3, "strict mode exits non-zero on unparseable payload");
}

console.log("check-latest-freshness tests passed");
