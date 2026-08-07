import assert from "node:assert/strict";

import { didNotFinish } from "./row-utils.mjs";

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

console.log("test-row-utils.mjs: all assertions passed");
