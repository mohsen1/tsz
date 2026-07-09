#!/usr/bin/env node
// Unit tests for the known-failures baseline comparator (#15399).
import assert from "node:assert/strict";
import {
  parseJunitFailures,
  parseBaseline,
  baselineGeneration,
  baselineIsReconciled,
  evaluate,
  renderBaseline,
  unionRuns,
} from "./known-failures-check.mjs";

let passed = 0;
function check(name, fn) {
  fn();
  passed++;
  console.log(`PASS ${name}`);
}

// Every test id that fails in JUNIT below. Individual tests vary a baseline off
// this set (add a now-passing id, add an absent id, drop one) so the variation
// is the visible part.
const ALL_FAILING = [
  "tsz-checker::disp::known_bad",
  "tsz-checker::disp::hung",
  "tsz-checker::disp::errored",
  "tsz-solver::rel::regressed",
];

// A minimal nextest-shaped junit document. Passing cases are self-closed;
// failing cases carry a <failure>/<error> child; a timeout is a <failure>.
const JUNIT = `<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="5" failures="3">
  <testsuite name="tsz-solver::rel" tests="2" failures="1">
    <testcase name="keeps_passing" classname="tsz-solver::rel" time="0.1"/>
    <testcase name="regressed" classname="tsz-solver::rel" time="0.2">
      <failure type="test failure">assertion failed</failure>
    </testcase>
  </testsuite>
  <testsuite name="tsz-checker::disp" tests="3" failures="2">
    <testcase name="known_bad" classname="tsz-checker::disp" time="0.3">
      <failure type="test failure">left != right</failure>
    </testcase>
    <testcase name="hung" classname="tsz-checker::disp" time="60.0">
      <failure type="slow-timeout">timed out</failure>
    </testcase>
    <testcase name="errored" classname="tsz-checker::disp" time="0.0">
      <error type="test error">panicked</error>
    </testcase>
  </testsuite>
</testsuites>`;

check("parseJunitFailures splits passes from failures and builds binary-id::test-name ids", () => {
  const { all, failing } = parseJunitFailures(JUNIT);
  assert.deepEqual([...all].sort(), [
    "tsz-checker::disp::errored",
    "tsz-checker::disp::hung",
    "tsz-checker::disp::known_bad",
    "tsz-solver::rel::keeps_passing",
    "tsz-solver::rel::regressed",
  ]);
  assert.deepEqual([...failing].sort(), [
    "tsz-checker::disp::errored",
    "tsz-checker::disp::hung",
    "tsz-checker::disp::known_bad",
    "tsz-solver::rel::regressed",
  ]);
  // a <failure> child (timeout) and an <error> child both count as failing;
  // the self-closed passing case does not.
  assert.ok(!failing.has("tsz-solver::rel::keeps_passing"));
  assert.ok(failing.has("tsz-checker::disp::hung"));
  assert.ok(failing.has("tsz-checker::disp::errored"));
});

check("testcase name with no classname falls back to the bare name", () => {
  const { all, failing } = parseJunitFailures(
    `<testcase name="lonely"><failure/></testcase>`,
  );
  assert.deepEqual([...all], ["lonely"]);
  assert.deepEqual([...failing], ["lonely"]);
});

check("a junit with no testcases yields empty sets (drives the infra-error guard)", () => {
  const { all, failing } = parseJunitFailures("<testsuites></testsuites>");
  assert.equal(all.size, 0);
  assert.equal(failing.size, 0);
});

check("parseBaseline ignores comments and blank lines", () => {
  const set = parseBaseline(
    "# header\n\n  tsz-checker::disp::known_bad  \n#another\ntsz-checker::disp::hung\n",
  );
  assert.deepEqual([...set].sort(), [
    "tsz-checker::disp::hung",
    "tsz-checker::disp::known_bad",
  ]);
});

check("new failure not in baseline is reported (regression -> block)", () => {
  // baseline omits `regressed`, so it surfaces as the one new failure.
  const baseline = parseBaseline(ALL_FAILING.filter((id) => id !== "tsz-solver::rel::regressed").join("\n"));
  const { newFailures, nowPassing } = evaluate(baseline, parseJunitFailures(JUNIT));
  assert.deepEqual(newFailures, ["tsz-solver::rel::regressed"]);
  assert.equal(nowPassing.length, 0);
});

check("all failures baselined -> no new failures, no shrink", () => {
  const baseline = parseBaseline(ALL_FAILING.join("\n"));
  const { newFailures, nowPassing } = evaluate(baseline, parseJunitFailures(JUNIT));
  assert.equal(newFailures.length, 0);
  assert.equal(nowPassing.length, 0);
});

