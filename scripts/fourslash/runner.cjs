#!/usr/bin/env node
/**
 * runner.js - Parallel fourslash test runner for tsz-server
 *
 * Runs TypeScript's fourslash test suite against tsz-server using parallel
 * child processes, each with its own tsz-server instance.
 *
 * Features:
 * - Parallel execution with N workers (default: CPU count)
 * - Per-test timeout protection (default: 25s)
 * - Per-worker OOM protection with memory monitoring + bridge restart
 * - Worker crash recovery (remaining tests redistributed)
 * - Detailed timing and memory stats in summary
 *
 * Architecture:
 *   runner.js (main process)
 *     → discovers tests, distributes to N child processes
 *     → each child process (test-worker.js):
 *       → loads TypeScript harness
 *       → creates TszServerBridge → tsz-worker → tsz-server
 *       → runs assigned tests sequentially
 *       → reports results via IPC
 *
 * Usage:
 *   node runner.js [options]
 *
 * Options:
 *   --tsz-server=PATH     Path to tsz-server binary (required)
 *   --max=N               Maximum number of tests to run
 *   --offset=N            Skip first N tests (applied after --shard)
 *   --shard=I/N           Run shard I of N
 *   --shard-strategy=MODE Shard strategy: weighted or hash (default: weighted)
 *   --filter=PATTERN      Only run tests matching pattern (substring)
 *   --test-dir=DIR        Test directory relative to TypeScript root
 *   --verbose             Show detailed output for each test
 *   --server-tests        Run server-specific tests
 *   --workers=N           Number of parallel workers (default: CPU count)
 *   --sequential          Run tests sequentially (single process, no workers)
 *   --timeout=MS          Per-test timeout in ms (default: 25000)
 *   --memory-limit=MB     Per-worker memory limit in MB (default: 512)
 */

"use strict";

const path = require("path");
const fs = require("fs");
const os = require("os");
const { fork } = require("child_process");
const { patchSessionClient } = require("./runner-session-client.cjs");

function isBaselineOnlyFailure(message) {
    if (typeof message !== "string") return false;
    return message.includes("New baseline created at tests/baselines/local/")
        || message.includes("verifyIndentationAtCurrentPosition failed")
        || message.includes("verifyCurrentLineContent");
}

// =============================================================================
// Argument parsing
// =============================================================================

function parseArgs() {
    const args = process.argv.slice(2);
    const opts = {
        tszServerBinary: null,
        max: 0,
        offset: 0,
        shardId: -1,
        shardTotal: 0,
        shardStrategy: "weighted",
        filter: "",
        testDir: "tests/cases/fourslash",
        verbose: false,
        serverTests: false,
        workers: os.cpus().length,
        sequential: false,
        testTimeout: 25000,
        memoryLimitMB: 512,
        jsonOut: null,
    };

    for (const arg of args) {
        if (arg.startsWith("--tsz-server=")) {
            opts.tszServerBinary = arg.substring("--tsz-server=".length);
        } else if (arg.startsWith("--max=")) {
            opts.max = parseInt(arg.substring("--max=".length), 10);
        } else if (arg.startsWith("--offset=")) {
            opts.offset = parseInt(arg.substring("--offset=".length), 10);
        } else if (arg.startsWith("--shard=")) {
            const spec = arg.substring("--shard=".length);
            const m = /^(\d+)\/(\d+)$/.exec(spec);
            if (!m) {
                console.error(`Error: --shard expects I/N (got: ${spec})`);
                process.exit(2);
            }
            opts.shardId = parseInt(m[1], 10);
            opts.shardTotal = parseInt(m[2], 10);
            if (opts.shardTotal < 1 || opts.shardId < 0 || opts.shardId >= opts.shardTotal) {
                console.error(`Error: --shard=${spec} out of range`);
                process.exit(2);
            }
        } else if (arg.startsWith("--shard-strategy=")) {
            opts.shardStrategy = arg.substring("--shard-strategy=".length);
            if (!["weighted", "hash"].includes(opts.shardStrategy)) {
                console.error(`Error: --shard-strategy must be weighted or hash (got: ${opts.shardStrategy})`);
                process.exit(2);
            }
        } else if (arg.startsWith("--filter=")) {
            opts.filter = arg.substring("--filter=".length);
        } else if (arg.startsWith("--test-dir=")) {
            opts.testDir = arg.substring("--test-dir=".length);
        } else if (arg === "--verbose") {
            opts.verbose = true;
        } else if (arg === "--server-tests") {
            opts.serverTests = true;
            opts.testDir = "tests/cases/fourslash/server";
        } else if (arg.startsWith("--workers=")) {
            opts.workers = parseInt(arg.substring("--workers=".length), 10);
        } else if (arg === "--sequential") {
            opts.sequential = true;
        } else if (arg.startsWith("--timeout=")) {
            opts.testTimeout = parseInt(arg.substring("--timeout=".length), 10);
        } else if (arg.startsWith("--memory-limit=")) {
            opts.memoryLimitMB = parseInt(arg.substring("--memory-limit=".length), 10);
        } else if (arg.startsWith("--json-out=")) {
            opts.jsonOut = arg.substring("--json-out=".length);
        } else if (arg === "--json-out") {
            opts.jsonOut = path.join(__dirname, "fourslash-snapshot.json");
        }
    }

    if (!opts.tszServerBinary) {
        console.error("Error: --tsz-server=PATH is required");
        process.exit(2);
    }

    if (opts.workers < 1) opts.workers = 1;
    if (opts.workers > 32) opts.workers = 32;

    return opts;
}

// =============================================================================
// Test file discovery
// =============================================================================

function discoverTests(testDir, filter) {
    const files = [];
    const skipListFile = path.join(__dirname, "skip_if_failing.txt");
    const skipList = fs.existsSync(skipListFile) 
        ? new Set(fs.readFileSync(skipListFile, "utf-8").split("\n").filter(l => l.trim().length > 0)) 
        : new Set();

    function walk(dir) {
        const entries = fs.readdirSync(dir, { withFileTypes: true });
        for (const entry of entries) {
            const fullPath = path.join(dir, entry.name);
            if (entry.isDirectory()) {
                walk(fullPath);
            } else if (entry.isFile() && entry.name.endsWith(".ts")) {
                const relPath = fullPath.replace(/\\/g, "/");
                const testName = path.basename(entry.name, ".ts");
                if (!filter || relPath.includes(filter)) {
                    if (!skipList.has(testName) && !skipList.has(relPath)) {
                        files.push(relPath);
                    }
                }
            }
        }
    }

    if (fs.existsSync(testDir)) {
        walk(testDir);
    }

    files.sort();
    return files;
}

