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

const DEFAULT_JUNIT = "target/nextest/signoff/junit.xml";
const DEFAULT_BASELINE = "scripts/ci/known-failures.txt";

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

const TESTCASE_RE = /<testcase\b([^>]*?)(?:\/>|>([\s\S]*?)<\/testcase>)/g;
const NAME_RE = /\bname="([^"]*)"/;
const CLASSNAME_RE = /\bclassname="([^"]*)"/;
const FAILURE_RE = /<(failure|error)\b/;

// Extract a single quoted attribute value from a `<testcase ...>` attribute run.
function attr(attrs, re) {
  const m = re.exec(attrs);
  return m ? m[1] : null;
}

// Parse a nextest junit document into the set of test ids that ran and the
// subset that failed. Each `<testcase>` carries `name` (test path within its
// binary) and `classname` (the binary-id); a failing case has a `<failure>` or
// `<error>` child. Timeouts surface as `<failure>`, so they count as failures.
// (A passing case may still carry `<system-out>`/`<system-err>` children, so
// failure is keyed on the failure/error tag, never on child presence.) The
// canonical id is `classname::name` (== nextest's `binary-id::test-name`).
export function parseJunitFailures(xml) {
  const all = new Set();
  const failing = new Set();
  TESTCASE_RE.lastIndex = 0;
  let m;
  while ((m = TESTCASE_RE.exec(xml)) !== null) {
    const attrs = m[1];
    const inner = m[2]; // undefined when the tag is self-closed
    const name = attr(attrs, NAME_RE);
    if (name === null) continue;
    const classname = attr(attrs, CLASSNAME_RE);
    const id = classname ? `${classname}::${name}` : name;
    all.add(id);
    if (inner !== undefined && FAILURE_RE.test(inner)) failing.add(id);
  }
  return { all, failing };
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
  let sawDirFlag = false;
  for (const dir of args.junitDirs) {
    sawDirFlag = true;
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
  if (paths.length === 0 && sawDirFlag && args.allowNoReports) {
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
    const run = parseJunitFailures(junitXml);

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
  if (newFailures.length > 0) {
    for (const id of newFailures) process.stderr.write(`  NEW FAILURE: ${id}\n`);
    process.stderr.write(
      `::error title=New test failure not in baseline::${newFailures.length} failing test(s) ` +
        `are absent from ${args.baseline}: ${newFailures.join(", ")}. ` +
        `Fix the regression, or (if intentional and reviewed) update the baseline with ` +
        `node scripts/ci/known-failures-check.mjs --update.\n`,
    );
    process.exit(1);
  }
  process.stdout.write("known-failures-check: no new failures.\n");
  process.exit(0);
}

// Only run main when invoked directly, so tests can import the pure functions.
const invokedDirectly =
  process.argv[1] && fs.realpathSync(process.argv[1]) === fs.realpathSync(new URL(import.meta.url).pathname);
if (invokedDirectly) main();
