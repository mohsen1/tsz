#!/usr/bin/env node
// Known-failures baseline comparator (#15399).
//
// While main carries a known-failing test set and hosted CI is reduced, the
// local/PR gate needs to distinguish a NEW failure (a regression this change
// introduced) from the pre-existing known set. `cargo nextest --profile signoff`
// runs the whole suite with fail-fast disabled and writes junit; this script
// diffs that junit against the committed baseline in scripts/ci/known-failures.txt:
//
//   - any FAILING test not in the baseline  -> exit 1 (a new regression, named)
//   - any BASELINED test that now PASSES     -> warn + list (shrink ratchet)
//   - everything else                        -> exit 0
//
// `--update` rewrites the baseline to exactly the current failing set. In normal
// PRs the baseline may only shrink (a CI guard greps the diff for additions); a
// growth requires an explicit, reviewed baseline change.
//
// Bootstrap: the committed baseline ships with the header only and no reconciled
// marker, so the script runs in advisory mode (reports the current failing set,
// exits 0) until the first `--update` on a full-suite run stamps the marker and
// flips it to the strict behavior above. Reconciliation is keyed on the marker,
// not on the line count, so an empty-but-reconciled baseline (a fully green tree)
// still blocks any failure.
//
// Node-less fallback: on a machine without node, run
//   cargo nextest run --profile signoff --workspace
// and compare the reported failures to scripts/ci/known-failures.txt by hand, or
// pin a nextest `default-filter` that excludes the known set.
//
// The unit CI lane runs several nextest passes (a general workspace pass plus
// batched checker integration targets, #15646), each writing junit to a
// distinct path. `--junit` may be repeated and `--junit-dir` reads every
// `*.xml` in a directory; the run under adjudication is the UNION of all
// provided reports. Every provided report must be readable and non-empty —
// a pass that died before recording tests must fail the gate, not shrink it.
//
// Usage:
//   node scripts/ci/known-failures-check.mjs [--junit <path>]... [--junit-dir <dir>] \
//     [--baseline <path>] [--allow-no-reports] [--update [--bump-generation]]
//
// Exit codes:
//   0 - no new failures (or advisory bootstrap / shrink-only)
//   1 - one or more failing tests are absent from the baseline (regression)
//   2 - junit or baseline unreadable/invalid (configuration error, surfaced loudly)
import fs from "node:fs";
import path from "node:path";
import {
  parseJunitCases,
  classifyFailure,
  parseOverrides,
  findBudgetOverride,
  profileBaseSlowPeriodSeconds,
} from "./nextest-overrides.mjs";

const DEFAULT_JUNIT = "target/nextest/signoff/junit.xml";
const DEFAULT_BASELINE = "scripts/ci/known-failures.txt";

// The gate adjudicates junit produced by the `signoff` nextest profile, so its
// budgets/overrides are the ones in scope for the slow-test enrichment below.
const GATE_PROFILE = "signoff";
const NEXTEST_CONFIG = ".config/nextest.toml";

// Written by `--update`; its presence (not the line count) marks the baseline as
// reconciled against a real full run. This keeps "reconciled but green" (empty
// list + marker -> strict, blocks any failure) distinct from "never reconciled"
// (no marker -> advisory), so the gate does not silently disable itself on a
// clean tree.
//
// The marker carries a reconcile GENERATION (`reconciled r<N>`; the bare form
// reads as r1). check-known-failures-growth.py rejects baseline additions
// unless the generation was bumped in the same diff, so a deliberate
// re-reconcile is authorized by the reviewed artifact itself: regenerate with
// `--update --bump-generation` (or edit the marker line by hand) and the
// growth is legal exactly once.
const RECONCILED_MARKER = "# baseline-status: reconciled";
const RECONCILED_MARKER_RE = /^# baseline-status: reconciled(?: r(\d+))?$/;

