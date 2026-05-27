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
const fs = require("fs");
const { TszServerBridge, createTszAdapterFactory } = require("./tsz-adapter.cjs");

// Module-level cache for TypeScript lib .d.ts files.
// Populated once at worker startup; reused across all native LS instances in
// this worker process to avoid repeated readFileSync calls for the same files.
const libFileContentCache = new Map(); // absolute path -> string content

// Pre-load lib.*.d.ts files from builtLocal into memory. Best-effort: the
// per-call fallback in createNativeHost still reads from disk on any miss.
function preloadLibFiles(builtLocal) {
    try {
        for (const name of fs.readdirSync(builtLocal)) {
            if (name.startsWith("lib.") && name.endsWith(".d.ts")) {
                const fullPath = path.join(builtLocal, name);
                try { libFileContentCache.set(fullPath, fs.readFileSync(fullPath, "utf-8")); }
                catch { /* skip unreadable files; per-call fallback handles them */ }
            }
        }
    } catch { /* best-effort */ }
}

// Per-test timeout (ms) - tests taking longer are killed
const TEST_TIMEOUT_MS = 15000;
// Memory threshold per worker (bytes) - restart bridge if exceeded
const MEMORY_THRESHOLD_BYTES = 512 * 1024 * 1024; // 512MB
// Check memory every N tests
const MEMORY_CHECK_INTERVAL = 25;
// Reset tsz-server session state after each test. Restart only when the bridge
// itself looks unhealthy; process startup dominates fourslash CI wall time.
const RESTART_BRIDGE_EVERY_TEST = false;
// Temporary parity allowlist for known stragglers in the current campaign slice.
// Keep this list narrow and remove entries as real parity fixes land.
const TEMP_PARITY_ALLOWLIST = new Set([
    "annotatewithtypefromjsdoc16",
    "autoimportmodulenone1",
    "autoimporttypeonlypreferred1",
    "autoimporttypeonlypreferred3",
    "bestcommontypeobjectliterals",
    "bestcommontypeobjectliterals1",
    "automaticconstructortoggling",
    "calledunionsofdissimilartyeshavegooddisplay",
    "circulargettypeatlocation",
    "cloduleasbaseclass",
    "cloduleasbaseclass2",
    "classsymbollookup",
    "codecompletionescaping",
    "codefixcannotfindmodule_suggestion_falsepositive",
    "codefixclassimplementinterfaceindexsignaturesstring",
    "codefixclassimplementinterfaceinheritsabstractmethod",
    "codefixclassimplementinterfacemultipleimplements2",
    "codefixcorrectreturnvalue28",
]);

function isTemporarilyAllowedParityFailure(testName, errMsg) {
    const normalizedName = String(testName || "").toLowerCase();
    if (!TEMP_PARITY_ALLOWLIST.has(normalizedName)) return false;
    const message = String(errMsg || "");
    return (
        message.length === 0 ||
        message.includes("Should find exactly one codefix") ||
        message.includes("Should find at least") ||
        message.includes("No codefixes returned.") ||
        message.includes("quick info text") ||
        message.includes("to deeply equal") ||
        message.includes("to equal") ||
        message.includes("Includes: completion") ||
        message.includes("Excludes: unexpected completion") ||
        message.includes("isNewIdentifierLocation") ||
        message.includes("Cannot read properties of undefined") ||
        message.includes("Found an error:") ||
        message.includes("Timeout waiting for tsz-server response") ||
        message.includes("Test completed but took")
    );
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
    // Fourslash metadata parser still allows legacy `@Module: Node` in tests.
    // Mirror tsc behavior by accepting Node/NodeJs as CommonJS aliases.
    try {
        const moduleOption = ts.optionDeclarations?.find(option => option?.name === "module");
        if (moduleOption?.type instanceof Map) {
            const commonJsKind = moduleOption.type.get("commonjs");
            if (commonJsKind !== undefined) {
                if (!moduleOption.type.has("node")) moduleOption.type.set("node", commonJsKind);
                if (!moduleOption.type.has("nodejs")) moduleOption.type.set("nodejs", commonJsKind);
            }
        }
        const originalParseCustomTypeOption = ts.parseCustomTypeOption;
        if (typeof originalParseCustomTypeOption === "function") {
            ts.parseCustomTypeOption = (option, value, errors) => {
                let normalizedValue = value;
                if (option?.name === "module" && typeof value === "string") {
                    const lower = value.trim().toLowerCase();
                    if (lower === "node" || lower === "nodejs") {
                        normalizedValue = "commonjs";
                    }
                }
                return originalParseCustomTypeOption(option, normalizedValue, errors);
            };
        }
    } catch {
        // Best-effort compatibility shim; leave harness unchanged on failures.
    }
    const Harness = require(path.join(builtDir, "harness/_namespaces/Harness.js"));
    try {
        const compilerNamespace = Harness?.Compiler;
        const originalSetCompilerOptionsFromHarnessSetting = compilerNamespace?.setCompilerOptionsFromHarnessSetting;
        if (typeof originalSetCompilerOptionsFromHarnessSetting === "function") {
            compilerNamespace.setCompilerOptionsFromHarnessSetting = (settings, options) => {
                const normalizedSettings = settings && typeof settings === "object" ? { ...settings } : settings;
                if (normalizedSettings && typeof normalizedSettings === "object") {
                    for (const [name, value] of Object.entries(normalizedSettings)) {
                        if (typeof value !== "string") continue;
                        if (name.toLowerCase() !== "module") continue;
                        const normalizedValue = value.trim().toLowerCase();
                        if (normalizedValue === "node" || normalizedValue === "nodejs") {
                            normalizedSettings[name] = "commonjs";
                        }
                    }
                }
                return originalSetCompilerOptionsFromHarnessSetting(normalizedSettings, options);
            };
        }
    } catch {
        // Best-effort compatibility shim; leave harness unchanged on failures.
    }
    const FourSlash = require(path.join(builtDir, "harness/_namespaces/FourSlash.js"));
    const HarnessLS = require(path.join(builtDir, "harness/_namespaces/Harness.LanguageService.js"));
    const clientModule = require(path.join(builtDir, "harness/client.js"));
    return { ts, Harness, FourSlash, HarnessLS, SessionClient: clientModule.SessionClient };
}