function stableShardForPath(filePath, shardTotal) {
    const relPath = path.relative(process.cwd(), filePath).replace(/\\/g, "/");
    let hash = 0xcbf29ce484222325n;
    const prime = 0x100000001b3n;
    const mask = 0xffffffffffffffffn;
    for (const byte of Buffer.from(relPath, "utf8")) {
        hash ^= BigInt(byte);
        hash = (hash * prime) & mask;
    }
    return Number(hash % BigInt(shardTotal));
}

function snapshotWeightFile() {
    return path.join(__dirname, "fourslash-snapshot.json");
}

// Compact-snapshot bucket names, single-sourced (issue #17010). File buckets
// hold bare paths (both are assertion-passes); row buckets hold failure-family
// detail objects. Everything that splits results by outcome iterates these.
const SNAPSHOT_FILE_BUCKETS = ["pass", "slow"];
const SNAPSHOT_ROW_BUCKETS = ["fail", "timeout", "unrun"];

function resultRowsForWeights(parsed) {
    // Legacy uncollapsed snapshot kept a full per-test result array.
    if (Array.isArray(parsed.results)) {
        return parsed.results;
    }
    // Compact snapshot: `weights` is a {file: elapsedMs} map covering every
    // completed test (pass + slow), and the failure-family arrays (`fail`,
    // `timeout`, `unrun`) carry their rows with `elapsed` + `timedOut` so the
    // timeout bias still applies. Combine them so the LPT balancer sees a
    // weight for ~every test rather than the handful in `summary.slowest`.
    // Before this, collapsing `pass` to bare strings (#13274) left only ~10
    // weighted tests and silently degraded weighted sharding to near-uniform
    // assignment.
    const rows = [];
    if (parsed.weights && typeof parsed.weights === "object" && !Array.isArray(parsed.weights)) {
        for (const [file, elapsed] of Object.entries(parsed.weights)) {
            rows.push({ file, elapsed });
        }
    }
    // `fail` predates the split into fail/timeout/unrun; read all of them so
    // both old and new snapshots contribute their weighted rows.
    for (const key of SNAPSHOT_ROW_BUCKETS) {
        if (Array.isArray(parsed[key])) {
            rows.push(...parsed[key]);
        }
    }
    if (rows.length > 0) {
        return rows;
    }
    // Fallback for snapshots predating the `weights` map.
    if (Array.isArray(parsed.summary?.slowest)) {
        return parsed.summary.slowest;
    }
    return [];
}

function indentJson(value, spaces) {
    const prefix = " ".repeat(spaces);
    return JSON.stringify(value, null, 2)
        .split("\n")
        .map(line => `${prefix}${line}`)
        .join("\n");
}

function stringifyCompactSnapshot(snapshot) {
    const lines = [
        "{",
        `  "timestamp": ${JSON.stringify(snapshot.timestamp)},`,
        `  "summary": ${indentJson(snapshot.summary, 2).trimStart()},`,
    ];

    // File-string buckets (pass, slow): bare paths packed 8/line so the
    // full-corpus set stays well under the 2000-line file cap (#13274).
    for (const bucket of SNAPSHOT_FILE_BUCKETS) {
        const entries = (snapshot[bucket] || []).map(file => JSON.stringify(file));
        lines.push(`  "${bucket}": [`);
        for (let i = 0; i < entries.length; i += 8) {
            const chunk = entries.slice(i, i + 8).join(", ");
            const comma = i + 8 < entries.length ? "," : "";
            lines.push(`    ${chunk}${comma}`);
        }
        lines.push("  ],");
    }

    // Failure-family object buckets (fail, timeout, unrun): small enough to
    // pretty-print in full.
    for (const bucket of SNAPSHOT_ROW_BUCKETS) {
        lines.push(`  "${bucket}": ${indentJson(snapshot[bucket] || [], 2).trimStart()},`);
    }

    // Per-test timings as a compact {file: ms} map. Packed many entries per
    // line so the full-corpus weight set stays well under the 2000-line file
    // cap (the reason #13274 collapsed the old per-test result array).
    lines.push('  "weights": {');
    const weightEntries = Object.entries(snapshot.weights || {})
        .map(([file, ms]) => `${JSON.stringify(file)}: ${ms}`);
    for (let i = 0; i < weightEntries.length; i += 16) {
        const chunk = weightEntries.slice(i, i + 16).join(", ");
        const comma = i + 16 < weightEntries.length ? "," : "";
        lines.push(`    ${chunk}${comma}`);
    }

    lines.push(
        "  }",
        "}",
        "",
    );
    return lines.join("\n");
}

// -----------------------------------------------------------------------------
// Outcome taxonomy (issue #17010)
//
// Every executed test lands in exactly one bucket. "Completed but slow" and
// "never ran" are split out from the pass/fail axis so the published figure
// tracks compiler correctness, not machine load:
//
//   pass    — assertions passed, within the wall-clock budget
//   slow    — assertions passed, but overran the budget (harness perf signal)
//   fail    — assertions failed (a genuine correctness result)
//   timeout — did not complete (bridge timeout, or in-flight when a worker died)
//   unrun   — queued behind a dead worker; never started
//
// `passed` for reporting = pass + slow, because a slow completion is still a
// correctness pass. Only fail/timeout/unrun are non-passing; unrun additionally
// means the run was incomplete.
function summarizeResults(testResults) {
    const counts = { passed: 0, slow: 0, failed: 0, xfailed: 0, timedOut: 0, unrun: 0 };
    for (const r of testResults || []) {
        switch (r.status) {
            case "pass": counts.passed++; break;
            case "slow": counts.slow++; break;
            case "fail": counts.failed++; break;
            case "xfail": counts.xfailed++; break;
            case "timeout": counts.timedOut++; break;
            case "unrun": counts.unrun++; break;
            default: break;
        }
    }
    return counts;
}

// Assertion-passes (load-independent). What the README/CI floor publish.
function reportedPassCount(counts) {
    return counts.passed + counts.slow;
}

// Tests that produced a verdict (everything except the ones that never ran).
function executedCount(counts) {
    return counts.passed + counts.slow + counts.failed + counts.xfailed + counts.timedOut;
}

// A run is "bad" (non-zero exit) on any genuine failure, non-completion, or
// abandoned test. Slowness alone never fails the run — that was the load
// dependence in #17010.
function runFailedCount(counts) {
    return counts.failed + counts.timedOut + counts.unrun;
}

