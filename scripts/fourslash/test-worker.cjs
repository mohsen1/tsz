#!/usr/bin/env node
/**
 * test-worker.js - Child process for parallel fourslash test execution.
 *
 * Spawned by runner.js via child_process.fork(). Each worker:
 * 1. Loads TypeScript harness modules
 * 2. Creates its own TszServerBridge (with its own tsz-server process)
 * 3. Runs assigned tests sequentially with per-test timeout
 * 4. Reports results back to parent via IPC
 * 5. Monitors memory usage and restarts if OOM threshold exceeded
 */

"use strict";

const path = require("path");
const { TszServerBridge, createTszAdapterFactory } = require("./tsz-adapter.cjs");
const { patchSessionClient } = require("./runner-session-client.cjs");

// Per-test timeout (ms) - tests taking longer are killed. Fallback only:
// runner.cjs always passes testTimeout explicitly (see its own default).
const TEST_TIMEOUT_MS = 25000;
// Memory threshold per worker (bytes) - restart bridge if exceeded
const MEMORY_THRESHOLD_BYTES = 512 * 1024 * 1024; // 512MB
// Check memory every N tests
const MEMORY_CHECK_INTERVAL = 25;
// Reset tsz-server session state after each test. Restart only when the bridge
// itself looks unhealthy; process startup dominates fourslash CI wall time.
const RESTART_BRIDGE_EVERY_TEST = false;

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
    // Accept fourslash synthetic paths under testType=Server. Without this,
    // ensureWatchablePath's Debug.assert(canWatchDirectoryOrFilePath(...))
    // rejects the non-OS-rooted paths used by the test fixtures and Server
    // mode aborts before the first request. Setting the predicate to true
    // is a test-harness-only concession.
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

const patchTestState = require("./test-worker-patch-test-state.cjs");


function runSingleTest(FourSlash, Harness, testFile, testType) {
    const basePath = path.dirname(testFile);
    const content = Harness.IO.readFile(testFile);
    if (content == null) throw new Error(`Could not read test file: ${testFile}`);
    FourSlash.runFourSlashTestContent(basePath, testType, content, testFile);
}

/**
 * Run a test and measure its wall-clock cost. Fourslash tests are
 * synchronous, so we can't use setTimeout; the bridge's own per-request
 * timeout (30s) is the hard guard against a genuinely stuck request.
 *
 * A test whose assertions pass but that overran `timeoutMs` is reported as
 * `slow: true` rather than thrown as a failure. "Completed but slow" is a
 * performance observation about the harness under load, not a correctness
 * result about the compiler, so it must not be folded into the failure
 * buckets (issue #17010). Returns { elapsed, slow }.
 */
function runTestWithTimeout(FourSlash, Harness, testFile, testType, timeoutMs) {
    const start = Date.now();
    runSingleTest(FourSlash, Harness, testFile, testType);
    const elapsed = Date.now() - start;
    return { elapsed, slow: elapsed > timeoutMs };
}