const BASELINE_HEADER = [
  "# Known-failures baseline for scripts/ci/known-failures-check.mjs (#15399).",
  "# One `binary-id::test-name` per line; blank lines and `#` comments ignored.",
  "#",
  "# Regenerate from a full signoff run (fail-fast disabled, junit on):",
  "#   cargo nextest run --profile signoff --workspace || true",
  "#   node scripts/ci/known-failures-check.mjs --update",
  "#",
  "# Until the first `--update` there is no reconciled marker and the checker",
  "# runs in advisory mode (never blocks). `--update` stamps the marker and",
  "# switches the checker to strict: any failing test not listed here is a new",
  "# regression, even when the list is empty (a fully green tree). In normal PRs",
  "# this list may only shrink; a deliberate re-reconcile must bump the marker's",
  "# `r<N>` generation in the same diff (`--update --bump-generation`), which is",
  "# what scripts/ci/check-known-failures-growth.py accepts as authorization.",
];

// Project a parsed junit case list (from `parseJunitCases`) into the id sets
// this gate diffs against the baseline: `all` = every test that ran, `failing`
// = those with a `<failure>`/`<error>` child. Timeouts surface as `<failure>`,
// so they count as failing here (routed to a distinct remedy only in main()).
export function runFromCases(cases) {
  const all = new Set();
  const failing = new Set();
  for (const c of cases) {
    all.add(c.id);
    if (c.failure) failing.add(c.id);
  }
  return { all, failing };
}

// Parse a nextest junit document into `{ all, failing }` id sets. Kept as a
// thin projection over the shared `parseJunitCases` so there is one junit
// parser (the canonical id is `classname::name` == `binary-id::test-name`).
export function parseJunitFailures(xml) {
  return runFromCases(parseJunitCases(xml));
}

// Parse the baseline text into a set of known-failing ids.
export function parseBaseline(text) {
  const set = new Set();
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    set.add(line);
  }
  return set;
}

// Reconcile generation of a baseline text: 0 when unreconciled (advisory),
// 1 for the bare legacy marker, N for `reconciled rN`.
export function baselineGeneration(text) {
  for (const raw of text.split(/\r?\n/)) {
    const m = RECONCILED_MARKER_RE.exec(raw.trim());
    if (m) return m[1] ? Number(m[1]) : 1;
  }
  return 0;
}

// A baseline is reconciled once `--update` has stamped the marker; only then
// does the checker enforce strictly. Unreconciled baselines are advisory.
export function baselineIsReconciled(text) {
  return baselineGeneration(text) > 0;
}

// Union several parsed junit results ({all, failing} pairs) into one run.
// The unit lane produces one junit per nextest pass (#15646); the run under
// adjudication is their union. A test can only appear in multiple reports if
// two passes ran it; a failure in any pass keeps it failing.
export function unionRuns(runs) {
  const all = new Set();
  const failing = new Set();
  for (const run of runs) {
    for (const id of run.all) all.add(id);
    for (const id of run.failing) failing.add(id);
  }
  return { all, failing };
}

// Diff a junit result against the baseline set.
//   newFailures - failing tests absent from the baseline (regressions -> block)
//   nowPassing  - baselined tests that ran and did not fail (shrink candidates)
export function evaluate(baseline, junit) {
  const newFailures = [...junit.failing].filter((id) => !baseline.has(id)).sort();
  const nowPassing = [...baseline]
    .filter((id) => junit.all.has(id) && !junit.failing.has(id))
    .sort();
  return { newFailures, nowPassing };
}

// Merge per-pass junit case lists into one id -> case map (#15646: the unit
// lane records one junit per pass). A failing case wins over a passing one for
// the same id, and the larger recorded time is kept — the conservative reading
// used elsewhere in this gate.
export function mergeCases(caseLists) {
  const byId = new Map();
  for (const cases of caseLists) {
    for (const c of cases) {
      const prev = byId.get(c.id);
      if (!prev) {
        byId.set(c.id, c);
        continue;
      }
      const failure = prev.failure || c.failure;
      const timeSeconds = Math.max(
        Number.isFinite(prev.timeSeconds) ? prev.timeSeconds : 0,
        Number.isFinite(c.timeSeconds) ? c.timeSeconds : 0,
      );
      byId.set(c.id, { ...prev, failure, timeSeconds });
    }
  }
  return byId;
}