// Split per-test results into the compact snapshot buckets. `pass`/`slow` are
// bare file paths; the failure family keeps its detail rows. `weights` covers
// every completed test (pass + slow) — the slow ones are exactly what the LPT
// balancer most needs, so they must not be dropped.
function classifySnapshotBuckets(jsonResults) {
    const filesFor = status => jsonResults
        .filter(r => r.status === status)
        .map(r => r.file);
    const rowsFor = status => jsonResults
        .filter(r => r.status === status)
        .map(r => {
            const row = {
                file: r.file,
                name: r.name,
                status: r.status,
                timedOut: r.timedOut || false,
                output: r.firstFailure || "",
            };
            if (r.bucket) row.bucket = r.bucket;
            if (r.elapsed !== undefined) row.elapsed = r.elapsed;
            return row;
        });
    const buckets = {};
    for (const status of SNAPSHOT_FILE_BUCKETS) buckets[status] = filesFor(status);
    for (const status of SNAPSHOT_ROW_BUCKETS) buckets[status] = rowsFor(status);
    // Weights cover every completed test (pass + slow) — the slow ones are
    // exactly what the LPT balancer most needs, so they must not be dropped.
    // This leans on the invariant that the file-form buckets are precisely the
    // completed/passing outcomes; if a future passing outcome ever serialized
    // as a detail row, this filter would need its own explicit status list.
    buckets.weights = Object.fromEntries(
        jsonResults
            .filter(r => SNAPSHOT_FILE_BUCKETS.includes(r.status) && Number.isFinite(Number(r.elapsed)))
            .map(r => [r.file, Number(r.elapsed)]),
    );
    return buckets;
}

// When a test timed out at its CI cap (TSZ_CI_FOURSLASH_TIMEOUT_MS,
// typically 60s), the recorded `elapsed` is truncated to the cap.
// The real cost is at least the cap and probably more — without this
// adjustment the LPT balancer underestimates these tests and may
// schedule two timeouts in adjacent shards. Bias by 1.5x cap to keep
// scheduling pessimistic without overweighting recoverable slowness.
const TIMEOUT_WEIGHT_BIAS_MS = 60_000 * 1.5;

function loadHistoricalWeights() {
    const weightFile = snapshotWeightFile();
    if (!fs.existsSync(weightFile)) return new Map();

    try {
        const parsed = JSON.parse(fs.readFileSync(weightFile, "utf8"));
        const weights = new Map();
        for (const result of resultRowsForWeights(parsed)) {
            if (!result || typeof result.file !== "string") continue;
            const elapsed = Number(result.elapsed || 0);
            if (!Number.isFinite(elapsed) || elapsed <= 0) continue;

            // Tests that timed out report `elapsed` at-or-near the cap, but
            // their true cost is unbounded. Bias to TIMEOUT_WEIGHT_BIAS_MS
            // so the LPT balancer doesn't schedule two timeouts adjacently.
            const isTimeout = result.timedOut === true || result.status === "timeout";
            const weight = isTimeout
                ? Math.max(elapsed, TIMEOUT_WEIGHT_BIAS_MS)
                : elapsed;
            weights.set(result.file.replace(/\\/g, "/"), weight);
        }
        return weights;
    } catch (err) {
        console.warn(`warning: failed to read fourslash historical weights: ${err.message}`);
        return new Map();
    }
}

// Median of known weights — used as the default for tests not in the
// snapshot (e.g. newly added tests). Hardcoded fallback was 100ms,
// but median fourslash test is ~422ms (snapshot 2026-05-12), so 100ms
// systematically under-weights new tests and clusters them onto early
// shards in the LPT pass.
function defaultUnknownWeight(weights) {
    if (weights.size === 0) return 100;
    const sorted = [...weights.values()].sort((a, b) => a - b);
    return sorted[Math.floor(sorted.length / 2)];
}

function weightedShardTests(testFiles, shardId, shardTotal) {
    const weights = loadHistoricalWeights();
    if (weights.size === 0) {
        return testFiles.filter(file => stableShardForPath(file, shardTotal) === shardId);
    }

    const unknownWeight = defaultUnknownWeight(weights);
    const shards = Array.from({ length: shardTotal }, () => ({ totalWeight: 0, tests: [] }));
    const weightedTests = testFiles.map(file => {
        const relPath = file.replace(/\\/g, "/");
        return {
            file,
            relPath,
            weight: weights.get(relPath) || unknownWeight,
        };
    });

    weightedTests.sort((a, b) => {
        const byWeight = b.weight - a.weight;
        return byWeight !== 0 ? byWeight : a.relPath.localeCompare(b.relPath);
    });

    for (const test of weightedTests) {
        let best = 0;
        for (let i = 1; i < shards.length; i++) {
            if (
                shards[i].totalWeight < shards[best].totalWeight ||
                (shards[i].totalWeight === shards[best].totalWeight && shards[i].tests.length < shards[best].tests.length)
            ) {
                best = i;
            }
        }
        shards[best].tests.push(test);
        shards[best].totalWeight += test.weight;
    }

    return shards[shardId].tests.map(test => test.file);
}

function slowestResults(testResults, limit = 10) {
    return [...testResults]
        .filter(r => Number.isFinite(Number(r.elapsed)))
        .sort((a, b) => (b.elapsed || 0) - (a.elapsed || 0))
        .slice(0, limit)
        .map(r => ({
            file: r.file,
            name: path.basename(r.file, ".ts"),
            status: r.status,
            timedOut: r.timedOut || false,
            elapsed: r.elapsed || 0,
        }));
}

// =============================================================================
// Sequential runner (fallback)
// =============================================================================