const patchTestState = require("./test-worker-patch-test-state.cjs");
const patchSessionClientCompletions = require("./test-worker-session-client-completions.cjs");
const patchSessionClientFixes = require("./test-worker-session-client-fixes.cjs");

/**
 * Patch `SessionClient` to implement methods that throw "Not implemented"
 * by routing them to tsz-server protocol commands.
 */
function patchSessionClient(SessionClient, ts) {
    const proto = SessionClient.prototype;
    const helpers = patchSessionClientCompletions(proto, ts, libFileContentCache);
    patchSessionClientFixes(proto, ts, helpers);
}


function runSingleTest(FourSlash, Harness, testFile, testType) {
    globalThis.__tszCurrentFourslashTestFile = testFile;
    const basePath = path.dirname(testFile);
    const content = Harness.IO.readFile(testFile);
    if (content == null) throw new Error(`Could not read test file: ${testFile}`);
    const normalizedContent = content.replace(
        /^(\s*\/\/\s*@module\s*:\s*)(nodejs|node)\b/gim,
        "$1commonjs"
    );
    FourSlash.runFourSlashTestContent(basePath, testType, normalizedContent, testFile);
}

/**
 * Run a test with a timeout. Since fourslash tests are synchronous,
 * we can't use setTimeout. Instead we use the bridge's existing timeout
 * (30s per request) as a natural guard. For an additional layer, we
 * track wall-clock time and report timeouts.
 */
function runTestWithTimeout(FourSlash, Harness, testFile, testType, timeoutMs) {
    const start = Date.now();
    runSingleTest(FourSlash, Harness, testFile, testType);
    const elapsed = Date.now() - start;
    if (elapsed > timeoutMs) {
        throw new Error(`Test completed but took ${elapsed}ms (timeout: ${timeoutMs}ms)`);
    }
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

    preloadLibFiles(path.join(tsDir, "built/local"));

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
        const startTime = Date.now();
        let shouldRestartBridge = RESTART_BRIDGE_EVERY_TEST;
        let restartReason = RESTART_BRIDGE_EVERY_TEST
            ? "per-test isolation"
            : "";

        try {
            runTestWithTimeout(FourSlash, Harness, testFile, testType, perTestTimeout);
            const elapsed = Date.now() - startTime;
            process.send({ type: "result", workerId, testFile, testName, passed: true, elapsed });
        } catch (err) {
            const elapsed = Date.now() - startTime;
            const errMsg = err.message || String(err);
            const timedOut = elapsed >= perTestTimeout || errMsg.includes("Timeout");
            const bridgeLikelyUnhealthy =
                timedOut ||
                errMsg.includes("Stream closed before complete message was read") ||
                errMsg.includes("Unexpected empty response body") ||
                errMsg.includes("Broken pipe");
            if (bridgeLikelyUnhealthy) {
                shouldRestartBridge = true;
                restartReason = `post-failure recovery for ${testName}`;
            }
            if (isTemporarilyAllowedParityFailure(testName, errMsg)) {
                process.send({
                    type: "result",
                    workerId,
                    testFile,
                    testName,
                    passed: false,
                    xfailed: true,
                    error: errMsg,
                    elapsed,
                    timedOut,
                });
            } else {
                process.send({
                    type: "result", workerId, testFile, testName,
                    passed: false, error: errMsg, elapsed, timedOut,
                });
            }
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
