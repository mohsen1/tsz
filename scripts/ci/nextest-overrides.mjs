#!/usr/bin/env node
// Pure helpers for reasoning about `.config/nextest.toml` slow-timeout overrides
// and nextest junit outcomes (#17675, part 3).
//
// The recurring failure mode this module underpins: a correct-but-heavy test
// (default-lib / global-merge rechecks are the canonical family) is added
// without joining a `slow-timeout` override, so under the base signoff budget
// it TIMES OUT. In junit a timeout is a `<failure type="slow-timeout">`, which
// the known-failures gate would otherwise report with the same "update the
// baseline" remedy as a genuine assertion failure — and baselining a correct
// test permanently masks it, exactly the "a red lane stops being read" hazard
// the issue is about. These helpers let the gate (a) route a timed-out test to
// the *override* remedy rather than the baseline, (b) warn on a still-passing
// test that already exceeds the base slow period before it ever times out, and
// (c) back a static fast-lane guard that fails when an override filter no
// longer matches any test in the tree.
//
// This is a deliberately small, line-oriented reader of the stable subset of
// `.config/nextest.toml` that we depend on (`[[profile.X.overrides]]` blocks
// with single-line `filter = '...'` and `slow-timeout = { ... }`), not a
// general TOML parser: it must run in the per-merge fast lane with no
// third-party dependency, and every shape it reads is pinned by the sibling
// tests against the real committed config.

// A `<testcase>` in a nextest junit report. Passing cases are self-closed;
// failing cases carry a `<failure>`/`<error>` child (a timeout is a
// `<failure type="slow-timeout">`).
const TESTCASE_RE = /<testcase\b([^>]*?)(?:\/>|>([\s\S]*?)<\/testcase>)/g;
const NAME_RE = /\bname="([^"]*)"/;
const CLASSNAME_RE = /\bclassname="([^"]*)"/;
const TIME_RE = /\btime="([^"]*)"/;
const FAILURE_TAG_RE = /<(failure|error)\b([^>]*?)(?:\/>|>([\s\S]*?)<\/(?:failure|error)>)/;
const TYPE_RE = /\btype="([^"]*)"/;
const MESSAGE_RE = /\bmessage="([^"]*)"/;

function attr(attrs, re) {
  const m = re.exec(attrs);
  return m ? m[1] : null;
}

// Parse a nextest junit document into per-testcase records:
//   { id, name, classname, timeSeconds, failure: null | { type, text } }
// `id` is `classname::name` (== nextest's `binary-id::test-name`), or the bare
// name when a case has no classname. `timeSeconds` is NaN when absent/unparsable.
export function parseJunitCases(xml) {
  const cases = [];
  TESTCASE_RE.lastIndex = 0;
  let m;
  while ((m = TESTCASE_RE.exec(xml)) !== null) {
    const attrs = m[1];
    const inner = m[2]; // undefined when the tag is self-closed
    const name = attr(attrs, NAME_RE);
    if (name === null) continue;
    const classname = attr(attrs, CLASSNAME_RE);
    const id = classname ? `${classname}::${name}` : name;
    const timeStr = attr(attrs, TIME_RE);
    const timeSeconds = timeStr === null ? NaN : Number(timeStr);
    let failure = null;
    if (inner !== undefined) {
      const fm = FAILURE_TAG_RE.exec(inner);
      if (fm) {
        const failAttrs = fm[2] || "";
        const body = fm[3] || "";
        failure = {
          type: attr(failAttrs, TYPE_RE) || "",
          text: (attr(failAttrs, MESSAGE_RE) || "") + " " + body,
        };
      }
    }
    cases.push({ id, name, classname, timeSeconds, failure });
  }
  return cases;
}