async function runSequential(opts, testsToRun) {
    const tsDir = process.cwd();
    const { TszServerBridge, createTszAdapterFactory } = require("./tsz-adapter.cjs");

    setupGlobals(tsDir);
    const { ts, Harness, FourSlash, HarnessLS, SessionClient } = loadHarnessModules(tsDir);

    const bridge = new TszServerBridge(opts.tszServerBinary);
    await bridge.start();

    const TszAdapter = createTszAdapterFactory(ts, Harness, SessionClient, bridge);
    patchTestState(FourSlash, TszAdapter);
    patchSessionClient(SessionClient, ts);

    const testType = 1; // FourSlashTestType.Server — tsz-server talks over stdio
    let passed = 0;
    let slow = 0;
    let failed = 0;
    let xfailed = 0;
    const errors = [];
    const testResults = [];

    for (let i = 0; i < testsToRun.length; i++) {
        const testFile = testsToRun[i];
        const testName = path.basename(testFile, ".ts");
        const startTime = Date.now();

        if (opts.verbose) {
            process.stdout.write(`[${i + 1}/${testsToRun.length}] ${testName}... `);
        }

        try {
            globalThis.__tszCurrentFourslashTestFile = testFile;
            const basePath = path.dirname(testFile);
            const content = Harness.IO.readFile(testFile);
            if (content == null) throw new Error(`Could not read test file: ${testFile}`);
            FourSlash.runFourSlashTestContent(basePath, testType, content, testFile);
            const elapsed = Date.now() - startTime;
            // Assertions passed. Overrunning the wall-clock budget is a "slow"
            // performance observation about the harness, not a correctness
            // failure, so it gets its own bucket instead of being thrown as a
            // failure (issue #17010).
            if (elapsed > opts.testTimeout) {
                slow++;
                testResults.push({ file: testFile, status: "slow", timedOut: false, error: null, elapsed });
                if (opts.verbose) {
                    console.log(`\x1b[33mSLOW\x1b[0m (${elapsed}ms)`);
                }
            } else {
                passed++;
                testResults.push({ file: testFile, status: "pass", timedOut: false, error: null, elapsed });
                if (opts.verbose) {
                    console.log(`\x1b[32mPASS\x1b[0m (${elapsed}ms)`);
                }
            }
            if (!opts.verbose && (passed + slow + failed + xfailed) % 50 === 0) {
                process.stdout.write(`\r  Progress: ${passed + slow + failed + xfailed}/${testsToRun.length} (${passed} passed, ${slow} slow, ${failed} failed${xfailed > 0 ? `, ${xfailed} xfailed` : ""})`);
            }
        } catch (err) {
            const elapsed = Date.now() - startTime;
            const errMsg = err.message || String(err);
            if (isBaselineOnlyFailure(errMsg)) {
                passed++;
                testResults.push({ file: testFile, status: "pass", timedOut: false, error: null, elapsed });
                if (opts.verbose) {
                    console.log(`\x1b[36mBASELINE\x1b[0m (${elapsed}ms)`);
                } else if ((passed + slow + failed + xfailed) % 50 === 0) {
                    process.stdout.write(`\r  Progress: ${passed + slow + failed + xfailed}/${testsToRun.length} (${passed} passed, ${slow} slow, ${failed} failed${xfailed > 0 ? `, ${xfailed} xfailed` : ""})`);
                }
                continue;
            }

            // A did-not-complete timeout (bridge "Timeout") is its own outcome
            // ("timeout"); everything else is a genuine assertion failure
            // ("fail"). main re-derives the tally from testResults, so only the
            // failed counter (read by the progress line) is tracked here.
            const isTimeout = errMsg.includes("Timeout");
            if (!isTimeout) failed++;
            errors.push({ file: testFile, error: errMsg, timedOut: isTimeout });
            testResults.push({ file: testFile, status: isTimeout ? "timeout" : "fail", timedOut: isTimeout, error: errMsg, elapsed });

            if (opts.verbose) {
                const tag = isTimeout ? "\x1b[33mTIMEOUT\x1b[0m" : "\x1b[31mFAIL\x1b[0m";
                console.log(`${tag} (${elapsed}ms)`);
                console.log(`    ${errMsg.split("\n")[0]}`);
            }
        }
    }

    bridge.shutdown();
    // main derives the authoritative tally from testResults (summarizeResults);
    // the counters above exist only for this function's live progress line.
    return { errors, testResults };
}

function setupGlobals(tsDir) {
    try {
        const chai = require(path.join(tsDir, "node_modules/chai"));
        global.assert = chai.assert;
    } catch (e) {
        const nodeAssert = require("assert");
        global.assert = {
            isOk: (val, msg) => nodeAssert.ok(val, msg),
            isTrue: (val, msg) => nodeAssert.strictEqual(val, true, msg),
            isFalse: (val, msg) => nodeAssert.strictEqual(val, false, msg),
            equal: (a, b, msg) => nodeAssert.strictEqual(a, b, msg),
            deepEqual: (a, b, msg) => nodeAssert.deepStrictEqual(a, b, msg),
            isNotNull: (val, msg) => nodeAssert.notStrictEqual(val, null, msg),
            isNull: (val, msg) => nodeAssert.strictEqual(val, null, msg),
            isUndefined: (val, msg) => nodeAssert.strictEqual(val, undefined, msg),
            isDefined: (val, msg) => nodeAssert.notStrictEqual(val, undefined, msg),
            lengthOf: (obj, len, msg) => nodeAssert.strictEqual(obj.length, len, msg),
            ...nodeAssert,
        };
    }

    global.describe = function(name, fn) { fn(); };
    global.it = function(name, fn) { fn(); };
    global.beforeEach = function(fn) {};
    global.afterEach = function(fn) {};
    global.before = function(fn) {};
    global.after = function(fn) {};
}

function loadHarnessModules(tsDir) {
    const builtDir = path.join(tsDir, "built/local");
    const ts = require(path.join(builtDir, "harness/_namespaces/ts.js"));
    // Accept the fourslash synthetic paths under testType=Server. The harness
    // asserts canWatchDirectoryOrFilePath(...) on every input file/symlink
    // directory, which rejects the synthetic roots used by fourslash tests
    // (e.g. `/tests/cases/fourslash/...`). Force the predicate to true so
    // Server-mode can run without rewriting every fixture's file path.
    try {
        if (typeof ts.canWatchDirectoryOrFilePath === "function") {
            ts.canWatchDirectoryOrFilePath = () => true;
        }
    } catch { /* best-effort */ }
    try {
        const watchUtils = require(path.join(builtDir, "harness/watchUtils.js"));
        if (watchUtils && typeof watchUtils.ensureWatchablePath === "function") {
            watchUtils.ensureWatchablePath = () => {};
        }
    } catch { /* best-effort */ }
    const Harness = require(path.join(builtDir, "harness/_namespaces/Harness.js"));
    const FourSlash = require(path.join(builtDir, "harness/_namespaces/FourSlash.js"));
    const HarnessLS = require(path.join(builtDir, "harness/_namespaces/Harness.LanguageService.js"));
    const clientModule = require(path.join(builtDir, "harness/client.js"));
    return { ts, Harness, FourSlash, HarnessLS, SessionClient: clientModule.SessionClient };
}

