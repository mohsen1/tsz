#!/usr/bin/env node
//
// Unit tests for the LPT (longest-processing-time-first) shard balancer and the
// outcome taxonomy in `runner.cjs`. Runs as a standalone Node script:
// `node shard-balance.test.cjs`.
//
// runner.cjs guards its main() with `require.main === module`, so requiring it
// exercises the real exported helpers directly. Only the timeout-bias formula
// is mirrored below, because runner exports the disk-reading `loadHistoricalWeights`
// rather than the pure results->weights core; the bias constant it uses is
// still pulled from runner so the two can't drift on the magnitude.

"use strict";

const assert = require("node:assert/strict");

const runner = require("./runner.cjs");
const { defaultUnknownWeight, TIMEOUT_WEIGHT_BIAS_MS } = runner;

// Mirror of the pure core inside runner.loadHistoricalWeights (which reads the
// snapshot from disk, so it can't be called directly on an in-memory array).
// Reuses runner's TIMEOUT_WEIGHT_BIAS_MS so only the shape is duplicated here.
function loadHistoricalWeightsFromResults(results) {
    const weights = new Map();
    for (const result of results || []) {
        if (!result || typeof result.file !== "string") continue;
        const elapsed = Number(result.elapsed || 0);
        if (!Number.isFinite(elapsed) || elapsed <= 0) continue;
        const isTimeout = result.timedOut === true || result.status === "timeout";
        const weight = isTimeout
            ? Math.max(elapsed, TIMEOUT_WEIGHT_BIAS_MS)
            : elapsed;
        weights.set(result.file.replace(/\\/g, "/"), weight);
    }
    return weights;
}

let failed = 0;
function test(name, fn) {
    try {
        fn();
        console.log(`  PASS  ${name}`);
    } catch (err) {
        failed++;
        console.error(`  FAIL  ${name}`);
        console.error(`    ${err.message}`);
    }
}

console.log("shard-balance.test.cjs");

test("timeout result is biased to TIMEOUT_WEIGHT_BIAS_MS (post PR #7521 follow-up)", () => {
    const weights = loadHistoricalWeightsFromResults([
        { file: "tests/codeFixTimeout.ts", elapsed: 26341, status: "timeout", timedOut: true },
        { file: "tests/codeFixTimeoutSibling.ts", elapsed: 25800, status: "timeout", timedOut: true },
    ]);
    assert.equal(weights.get("tests/codeFixTimeout.ts"), TIMEOUT_WEIGHT_BIAS_MS);
    assert.equal(weights.get("tests/codeFixTimeoutSibling.ts"), TIMEOUT_WEIGHT_BIAS_MS);
});

test("non-timeout result keeps raw elapsed", () => {
    const weights = loadHistoricalWeightsFromResults([
        { file: "tests/fast.ts", elapsed: 50, status: "pass", timedOut: false },
        { file: "tests/slow.ts", elapsed: 7492, status: "pass", timedOut: false },
    ]);
    assert.equal(weights.get("tests/fast.ts"), 50);
    assert.equal(weights.get("tests/slow.ts"), 7492);
});

test("missing / zero / non-numeric elapsed is skipped", () => {
    const weights = loadHistoricalWeightsFromResults([
        { file: "tests/zero.ts", elapsed: 0, status: "pass" },
        { file: "tests/missing.ts", status: "pass" },
        { file: "tests/nan.ts", elapsed: "not a number", status: "pass" },
        { file: "tests/ok.ts", elapsed: 100, status: "pass" },
    ]);
    assert.equal(weights.has("tests/zero.ts"), false);
    assert.equal(weights.has("tests/missing.ts"), false);
    assert.equal(weights.has("tests/nan.ts"), false);
    assert.equal(weights.get("tests/ok.ts"), 100);
});

test("default unknown weight is median, not arbitrary 100ms", () => {
    // Pre-PR: defaultUnknownWeight always returned 100, which systematically
    // under-weighted any test missing from the snapshot.
    const weights = new Map([
        ["a", 100],
        ["b", 200],
        ["c", 422],
        ["d", 800],
        ["e", 1500],
    ]);
    assert.equal(defaultUnknownWeight(weights), 422);

    // Edge: empty weights -> 100 (the prior default).
    assert.equal(defaultUnknownWeight(new Map()), 100);
});

test("timeout bias prevents two timeouts being indistinguishable at LPT input", () => {
    // Without the bias, both timeouts get weight ~26000. After bias, both get
    // 90000. Either way they distribute across shards, but the biased weight
    // means the LPT scheduler reserves more of the shard for them and other
    // long tests sort beneath instead of being interleaved.
    const weights = loadHistoricalWeightsFromResults([
        { file: "t/to1.ts", elapsed: 26341, status: "timeout", timedOut: true },
        { file: "t/to2.ts", elapsed: 25800, status: "timeout", timedOut: true },
        { file: "t/slow.ts", elapsed: 7492, status: "pass" },
    ]);
    assert.equal(weights.get("t/to1.ts"), TIMEOUT_WEIGHT_BIAS_MS); // 90000
    assert.equal(weights.get("t/to2.ts"), TIMEOUT_WEIGHT_BIAS_MS); // 90000
    assert.equal(weights.get("t/slow.ts"), 7492);
    // The slow non-timeout test stays well below the biased timeouts, so the
    // LPT will schedule the timeouts onto separate shards before considering
    // the slow test.
});

// -----------------------------------------------------------------------------
// Outcome taxonomy (issue #17010) — exercised against the REAL exported helpers
// in runner.cjs, so these can't drift from production the way the mirrors above
// can.
// -----------------------------------------------------------------------------

