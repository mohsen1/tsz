import assert from "node:assert/strict";

import {
  didNotFinish,
  GREEN_COMPAT,
  hasExactProjectEvidence,
  isPositiveFiniteTiming,
  isSpeedChartEligible,
  isSpeedRatioEligible,
  SLOWDOWN_FAILURE_FACTOR,
} from "./row-utils.mjs";
import { fixtureStubEvidenceFor } from "./lib/fixture-stub-inventory.mjs";

const ROOT = new URL("../..", import.meta.url).pathname;

// A completed measurement (both compilers finished, exit 0) is never DNF.
assert.equal(
  didNotFinish({ name: "ok", tsz_ms: 100, tsgo_ms: 200, winner: "tsz" }),
  false,
  "a plain completed row is not DNF",
);
assert.equal(
  didNotFinish({
    name: "ok-compat",
    tsz_ms: 100,
    tsgo_ms: 200,
    winner: "tsz",
    compatibility: {
      exit_class: "exit success",
      diagnostic_status: "none",
      exit_codes: { tsz: [0], tsgo: [0] },
    },
  }),
  false,
  "a green compatibility row with zero exit codes is not DNF",
);

// The merge step's explicit error stub is DNF.
assert.equal(didNotFinish({ name: "err", winner: "error" }), true, "winner:error is DNF");

// A timeout is DNF regardless of the recorded ceiling time.
assert.equal(
  didNotFinish({
    name: "large-ts-repo",
    tsz_ms: 1_500_000,
    tsgo_ms: 34_900,
    winner: "tsz",
    compatibility: { exit_class: "timeout", exit_codes: { tsz: [124], tsgo: [1] } },
  }),
  true,
  "an exit_class of timeout is DNF",
);

// A non-zero exit code on EITHER side is DNF, even with an otherwise green
// exit_class and even when the other compiler completed — and a "nonzero exit"
// exit_class is DNF too. These are the cases the slowdown-failure heuristic and
// the winner:error check both miss.
for (const [name, exit_class, exit_codes, message] of [
  ["tsz-killed", "exit success", { tsz: [124], tsgo: [0] }, "a non-zero tsz exit code is DNF"],
  ["tsgo-errored", "exit success", { tsz: [0], tsgo: [2] }, "a non-zero tsgo exit code is DNF even when tsz completed"],
  ["nonzero-exit-class", "nonzero exit", { tsz: [1], tsgo: [0] }, "an exit_class of nonzero exit is DNF"],
]) {
  assert.equal(
    didNotFinish({ name, tsz_ms: 400, tsgo_ms: 350, winner: "tsz", compatibility: { exit_class, exit_codes } }),
    true,
    message,
  );
}

// The #16196 invariant, asserted directly: whenever a reported ratio equals the
// timeout ceiling over the other side's time (to two decimals), the row did not
// complete — so any such row must be DNF and never contribute that ratio.
{
  const ceilingMs = 1_500_000;
  const tsgoMs = 34_900;
  const fabricatedRatio = ceilingMs / tsgoMs; // "42.99x" — contains no measurement of tsz
  const row = {
    name: "large-ts-repo",
    tsz_ms: ceilingMs,
    tsgo_ms: tsgoMs,
    winner: "tsz",
    compatibility: { exit_class: "timeout", exit_codes: { tsz: [124], tsgo: [1] } },
  };
  assert.ok(didNotFinish(row), "a row whose ratio is ceiling/other_time must be DNF");
  assert.equal(
    fabricatedRatio.toFixed(2),
    "42.98",
    "sanity: the reported large-ts-repo ratio was exactly ceiling/tsgo_time",
  );
}

// Guards against undefined/missing shapes.
assert.equal(didNotFinish(null), false);
assert.equal(didNotFinish(undefined), false);
assert.equal(didNotFinish({ name: "bare" }), false, "a row with no compatibility metadata is not DNF");

// --- isPositiveFiniteTiming: the single definition of "a usable timing" ---
for (const value of [null, undefined, "", 0, -1, -0.5, "x", NaN, Infinity, -Infinity]) {
  assert.equal(isPositiveFiniteTiming(value), false, `${String(value)} is not a usable timing`);
}
for (const value of [0.5, 1, 8, "12", 1500]) {
  assert.equal(isPositiveFiniteTiming(value), true, `${String(value)} is a usable timing`);
}

// --- isSpeedRatioEligible: the one canonical "successful timing pair" gate ---
// The regression this consolidation fixes (#17302): a row that did NOT finish
// but still carries finite timings and a non-error winner must be excluded from
// every speed-ratio surface. Two sites (check-artifact-readiness and
// benchmark-artifact-selection) previously omitted this guard.
const dnfWithFiniteTimings = [
  {
    label: "timeout ceiling",
    row: { name: "big", tsz_ms: 1_500_000, tsgo_ms: 40_000, winner: "tsz", compatibility: { exit_class: "timeout" } },
  },
  {
    label: "nonzero exit_class",
    row: { name: "err", tsz_ms: 10, tsgo_ms: 12, winner: "tsz", compatibility: { exit_class: "nonzero exit" } },
  },
  {
    label: "nonzero exit code on one side",
    row: { name: "killed", tsz_ms: 10, tsgo_ms: 12, winner: "tsz", compatibility: { exit_codes: { tsz: [0], tsgo: [1] } } },
  },
  {
    label: "winner:error stub",
    row: { name: "stub", tsz_ms: 10, tsgo_ms: 12, winner: "error" },
  },
];
for (const { label, row } of dnfWithFiniteTimings) {
  assert.ok(didNotFinish(row), `sanity: ${label} row is DNF`);
  assert.equal(isSpeedRatioEligible(row), false, `${label}: DNF row with finite timings is not speed-ratio eligible`);
  assert.equal(isSpeedChartEligible(row), false, `${label}: DNF row with finite timings never charts`);
}