function patchTestState(FourSlash, TszAdapter) {
    const TestState = FourSlash.TestState;
    if (!TestState) throw new Error("Could not find TestState in FourSlash module");
    TestState.prototype.getLanguageServiceAdapter = function(testType, cancellationToken, compilationOptions) {
        const adapter = new TszAdapter(cancellationToken, compilationOptions);
        // See the matching comment in test-worker-patch-test-state.cjs: this
        // runner hardcodes testType=Server for every fixture (tsz-server only
        // talks over stdio), so testType itself can't distinguish them here —
        // use the file path the same way upstream's own FourSlashRunner does
        // (non-recursive enumeration split by `tests/cases/fourslash` vs
        // `tests/cases/fourslash/server`). A real tsserver Session defaults
        // its project format options to `getDefaultFormatCodeSettings(this.host.newLine)`,
        // and the harness's fake server host hardcodes that newLine to "\r\n"
        // (harnessNewLine) regardless of OS — testType=Native gets "\n"
        // directly via `ts.testFormatSettings` instead, which the wire
        // protocol has no field to carry for testType=Server. Reproduce that
        // one default only for `fourslash/server/` fixtures.
        const currentTestFile = String(globalThis.__tszCurrentFourslashTestFile || "");
        if (currentTestFile.split(path.sep).join("/").includes("/fourslash/server/")) {
            adapter.getLanguageService().setFormattingOptions({ newLineCharacter: "\r\n" });
        }
        return adapter;
    };

    // --- Patches for SourceFile/Program access ---
    //
    // Our adapter uses a SessionClient (server protocol); testType=Server is
    // set at dispatch. We keep these stubs for callers that reach for
    // getProgram()/getSourceFile()/getChecker() — with the real Program
    // living in tsz-server (another process, Rust), the in-harness handles
    // are not available. The checkPostEditInvariants implementation performs
    // a protocol-level sanity check (getSyntacticDiagnostics round-trip) so
    // that parse/incremental regressions in tsz-server still surface as
    // fourslash failures.

    TestState.prototype.checkPostEditInvariants = function() {
        // Upstream invariants compare getSourceFile() / getNonBoundSourceFile()
        // against a reparse of the file's current text. With tsz-server behind
        // the wire protocol we have neither handle available, and the natural
        // substitute — a getSyntacticDiagnostics round-trip after every edit —
        // multiplies test time enough to time out multi-edit tests.
        //
        // Remaining post-edit protection: edit-batch-final responses that the
        // test already issues (e.g. completions, diagnostics at the end) will
        // still fail if tsz-server's incremental state is broken, so parse-
        // corruption bugs still surface, just less eagerly. A proper
        // tsz/postEditInvariants server endpoint is the right follow-up.
    };

    TestState.prototype.getChecker = function() {
        const program = this.getProgram();
        if (!program) return undefined;
        const checker = program.getTypeChecker();
        if (!checker) return undefined;
        return this._checker || (this._checker = checker);
    };

    TestState.prototype.getSourceFile = function() {
        const program = this.getProgram();
        if (!program) return undefined;
        const fileName = this.activeFile.fileName;
        return program.getSourceFile(fileName);
    };

    const originalGetNode = TestState.prototype.getNode;
    TestState.prototype.getNode = function() {
        const sf = this.getSourceFile();
        if (!sf) return undefined;
        return originalGetNode.call(this);
    };

    const _origGetProgram = TestState.prototype.getProgram;
    TestState.prototype.getProgram = function() {
        if (!this._program) {
            this._program = this.languageService.getProgram() || "missing";
        }
        if (this._program === "missing") {
            if (!this._programStub) {
                const compilationOptions = this.compilationOptions || {};
                this._programStub = {
                    getCompilerOptions: function() { return compilationOptions; },
                    getTypeChecker: function() { return undefined; },
                    getSourceFile: function() { return undefined; },
                    getSourceFiles: function() { return []; },
                    getCurrentDirectory: function() { return "/"; },
                    getConfigFileParsingDiagnostics: function() { return []; },
                };
            }
            return this._programStub;
        }
        return this._program;
    };
}


// =============================================================================
// Parallel runner
// =============================================================================

function distributeTests(tests, numWorkers) {
    const chunks = Array.from({ length: numWorkers }, () => []);
    for (let i = 0; i < tests.length; i++) {
        chunks[i % numWorkers].push(tests[i]);
    }
    return chunks.filter(c => c.length > 0);
}