async function main() {
    // Wait for config from parent
    const config = await new Promise((resolve) => {
        process.on("message", (msg) => {
            if (msg.type === "config") resolve(msg);
        });
    });

    const { testFiles, tszServerBinary, tsDir, workerId, testTimeout, memoryThreshold } = config;
    const perTestTimeout = testTimeout || TEST_TIMEOUT_MS;
    const memThreshold = memoryThreshold || MEMORY_THRESHOLD_BYTES;

    // Change to TypeScript directory (harness expects it)
    process.chdir(tsDir);

    // Set up globals and load harness
    setupGlobals(tsDir);
    const { ts, Harness, FourSlash, HarnessLS, SessionClient } = loadHarnessModules(tsDir);

    const sleep = (ms) => new Promise(resolve => setTimeout(resolve, ms));
    const startBridgeWithRetries = async (candidateBridge, attempts = 4) => {
        let lastErr;
        for (let attempt = 1; attempt <= attempts; attempt++) {
            try {
                await candidateBridge.start();
                return;
            } catch (err) {
                lastErr = err;
                // Avoid tight spawn loops when the OS is under process pressure.
                if (attempt < attempts) {
                    await sleep(40 * attempt);
                }
            }
        }
        throw lastErr;
    };

    // Start our own tsz-server bridge
    let bridge = new TszServerBridge(tszServerBinary);
    await startBridgeWithRetries(bridge);

    // Create adapter and patch TestState
    let TszAdapter = createTszAdapterFactory(ts, Harness, SessionClient, bridge);
    patchTestState(FourSlash, TszAdapter);
    patchSessionClient(SessionClient, ts);

    const restartBridge = async (reason) => {
        const previousBridge = bridge;
        const nextBridge = new TszServerBridge(tszServerBinary);
        await startBridgeWithRetries(nextBridge);
        bridge = nextBridge;
        TszAdapter = createTszAdapterFactory(ts, Harness, SessionClient, bridge);
        patchTestState(FourSlash, TszAdapter);
        try {
            previousBridge.shutdown();
        } catch { /* ignore */ }
        process.send({ type: "bridge_restart", workerId, reason });
    };

    const testType = 1; // FourSlashTestType.Server — tsz-server talks over stdio

    // Signal ready
    process.send({ type: "ready", workerId });

    // Run assigned tests
    let testsRun = 0;
    for (const testFile of testFiles) {
        const testName = path.basename(testFile, ".ts");
        process.send({ type: "start", workerId, testFile, testName });
        const startTime = Date.now();
        let shouldRestartBridge = RESTART_BRIDGE_EVERY_TEST;
        let restartReason = RESTART_BRIDGE_EVERY_TEST
            ? "per-test isolation"
            : "";

        try {
            const { slow } = runTestWithTimeout(FourSlash, Harness, testFile, testType, perTestTimeout);
            const elapsed = Date.now() - startTime;
            process.send({ type: "result", workerId, testFile, testName, passed: true, slow, elapsed });
        } catch (err) {
            const elapsed = Date.now() - startTime;
            const errMsg = err.message || String(err);
            // A genuine non-completion is signalled by the bridge's own
            // "Timeout" error. Overrunning the wall-clock budget on the
            // success path is "slow" (handled above), not a timeout — so a
            // slow *assertion failure* stays a plain failure here.
            const timedOut = errMsg.includes("Timeout");
            const bridgeLikelyUnhealthy =
                timedOut ||
                errMsg.includes("Stream closed before complete message was read") ||
                errMsg.includes("Unexpected empty response body") ||
                errMsg.includes("Broken pipe");
            if (bridgeLikelyUnhealthy) {
                shouldRestartBridge = true;
                restartReason = `post-failure recovery for ${testName}`;
            }
            process.send({
                type: "result", workerId, testFile, testName,
                passed: false, error: errMsg, elapsed, timedOut,
            });
        }

        testsRun++;
        if (shouldRestartBridge) {
            try {
                await restartBridge(restartReason);
            } catch (restartErr) {
                process.send({
                    type: "error", workerId,
                    error: `Bridge restart failed: ${restartErr.message}`,
                });
            }
        } else {
            try {
                bridge.resetSession();
            } catch (resetErr) {
                try {
                    await restartBridge(`reset recovery after ${testName}: ${resetErr.message}`);
                } catch (restartErr) {
                    process.send({
                        type: "error", workerId,
                        error: `Bridge restart failed after reset failure: ${restartErr.message}`,
                    });
                }
            }
        }

        // Periodic memory check
        if (testsRun % MEMORY_CHECK_INTERVAL === 0) {
            const memUsage = process.memoryUsage();
            const heapUsed = memUsage.heapUsed;
            const rss = memUsage.rss;

            if (rss > memThreshold) {
                // Report memory pressure
                process.send({
                    type: "memory_warning", workerId,
                    rss, heapUsed, threshold: memThreshold,
                });

                // Try to reclaim memory
                if (global.gc) {
                    global.gc();
                }

                // If still over threshold after GC, restart bridge
                const afterGc = process.memoryUsage().rss;
                if (afterGc > memThreshold) {
                    try {
                        await restartBridge(
                            `RSS ${(afterGc / 1024 / 1024).toFixed(0)}MB > ${(memThreshold / 1024 / 1024).toFixed(0)}MB threshold`
                        );
                    } catch (restartErr) {
                        process.send({
                            type: "error", workerId,
                            error: `Bridge restart failed: ${restartErr.message}`,
                        });
                    }
                }
            }
        }
    }

    // Done
    bridge.shutdown();
    const finalMem = process.memoryUsage();
    process.send({
        type: "done", workerId,
        stats: {
            testsRun,
            peakRss: finalMem.rss,
            heapUsed: finalMem.heapUsed,
        },
    });
}

main().catch(err => {
    if (process.send) {
        process.send({ type: "error", error: err.message || String(err) });
    }
    process.exit(1);
});