// A representative per-test result set covering every outcome.
const SAMPLE_RESULTS = [
    { file: "t/p1.ts", name: "p1", status: "pass", timedOut: false, elapsed: 120 },
    { file: "t/p2.ts", name: "p2", status: "pass", timedOut: false, elapsed: 300 },
    { file: "t/slow1.ts", name: "slow1", status: "slow", timedOut: false, elapsed: 38298 },
    { file: "t/slow2.ts", name: "slow2", status: "slow", timedOut: false, elapsed: 29185 },
    { file: "t/f1.ts", name: "f1", status: "fail", timedOut: false, elapsed: 90, firstFailure: "boom" },
    { file: "t/to1.ts", name: "to1", status: "timeout", timedOut: true, elapsed: 60000, firstFailure: "Timeout" },
    { file: "t/u1.ts", name: "u1", status: "unrun", timedOut: false, elapsed: 0, firstFailure: "Not run" },
    { file: "t/x1.ts", name: "x1", status: "xfail", timedOut: false, elapsed: 50 },
];

test("summarizeResults tallies each outcome into its own disjoint bucket", () => {
    const c = runner.summarizeResults(SAMPLE_RESULTS);
    assert.deepEqual(c, { passed: 2, slow: 2, failed: 2, timedOut: 1, unrun: 1 });
});

test("reportedPassCount folds slow into passing (load-independent figure)", () => {
    const c = runner.summarizeResults(SAMPLE_RESULTS);
    assert.equal(runner.reportedPassCount(c), 4); // 2 pass + 2 slow
});

test("executedCount excludes tests that never ran (unrun)", () => {
    const c = runner.summarizeResults(SAMPLE_RESULTS);
    // A legacy xfail is folded into fail: 2 pass + 2 slow + 2 fail + 1 timeout.
    assert.equal(runner.executedCount(c), 7);
});

test("runFailedCount folds legacy xfail into fail and never trips on slow", () => {
    const c = runner.summarizeResults(SAMPLE_RESULTS);
    assert.equal(runner.runFailedCount(c), 4); // 2 fail + 1 timeout + 1 unrun
    // A run whose only blemish is slowness exits clean.
    const slowOnly = runner.summarizeResults([
        { status: "pass" }, { status: "slow" }, { status: "slow" },
    ]);
    assert.equal(runner.runFailedCount(slowOnly), 0);
});

test("classifySnapshotBuckets splits pass/slow/fail/timeout/unrun; weights = pass+slow", () => {
    const b = runner.classifySnapshotBuckets(SAMPLE_RESULTS);
    assert.deepEqual(b.pass, ["t/p1.ts", "t/p2.ts"]);
    assert.deepEqual(b.slow, ["t/slow1.ts", "t/slow2.ts"]);
    assert.deepEqual(b.fail.map(r => r.name), ["f1"]);
    assert.deepEqual(b.timeout.map(r => r.name), ["to1"]);
    assert.deepEqual(b.unrun.map(r => r.name), ["u1"]);
    // Slow tests carry real timings and are exactly what the LPT balancer needs
    // — they must land in the weight map alongside the fast passes.
    assert.deepEqual(b.weights, {
        "t/p1.ts": 120,
        "t/p2.ts": 300,
        "t/slow1.ts": 38298,
        "t/slow2.ts": 29185,
    });
});

test("snapshot round-trips: classify -> stringify -> parse -> weights cover pass+slow, bias timeouts", () => {
    const buckets = runner.classifySnapshotBuckets(SAMPLE_RESULTS);
    const snapshot = { timestamp: "2026-08-09T00:00:00.000Z", summary: { total: 8 }, ...buckets };
    const parsed = JSON.parse(runner.stringifyCompactSnapshot(snapshot));
    assert.deepEqual(parsed.pass, ["t/p1.ts", "t/p2.ts"]);
    assert.deepEqual(parsed.slow, ["t/slow1.ts", "t/slow2.ts"]);
    assert.equal(parsed.timeout.length, 1);
    assert.equal(parsed.unrun.length, 1);

    // The weight-loader reads weights (pass+slow) plus the failure-family rows.
    const rows = runner.resultRowsForWeights(parsed);
    const byFile = new Map(rows.map(r => [r.file, r]));
    assert.equal(byFile.get("t/slow1.ts").elapsed, 38298); // slow test weighted
    assert.equal(byFile.get("t/p1.ts").elapsed, 120);
    // The timeout row is carried so its bias still applies downstream.
    assert.ok(byFile.has("t/to1.ts"));
});

test("resultRowsForWeights still reads legacy `fail`-only + `results` snapshots", () => {
    // Legacy compact snapshot: weights map + a single `fail` array.
    const legacyCompact = {
        weights: { "t/a.ts": 100 },
        fail: [{ file: "t/b.ts", elapsed: 26000, status: "timeout", timedOut: true }],
    };
    const rows = runner.resultRowsForWeights(legacyCompact);
    const files = rows.map(r => r.file).sort();
    assert.deepEqual(files, ["t/a.ts", "t/b.ts"]);

    // Even older uncollapsed snapshot: a full `results` array.
    const legacyFull = { results: [{ file: "t/c.ts", elapsed: 5 }] };
    assert.deepEqual(runner.resultRowsForWeights(legacyFull).map(r => r.file), ["t/c.ts"]);
});

if (failed > 0) {
    console.error(`\n${failed} test(s) failed`);
    process.exit(1);
}
console.log(`\nAll tests passed`);