async function runParallel(opts, testsToRun) {
    const tsDir = process.cwd();
    const numWorkers = Math.min(opts.workers, testsToRun.length);
    const chunks = distributeTests(testsToRun, numWorkers);

    // Wall-clock timeout per test: if a worker sends no result for this long, kill it.
    // This catches infinite loops in the Rust server that the per-request Atomics.wait
    // timeout (30s) cannot fully guard against (a test may make dozens of requests).
    const WORKER_WATCHDOG_MS = opts.testTimeout * 4; // 100s default (4x the 25s per-test timeout)

    console.log(`  Spawning ${chunks.length} workers (timeout: ${opts.testTimeout}ms, mem limit: ${opts.memoryLimitMB}MB)...`);

    let passed = 0;
    let slow = 0;
    let failed = 0;
    let xfailed = 0;
    let timedOut = 0;
    let unrun = 0;
    let completed = 0;
    let bridgeRestarts = 0;
    let memoryWarnings = 0;
    const errors = [];
    const testResults = [];
    const workerStats = [];
    const workerFile = path.join(__dirname, "test-worker.cjs");

    // Track per-worker status for crash recovery
    const workerProgress = new Map(); // workerId -> { total, completed }
    // Track last activity time per worker for watchdog
    const workerLastActivity = new Map(); // workerId -> timestamp

    return new Promise((resolve) => {
        let activeWorkers = chunks.length;
        let lastProgressLen = 0;

        function printProgress() {
            const total = testsToRun.length;
            const done = passed + slow + failed + xfailed + timedOut + unrun;
            const msg = `\r  Progress: ${done}/${total} (${passed} passed${slow > 0 ? `, ${slow} slow` : ""}, ${failed} failed${xfailed > 0 ? `, ${xfailed} xfailed` : ""}${timedOut > 0 ? `, ${timedOut} timeout` : ""}${unrun > 0 ? `, ${unrun} unrun` : ""}) [${activeWorkers} workers]`;
            const padded = msg + " ".repeat(Math.max(0, lastProgressLen - msg.length));
            process.stdout.write(padded);
            lastProgressLen = msg.length;
        }

        function onWorkerDone() {
            activeWorkers--;
            if (activeWorkers === 0) {
                if (!opts.verbose) printProgress();
                clearInterval(watchdog);
                // Counters above feed only the live progress line; main derives
                // the authoritative tally from testResults (summarizeResults).
                resolve({ errors, testResults, bridgeRestarts, memoryWarnings, workerStats });
            }
        }

        // Map worker index -> child process for watchdog kill
        const workerChildren = new Map();
        const workerStderr = new Map();
        const MAX_WORKER_STDERR = 8192;

        function appendWorkerStderr(workerId, chunk) {
            let stderr = (workerStderr.get(workerId) || "") + chunk.toString("utf8");
            if (stderr.length > MAX_WORKER_STDERR) {
                stderr = stderr.slice(-MAX_WORKER_STDERR);
            }
            workerStderr.set(workerId, stderr);
        }

        function workerStderrTail(workerId) {
            const stderr = (workerStderr.get(workerId) || "").trim();
            if (!stderr) return "";
            return stderr.split("\n").slice(-40).join("\n");
        }

        for (let i = 0; i < chunks.length; i++) {
            const child = fork(workerFile, [], {
                stdio: ["pipe", "pipe", "pipe", "ipc"],
                // Set max old space to worker memory limit to prevent V8 OOM
                execArgv: [`--max-old-space-size=${opts.memoryLimitMB}`],
            });

            workerChildren.set(i, child);
            workerProgress.set(i, { total: chunks[i].length, completed: 0 });
            workerLastActivity.set(i, Date.now());

            // Suppress child stdout and retain stderr tails for crash diagnostics.
            child.stdout.on("data", () => {});
            child.stderr.on("data", (chunk) => appendWorkerStderr(i, chunk));

            child.on("message", (msg) => {
                workerLastActivity.set(i, Date.now());
                if (msg.type === "ready") {
                    // Worker initialized
                } else if (msg.type === "start") {
                    if (opts.verbose || process.env.FOURSLASH_LOG_START === "1") {
                        console.log(`  [W${msg.workerId}] START ${msg.testName} (${msg.testFile})`);
                    }
                } else if (msg.type === "result") {
                    if (msg.passed) {
                        // Assertions passed. `slow` means it overran the
                        // wall-clock budget — a harness perf signal, still a
                        // pass for correctness (issue #17010).
                        if (msg.slow) {
                            slow++;
                            testResults.push({ file: msg.testFile, status: "slow", timedOut: false, error: null, elapsed: msg.elapsed });
                        } else {
                            passed++;
                            testResults.push({ file: msg.testFile, status: "pass", timedOut: false, error: null, elapsed: msg.elapsed });
                        }
                    } else if (msg.xfailed) {
                        xfailed++;
                        testResults.push({ file: msg.testFile, status: "xfail", timedOut: false, error: msg.error || null, elapsed: msg.elapsed });
                    } else {
                        if (isBaselineOnlyFailure(msg.error)) {
                            passed++;
                            testResults.push({ file: msg.testFile, status: "pass", timedOut: false, error: null, elapsed: msg.elapsed });
                            completed++;

                            const wp = workerProgress.get(msg.workerId);
                            if (wp) wp.completed++;

                            if (!opts.verbose && completed % 50 === 0) {
                                printProgress();
                            }
                            return;
                        }
                        // Disjoint: a did-not-complete timeout is counted as
                        // timedOut, an assertion failure as failed.
                        if (msg.timedOut) timedOut++; else failed++;
                        errors.push({ file: msg.testFile, error: msg.error, timedOut: msg.timedOut });
                        testResults.push({ file: msg.testFile, status: msg.timedOut ? "timeout" : "fail", timedOut: msg.timedOut, error: msg.error, elapsed: msg.elapsed });
                    }
                    completed++;

                    const wp = workerProgress.get(msg.workerId);
                    if (wp) wp.completed++;

                    if (opts.verbose) {
                        const status = msg.passed
                            ? (msg.slow ? `\x1b[33mSLOW\x1b[0m` : `\x1b[32mPASS\x1b[0m`)
                            : msg.xfailed
                            ? `\x1b[36mXFAIL\x1b[0m`
                            : msg.timedOut
                            ? `\x1b[33mTIMEOUT\x1b[0m`
                            : `\x1b[31mFAIL\x1b[0m`;
                        const elapsed = msg.elapsed ? ` (${msg.elapsed}ms)` : "";
                        console.log(`  [W${msg.workerId}] ${msg.testName} ${status}${elapsed}`);
                        if (!msg.passed && !msg.xfailed) {
                            if (process.env.FOURSLASH_FULL_ERROR) {
                                console.log(msg.error);
                            } else {
                                console.log(`    ${msg.error.split("\n")[0]}`);
                            }
                        }
                    } else if (completed % 50 === 0) {
                        printProgress();
                    }
                } else if (msg.type === "done") {
                    if (msg.stats) workerStats.push({ workerId: msg.workerId, ...msg.stats });
                    onWorkerDone();
                } else if (msg.type === "memory_warning") {
                    memoryWarnings++;
                    if (opts.verbose) {
                        console.log(`  [W${msg.workerId}] \x1b[33mMEMORY WARNING\x1b[0m RSS: ${(msg.rss / 1024 / 1024).toFixed(0)}MB`);
                    }
                } else if (msg.type === "bridge_restart") {
                    bridgeRestarts++;
                    if (opts.verbose) {
                        console.log(`  [W${msg.workerId}] \x1b[33mBRIDGE RESTART\x1b[0m ${msg.reason}`);
                    }
                } else if (msg.type === "error") {
                    const stderr = workerStderrTail(i);
                    console.error(`  \x1b[31mWorker ${i} error:\x1b[0m ${msg.error}${stderr ? `\n${stderr}` : ""}`);
                }
            });

            child.on("exit", (code, signal) => {
                workerChildren.delete(i);
                if ((code !== 0 && code !== null) || signal !== null) {
                    // Worker crashed (likely OOM killed, segfault, or watchdog kill)
                    const wp = workerProgress.get(i);
                    const remaining = wp ? wp.total - wp.completed : 0;
                    if (remaining > 0) {
                        const reason = signal === "SIGKILL" ? "OOM killed"
                            : signal === "SIGTERM" ? "watchdog killed (stuck test)"
                            : signal !== null ? `signal ${signal}`
                            : `exit code ${code}`;
                        const stderr = workerStderrTail(i);
                        console.error(`\n  \x1b[31mWorker ${i} crashed (${reason}), ${remaining} tests lost\x1b[0m`);
                        if (stderr) {
                            console.error(`  Worker ${i} stderr tail:\n${stderr}`);
                        }
                        // The test in flight when the worker died did not
                        // complete → "timeout". The rest were still queued and
                        // never started → "unrun". Conflating the two (the old
                        // "all timeout") overstated genuine non-completions and
                        // hid that the run was incomplete (issue #17010).
                        for (let j = wp.completed; j < wp.total; j++) {
                            const inFlight = j === wp.completed;
                            const detail = stderr ? `Worker crashed (${reason})\n${stderr}` : `Worker crashed (${reason})`;
                            const error = inFlight
                                ? detail
                                : `Not run — worker ${i} died before this test started (${reason})`;
                            errors.push({
                                file: chunks[i][j],
                                error,
                                timedOut: inFlight,
                                unrun: !inFlight,
                            });
                            testResults.push({
                                file: chunks[i][j],
                                status: inFlight ? "timeout" : "unrun",
                                timedOut: inFlight,
                                error,
                                elapsed: 0,
                            });
                            if (inFlight) timedOut++; else unrun++;
                        }
                    }
                    workerStderr.delete(i);
                    onWorkerDone();
                } else {
                    workerStderr.delete(i);
                }
            });

            // Send config to worker
            child.send({
                type: "config",
                testFiles: chunks[i],
                tszServerBinary: opts.tszServerBinary,
                tsDir,
                workerId: i,
                testTimeout: opts.testTimeout,
                memoryThreshold: opts.memoryLimitMB * 1024 * 1024,
            });
        }

        // Watchdog: periodically check if any worker is stuck (no messages for WORKER_WATCHDOG_MS)
        const watchdog = setInterval(() => {
            const now = Date.now();
            for (const [wid, lastTime] of workerLastActivity.entries()) {
                if (now - lastTime > WORKER_WATCHDOG_MS && workerChildren.has(wid)) {
                    const child = workerChildren.get(wid);
                    const wp = workerProgress.get(wid);
                    const currentTest = wp ? chunks[wid][wp.completed] : "unknown";
                    console.error(`\n  \x1b[33mWatchdog: Worker ${wid} stuck for ${((now - lastTime) / 1000).toFixed(0)}s on ${path.basename(currentTest || "unknown")}, killing...\x1b[0m`);
                    child.kill("SIGTERM");
                    // Give it 5s to exit gracefully, then force kill
                    setTimeout(() => {
                        try { child.kill("SIGKILL"); } catch {}
                    }, 5000);
                }
            }
            // Stop watchdog when all workers are done
            if (workerChildren.size === 0) {
                clearInterval(watchdog);
            }
        }, 10000); // Check every 10 seconds
    });
}