// Classify a failure (as returned in a case's `.failure`) as a nextest timeout
// vs. a genuine test failure. A timeout is the "test is correct but too slow
// for its budget" case. Prefer nextest's structured `type` token
// (`slow-timeout`, `test timeout`, …); fall back to the message text only when
// the type is absent or unrecognised, since its exact spelling is
// under-documented across nextest versions. This only routes the remedy
// message — the gate's pass/fail verdict never depends on it — so the text
// fallback erring toward "timeout" is harmless.
export function classifyFailure(failure) {
  if (!failure) return null;
  const type = failure.type.toLowerCase();
  if (type.includes("timeout") || type.includes("timed out")) return "timeout";
  if (type === "test failure" || type === "test error") return "failure";
  const text = failure.text.toLowerCase();
  if (text.includes("timeout") || text.includes("timed out") || text.includes("time out")) {
    return "timeout";
  }
  return "failure";
}

function durationToSeconds(raw) {
  const m = /^([0-9]+(?:\.[0-9]+)?)\s*(ms|s|m)$/.exec(raw.trim());
  if (!m) return null;
  const value = Number(m[1]);
  switch (m[2]) {
    case "ms":
      return value / 1000;
    case "m":
      return value * 60;
    default:
      return value;
  }
}

// Parse a single-line `slow-timeout = { period = "Ns", terminate-after = K }`
// inline table. Returns null when the line carries no slow-timeout. Shared by
// both readers below so the accepted shape lives in one place: `periodSeconds`
// is the warn threshold, `budgetSeconds` is period × terminate-after (nextest
// terminates a test after that many periods).
function parseSlowTimeout(line) {
  const st = /slow-timeout\s*=\s*\{([^}]*)\}/.exec(line);
  if (!st) return null;
  const pm = /period\s*=\s*"([^"]*)"/.exec(st[1]);
  const periodSeconds = pm ? durationToSeconds(pm[1]) : null;
  if (periodSeconds === null) return { periodSeconds: null, budgetSeconds: null };
  const tm = /terminate-after\s*=\s*([0-9]+)/.exec(st[1]);
  return { periodSeconds, budgetSeconds: periodSeconds * (tm ? Number(tm[1]) : 1) };
}

// Pull the `test(...)` predicates out of a nextest filterset expression string.
// Each predicate is either a substring literal (`test(foo)` — nextest's default
// substring match, `=` prefix for exact) or a regex (`test(/re/)`).
function parseFilterPredicates(filterRaw) {
  const literals = [];
  const regexes = [];
  const re = /\btest\(([^)]*)\)/g;
  let m;
  while ((m = re.exec(filterRaw)) !== null) {
    let inner = m[1].trim();
    if (inner.length >= 2 && inner.startsWith("/") && inner.endsWith("/")) {
      regexes.push(inner.slice(1, -1));
    } else {
      if (inner.startsWith("=")) inner = inner.slice(1);
      if (inner) literals.push(inner);
    }
  }
  return { literals, regexes };
}

