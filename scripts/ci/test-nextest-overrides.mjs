#!/usr/bin/env node
// Unit tests for the nextest override/budget helpers and the timeout-aware
// enrichment of the known-failures gate (#17675, part 3).
import assert from "node:assert/strict";
import fs from "node:fs";
import {
  parseJunitCases,
  classifyFailure,
  parseOverrides,
  countOverrideHeaders,
  overrideMatchesTest,
  findBudgetOverride,
  budgetedOverridesForProfile,
  profileBaseSlowPeriodSeconds,
  collectLiteralFilters,
} from "./nextest-overrides.mjs";
import { mergeCases, splitTimeouts, slowUncoveredPassing } from "./known-failures-check.mjs";
import { findOrphanedLiterals } from "./check-nextest-overrides.mjs";

let passed = 0;
function check(name, fn) {
  fn();
  passed++;
  console.log(`PASS ${name}`);
}

// A minimal nextest-shaped junit: a fast pass, a genuine failure, a timeout
// (nextest's `<failure type="slow-timeout">`), a panic `<error>`, and a
// slow-but-passing case.
const JUNIT = `<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run">
  <testsuite name="tsz-solver::rel">
    <testcase name="fast" classname="tsz-solver::rel" time="0.10"/>
    <testcase name="regressed" classname="tsz-solver::rel" time="0.20">
      <failure type="test failure">assertion failed</failure>
    </testcase>
  </testsuite>
  <testsuite name="tsz-cli::driver">
    <testcase name="collect_diagnostics_reports_default_lib_breakage_from_global_node_merge" classname="tsz-cli::driver" time="240.0">
      <failure type="slow-timeout" message="Test timed out after 240s">timed out</failure>
    </testcase>
    <testcase name="panics" classname="tsz-cli::driver" time="0.0">
      <error type="test error">panicked at 'boom'</error>
    </testcase>
    <testcase name="heavy_but_passing" classname="tsz-cli::driver" time="45.0"/>
  </testsuite>
</testsuites>`;

check("parseJunitCases captures id, time, and failure type/text", () => {
  const cases = parseJunitCases(JUNIT);
  const byId = new Map(cases.map((c) => [c.id, c]));
  assert.equal(cases.length, 5);
  assert.equal(byId.get("tsz-solver::rel::fast").failure, null);
  assert.equal(byId.get("tsz-solver::rel::fast").timeSeconds, 0.1);
  assert.equal(byId.get("tsz-solver::rel::regressed").failure.type, "test failure");
  const timeout = byId.get(
    "tsz-cli::driver::collect_diagnostics_reports_default_lib_breakage_from_global_node_merge",
  );
  assert.equal(timeout.failure.type, "slow-timeout");
  assert.ok(timeout.failure.text.includes("timed out"));
  assert.equal(byId.get("tsz-cli::driver::panics").failure.type, "test error");
});

check("classifyFailure distinguishes timeout from genuine failure/error", () => {
  assert.equal(classifyFailure(null), null);
  assert.equal(classifyFailure({ type: "test failure", text: "left != right" }), "failure");
  assert.equal(classifyFailure({ type: "test error", text: "panicked" }), "failure");
  assert.equal(classifyFailure({ type: "slow-timeout", text: "timed out" }), "timeout");
  // robust to alternate nextest spellings via the message text
  assert.equal(classifyFailure({ type: "", text: "Test timed out after 60s" }), "timeout");
  assert.equal(classifyFailure({ type: "test timeout", text: "" }), "timeout");
});

const TOML = `
[profile.signoff]
slow-timeout = { period = "30s", terminate-after = 2 }

[[profile.default.overrides]]
filter = 'test(heavy_alpha) | test(heavy_beta)'
slow-timeout = { period = "180s", terminate-after = 2 }
threads-required = "num-test-threads"

[[profile.default.overrides]]
filter = 'test(/^regex_only_/) | test(=exact_name)'
threads-required = "num-test-threads"

[[profile.precommit.overrides]]
filter = 'test(precommit_heavy)'
slow-timeout = { period = "90s", terminate-after = 2 }
`;