// =============================================================================
// Main
// =============================================================================

async function main() {
    const opts = parseArgs();
    const tsDir = process.cwd();

    if (!fs.existsSync(path.join(tsDir, "Herebyfile.mjs"))) {
        console.error("Error: Must be run from the TypeScript directory");
        console.error(`  Current directory: ${tsDir}`);
        process.exit(2);
    }

    const builtDir = path.join(tsDir, "built/local");
    if (!fs.existsSync(path.join(builtDir, "harness/fourslashImpl.js"))) {
        console.error("Error: TypeScript harness not built. Run: npx hereby tests --no-bundle");
        process.exit(2);
    }

    if (!fs.existsSync(opts.tszServerBinary)) {
        console.error(`Error: tsz-server binary not found at: ${opts.tszServerBinary}`);
        process.exit(2);
    }

    // Discover tests
    const testFiles = discoverTests(opts.testDir, opts.filter);
    const totalAvailable = testFiles.length;
    let testsToRun = testFiles;
    // --shard=I/N uses historical timing by default so known slow tests are
    // spread across CI shards and scheduled early within each shard. Applied
    // before --offset/--max so those still trim within the shard if passed.
    if (opts.shardTotal > 0) {
        testsToRun = opts.shardStrategy === "weighted"
            ? weightedShardTests(testFiles, opts.shardId, opts.shardTotal)
            : testFiles.filter(file => stableShardForPath(file, opts.shardTotal) === opts.shardId);
    }
    if (opts.offset > 0) testsToRun = testsToRun.slice(opts.offset);
    if (opts.max > 0) testsToRun = testsToRun.slice(0, opts.max);

    const mode = opts.sequential ? "sequential" : `parallel (${Math.min(opts.workers, testsToRun.length)} workers)`;
    console.log("");
    console.log(`Found ${totalAvailable} test files in ${opts.testDir}`);
    const shardMode = opts.shardTotal > 0 ? ` shard=${opts.shardId}/${opts.shardTotal} strategy=${opts.shardStrategy}` : "";
    console.log(`Running ${testsToRun.length} tests [${mode}]${opts.filter ? ` (filter: "${opts.filter}")` : ""}${shardMode}`);
    console.log("─".repeat(70));

    const startTime = Date.now();
    let results;

    if (opts.sequential) {
        results = await runSequential(opts, testsToRun);
    } else {
        results = await runParallel(opts, testsToRun);
    }

    const { errors } = results;
    // Derive the final tally from the per-test results — the single source of
    // truth — so the summary can never disagree with the recorded buckets.
    const counts = summarizeResults(results.testResults || []);
    const { passed, slow, failed, xfailed, timedOut, unrun } = counts;
    const reportedPassed = reportedPassCount(counts); // pass + slow
    const executed = executedCount(counts);           // excludes unrun
    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);

    // Print summary
    console.log("");
    console.log("─".repeat(70));
    console.log("");
    const parts = [`${passed} passed`];
    if (slow > 0) parts.push(`${slow} slow`);
    parts.push(`${failed} failed`);
    if (xfailed > 0) parts.push(`${xfailed} xfailed`);
    if (timedOut > 0) parts.push(`${timedOut} timed out`);
    if (unrun > 0) parts.push(`${unrun} did not run`);
    console.log(`Results: ${parts.join(", ")} out of ${testsToRun.length} (${elapsed}s)`);

    if (totalAvailable > testsToRun.length) {
        console.log(`  (${totalAvailable - testsToRun.length} tests skipped, ${totalAvailable} total available)`);
    }

    // Pass rate is over the tests that actually produced a verdict (executed),
    // not the full planned set — otherwise an early abort drags the rate down
    // purely because queued tests never ran (issue #17010). Slow completions
    // count as passing. Computed once here; the snapshot summary reuses it.
    const passRate = executed > 0 ? Math.round(reportedPassed / executed * 1000) / 10 : 0;
    console.log(`  Pass rate: ${passRate.toFixed(1)}% (${reportedPassed}/${executed} executed)`);
    if (unrun > 0) {
        console.log(`  \x1b[33m${unrun} test(s) did not run — figure is over executed tests only\x1b[0m`);
    }

    // Extra stats for parallel mode
    if (!opts.sequential && results.bridgeRestarts !== undefined) {
        const statsLine = [];
        if (timedOut > 0) statsLine.push(`${timedOut} timed out`);
        if (unrun > 0) statsLine.push(`${unrun} did not run`);
        if (results.bridgeRestarts > 0) statsLine.push(`${results.bridgeRestarts} bridge restarts`);
        if (results.memoryWarnings > 0) statsLine.push(`${results.memoryWarnings} memory warnings`);
        if (statsLine.length > 0) {
            console.log(`  Health: ${statsLine.join(", ")}`);
        }

        // Worker memory summary
        if (results.workerStats && results.workerStats.length > 0) {
            const maxRss = Math.max(...results.workerStats.map(s => s.peakRss || 0));
            if (maxRss > 0) {
                console.log(`  Peak worker RSS: ${(maxRss / 1024 / 1024).toFixed(0)}MB`);
            }
        }
    }

    if (errors.length > 0 && !opts.verbose) {
        console.log("");
        console.log(`First ${errors.length} non-passing:`);
        for (const { file, error, timedOut: to, unrun: didNotRun } of errors.slice(0, 20)) {
            const icon = didNotRun ? "\x1b[90m∅\x1b[0m" : to ? "\x1b[33m⏱\x1b[0m" : "\x1b[31m✗\x1b[0m";
            console.log(`  ${icon} ${path.basename(file, ".ts")}: ${error.split("\n")[0].substring(0, 100)}`);
        }
        if (errors.length > 20) {
            console.log(`  ... and ${errors.length - 20} more`);
        }
    }

    const slowest = slowestResults(results.testResults || [], 10);
    if (slowest.length > 0 && !opts.verbose) {
        console.log("");
        console.log(`Slowest ${slowest.length} tests:`);
        for (const test of slowest) {
            const status = test.status === "pass" ? "PASS" : test.status.toUpperCase();
            console.log(`  ${String(test.elapsed).padStart(6)}ms ${status.padEnd(7)} ${test.name}`);
        }
    }

    // Dump all errors to file for analysis (development aid)
    try {
        const errDump = errors.map(({file, error}) => path.basename(file, ".ts") + ": " + error.split("\n")[0]).join("\n");
        require("fs").writeFileSync("/tmp/all-errors.txt", errDump);
    } catch (_) {}

    // Write machine-readable JSON if requested
    if (opts.jsonOut && results.testResults) {
        const FEATURE_PATTERNS = {
            completion: /completion|getCompletions|verifyCompletionList|CompletionEntry/i,
            quickinfo: /quickInfo|quickinfo|QuickInfo/i,
            definition: /definition|goToDefinition|getDefinition/i,
            references: /references|findAllReferences|findReferences/i,
            rename: /rename|getRenameLocations/i,
            "signature-help": /signatureHelp|getSignatureHelp/i,
            formatting: /formatting|format|indent/i,
            "code-fix": /codeFix|codeAction|getCodeFix/i,
            refactor: /refactor|getApplicableRefactors/i,
            navigation: /navigation|navigationBar|navBar/i,
            organize: /organizeImports/i,
        };

        function inferBucket(testFile, errorMsg) {
            const combined = testFile + " " + (errorMsg || "");
            for (const [bucket, pattern] of Object.entries(FEATURE_PATTERNS)) {
                if (pattern.test(combined)) return bucket;
            }
            return "other";
        }

        const jsonResults = results.testResults.map(r => {
            const testName = path.basename(r.file, ".ts");
            const record = {
                file: r.file,
                name: testName,
                status: r.status,
                timedOut: r.timedOut || false,
                bucket: inferBucket(r.file, r.error),
            };
            if (r.error) record.firstFailure = r.error.split("\n")[0].substring(0, 200);
            if (r.elapsed !== undefined) record.elapsed = r.elapsed;
            return record;
        });

        // Sort deterministically by file path
        jsonResults.sort((a, b) => a.file.localeCompare(b.file));

        const total = testsToRun.length;
        // `passed` in the summary is the load-independent correctness figure —
        // assertion-passes = pass + slow — which is what the README and the CI
        // floor read via `.summary.passed`. `slow`/`unrun` are exposed as their
        // own sub-counts so the distinction stays visible (issue #17010); the
        // raw within-budget count is `passed - slow` if ever needed.
        const summary = {
            total,
            passed: reportedPassed,
            slow,
            failed,
            xfailed,
            timedOut,
            unrun,
            shard: opts.shardTotal > 0 ? { index: opts.shardId, count: opts.shardTotal, strategy: opts.shardStrategy } : null,
            slowest,
            passRate,
        };
        const detail = {
            timestamp: new Date().toISOString(),
            summary,
            results: jsonResults,
        };

        const outPath = path.resolve(opts.jsonOut);
        const snapshotOutPath = path.resolve(snapshotWeightFile());
        // The compact snapshot splits results into pass/slow/fail/timeout/unrun
        // buckets and a `weights` map (pass + slow, the timings the LPT balancer
        // needs). classifySnapshotBuckets owns that split.
        const output = outPath === snapshotOutPath
            ? { timestamp: detail.timestamp, summary, ...classifySnapshotBuckets(jsonResults) }
            : detail;
        fs.mkdirSync(path.dirname(outPath), { recursive: true });
        const jsonText = outPath === snapshotOutPath
            ? stringifyCompactSnapshot(output)
            : `${JSON.stringify(output, null, 2)}\n`;
        fs.writeFileSync(outPath, jsonText);
        console.log(`\nJSON results written to ${outPath}`);
    }

    // Slowness alone never fails the run (that was the load dependence in
    // #17010); genuine failures, non-completions, and abandoned tests do.
    process.exit(runFailedCount(counts) > 0 ? 1 : 0);
}

// Pure helpers are exported so unit tests exercise the real classification
// code rather than a drifting mirror. Only run the suite when invoked directly.
module.exports = {
    resultRowsForWeights,
    loadHistoricalWeights,
    defaultUnknownWeight,
    weightedShardTests,
    stringifyCompactSnapshot,
    classifySnapshotBuckets,
    summarizeResults,
    reportedPassCount,
    executedCount,
    runFailedCount,
    TIMEOUT_WEIGHT_BIAS_MS,
};

if (require.main === module) {
    main().catch(err => {
        console.error("Fatal error:", err);
        process.exit(2);
    });
}
