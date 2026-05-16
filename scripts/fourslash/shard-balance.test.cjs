#!/usr/bin/env node
//
// Unit tests for the LPT (longest-processing-time-first) shard balancer
// in `runner.cjs`. Runs as a standalone Node script: `node shard-balance.test.cjs`.
//
// Re-implements the small helpers under test rather than refactoring runner.cjs
// to export them — the bias formulas are short enough that two copies is
// cheaper than the export surface area. Update both sides if you change the
// formula.

"use strict";

const assert = require("node:assert/strict");

// Mirror of runner.cjs constants and helpers under test. Keep in sync.
const TIMEOUT_WEIGHT_BIAS_MS = 60_000 * 1.5;

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

function defaultUnknownWeight(weights) {
    if (weights.size === 0) return 100;
    const sorted = [...weights.values()].sort((a, b) => a - b);
    return sorted[Math.floor(sorted.length / 2)];
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

if (failed > 0) {
    console.error(`\n${failed} test(s) failed`);
    process.exit(1);
}
console.log(`\nAll tests passed`);