check("parseOverrides reads filters, budgets, and literal/regex predicates", () => {
  const overrides = parseOverrides(TOML);
  assert.equal(overrides.length, 3);
  assert.equal(countOverrideHeaders(TOML), 3); // every header yielded a record
  const first = overrides[0];
  assert.deepEqual(first.literals, ["heavy_alpha", "heavy_beta"]);
  assert.equal(first.budgetSeconds, 360); // 180s period x terminate-after 2
  const second = overrides[1];
  assert.deepEqual(second.regexes, ["^regex_only_"]);
  assert.deepEqual(second.literals, ["exact_name"]); // `=` prefix stripped
  assert.equal(second.budgetSeconds, null); // threads-required only
});

check("countOverrideHeaders flags a block whose filter was wrapped across lines", () => {
  // A formatter split the long filter — the block header is still present but
  // parseOverrides drops it, so the counts diverge and the guard can fail loud.
  const wrapped = `
[[profile.default.overrides]]
filter = '''test(a)
  | test(b)'''
slow-timeout = { period = "90s", terminate-after = 2 }
`;
  assert.equal(countOverrideHeaders(wrapped), 1);
  assert.equal(parseOverrides(wrapped).length, 0);
});

check("profileBaseSlowPeriodSeconds reads the profile's base period", () => {
  assert.equal(profileBaseSlowPeriodSeconds(TOML, "signoff"), 30);
  assert.equal(profileBaseSlowPeriodSeconds(TOML, "nonexistent"), null);
});

check("overrideMatchesTest matches substring literals and regexes", () => {
  const overrides = parseOverrides(TOML);
  assert.ok(overrideMatchesTest(overrides[0], "mod::heavy_alpha_variant")); // substring
  assert.ok(!overrideMatchesTest(overrides[0], "unrelated"));
  assert.ok(overrideMatchesTest(overrides[1], "regex_only_thing"));
});

check("budgeted overrides include default + active profile, exclude budgetless", () => {
  const overrides = parseOverrides(TOML);
  const signoff = budgetedOverridesForProfile(overrides, "signoff");
  // the default slow-timeout block applies (inherited); the threads-only block
  // is excluded; the precommit block is a different profile.
  assert.equal(signoff.length, 1);
  assert.equal(signoff[0].literals[0], "heavy_alpha");
  const precommit = budgetedOverridesForProfile(overrides, "precommit");
  assert.equal(precommit.length, 2); // default budgeted block + precommit block
});

check("findBudgetOverride locates the covering slow-timeout override", () => {
  const overrides = parseOverrides(TOML);
  assert.ok(findBudgetOverride(overrides, "signoff", "x::heavy_beta"));
  assert.equal(findBudgetOverride(overrides, "signoff", "x::exact_name"), null); // budgetless
  assert.equal(findBudgetOverride(overrides, "signoff", "x::precommit_heavy"), null); // wrong profile
});

check("collectLiteralFilters dedupes literals across profiles with their sources", () => {
  const literals = collectLiteralFilters(parseOverrides(TOML));
  const names = literals.map((l) => l.literal);
  assert.deepEqual(names, ["exact_name", "heavy_alpha", "heavy_beta", "precommit_heavy"]);
  assert.deepEqual(literals.find((l) => l.literal === "heavy_alpha").profiles, ["default"]);
});

check("mergeCases: failing case and larger time win across passes", () => {
  const a = parseJunitCases(`<testcase name="t" classname="c" time="1.0"/>`);
  const b = parseJunitCases(`<testcase name="t" classname="c" time="9.0"><failure type="test failure">x</failure></testcase>`);
  const merged = mergeCases([a, b]);
  assert.equal(merged.get("c::t").timeSeconds, 9.0);
  assert.ok(merged.get("c::t").failure);
});

