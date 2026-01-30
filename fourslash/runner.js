#!/usr/bin/env node
/**
 * runner.js - Parallel fourslash test runner for tsz-server
 *
 * Runs TypeScript's fourslash test suite against tsz-server using parallel
 * child processes, each with its own tsz-server instance.
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
 *   --tsz-server=PATH   Path to tsz-server binary (required)
 *   --max=N             Maximum number of tests to run
 *   --filter=PATTERN    Only run tests matching pattern (substring)
 *   --test-dir=DIR      Test directory relative to TypeScript root
 *   --verbose           Show detailed output for each test
 *   --server-tests      Run server-specific tests
 *   --workers=N         Number of parallel workers (default: CPU count)
 *   --sequential        Run tests sequentially (single process, no workers)
 */

"use strict";

const path = require("path");
const fs = require("fs");
const os = require("os");
const { fork } = require("child_process");

// =============================================================================
// Argument parsing
// =============================================================================

function parseArgs() {
    const args = process.argv.slice(2);
    const opts = {
        tszServerBinary: null,
        max: 0,
        filter: "",
        testDir: "tests/cases/fourslash",
        verbose: false,
        serverTests: false,
        workers: os.cpus().length,
        sequential: false,
    };

    for (const arg of args) {
        if (arg.startsWith("--tsz-server=")) {
            opts.tszServerBinary = arg.substring("--tsz-server=".length);
        } else if (arg.startsWith("--max=")) {
            opts.max = parseInt(arg.substring("--max=".length), 10);
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
        }
    }

    if (!opts.tszServerBinary) {
        console.error("Error: --tsz-server=PATH is required");
        process.exit(2);
    }

    // Clamp workers to reasonable range
    if (opts.workers < 1) opts.workers = 1;
    if (opts.workers > 32) opts.workers = 32;

    return opts;
}

// =============================================================================
// Test file discovery
// =============================================================================