check("baselined test that now passes is a shrink candidate (ran + not failing)", () => {
  // `keeps_passing` ran and passed but is listed in the baseline -> shrink.
  const baseline = parseBaseline(["tsz-solver::rel::keeps_passing", ...ALL_FAILING].join("\n"));
  const { newFailures, nowPassing } = evaluate(baseline, parseJunitFailures(JUNIT));
  assert.equal(newFailures.length, 0);
  assert.deepEqual(nowPassing, ["tsz-solver::rel::keeps_passing"]);
});

check("a baselined test absent from this run is neither new nor shrink (never blocks)", () => {
  // Partial/filtered run: `tsz-core::gone::not_run` is baselined but absent from
  // JUNIT; every currently-failing test is baselined, so the only interesting
  // case is the absent one -> it must not be flagged as new or as a shrink.
  const baseline = parseBaseline([...ALL_FAILING, "tsz-core::gone::not_run"].join("\n"));
  const { newFailures, nowPassing } = evaluate(baseline, parseJunitFailures(JUNIT));
  assert.equal(newFailures.length, 0);
  assert.equal(nowPassing.length, 0);
});

check("unionRuns merges per-pass reports into one adjudicated run (#15646)", () => {
  // The unit lane records one junit per nextest pass; the gate judges their
  // union. A test failing in ANY pass stays failing, and passes contribute
  // disjoint test populations.
  const passA = parseJunitFailures(
    `<testcase name="a_ok" classname="tsz-core::x"/>` +
      `<testcase name="a_bad" classname="tsz-core::x"><failure/></testcase>`,
  );
  const passB = parseJunitFailures(
    `<testcase name="b_ok" classname="tsz-checker::y"/>`,
  );
  const merged = unionRuns([passA, passB]);
  assert.deepEqual([...merged.all].sort(), [
    "tsz-checker::y::b_ok",
    "tsz-core::x::a_ok",
    "tsz-core::x::a_bad",
  ].sort());
  assert.deepEqual([...merged.failing], ["tsz-core::x::a_bad"]);
  // union with an empty list is empty (callers guard the no-reports case)
  assert.equal(unionRuns([]).all.size, 0);
});

check("a test failing in one pass and passing in another stays failing", () => {
  // Overlapping populations can only happen when two passes ran the same
  // test (e.g. a retry pass); the conservative reading is "it failed".
  const failed = parseJunitFailures(
    `<testcase name="flaky" classname="tsz-core::x"><failure/></testcase>`,
  );
  const passed = parseJunitFailures(`<testcase name="flaky" classname="tsz-core::x"/>`);
  assert.deepEqual([...unionRuns([failed, passed]).failing], ["tsz-core::x::flaky"]);
  assert.deepEqual([...unionRuns([passed, failed]).failing], ["tsz-core::x::flaky"]);
});

check("renderBaseline round-trips through parseBaseline and is sorted + deduped", () => {
  const body = renderBaseline(["tsz-b::t::z", "tsz-a::t::a", "tsz-b::t::z"]);
  assert.ok(body.startsWith("# Known-failures baseline"));
  assert.deepEqual([...parseBaseline(body)].sort(), ["tsz-a::t::a", "tsz-b::t::z"]);
  // ids appear in sorted order after the header block
  const lines = body.split("\n").filter((l) => l && !l.startsWith("#"));
  assert.deepEqual(lines, ["tsz-a::t::a", "tsz-b::t::z"]);
});

check("--update render (default) is reconciled -> strict even when empty (green tree blocks)", () => {
  const body = renderBaseline([]); // what `--update` writes on a fully-green run
  assert.equal(parseBaseline(body).size, 0);
  assert.ok(baselineIsReconciled(body), "empty --update baseline must still be reconciled");
});

check("committed bootstrap render is unreconciled -> advisory (does not block)", () => {
  const body = renderBaseline([], { reconciled: false });
  assert.equal(parseBaseline(body).size, 0);
  assert.ok(!baselineIsReconciled(body), "bootstrap baseline must be unreconciled");
});

check("reconcile generation: 0 unreconciled, bare marker reads as r1, rN parses", () => {
  assert.equal(baselineGeneration("# header only\n"), 0);
  // legacy bare marker (pre-generation baselines) still reads as reconciled
  assert.equal(baselineGeneration("# h\n# baseline-status: reconciled\n"), 1);
  assert.equal(baselineGeneration("# h\n# baseline-status: reconciled r4\n"), 4);
  assert.ok(baselineIsReconciled("# baseline-status: reconciled r4\n"));
});

check("renderBaseline stamps the requested generation and round-trips", () => {
  const body = renderBaseline(["tsz-a::t::x"], { generation: 2 });
  assert.equal(baselineGeneration(body), 2);
  // default generation is r1 (first reconcile)
  assert.equal(baselineGeneration(renderBaseline([])), 1);
});

console.log(`\nAll ${passed} known-failures-check tests passed.`);