// Parse `.config/nextest.toml` into the `[[profile.X.overrides]]` blocks we
// care about. Returns records:
//   { profile, filterRaw, literals, regexes, budgetSeconds }
// `budgetSeconds` is null for override blocks that set no `slow-timeout` (e.g. a
// `threads-required`-only block); those still count for the static
// orphaned-filter guard, which cares about every filter. `countOverrideHeaders`
// lets a caller assert every header yielded a record (a reformat that wraps a
// `filter = '...'` across lines would otherwise drop a block silently).
export function parseOverrides(tomlText) {
  const overrides = [];
  let cur = null;
  const flush = () => {
    if (cur && cur.filterRaw !== null) overrides.push(cur);
    cur = null;
  };
  for (const raw of tomlText.split(/\r?\n/)) {
    const line = raw.trim();
    if (line.startsWith("#")) continue;
    const hdr = OVERRIDE_HEADER_RE.exec(line);
    if (hdr) {
      flush();
      cur = { profile: hdr[1], filterRaw: null, literals: [], regexes: [], budgetSeconds: null };
      continue;
    }
    // Any other table header (a `[profile.X]` base block, `[[...]]`, `[...]`)
    // ends the current override block.
    if (line.startsWith("[")) {
      flush();
      continue;
    }
    if (!cur) continue;
    const fm = /^filter\s*=\s*(['"])([\s\S]*?)\1\s*$/.exec(line);
    if (fm) {
      cur.filterRaw = fm[2];
      const { literals, regexes } = parseFilterPredicates(fm[2]);
      cur.literals = literals;
      cur.regexes = regexes;
    }
    const slow = parseSlowTimeout(line);
    if (slow) cur.budgetSeconds = slow.budgetSeconds;
  }
  flush();
  return overrides;
}

const OVERRIDE_HEADER_RE = /^\[\[profile\.([A-Za-z0-9_-]+)\.overrides\]\]$/;

// Number of `[[profile.X.overrides]]` blocks in the config. Every override
// block has a filter, so `parseOverrides(...).length` must equal this; a
// mismatch means a block's `filter = '...'` did not parse (e.g. a formatter
// wrapped it across lines) and its protection silently vanished.
export function countOverrideHeaders(tomlText) {
  let n = 0;
  for (const raw of tomlText.split(/\r?\n/)) {
    if (OVERRIDE_HEADER_RE.test(raw.trim())) n++;
  }
  return n;
}

// The base `slow-timeout` period (seconds) for a top-level `[profile.X]` block,
// or null when the profile does not set one. Used as the threshold above which
// a still-passing, non-overridden test is "approaching its budget".
export function profileBaseSlowPeriodSeconds(tomlText, profile) {
  const header = `[profile.${profile}]`;
  const lines = tomlText.split(/\r?\n/);
  let inSection = false;
  for (const raw of lines) {
    const line = raw.trim();
    if (line === header) {
      inSection = true;
      continue;
    }
    if (inSection && line.startsWith("[")) break;
    if (inSection && !line.startsWith("#")) {
      const slow = parseSlowTimeout(line);
      if (slow && slow.periodSeconds !== null) return slow.periodSeconds;
    }
  }
  return null;
}

// True when `testName` (a nextest test-name, i.e. the part after `binary::`)
// matches this override's filter. Mirrors nextest's default substring match for
// bare `test(...)` and regex match for `test(/re/)`.
export function overrideMatchesTest(record, testName) {
  for (const sub of record.literals) {
    if (testName.includes(sub)) return true;
  }
  for (const rx of record.regexes) {
    try {
      if (new RegExp(rx).test(testName)) return true;
    } catch {
      // A filter regex nextest accepts but the JS engine rejects is treated as
      // non-matching here; the static guard reports unparsable regexes itself.
    }
  }
  return false;
}

// The overrides in effect for `activeProfile` that carry a slow-timeout budget.
// nextest applies a profile's own overrides plus the `default` profile's for
// settings the active profile does not itself override, so both are in scope.
export function budgetedOverridesForProfile(overrides, activeProfile) {
  return overrides.filter(
    (o) =>
      o.budgetSeconds !== null &&
      (o.profile === activeProfile || o.profile === "default"),
  );
}

// The slow-timeout override covering `testName` under `activeProfile`, or null.
export function findBudgetOverride(overrides, activeProfile, testName) {
  for (const o of budgetedOverridesForProfile(overrides, activeProfile)) {
    if (overrideMatchesTest(o, testName)) return o;
  }
  return null;
}

// Every distinct literal `test(NAME)` substring referenced by any override
// filter, with the source profile(s). Drives the static orphaned-filter guard.
export function collectLiteralFilters(overrides) {
  const byLiteral = new Map();
  for (const o of overrides) {
    for (const lit of o.literals) {
      if (!byLiteral.has(lit)) byLiteral.set(lit, new Set());
      byLiteral.get(lit).add(o.profile);
    }
  }
  return [...byLiteral.entries()]
    .map(([literal, profiles]) => ({ literal, profiles: [...profiles].sort() }))
    .sort((a, b) => (a.literal < b.literal ? -1 : a.literal > b.literal ? 1 : 0));
}