// Split a list of new-failure ids into timeouts (correct-but-too-slow: the fix
// is a slow-timeout override, never the baseline) and genuine failures, using
// the merged case map's failure classification. Ids without a case record (or
// without a failure child) fall to `genuine` so nothing is silently dropped.
export function splitTimeouts(newFailures, casesById) {
  const timeouts = [];
  const genuine = [];
  for (const id of newFailures) {
    const c = casesById.get(id);
    if (c && classifyFailure(c.failure) === "timeout") timeouts.push(id);
    else genuine.push(id);
  }
  return { timeouts, genuine };
}

// Passing tests whose recorded time already exceeds the profile's base slow
// period yet are not covered by any slow-timeout override — the latent heavy
// tests that will flip the lane red once they drift over the terminate budget.
// Reported as warnings so authors can join an override before that happens.
export function slowUncoveredPassing(casesById, overrides, profile, baseSeconds) {
  if (!Number.isFinite(baseSeconds)) return [];
  const out = [];
  for (const c of casesById.values()) {
    if (c.failure) continue;
    if (!Number.isFinite(c.timeSeconds) || c.timeSeconds <= baseSeconds) continue;
    if (findBudgetOverride(overrides, profile, c.name)) continue;
    out.push({ id: c.id, timeSeconds: c.timeSeconds });
  }
  return out.sort((a, b) => b.timeSeconds - a.timeSeconds);
}

// Render a baseline file body from a set/iterable of failing ids. `--update`
// produces a reconciled file (marker stamped with its generation); the
// committed bootstrap file is rendered unreconciled.
export function renderBaseline(ids, { reconciled = true, generation = 1 } = {}) {
  const sorted = [...new Set(ids)].sort();
  const header = reconciled
    ? [...BASELINE_HEADER, `${RECONCILED_MARKER} r${generation}`]
    : BASELINE_HEADER;
  return [...header, ...sorted].join("\n") + "\n";
}

// Load nextest override/budget context for the slow-test enrichment. Best
// effort: a missing or unreadable config disables the enrichment (empty
// overrides, null base period) but never fails the gate — the pass/fail verdict
// is owned entirely by the baseline diff.
function loadNextestBudgets() {
  try {
    const toml = fs.readFileSync(NEXTEST_CONFIG, "utf8");
    return {
      overrides: parseOverrides(toml),
      baseSlowSeconds: profileBaseSlowPeriodSeconds(toml, GATE_PROFILE),
    };
  } catch {
    return { overrides: [], baseSlowSeconds: null };
  }
}

function parseArgs(argv) {
  const out = {
    junits: [],
    junitDirs: [],
    baseline: DEFAULT_BASELINE,
    update: false,
    bumpGeneration: false,
    allowNoReports: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--junit") out.junits.push(argv[++i]);
    else if (a === "--junit-dir") out.junitDirs.push(argv[++i]);
    else if (a === "--baseline") out.baseline = argv[++i];
    else if (a === "--update") out.update = true;
    else if (a === "--bump-generation") out.bumpGeneration = true;
    else if (a === "--allow-no-reports") out.allowNoReports = true;
    else if (a === "--help" || a === "-h") out.help = true;
    else {
      out.error = `unknown argument '${a}'`;
      return out;
    }
  }
  return out;
}

