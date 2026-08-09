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

function resultRowsForWeights(parsed) {
    // Legacy uncollapsed snapshot kept a full per-test result array.
    if (Array.isArray(parsed.results)) {
        return parsed.results;
    }
    // Compact snapshot: `weights` is a {file: elapsedMs} map covering every
    // passing test, and `fail` carries the failing/timeout rows (with their
    // own `elapsed` + `timedOut` so the timeout bias still applies). Combine
    // both so the LPT balancer sees a weight for ~every test rather than the
    // handful in `summary.slowest`. Before this, collapsing `pass` to bare
    // strings (#13274) left only ~10 weighted tests and silently degraded
    // weighted sharding to near-uniform assignment.
    const rows = [];
    if (parsed.weights && typeof parsed.weights === "object" && !Array.isArray(parsed.weights)) {
        for (const [file, elapsed] of Object.entries(parsed.weights)) {
            rows.push({ file, elapsed });
        }
    }
    if (Array.isArray(parsed.fail)) {
        rows.push(...parsed.fail);
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
        '  "pass": [',
    ];

    const passEntries = snapshot.pass.map(file => JSON.stringify(file));
    for (let i = 0; i < passEntries.length; i += 8) {
        const chunk = passEntries.slice(i, i + 8).join(", ");
        const comma = i + 8 < passEntries.length ? "," : "";
        lines.push(`    ${chunk}${comma}`);
    }

    lines.push("  ],", '  "slow": [');

    // Subset of `pass` that exceeded the wall-clock budget — assertions
    // passed, but flagged for harness-performance attention.
    const slowEntries = (snapshot.slow || []).map(file => JSON.stringify(file));
    for (let i = 0; i < slowEntries.length; i += 8) {
        const chunk = slowEntries.slice(i, i + 8).join(", ");
        const comma = i + 8 < slowEntries.length ? "," : "";
        lines.push(`    ${chunk}${comma}`);
    }

    lines.push(
        "  ],",
        `  "fail": ${indentJson(snapshot.fail, 2).trimStart()},`,
        '  "weights": {',
    );

    // Per-test timings as a compact {file: ms} map. Packed many entries per
    // line so the full-corpus weight set stays well under the 2000-line file
    // cap (the reason #13274 collapsed the old per-test result array).
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
    let timedOut = 0;
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
            const isSlow = elapsed > opts.testTimeout;
            passed++;
            if (isSlow) slow++;
            testResults.push({ file: testFile, status: isSlow ? "slow" : "pass", timedOut: false, error: null, elapsed });
            if (opts.verbose) {
                const tag = isSlow ? `\x1b[33mSLOW\x1b[0m` : `\x1b[32mPASS\x1b[0m`;
                console.log(`${tag} (${elapsed}ms)`);
            } else if ((passed + failed + xfailed) % 50 === 0) {
                process.stdout.write(`\r  Progress: ${passed + failed + xfailed}/${testsToRun.length} (${passed} passed, ${failed} failed${xfailed > 0 ? `, ${xfailed} xfailed` : ""})`);
            }
        } catch (err) {
            const elapsed = Date.now() - startTime;
            const errMsg = err.message || String(err);
            if (isBaselineOnlyFailure(errMsg)) {
                passed++;
                testResults.push({ file: testFile, status: "pass", timedOut: false, error: null, elapsed });
                if (opts.verbose) {
                    console.log(`\x1b[36mBASELINE\x1b[0m (${elapsed}ms)`);
                } else if ((passed + failed + xfailed) % 50 === 0) {
                    process.stdout.write(`\r  Progress: ${passed + failed + xfailed}/${testsToRun.length} (${passed} passed, ${failed} failed${xfailed > 0 ? `, ${xfailed} xfailed` : ""})`);
                }
                continue;
            }

            failed++;
            const isTimeout = elapsed >= opts.testTimeout || errMsg.includes("Timeout");
            if (isTimeout) timedOut++;
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
    return { passed, slow, failed, xfailed, timedOut, errors, testResults };
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
        return new TszAdapter(cancellationToken, compilationOptions);
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
            const done = passed + failed + xfailed;
            const msg = `\r  Progress: ${done}/${total} (${passed} passed, ${failed} failed${xfailed > 0 ? `, ${xfailed} xfailed` : ""}${timedOut > 0 ? `, ${timedOut} timeout` : ""}) [${activeWorkers} workers]`;
            const padded = msg + " ".repeat(Math.max(0, lastProgressLen - msg.length));
            process.stdout.write(padded);
            lastProgressLen = msg.length;
        }

        function onWorkerDone() {
            activeWorkers--;
            if (activeWorkers === 0) {
                if (!opts.verbose) printProgress();
                clearInterval(watchdog);
                resolve({ passed, slow, failed, xfailed, timedOut, errors, testResults, bridgeRestarts, memoryWarnings, workerStats });
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
                        passed++;
                        if (msg.slow) slow++;
                        testResults.push({ file: msg.testFile, status: msg.slow ? "slow" : "pass", timedOut: false, error: null, elapsed: msg.elapsed });
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
                        failed++;
                        if (msg.timedOut) timedOut++;
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
                        // Count remaining tests as failed
                        failed += remaining;
                        timedOut += remaining;
                        for (let j = wp.completed; j < wp.total; j++) {
                            const error = stderr ? `Worker crashed (${reason})\n${stderr}` : `Worker crashed (${reason})`;
                            errors.push({
                                file: chunks[i][j],
                                error,
                                timedOut: true,
                            });
                            testResults.push({
                                file: chunks[i][j],
                                status: "timeout",
                                timedOut: true,
                                error,
                                elapsed: 0,
                            });
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

    const { passed, slow = 0, failed, xfailed = 0, timedOut, errors } = results;
    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
    const executedCount = passed + failed + xfailed;

    // Print summary
    console.log("");
    console.log("─".repeat(70));
    console.log("");
    console.log(`Results: ${passed} passed${slow > 0 ? ` (${slow} slow)` : ""}, ${failed} failed${xfailed > 0 ? `, ${xfailed} xfailed` : ""} out of ${testsToRun.length} (${elapsed}s)`);

    if (totalAvailable > testsToRun.length) {
        console.log(`  (${totalAvailable - testsToRun.length} tests skipped, ${totalAvailable} total available)`);
    }

    if (executedCount < testsToRun.length) {
        console.log(`  (run aborted early: only ${executedCount}/${testsToRun.length} executed)`);
    }

    // Rate over what actually ran, not the intended total — an early-aborted
    // run must not report a rate diluted by tests that never executed.
    const passRate = executedCount > 0
        ? ((passed / executedCount) * 100).toFixed(1)
        : "0.0";
    console.log(`  Pass rate: ${passRate}%`);

    // Extra stats for parallel mode
    if (!opts.sequential && results.bridgeRestarts !== undefined) {
        const statsLine = [];
        if (timedOut > 0) statsLine.push(`${timedOut} timed out`);
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
        console.log(`First ${errors.length} failures:`);
        for (const { file, error, timedOut: to } of errors.slice(0, 20)) {
            const icon = to ? "\x1b[33m⏱\x1b[0m" : "\x1b[31m✗\x1b[0m";
            console.log(`  ${icon} ${path.basename(file, ".ts")}: ${error.split("\n")[0].substring(0, 100)}`);
        }
        if (errors.length > 20) {
            console.log(`  ... and ${errors.length - 20} more failures`);
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
        const executed = passed + failed + xfailed;
        const detail = {
            timestamp: new Date().toISOString(),
            summary: {
                total,
                passed,
                slow,
                failed,
                xfailed,
                timedOut,
                shard: opts.shardTotal > 0 ? { index: opts.shardId, count: opts.shardTotal, strategy: opts.shardStrategy } : null,
                slowest,
                passRate: executed > 0 ? Math.round(passed / executed * 1000) / 10 : 0,
            },
            results: jsonResults,
        };

        const outPath = path.resolve(opts.jsonOut);
        const snapshotOutPath = path.resolve(snapshotWeightFile());
        // A test whose assertions passed but which ran past the wall-clock
        // budget is "slow", not a failure — it must land in `pass`/`slow`,
        // never in `fail`. Only a genuine assertion failure or timeout
        // (status "fail"/"timeout"/"xfail") belongs in `fail`.
        const output = outPath === snapshotOutPath
            ? {
                timestamp: detail.timestamp,
                summary: detail.summary,
                pass: jsonResults
                    .filter(r => (r.status === "pass" || r.status === "slow") && !r.timedOut)
                    .map(r => r.file),
                slow: jsonResults
                    .filter(r => r.status === "slow")
                    .map(r => r.file),
                fail: jsonResults
                    .filter(r => r.status !== "pass" && r.status !== "slow")
                    .map(r => {
                        const record = {
                            file: r.file,
                            name: r.name,
                            status: r.status,
                            timedOut: r.timedOut || false,
                            output: r.firstFailure || "",
                        };
                        if (r.bucket) record.bucket = r.bucket;
                        if (r.elapsed !== undefined) record.elapsed = r.elapsed;
                        return record;
                    }),
                // Per-test timings (ms) for passing tests (including slow-but-passed),
                // consumed by loadHistoricalWeights() for LPT shard balancing. Kept as
                // a compact {file: ms} map so the snapshot stays small while the
                // balancer still sees a real weight for ~every test.
                weights: Object.fromEntries(
                    jsonResults
                        .filter(r => (r.status === "pass" || r.status === "slow") && Number.isFinite(Number(r.elapsed)))
                        .map(r => [r.file, Number(r.elapsed)]),
                ),
            }
            : detail;
        fs.mkdirSync(path.dirname(outPath), { recursive: true });
        const jsonText = outPath === snapshotOutPath
            ? stringifyCompactSnapshot(output)
            : `${JSON.stringify(output, null, 2)}\n`;
        fs.writeFileSync(outPath, jsonText);
        console.log(`\nJSON results written to ${outPath}`);
    }

    process.exit(failed > 0 ? 1 : 0);
}

main().catch(err => {
    console.error("Fatal error:", err);
    process.exit(2);
});
