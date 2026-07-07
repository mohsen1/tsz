#!/usr/bin/env node
import assert from "node:assert/strict";
import {
  collectSentinelIssues,
  splitSentinels,
  createSentinelIssue,
  closeSentinelIssue,
  closeDuplicateSentinels,
  SENTINEL_PER_PAGE,
} from "./lib/sentinel-issues.mjs";

const MARKER = "<!-- test-sentinel -->";
const TITLE = "🟡 something is wrong — tracked";
const REPO = "o/r";

function pagedFetch(pages, seenPages = []) {
  return (args) => {
    const url = String(args[args.length - 1]);
    const page = Number((url.match(/[?&]page=(\d+)/) || [])[1] || 1);
    seenPages.push(page);
    return pages[page - 1] ?? [];
  };
}

let passed = 0;
function test(name, fn) {
  fn();
  passed += 1;
  console.log(`ok - ${name}`);
}

test("matches on the body marker", () => {
  const matches = collectSentinelIssues(
    REPO,
    () => [{ number: 7, title: "edited by hand", body: `x ${MARKER} y` }],
    { marker: MARKER, title: TITLE },
  );
  assert.deepEqual(matches.map((m) => m.number), [7]);
});

test("falls back to the exact bot title when a body edit stripped the marker", () => {
  const matches = collectSentinelIssues(
    REPO,
    () => [
      { number: 8, title: TITLE, body: "claimed + rewritten body, marker gone" },
      { number: 9, title: `${TITLE} (extra)`, body: "near-title must not match" },
    ],
    { marker: MARKER, title: TITLE },
  );
  assert.deepEqual(matches.map((m) => m.number), [8], "only the exact title matches");
});

test("skips pull requests even when their body quotes the marker", () => {
  const matches = collectSentinelIssues(
    REPO,
    () => [
      { number: 10, title: "fix: sentinel lookup", body: MARKER, pull_request: { url: "x" } },
      { number: 11, title: TITLE, body: MARKER },
    ],
    { marker: MARKER, title: TITLE },
  );
  assert.deepEqual(matches.map((m) => m.number), [11]);
});

test("collects ALL matches across pages, oldest (lowest number) first", () => {
  const page1 = Array.from({ length: SENTINEL_PER_PAGE }, (_, i) =>
    i === 3
      ? { number: 900, title: TITLE, body: MARKER }
      : { number: 5000 - i, title: "noise", body: "unrelated" },
  );
  const page2 = [{ number: 42, title: "old sentinel", body: `${MARKER}\nstale` }];
  const seenPages = [];
  const matches = collectSentinelIssues(REPO, pagedFetch([page1, page2], seenPages), {
    marker: MARKER,
    title: TITLE,
  });
  assert.deepEqual(seenPages, [1, 2], "pages until a short page ends the walk");
  assert.deepEqual(matches.map((m) => m.number), [42, 900], "all matches, oldest first");
});

test("the lookup does not depend on labels — a re-labeled sentinel is still found", () => {
  const matches = collectSentinelIssues(
    REPO,
    () => [{ number: 6, title: "renamed", body: MARKER, labels: [] }],
    { marker: MARKER, title: TITLE },
  );
  assert.deepEqual(matches.map((m) => m.number), [6]);
});

test("createSentinelIssue owns the tech-debt label", () => {
  const calls = [];
  createSentinelIssue(REPO, (args) => {
    calls.push(args);
    return { status: 0, stdout: "https://gh/issues/1", stderr: "" };
  }, { title: TITLE, body: `${MARKER}\nbody` });
  assert.equal(calls.length, 1);
  assert.equal(calls[0][1], "create");
  const labelIndex = calls[0].indexOf("--label");
  assert.equal(calls[0][labelIndex + 1], "tech-debt", "sentinels are created labeled for triage");
});

test("splitSentinels names the oldest canonical and the rest duplicates", () => {
  assert.deepEqual(splitSentinels([]), { canonical: null, duplicates: [] });
  const a = { number: 10 };
  const b = { number: 20 };
  assert.deepEqual(splitSentinels([a, b]), { canonical: a, duplicates: [b] });
});

test("returns [] on an empty or non-array listing", () => {
  assert.deepEqual(collectSentinelIssues(REPO, () => [], { marker: MARKER, title: TITLE }), []);
  assert.deepEqual(
    collectSentinelIssues(REPO, () => ({ message: "rate limited" }), { marker: MARKER, title: TITLE }),
    [],
  );
});

test("tolerates malformed listing entries", () => {
  const matches = collectSentinelIssues(
    REPO,
    () => [null, "junk", { number: 3 }, { number: 4, title: TITLE }],
    { marker: MARKER, title: TITLE },
  );
  assert.deepEqual(matches.map((m) => m.number), [4]);
});

test("closeDuplicateSentinels comments with the canonical pointer and closes as not planned", () => {
  const calls = [];
  const closed = closeDuplicateSentinels(
    [{ number: 15532 }, { number: 15600 }, { number: null }],
    15401,
    REPO,
    (args) => {
      calls.push(args);
      return { status: 0, stdout: "", stderr: "" };
    },
    "2026-07-07T00:00:00Z",
  );
  assert.deepEqual(closed, [15532, 15600]);
  const comments = calls.filter((c) => c[1] === "comment");
  const closes = calls.filter((c) => c[1] === "close");
  assert.equal(comments.length, 2);
  assert.equal(closes.length, 2);
  assert.match(comments[0][comments[0].length - 1], /#15401/, "comment points at the canonical issue");
  assert.ok(closes.every((c) => c.includes("not planned")), "duplicates close as not planned");
});

test("closeSentinelIssue comments then closes as completed", () => {
  const calls = [];
  closeSentinelIssue(80, REPO, (args) => {
    calls.push(args);
    return { status: 0, stdout: "", stderr: "" };
  }, "✅ recovered. Closing.");
  assert.deepEqual(calls.map((c) => c[1]), ["comment", "close"]);
  assert.ok(calls[0].includes("✅ recovered. Closing."));
  assert.ok(calls[1].includes("completed"), "canonical recovery closes as completed");
});

test("closeDuplicateSentinels with no duplicates is a no-op", () => {
  const closed = closeDuplicateSentinels([], 1, REPO, () => {
    throw new Error("must not be called");
  }, "2026-07-07T00:00:00Z");
  assert.deepEqual(closed, []);
});

console.log(`\n${passed} sentinel-issues tests passed`);