// A status error, or a missing/sentinel timing on either side, is ineligible.
assert.equal(isSpeedRatioEligible({ tsz_ms: 8, tsgo_ms: 12, status: "error" }), false, "status error is ineligible");
assert.equal(isSpeedRatioEligible({ tsz_ms: 8, tsgo_ms: null }), false, "missing tsgo timing is ineligible");
assert.equal(isSpeedRatioEligible({ tsz_ms: 0, tsgo_ms: 12 }), false, "zero tsz timing is ineligible");
assert.equal(isSpeedRatioEligible(null), false, "null row is ineligible");
assert.equal(isSpeedRatioEligible(undefined), false, "undefined row is ineligible");

// A clean completed pair is eligible.
const cleanPair = { name: "ok", tsz_ms: 8, tsgo_ms: 12, winner: "tsz" };
assert.equal(isSpeedRatioEligible(cleanPair), true, "a clean measured pair is eligible");
assert.equal(
  isSpeedRatioEligible({ name: "ok-green", tsz_ms: 8, tsgo_ms: 12, winner: "tsz", compatibility: GREEN_COMPAT }),
  true,
  "a schema-v2-evidenced green completed pair is eligible",
);
assert.equal(
  hasExactProjectEvidence(GREEN_COMPAT, "utility-types-project"),
  true,
  "source-derived zero-stub evidence keeps a real-dependency project eligible",
);
for (const semantic_completion of [undefined, null, "deferred", "cycle", "limit"]) {
  const compatibility = { ...GREEN_COMPAT, semantic_completion };
  assert.equal(
    hasExactProjectEvidence(compatibility, "utility-types-project"),
    false,
    `${String(semantic_completion)} completion cannot certify project evidence`,
  );
  assert.equal(
    isSpeedRatioEligible({
      name: "utility-types-project",
      tsz_ms: 8,
      tsgo_ms: 12,
      winner: "tsz",
      compatibility,
    }),
    false,
    `${String(semantic_completion)} completion cannot reach a timing claim`,
  );
}
{
  const omitted = { ...GREEN_COMPAT };
  delete omitted.stub_inventory_schema;
  delete omitted.stubbed_modules;
  delete omitted.stubbed_any_members;
  delete omitted.stub_inventory_fingerprint;
  assert.equal(
    hasExactProjectEvidence(omitted, "utility-types-project"),
    false,
    "an artifact cannot omit stub evidence and remain exact",
  );
}
for (const name of ["msw-project", "effect-project", "drizzle-orm-project"]) {
  assert.equal(
    hasExactProjectEvidence(GREEN_COMPAT, name),
    false,
    `${name}: forged zero stub fields disagree with source inventory`,
  );
  const actual = fixtureStubEvidenceFor(ROOT, name);
  const compatibility = {
    ...GREEN_COMPAT,
    stub_inventory_schema: actual.stubInventorySchema,
    stubbed_modules: actual.stubbedModules,
    stubbed_any_members: actual.stubbedAnyMembers,
    stub_inventory_fingerprint: actual.stubInventoryFingerprint,
  };
  assert.equal(
    hasExactProjectEvidence(compatibility, name),
    false,
    `${name}: honest nonzero stub evidence remains non-green and non-timing`,
  );
  assert.equal(
    isSpeedRatioEligible({ name, tsz_ms: 8, tsgo_ms: 12, winner: "tsz", compatibility }),
    false,
    `${name}: a forged timing pair is rejected by the shared consumer gate`,
  );
}
assert.equal(
  isSpeedRatioEligible({
    name: "legacy-green",
    tsz_ms: 8,
    tsgo_ms: 12,
    winner: "tsz",
    compatibility: { exit_class: "exit success", exit_codes: { tsc: [0], tsz: [0], tsgo: [0] } },
  }),
  false,
  "a phase-only compatibility label cannot certify a timing pair",
);

// --- isSpeedChartEligible: base gate + the slowdown-failure threshold ---
assert.equal(SLOWDOWN_FAILURE_FACTOR, 1.5, "the shared slowdown-failure threshold is 1.5x");
assert.equal(isSpeedChartEligible(cleanPair), true, "a fast eligible pair charts");
assert.equal(
  isSpeedChartEligible({ tsz_ms: 20, tsgo_ms: 12 }),
  false,
  "a >=1.5x-slower pair is eligible for a ratio but must not chart",
);
assert.equal(
  isSpeedRatioEligible({ tsz_ms: 20, tsgo_ms: 12 }),
  true,
  "a >=1.5x-slower pair still has a real measured ratio",
);
assert.equal(
  isSpeedChartEligible({ tsz_ms: 18, tsgo_ms: 12 }),
  false,
  "exactly 1.5x is a failure (threshold is a strict <)",
);
assert.equal(
  isSpeedChartEligible({ tsz_ms: 12, tsgo_ms: 12 }, 1),
  false,
  "the threshold factor is configurable: at factor 1 a tsz equal to tsgo is dropped",
);
assert.equal(
  isSpeedChartEligible({ tsz_ms: 12, tsgo_ms: 12 }),
  true,
  "the same row charts at the default 1.5x factor",
);

console.log("test-row-utils.mjs: all assertions passed");
