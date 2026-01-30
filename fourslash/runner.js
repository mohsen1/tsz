#!/usr/bin/env node
/**
 * runner.js - Fourslash test runner for tsz-server
 *
 * Runs TypeScript's fourslash test suite against tsz-server by:
 * 1. Loading TypeScript's built harness modules (non-bundled CJS from built/local/)
 * 2. Monkey-patching TestState.getLanguageServiceAdapter to return our TszServerLanguageServiceAdapter
 * 3. Discovering fourslash test files
 * 4. Running each test through the harness
 * 5. Reporting pass/fail results
 *
 * Must be run with CWD set to the TypeScript directory.
 *
 * Usage:
 *   node runner.js [options]
 *
 * Options:
 *   --tsz-server=PATH   Path to tsz-server binary (required)
 *   --max=N             Maximum number of tests to run
 *   --filter=PATTERN    Only run tests matching pattern (substring)
 *   --test-dir=DIR      Test directory relative to TypeScript root (default: tests/cases/fourslash)
 *   --verbose           Show detailed output for each test
 *   --server-tests      Run server-specific tests (tests/cases/fourslash/server/)
 */

"use strict";

const path = require("path");
const fs = require("fs");
const { TszServerBridge, createTszAdapterFactory } = require("./tsz-adapter");

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
        }
    }

    if (!opts.tszServerBinary) {
        console.error("Error: --tsz-server=PATH is required");
        process.exit(2);
    }

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
                // Use forward-slash paths (TypeScript convention)
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
// Set up globals required by the TypeScript harness
// =============================================================================

function setupGlobals(tsDir) {
    // chai provides the global `assert` used by harnessLanguageService.ts
    try {
        const chai = require(path.join(tsDir, "node_modules/chai"));
        global.assert = chai.assert;
    } catch (e) {
        // Fallback: use Node.js assert
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

    // Stub mocha globals (describe/it/beforeEach/afterEach) since we don't use mocha
    global.describe = function(name, fn) { fn(); };
    global.it = function(name, fn) { fn(); };
    global.beforeEach = function(fn) { /* ignore */ };
    global.afterEach = function(fn) { /* ignore */ };
    global.before = function(fn) { /* ignore */ };
    global.after = function(fn) { /* ignore */ };
}

// =============================================================================
// Load TypeScript harness modules
// =============================================================================

function loadHarnessModules(tsDir) {
    const builtDir = path.join(tsDir, "built/local");

    // Load the modules we need from the non-bundled build
    // These are CJS modules compiled with module: NodeNext + preserveConstEnums
    const ts = require(path.join(builtDir, "harness/_namespaces/ts.js"));
    const Harness = require(path.join(builtDir, "harness/_namespaces/Harness.js"));
    const FourSlash = require(path.join(builtDir, "harness/_namespaces/FourSlash.js"));
    const HarnessLS = require(path.join(builtDir, "harness/_namespaces/Harness.LanguageService.js"));
    const clientModule = require(path.join(builtDir, "harness/client.js"));

    return { ts, Harness, FourSlash, HarnessLS, SessionClient: clientModule.SessionClient };
}

// =============================================================================
// Monkey-patch TestState to use our adapter
// =============================================================================

function patchTestState(FourSlash, TszAdapter) {
    const TestState = FourSlash.TestState;
    if (!TestState) {
        throw new Error("Could not find TestState in FourSlash module");
    }

    const originalGetAdapter = TestState.prototype.getLanguageServiceAdapter;

    TestState.prototype.getLanguageServiceAdapter = function(testType, cancellationToken, compilationOptions) {
        // Always return our tsz-server adapter regardless of test type
        return new TszAdapter(cancellationToken, compilationOptions);
    };

    return originalGetAdapter;
}

// =============================================================================
// Run a single test
// =============================================================================

function runSingleTest(FourSlash, Harness, testFile, testType) {
    const basePath = path.dirname(testFile);
    const content = Harness.IO.readFile(testFile);
    if (content == null) {
        throw new Error(`Could not read test file: ${testFile}`);
    }
    FourSlash.runFourSlashTestContent(basePath, testType, content, testFile);
}

// =============================================================================
// Main
// =============================================================================

async function main() {
    const opts = parseArgs();
    const tsDir = process.cwd(); // Must be run from TypeScript directory

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

    // Set up globals
    setupGlobals(tsDir);

    // Load harness modules
    console.log("Loading TypeScript harness modules...");
    const { ts, Harness, FourSlash, HarnessLS, SessionClient } = loadHarnessModules(tsDir);
    console.log(`  TypeScript version: ${ts.version}`);

    // Start tsz-server bridge
    console.log(`Starting tsz-server: ${opts.tszServerBinary}`);
    const bridge = new TszServerBridge(opts.tszServerBinary);
    await bridge.start();
    console.log("  tsz-server ready");

    // Create the adapter factory
    const TszAdapter = createTszAdapterFactory(ts, Harness, SessionClient, bridge);

    // Monkey-patch TestState
    patchTestState(FourSlash, TszAdapter);
    console.log("  TestState patched to use TszServerLanguageServiceAdapter");

    // FourSlashTestType: Native=0, Server=1
    // We use Native (0) because Server mode enforces "watchable paths"
    // (e.g., /home/src/workspaces/project) which regular fourslash tests don't use.
    // Our monkey-patched getLanguageServiceAdapter returns our TszServerLanguageServiceAdapter
    // regardless of test type, so the actual language service calls still go through tsz-server.
    const testType = 0; // FourSlashTestType.Native

    // Discover tests
    const testFiles = discoverTests(opts.testDir, opts.filter);
    const totalAvailable = testFiles.length;
    const testsToRun = opts.max > 0 ? testFiles.slice(0, opts.max) : testFiles;

    console.log("");
    console.log(`Found ${totalAvailable} test files in ${opts.testDir}`);
    console.log(`Running ${testsToRun.length} tests${opts.filter ? ` (filter: "${opts.filter}")` : ""}`);
    console.log("─".repeat(70));

    // Run tests
    let passed = 0;
    let failed = 0;
    let errors = [];
    const startTime = Date.now();

    for (let i = 0; i < testsToRun.length; i++) {
        const testFile = testsToRun[i];
        const testName = path.basename(testFile, ".ts");

        if (opts.verbose) {
            process.stdout.write(`[${i + 1}/${testsToRun.length}] ${testName}... `);
        }

        try {
            runSingleTest(FourSlash, Harness, testFile, testType);
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

    // Cleanup
    bridge.shutdown();

    process.exit(failed > 0 ? 1 : 0);
}

main().catch(err => {
    console.error("Fatal error:", err);
    process.exit(2);
});