// Expand --junit/--junit-dir into the concrete report list. With neither flag,
// fall back to the single default path (the local signoff flow before #15646).
// An empty/missing --junit-dir is a configuration error unless the caller
// passed --allow-no-reports (the unit lane does: a narrowed package override
// can legitimately select zero tests, and this checker owns that verdict).
function resolveJunitPaths(args) {
  const paths = [...args.junits];
  for (const dir of args.junitDirs) {
    let entries;
    try {
      entries = fs.readdirSync(dir);
    } catch (err) {
      if (args.allowNoReports) continue;
      return { error: `cannot read junit dir ${dir}: ${err.message}` };
    }
    const xml = entries.filter((name) => name.endsWith(".xml")).sort();
    if (xml.length === 0 && !args.allowNoReports) {
      return { error: `junit dir ${dir} contains no *.xml reports` };
    }
    for (const name of xml) paths.push(path.join(dir, name));
  }
  // Reachable only with --allow-no-reports: without it, an empty/unreadable
  // --junit-dir already returned an error above.
  if (paths.length === 0 && args.junitDirs.length > 0) {
    return { paths: [], noReports: true };
  }
  if (paths.length === 0) paths.push(DEFAULT_JUNIT);
  return { paths };
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    process.stdout.write(
      "usage: node scripts/ci/known-failures-check.mjs [--junit <path>]... " +
        "[--junit-dir <dir>] [--baseline <path>] [--allow-no-reports] " +
        "[--update [--bump-generation]]\n",
    );
    process.exit(0);
  }
  if (args.error) {
    process.stderr.write(`known-failures-check: ${args.error}\n`);
    process.exit(2);
  }

  const resolved = resolveJunitPaths(args);
  if (resolved.error) {
    process.stderr.write(`known-failures-check: ${resolved.error}\n`);
    process.exit(2);
  }
  if (resolved.noReports) {
    process.stdout.write(
      "known-failures-check: no junit reports (no tests in selection); nothing to adjudicate.\n",
    );
    process.exit(0);
  }

  const runs = [];
  const caseLists = [];
  for (const junitPath of resolved.paths) {
    let junitXml;
    try {
      junitXml = fs.readFileSync(junitPath, "utf8");
    } catch (err) {
      process.stderr.write(
        `known-failures-check: cannot read junit ${junitPath}: ${err.message}\n` +
          `Run the suite first: cargo nextest run --profile signoff --workspace || true\n`,
      );
      process.exit(2);
    }
    const cases = parseJunitCases(junitXml);
    caseLists.push(cases);
    const run = runFromCases(cases);

    // A junit with zero testcases means the pass produced no results (build
    // failed, or the runner was killed before any test recorded). Treat it as
    // an infra error rather than "nothing failed" so a truncated pass can
    // neither pass the gate nor be written as an empty baseline. (Partial runs
    // that record most but not all tests are not caught here; the slice-2 full
    // CI tier owns that.)
    if (run.all.size === 0) {
      process.stderr.write(
        `known-failures-check: junit ${junitPath} recorded no testcases; ` +
          `treating as a failed/incomplete run.\n`,
      );
      process.exit(2);
    }
    runs.push(run);
  }
  const junit = unionRuns(runs);
  const casesById = mergeCases(caseLists);
  const { overrides, baseSlowSeconds } = loadNextestBudgets();

  if (args.update) {
    // Preserve the reconcile generation across rewrites; growth authorization
    // is an explicit `--bump-generation` (see the marker note above). The
    // first reconcile of an unreconciled baseline starts at r1.
    let existingGeneration = 0;
    try {
      existingGeneration = baselineGeneration(fs.readFileSync(args.baseline, "utf8"));
    } catch {
      // No baseline yet: bootstrap.
    }
    const generation = Math.max(existingGeneration, 1) + (args.bumpGeneration ? 1 : 0);
    fs.mkdirSync(path.dirname(args.baseline), { recursive: true });
    fs.writeFileSync(args.baseline, renderBaseline(junit.failing, { generation }));
    process.stdout.write(
      `known-failures-check: wrote ${junit.failing.size} known failure(s) to ` +
        `${args.baseline} (reconcile generation r${generation}).\n`,
    );
    process.exit(0);
  }

  let baselineText;
  try {
    baselineText = fs.readFileSync(args.baseline, "utf8");
  } catch (err) {
    process.stderr.write(
      `known-failures-check: cannot read baseline ${args.baseline}: ${err.message}\n`,
    );
    process.exit(2);
  }
  const baseline = parseBaseline(baselineText);

  // Unreconciled baseline (no `--update` yet): advisory only, never blocks.
  if (!baselineIsReconciled(baselineText)) {
    const failing = [...junit.failing].sort();
    process.stdout.write(
      `known-failures-check: baseline ${args.baseline} is unreconciled. ` +
        `Advisory only; ${failing.length} test(s) currently failing.\n`,
    );
    for (const id of failing) process.stdout.write(`  would-baseline: ${id}\n`);
    process.stdout.write(
      `Populate with a full run once: cargo nextest run --profile signoff --workspace || true; ` +
        `node scripts/ci/known-failures-check.mjs --update\n`,
    );
    process.exit(0);
  }

  const { newFailures, nowPassing } = evaluate(baseline, junit);
  process.stdout.write(
    `known-failures-check: ${junit.all.size} test(s) ran across ${runs.length} report(s), ` +
      `${junit.failing.size} failing, baseline has ${baseline.size} known failure(s).\n`,
  );
  for (const id of nowPassing) {
    process.stdout.write(`  shrink: ${id} is baselined but now passes -> drop it with --update.\n`);
  }

  // Advisory: still-passing tests that already exceed the base slow period and
  // are not covered by a slow-timeout override — join an override before they
  // drift over the terminate budget and flip the lane red (#17675, part 3).
  for (const slow of slowUncoveredPassing(casesById, overrides, GATE_PROFILE, baseSlowSeconds)) {
    process.stdout.write(
      `::warning title=Slow test approaching its budget::${slow.id} passed but took ` +
        `${slow.timeSeconds.toFixed(1)}s, over the ${baseSlowSeconds}s base slow period, ` +
        `and joins no slow-timeout override in ${NEXTEST_CONFIG}. If it is correct-but-heavy, ` +
        `add it to an override now so a small future slowdown does not time it out.\n`,
    );
  }

  if (newFailures.length > 0) {
    const { timeouts, genuine } = splitTimeouts(newFailures, casesById);
    for (const id of newFailures) process.stderr.write(`  NEW FAILURE: ${id}\n`);
    if (timeouts.length > 0) {
      // A timeout is a correct-but-too-slow test hitting its budget. Baselining
      // it would permanently mask a correct test, so route it to the override
      // remedy explicitly rather than the generic "update the baseline" path.
      process.stderr.write(
        `::error title=Test timed out (add a slow-timeout override, do NOT baseline)::` +
          `${timeouts.length} new failure(s) are nextest TIMEOUTS: ${timeouts.join(", ")}. ` +
          `A timeout means the test is correct but exceeded its slow-timeout budget — add it to a ` +
          `slow-timeout override in ${NEXTEST_CONFIG}; do NOT add it to ${args.baseline} ` +
          `(baselining a correct test masks it and normalizes a red lane).\n`,
      );
    }
    if (genuine.length > 0) {
      process.stderr.write(
        `::error title=New test failure not in baseline::${genuine.length} failing test(s) ` +
          `are absent from ${args.baseline}: ${genuine.join(", ")}. ` +
          `Fix the regression, or (if intentional and reviewed) update the baseline with ` +
          `node scripts/ci/known-failures-check.mjs --update.\n`,
      );
    }
    process.exit(1);
  }
  process.stdout.write("known-failures-check: no new failures.\n");
  process.exit(0);
}

// Only run main when invoked directly, so tests can import the pure functions.
const invokedDirectly =
  process.argv[1] && fs.realpathSync(process.argv[1]) === fs.realpathSync(new URL(import.meta.url).pathname);
if (invokedDirectly) main();