check("splitTimeouts routes timeouts away from genuine failures", () => {
  const casesById = mergeCases([parseJunitCases(JUNIT)]);
  const timeoutId =
    "tsz-cli::driver::collect_diagnostics_reports_default_lib_breakage_from_global_node_merge";
  const { timeouts, genuine } = splitTimeouts(
    [timeoutId, "tsz-solver::rel::regressed", "tsz-cli::driver::panics"],
    casesById,
  );
  assert.deepEqual(timeouts, [timeoutId]);
  assert.deepEqual(genuine.sort(), [
    "tsz-cli::driver::panics",
    "tsz-solver::rel::regressed",
  ]);
  // an id with no case record falls to genuine (never silently dropped)
  const missing = splitTimeouts(["c::gone"], casesById);
  assert.deepEqual(missing.genuine, ["c::gone"]);
});

check("slowUncoveredPassing flags a slow passing test with no override", () => {
  const casesById = mergeCases([parseJunitCases(JUNIT)]);
  // base 30s, no override covers `heavy_but_passing` (45s) -> flagged; the
  // timeout is a failure (excluded), and the fast pass is under budget.
  const overrides = parseOverrides(TOML);
  const slow = slowUncoveredPassing(casesById, overrides, "signoff", 30);
  assert.deepEqual(slow.map((s) => s.id), ["tsz-cli::driver::heavy_but_passing"]);
});

check("slowUncoveredPassing skips a slow passing test that IS overridden", () => {
  const junit = `<testcase name="heavy_alpha" classname="c" time="120.0"/>`;
  const casesById = mergeCases([parseJunitCases(junit)]);
  const overrides = parseOverrides(TOML);
  assert.equal(slowUncoveredPassing(casesById, overrides, "signoff", 30).length, 0);
});

check("slowUncoveredPassing is a no-op when the base period is unknown", () => {
  const casesById = mergeCases([parseJunitCases(JUNIT)]);
  assert.equal(slowUncoveredPassing(casesById, [], "signoff", null).length, 0);
});

// The real committed config must parse and every profile's slow-timeout budget
// must be well-formed — this pins the reader against config drift.
check("the committed .config/nextest.toml parses into well-formed overrides", () => {
  const toml = fs.readFileSync(new URL("../../.config/nextest.toml", import.meta.url), "utf8");
  const overrides = parseOverrides(toml);
  assert.ok(overrides.length >= 4, "expected several override blocks");
  for (const o of overrides) {
    assert.ok(o.filterRaw && o.filterRaw.length > 0, "every override has a filter");
    if (o.budgetSeconds !== null) {
      assert.ok(o.budgetSeconds > 0, `budget parses for ${o.filterRaw}`);
    }
  }
  assert.equal(profileBaseSlowPeriodSeconds(toml, "signoff"), 30);
  // the #17675 heavy tests must be covered by a budgeted override under signoff
  for (const name of [
    "global_lib_merge_keeps_one_element_identity_for_array_callbacks",
    "collect_diagnostics_reports_default_lib_breakage_from_global_node_merge",
  ]) {
    assert.ok(findBudgetOverride(overrides, "signoff", name), `${name} is covered`);
  }
});

check("findOrphanedLiterals resolves real names and flags a fabricated one", () => {
  const orphans = findOrphanedLiterals([
    // pinned by an actual override in the committed config -> exists
    "collect_diagnostics_reports_default_lib_breakage_from_global_node_merge",
    // appears nowhere in the tree -> orphaned
    "zzz_nonexistent_override_filter_name_9f3a1c",
  ]);
  assert.deepEqual(orphans, ["zzz_nonexistent_override_filter_name_9f3a1c"]);
  assert.deepEqual(findOrphanedLiterals([]), []);
});

console.log(`\nAll ${passed} nextest-overrides tests passed.`);