function discoverTests(testDir, filter) {
    const files = [];

    function walk(dir) {
        const entries = fs.readdirSync(dir, { withFileTypes: true });
        for (const entry of entries) {
            const fullPath = path.join(dir, entry.name);
            if (entry.isDirectory()) {
                walk(fullPath);
            } else if (entry.isFile() && entry.name.endsWith(".ts")) {
                const relPath = fullPath.replace(/\\/g, "/");
                if (!filter || relPath.includes(filter)) {
                    files.push(relPath);
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

// =============================================================================
// Sequential runner (fallback, same as original)
// =============================================================================

async function runSequential(opts, testsToRun) {
    const tsDir = process.cwd();
    const { TszServerBridge, createTszAdapterFactory } = require("./tsz-adapter");

    // Set up globals
    setupGlobals(tsDir);

    // Load harness modules
    const { ts, Harness, FourSlash, HarnessLS, SessionClient } = loadHarnessModules(tsDir);

    // Start tsz-server bridge
    const bridge = new TszServerBridge(opts.tszServerBinary);
    await bridge.start();

    const TszAdapter = createTszAdapterFactory(ts, Harness, SessionClient, bridge);
    patchTestState(FourSlash, TszAdapter);

    const testType = 0;
    let passed = 0;
    let failed = 0;
    const errors = [];

    for (let i = 0; i < testsToRun.length; i++) {
        const testFile = testsToRun[i];
        const testName = path.basename(testFile, ".ts");

        if (opts.verbose) {
            process.stdout.write(`[${i + 1}/${testsToRun.length}] ${testName}... `);
        }

        try {
            const basePath = path.dirname(testFile);
            const content = Harness.IO.readFile(testFile);
            if (content == null) throw new Error(`Could not read test file: ${testFile}`);
            FourSlash.runFourSlashTestContent(basePath, testType, content, testFile);
            passed++;
            if (opts.verbose) {
                console.log("\x1b[32mPASS\x1b[0m");
            } else if ((passed + failed) % 50 === 0) {
                process.stdout.write(`\r  Progress: ${passed + failed}/${testsToRun.length} (${passed} passed, ${failed} failed)`);
            }
        } catch (err) {
            failed++;
            const errMsg = err.message || String(err);
            errors.push({ file: testFile, error: errMsg });

            if (opts.verbose) {
                console.log("\x1b[31mFAIL\x1b[0m");
                console.log(`    ${errMsg.split("\n")[0]}`);
            }
        }
    }

    bridge.shutdown();
    return { passed, failed, errors };
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
}

// =============================================================================
// Parallel runner
// =============================================================================

function distributeTests(tests, numWorkers) {
    const chunks = Array.from({ length: numWorkers }, () => []);
    // Round-robin distribution for even load
    for (let i = 0; i < tests.length; i++) {
        chunks[i % numWorkers].push(tests[i]);
    }
    return chunks.filter(c => c.length > 0);
}

async function runParallel(opts, testsToRun) {
    const tsDir = process.cwd();
    const numWorkers = Math.min(opts.workers, testsToRun.length);
    const chunks = distributeTests(testsToRun, numWorkers);

    console.log(`  Spawning ${chunks.length} worker processes...`);

    let passed = 0;
    let failed = 0;
    let completed = 0;
    const errors = [];
    const workerFile = path.join(__dirname, "test-worker.js");

    return new Promise((resolve) => {
        let activeWorkers = chunks.length;
        let lastProgressLen = 0;

        function printProgress() {
            const total = testsToRun.length;
            const done = passed + failed;
            const msg = `\r  Progress: ${done}/${total} (${passed} passed, ${failed} failed) [${activeWorkers} workers]`;
            // Pad with spaces to clear previous longer line
            const padded = msg + " ".repeat(Math.max(0, lastProgressLen - msg.length));
            process.stdout.write(padded);
            lastProgressLen = msg.length;
        }

        for (let i = 0; i < chunks.length; i++) {
            const child = fork(workerFile, [], {
                stdio: ["pipe", "pipe", "pipe", "ipc"],
            });

            // Suppress child stdout/stderr (they load harness modules which print)
            child.stdout.on("data", () => {});
            child.stderr.on("data", () => {});

            child.on("message", (msg) => {
                if (msg.type === "ready") {
                    // Worker is ready, already running tests
                } else if (msg.type === "result") {
                    if (msg.passed) {
                        passed++;
                    } else {
                        failed++;
                        errors.push({ file: msg.testFile, error: msg.error });
                    }
                    completed++;

                    if (opts.verbose) {
                        const status = msg.passed
                            ? "\x1b[32mPASS\x1b[0m"
                            : "\x1b[31mFAIL\x1b[0m";
                        console.log(`  [W${msg.workerId}] ${msg.testName} ${status}`);
                        if (!msg.passed) {
                            console.log(`    ${msg.error.split("\n")[0]}`);
                        }
                    } else if (completed % 50 === 0) {
                        printProgress();
                    }
                } else if (msg.type === "done") {
                    activeWorkers--;
                    if (activeWorkers === 0) {
                        if (!opts.verbose) printProgress();
                        resolve({ passed, failed, errors });
                    }
                } else if (msg.type === "error") {
                    console.error(`  Worker ${i} error: ${msg.error}`);
                }
            });

            child.on("exit", (code) => {
                if (code !== 0 && code !== null) {
                    // Worker crashed - count remaining tests as failed
                    activeWorkers--;
                    if (activeWorkers === 0) {
                        if (!opts.verbose) printProgress();
                        resolve({ passed, failed, errors });
                    }
                }
            });

            // Send config to worker
            child.send({
                type: "config",
                testFiles: chunks[i],
                tszServerBinary: opts.tszServerBinary,
                tsDir,
                workerId: i,
            });
        }
    });
}

// =============================================================================
// Main
// =============================================================================

async function main() {
    const opts = parseArgs();
    const tsDir = process.cwd();

    // Verify we're in the TypeScript directory
    if (!fs.existsSync(path.join(tsDir, "Herebyfile.mjs"))) {
        console.error("Error: Must be run from the TypeScript directory");
        console.error(`  Current directory: ${tsDir}`);
        process.exit(2);
    }

    // Verify the non-bundled build exists
    const builtDir = path.join(tsDir, "built/local");
    if (!fs.existsSync(path.join(builtDir, "harness/fourslashImpl.js"))) {
        console.error("Error: TypeScript harness not built. Run: npx hereby tests --no-bundle");
        process.exit(2);
    }

    // Verify tsz-server binary exists
    if (!fs.existsSync(opts.tszServerBinary)) {
        console.error(`Error: tsz-server binary not found at: ${opts.tszServerBinary}`);
        process.exit(2);
    }

    // Discover tests
    const testFiles = discoverTests(opts.testDir, opts.filter);
    const totalAvailable = testFiles.length;
    const testsToRun = opts.max > 0 ? testFiles.slice(0, opts.max) : testFiles;

    const mode = opts.sequential ? "sequential" : `parallel (${Math.min(opts.workers, testsToRun.length)} workers)`;
    console.log("");
    console.log(`Found ${totalAvailable} test files in ${opts.testDir}`);
    console.log(`Running ${testsToRun.length} tests [${mode}]${opts.filter ? ` (filter: "${opts.filter}")` : ""}`);
    console.log("─".repeat(70));

    const startTime = Date.now();
    let results;

    if (opts.sequential) {
        results = await runSequential(opts, testsToRun);
    } else {
        results = await runParallel(opts, testsToRun);
    }

    const { passed, failed, errors } = results;
    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);

    // Print summary
    console.log("");
    console.log("─".repeat(70));
    console.log("");
    console.log(`Results: ${passed} passed, ${failed} failed out of ${testsToRun.length} (${elapsed}s)`);

    if (totalAvailable > testsToRun.length) {
        console.log(`  (${totalAvailable - testsToRun.length} tests skipped, ${totalAvailable} total available)`);
    }

    const passRate = testsToRun.length > 0
        ? ((passed / testsToRun.length) * 100).toFixed(1)
        : "0.0";
    console.log(`  Pass rate: ${passRate}%`);

    if (errors.length > 0 && !opts.verbose) {
        console.log("");
        console.log(`First ${Math.min(errors.length, 20)} failures:`);
        for (const { file, error } of errors.slice(0, 20)) {
            console.log(`  \x1b[31m✗\x1b[0m ${path.basename(file, ".ts")}: ${error.split("\n")[0].substring(0, 100)}`);
        }
        if (errors.length > 20) {
            console.log(`  ... and ${errors.length - 20} more failures`);
        }
    }

    process.exit(failed > 0 ? 1 : 0);
}

main().catch(err => {
    console.error("Fatal error:", err);
    process.exit(2);
});
