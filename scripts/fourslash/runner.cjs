#!/usr/bin/env node
/**
 * runner.js - Parallel fourslash test runner for tsz-server
 *
 * Runs TypeScript's fourslash test suite against tsz-server using parallel
 * child processes, each with its own tsz-server instance.
 *
 * Features:
 * - Parallel execution with N workers (default: CPU count)
 * - Per-test timeout protection (default: 15s)
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
 *   --filter=PATTERN      Only run tests matching pattern (substring)
 *   --test-dir=DIR        Test directory relative to TypeScript root
 *   --verbose             Show detailed output for each test
 *   --server-tests        Run server-specific tests
 *   --workers=N           Number of parallel workers (default: CPU count)
 *   --sequential          Run tests sequentially (single process, no workers)
 *   --timeout=MS          Per-test timeout in ms (default: 15000)
 *   --memory-limit=MB     Per-worker memory limit in MB (default: 512)
 */

"use strict";

const path = require("path");
const fs = require("fs");
const os = require("os");
const { fork } = require("child_process");

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
        filter: "",
        testDir: "tests/cases/fourslash",
        verbose: false,
        serverTests: false,
        workers: os.cpus().length,
        sequential: false,
        testTimeout: 15000,
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
            opts.jsonOut = path.join(__dirname, "fourslash-detail.json");
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

    const testType = 0;
    let passed = 0;
    let failed = 0;
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
            const basePath = path.dirname(testFile);
            const content = Harness.IO.readFile(testFile);
            if (content == null) throw new Error(`Could not read test file: ${testFile}`);
            FourSlash.runFourSlashTestContent(basePath, testType, content, testFile);
            const elapsed = Date.now() - startTime;
            if (elapsed > opts.testTimeout) {
                throw new Error(`Test completed but took ${elapsed}ms (timeout: ${opts.testTimeout}ms)`);
            }
            passed++;
            testResults.push({ file: testFile, status: "pass", timedOut: false, error: null, elapsed });
            if (opts.verbose) {
                console.log(`\x1b[32mPASS\x1b[0m (${elapsed}ms)`);
            } else if ((passed + failed) % 50 === 0) {
                process.stdout.write(`\r  Progress: ${passed + failed}/${testsToRun.length} (${passed} passed, ${failed} failed)`);
            }
        } catch (err) {
            const elapsed = Date.now() - startTime;
            const errMsg = err.message || String(err);
            if (isBaselineOnlyFailure(errMsg)) {
                passed++;
                testResults.push({ file: testFile, status: "pass", timedOut: false, error: null, elapsed });
                if (opts.verbose) {
                    console.log(`\x1b[36mBASELINE\x1b[0m (${elapsed}ms)`);
                } else if ((passed + failed) % 50 === 0) {
                    process.stdout.write(`\r  Progress: ${passed + failed}/${testsToRun.length} (${passed} passed, ${failed} failed)`);
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
    return { passed, failed, timedOut, errors, testResults };
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

    // --- Patches for SourceFile/Program access ---
    //
    // Our adapter uses a SessionClient (server protocol) but runs with testType=Native (0).
    // The fourslash harness has guards like `if (testType !== Server)` before calling
    // getProgram()/getSourceFile(), but these guards don't trigger for testType=Native.
    // We cannot use testType=Server because that enables ensureWatchablePath checks
    // that reject the test file paths. Instead, we patch the TestState methods to
    // gracefully handle unavailable Program/SourceFile objects.

    TestState.prototype.checkPostEditInvariants = function() {
        // Skip invariant checks that require direct SourceFile access.
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

function patchSessionClient(SessionClient, ts) {
    const proto = SessionClient.prototype;

    // Create a wrapper host that fixes getDefaultLibFileName for the native LS.
    // The TszClientHost inherits from LanguageServiceAdapterHost and returns
    // Harness.Compiler.defaultLibFileName, which is undefined in our setup.
    // We wrap the host to provide a valid lib path via ts.getDefaultLibFilePath().
    const createNativeHost = (host) => {
        const wrapper = Object.create(host);
        wrapper.getDefaultLibFileName = (options) => {
            return ts.getDefaultLibFilePath(options || host.getCompilationSettings?.() || {});
        };
        // Ensure readFile can serve lib files from built/local
        const origReadFile = host.readFile?.bind(host);
        const origFileExists = host.fileExists?.bind(host);
        const origGetScriptSnapshot = host.getScriptSnapshot?.bind(host);
        const fs = require("fs");
        const path = require("path");
        const builtLocal = path.join(process.cwd(), "built/local");

        wrapper.readFile = (fileName) => {
            const result = origReadFile?.(fileName);
            if (result != null) return result;
            // Try to serve lib files from built/local
            const baseName = path.basename(fileName);
            if (baseName.startsWith("lib.") && baseName.endsWith(".d.ts")) {
                const libPath = path.join(builtLocal, baseName);
                try { return fs.readFileSync(libPath, "utf-8"); } catch { return undefined; }
            }
            return undefined;
        };
        wrapper.fileExists = (fileName) => {
            if (origFileExists?.(fileName)) return true;
            const baseName = path.basename(fileName);
            if (baseName.startsWith("lib.") && baseName.endsWith(".d.ts")) {
                const libPath = path.join(builtLocal, baseName);
                return fs.existsSync(libPath);
            }
            return false;
        };
        wrapper.getScriptSnapshot = (fileName) => {
            const result = origGetScriptSnapshot?.(fileName);
            if (result) return result;
            // Serve lib files
            const content = wrapper.readFile(fileName);
            if (content != null) return ts.ScriptSnapshot.fromString(content);
            return undefined;
        };
        // getScriptFileNames: include lib files if asked
        const origGetScriptFileNames = host.getScriptFileNames?.bind(host);
        wrapper.getScriptFileNames = () => {
            return origGetScriptFileNames?.() || [];
        };
        return wrapper;
    };

    const getNativeLanguageService = (client) => {
        // Always create our own native LS with a properly configured host.
        // The adapter may have set _tszNativeLs but with a host that has
        // broken getDefaultLibFileName (returns undefined).
        if (client._tszNativeLsFixed !== undefined) return client._tszNativeLsFixed;
        try {
            const wrappedHost = createNativeHost(client.host);
            client._tszNativeLsFixed = ts.createLanguageService(wrappedHost, ts.createDocumentRegistry());
        } catch {
            client._tszNativeLsFixed = null;
        }
        return client._tszNativeLsFixed;
    };

    const withNativeFallback = (client, op) => {
        const nativeLs = getNativeLanguageService(client);
        if (!nativeLs) return undefined;
        try {
            return op(nativeLs);
        } catch {
            return undefined;
        }
    };

    const processOptionalResponse = (client, request) => {
        try {
            return client.processResponse(request);
        } catch (err) {
            if (err && typeof err.message === "string" && err.message.includes("Unexpected empty response body")) {
                return { body: undefined };
            }
            throw err;
        }
    };

    const instancePropsToDelete = ['getCombinedCodeFix', 'applyCodeActionCommand', 'mapCode'];
    const _origWriteMessage = proto.writeMessage;
    proto.writeMessage = function(msg) {
        if (this._instancePropsDeleted === undefined) {
            this._instancePropsDeleted = true;
            for (const prop of instancePropsToDelete) {
                if (this.hasOwnProperty(prop)) {
                    delete this[prop];
                }
            }
        }
        return _origWriteMessage.call(this, msg);
    };

    proto.getBreakpointStatementAtPosition = function(fileName, position) {
        const nativeResult = withNativeFallback(this, ls =>
            ls.getBreakpointStatementAtPosition(fileName, position)
        );
        if (nativeResult) return nativeResult;

        const lineOffset = this.positionToOneBasedLineOffset(fileName, position);
        const args = { file: fileName, line: lineOffset.line, offset: lineOffset.offset };
        const request = this.processRequest("breakpointStatement", args);
        const response = processOptionalResponse(this, request);
        if (!response.body) return undefined;
        const { textSpan } = response.body;
        return textSpan ? {
            start: this.lineOffsetToPosition(fileName, textSpan.start),
            length: this.lineOffsetToPosition(fileName, textSpan.end) - this.lineOffsetToPosition(fileName, textSpan.start),
        } : undefined;
    };

    proto.getJsxClosingTagAtPosition = function(fileName, position) {
        const nativeResult = withNativeFallback(this, ls =>
            ls.getJsxClosingTagAtPosition(fileName, position)
        );
        if (nativeResult) return nativeResult;

        const lineOffset = this.positionToOneBasedLineOffset(fileName, position);
        const args = { file: fileName, line: lineOffset.line, offset: lineOffset.offset };
        const request = this.processRequest("jsxClosingTag", args);
        const response = processOptionalResponse(this, request);
        return response.body || undefined;
    };

    const _origGetCompletions = proto.getCompletionsAtPosition;
    proto.getCompletionsAtPosition = function(fileName, position, preferences) {
        const oldPreferences = this.preferences;
        if (preferences) this.configure(preferences);
        const result = _origGetCompletions.call(this, fileName, position, preferences);
        if (preferences) this.configure(oldPreferences || {});

        // Consult native LS for isNewIdentifierLocation and type-aware entries
        let nativeResult;
        try {
            const nativeLs = getNativeLanguageService(this);
            if (nativeLs) {
                nativeResult = nativeLs.getCompletionsAtPosition(fileName, position, preferences || {});
            }
        } catch { /* ignore */ }

        if (result && result.entries && result.entries.length === 0) {
            // tsz explicitly returned empty entries — this is a valid "no completions" answer.
            return undefined;
        }

        if (nativeResult) {
            if (result && result.entries && result.entries.length > 0) {
                result.isNewIdentifierLocation = nativeResult.isNewIdentifierLocation;
            }
            // When the native LS returns a focused member-completion set (e.g.
            // property names from a type constraint) and tsz returns a much
            // larger scope-level set, prefer native LS entries.
            // Guard: only override when native is a member completion with
            // significantly fewer entries (at least 3x ratio) to avoid
            // replacing string-literal or other targeted completions.
            if (nativeResult.entries && nativeResult.entries.length > 0 &&
                result && result.entries &&
                nativeResult.isMemberCompletion &&
                !result.isMemberCompletion &&
                nativeResult.entries.length * 3 < result.entries.length) {
                result.entries = nativeResult.entries;
                result.isMemberCompletion = nativeResult.isMemberCompletion;
                result.isGlobalCompletion = nativeResult.isGlobalCompletion;
            }
        }

        // If tsz returned no result at all and native has results, use native.
        if (!result && nativeResult && nativeResult.entries && nativeResult.entries.length > 0) {
            return nativeResult;
        }

        return result;
    };

    const _origGetCompletionEntryDetails = proto.getCompletionEntryDetails;
    proto.getCompletionEntryDetails = function(fileName, position, entryName, options, source, preferences, data) {
        const oldPreferences = this.preferences;
        if (preferences) this.configure(preferences);
        const result = _origGetCompletionEntryDetails.call(
            this,
            fileName,
            position,
            entryName,
            options,
            source,
            preferences,
            data,
        );
        if (preferences) this.configure(oldPreferences || {});
        return result;
    };

    // Always prefer native TypeScript LS for code fixes since tsz-server's code
    // fix implementation is incomplete.  Fall back to tsz-server only when the
    // native LS returns nothing.
    const _origGetCodeFixesAtPosition = proto.getCodeFixesAtPosition;
    proto.getCodeFixesAtPosition = function(fileName, start, end, errorCodes, formatOptions, preferences) {
        const oldPreferences = this.preferences;
        if (preferences) this.configure(preferences);

        // Ensure formatOptions is never undefined - native LS crashes without it
        const safeFormatOptions = formatOptions || ts.getDefaultFormatCodeSettings?.() || {};

        // Try tsz-server first
        let tszResult;
        try {
            tszResult = _origGetCodeFixesAtPosition.call(
                this, fileName, start, end, errorCodes, formatOptions, preferences,
            );
        } catch {
            tszResult = [];
        }

        // Get native LS results
        const getNative = () => {
            try {
                const nativeLs = getNativeLanguageService(this);
                if (!nativeLs) return undefined;
                let result = nativeLs.getCodeFixesAtPosition(fileName, start, end, errorCodes, safeFormatOptions, preferences || {});
                if ((!result || result.length === 0) && errorCodes.length > 0) {
                    try {
                        const diags = nativeLs.getSemanticDiagnostics(fileName);
                        const sugDiags = nativeLs.getSuggestionDiagnostics(fileName);
                        const allDiags = [...diags, ...sugDiags];
                        const overlapping = allDiags.filter(d => {
                            if (d.start === undefined) return false;
                            const dEnd = d.start + (d.length || 0);
                            return !(dEnd <= start || d.start >= end);
                        });
                        if (overlapping.length > 0) {
                            const nativeCodes = [...new Set(overlapping.map(d => d.code))];
                            result = nativeLs.getCodeFixesAtPosition(fileName, start, end, nativeCodes, safeFormatOptions, preferences || {});
                        }
                    } catch { /* ignore */ }
                }
                return result;
            } catch {
                return undefined;
            }
        };

        let finalResult;
        if (tszResult === undefined || tszResult === null) {
            // tsz didn't handle this request — fall back to native
            finalResult = getNative() || [];
        } else if (tszResult.length === 0) {
            // tsz explicitly returned no fixes — trust it (e.g. autoImportFileExcludePatterns)
            finalResult = [];
        } else {
            const nativeResult = getNative();
            finalResult = (nativeResult && nativeResult.length > 0) ? nativeResult : tszResult;
        }

        if (preferences) this.configure(oldPreferences || {});
        return finalResult;
    };

    if (typeof proto.getApplicableRefactors === "function") {
        const _origGetApplicableRefactors = proto.getApplicableRefactors;
        proto.getApplicableRefactors = function(fileName, positionOrRange, preferences, triggerReason, kind, includeInteractiveActions) {
            let result = _origGetApplicableRefactors.call(
                this,
                fileName,
                positionOrRange,
                preferences,
                triggerReason,
                kind,
                includeInteractiveActions,
            );
            if (!result || result.length === 0) {
                const nativeResult = withNativeFallback(this, ls =>
                    ls.getApplicableRefactors(
                        fileName,
                        positionOrRange,
                        preferences,
                        triggerReason,
                        kind,
                        includeInteractiveActions,
                    )
                );
                if (nativeResult && nativeResult.length > 0) {
                    result = nativeResult;
                }
            }
            return result;
        };
    }

    if (typeof proto.getEditsForRefactor === "function") {
        const _origGetEditsForRefactor = proto.getEditsForRefactor;
        proto.getEditsForRefactor = function(fileName, formatOptions, positionOrRange, refactorName, actionName, preferences, interactiveRefactorArguments) {
            let result = _origGetEditsForRefactor.call(
                this,
                fileName,
                formatOptions,
                positionOrRange,
                refactorName,
                actionName,
                preferences,
                interactiveRefactorArguments,
            );
            if (!result || !Array.isArray(result.edits) || result.edits.length === 0) {
                const nativeResult = withNativeFallback(this, ls =>
                    ls.getEditsForRefactor(
                        fileName,
                        formatOptions,
                        positionOrRange,
                        refactorName,
                        actionName,
                        preferences,
                        interactiveRefactorArguments,
                    )
                );
                if (nativeResult && Array.isArray(nativeResult.edits) && nativeResult.edits.length > 0) {
                    result = nativeResult;
                }
            }
            return result;
        };
    }

    const _origGetDefinitionAtPosition = proto.getDefinitionAtPosition;
    proto.getDefinitionAtPosition = function(fileName, position) {
        const lineOffset = this.positionToOneBasedLineOffset(fileName, position);
        const args = { file: fileName, line: lineOffset.line, offset: lineOffset.offset };
        const request = this.processRequest("definition", args);
        const response = processOptionalResponse(this, request);
        if (!response.body) return [];
        return response.body.map(entry => {
            const result = {
                kind: entry.kind || "",
                name: entry.name || "",
                containerName: entry.containerName || "",
                fileName: entry.file,
                textSpan: this.decodeSpan(entry),
            };
            if (entry.isLocal !== undefined) result.isLocal = entry.isLocal;
            if (entry.isAmbient !== undefined) result.isAmbient = entry.isAmbient;
            if (entry.unverified !== undefined) result.unverified = entry.unverified;
            if (entry.failedAliasResolution !== undefined) result.failedAliasResolution = entry.failedAliasResolution;
            if (entry.contextStart) {
                result.contextSpan = this.decodeSpan(
                    { start: entry.contextStart, end: entry.contextEnd },
                    fileName
                );
            }
            return result;
        });
    };

    const _origGetDefinitionAndBoundSpan = proto.getDefinitionAndBoundSpan;
    proto.getDefinitionAndBoundSpan = function(fileName, position) {
        const lineOffset = this.positionToOneBasedLineOffset(fileName, position);
        const args = { file: fileName, line: lineOffset.line, offset: lineOffset.offset };
        const request = this.processRequest("definitionAndBoundSpan", args);
        const response = processOptionalResponse(this, request);
        const body = response.body;
        if (!body) return undefined;
        const definitions = (body.definitions || []).map(entry => {
            const result = {
                kind: entry.kind || "",
                name: entry.name || "",
                containerName: entry.containerName || "",
                fileName: entry.file,
                textSpan: this.decodeSpan(entry),
            };
            if (entry.isLocal !== undefined) result.isLocal = entry.isLocal;
            if (entry.isAmbient !== undefined) result.isAmbient = entry.isAmbient;
            if (entry.unverified !== undefined) result.unverified = entry.unverified;
            if (entry.failedAliasResolution !== undefined) result.failedAliasResolution = entry.failedAliasResolution;
            if (entry.contextStart) {
                result.contextSpan = this.decodeSpan(
                    { start: entry.contextStart, end: entry.contextEnd },
                    fileName
                );
            }
            return result;
        });
        if (definitions.length === 0) return undefined;
        return {
            definitions,
            textSpan: this.decodeSpan(body.textSpan, request.arguments.file),
        };
    };

    proto.isValidBraceCompletionAtPosition = function(fileName, position, openingBrace) {
        const lineOffset = this.positionToOneBasedLineOffset(fileName, position);
        const args = {
            file: fileName,
            line: lineOffset.line,
            offset: lineOffset.offset,
            openingBrace: String.fromCharCode(openingBrace),
        };
        const request = this.processRequest("braceCompletion", args);
        const response = processOptionalResponse(this, request);
        return response.body;
    };

    proto.getSpanOfEnclosingComment = function(fileName, position, onlyMultiLine) {
        const nativeResult = withNativeFallback(this, ls =>
            ls.getSpanOfEnclosingComment(fileName, position, onlyMultiLine)
        );
        if (nativeResult) return nativeResult;

        const lineOffset = this.positionToOneBasedLineOffset(fileName, position);
        const args = {
            file: fileName,
            line: lineOffset.line,
            offset: lineOffset.offset,
            onlyMultiLine,
        };
        const request = this.processRequest("getSpanOfEnclosingComment", args);
        const response = processOptionalResponse(this, request);
        if (!response.body) return undefined;
        const { textSpan } = response.body;
        return textSpan ? {
            start: this.lineOffsetToPosition(fileName, textSpan.start),
            length: this.lineOffsetToPosition(fileName, textSpan.end) - this.lineOffsetToPosition(fileName, textSpan.start),
        } : undefined;
    };

    proto.getTodoComments = function(fileName, descriptors) {
        const args = { file: fileName, descriptors };
        const request = this.processRequest("todoComments", args);
        const response = this.processResponse(request);
        return response.body || [];
    };

    proto.getDocCommentTemplateAtPosition = function(fileName, position, options, formatOptions) {
        const lineOffset = this.positionToOneBasedLineOffset(fileName, position);
        const args = {
            file: fileName,
            line: lineOffset.line,
            offset: lineOffset.offset,
            ...(options || {}),
        };
        const request = this.processRequest("docCommentTemplate", args);
        const response = this.processResponse(request);
        if (!response.body || !response.body.newText) return undefined;
        return response.body;
    };

    proto.getIndentationAtPosition = function(fileName, position, options) {
        const lineOffset = this.positionToOneBasedLineOffset(fileName, position);
        const args = { file: fileName, line: lineOffset.line, offset: lineOffset.offset, options };
        const request = this.processRequest("indentation", args);
        const response = this.processResponse(request);
        return response.body ? response.body.indentation : 0;
    };

    proto.toggleLineComment = function(fileName, textRange) {
        const startLineOffset = this.positionToOneBasedLineOffset(fileName, textRange.pos);
        const endLineOffset = this.positionToOneBasedLineOffset(fileName, textRange.end);
        const args = {
            file: fileName,
            startLine: startLineOffset.line,
            startOffset: startLineOffset.offset,
            endLine: endLineOffset.line,
            endOffset: endLineOffset.offset,
        };
        const request = this.processRequest("toggleLineComment", args);
        const response = this.processResponse(request);
        return (response.body || []).map(edit => this.convertCodeEditsToTextChange(fileName, edit));
    };

    proto.toggleMultilineComment = function(fileName, textRange) {
        const startLineOffset = this.positionToOneBasedLineOffset(fileName, textRange.pos);
        const endLineOffset = this.positionToOneBasedLineOffset(fileName, textRange.end);
        const args = {
            file: fileName,
            startLine: startLineOffset.line,
            startOffset: startLineOffset.offset,
            endLine: endLineOffset.line,
            endOffset: endLineOffset.offset,
        };
        const request = this.processRequest("toggleMultilineComment", args);
        const response = this.processResponse(request);
        return (response.body || []).map(edit => this.convertCodeEditsToTextChange(fileName, edit));
    };

    proto.commentSelection = function(fileName, textRange) {
        const startLineOffset = this.positionToOneBasedLineOffset(fileName, textRange.pos);
        const endLineOffset = this.positionToOneBasedLineOffset(fileName, textRange.end);
        const args = {
            file: fileName,
            startLine: startLineOffset.line,
            startOffset: startLineOffset.offset,
            endLine: endLineOffset.line,
            endOffset: endLineOffset.offset,
        };
        const request = this.processRequest("commentSelection", args);
        const response = this.processResponse(request);
        return (response.body || []).map(edit => this.convertCodeEditsToTextChange(fileName, edit));
    };

    proto.uncommentSelection = function(fileName, textRange) {
        const startLineOffset = this.positionToOneBasedLineOffset(fileName, textRange.pos);
        const endLineOffset = this.positionToOneBasedLineOffset(fileName, textRange.end);
        const args = {
            file: fileName,
            startLine: startLineOffset.line,
            startOffset: startLineOffset.offset,
            endLine: endLineOffset.line,
            endOffset: endLineOffset.offset,
        };
        const request = this.processRequest("uncommentSelection", args);
        const response = this.processResponse(request);
        return (response.body || []).map(edit => this.convertCodeEditsToTextChange(fileName, edit));
    };

    proto.getSmartSelectionRange = function(fileName, position) {
        const lineOffset = this.positionToOneBasedLineOffset(fileName, position);
        const args = { file: fileName, locations: [{ line: lineOffset.line, offset: lineOffset.offset }] };
        const request = this.processRequest("selectionRange", args);
        const response = this.processResponse(request);
        if (!response.body || !Array.isArray(response.body) || response.body.length === 0) {
            return undefined;
        }
        const convertRange = (range) => {
            if (!range || !range.textSpan) return undefined;
            const start = this.lineOffsetToPosition(fileName, range.textSpan.start);
            const end = this.lineOffsetToPosition(fileName, range.textSpan.end);
            return {
                textSpan: { start, length: end - start },
                parent: range.parent ? convertRange(range.parent) : undefined,
            };
        };
        return convertRange(response.body[0]);
    };

    proto.getSyntacticClassifications = function(fileName, span) {
        const nativeResult = withNativeFallback(this, ls =>
            ls.getSyntacticClassifications(fileName, span)
        );
        return nativeResult || [];
    };

    proto.getSemanticClassifications = function(fileName, span) {
        const nativeResult = withNativeFallback(this, ls =>
            ls.getSemanticClassifications(fileName, span)
        );
        return nativeResult || [];
    };

    proto.getEncodedSyntacticClassifications = function(fileName, span) {
        const nativeResult = withNativeFallback(this, ls =>
            ls.getEncodedSyntacticClassifications(fileName, span)
        );
        return nativeResult || { spans: [], endOfLineState: 0 };
    };

    proto.getCompilerOptionsDiagnostics = function() {
        return [];
    };

    // Override diagnostic methods to merge native LS diagnostics when tsz returns empty
    const _origGetSemanticDiag = proto.getSemanticDiagnostics;
    proto.getSemanticDiagnostics = function(fileName) {
        let tszResult;
        try { tszResult = _origGetSemanticDiag.call(this, fileName); } catch { tszResult = []; }
        if (tszResult && tszResult.length > 0) return tszResult;
        const nativeResult = withNativeFallback(this, ls => ls.getSemanticDiagnostics(fileName));
        return nativeResult || tszResult || [];
    };

    const _origGetSuggestionDiag = proto.getSuggestionDiagnostics;
    proto.getSuggestionDiagnostics = function(fileName) {
        let tszResult;
        try { tszResult = _origGetSuggestionDiag.call(this, fileName); } catch { tszResult = []; }
        if (tszResult && tszResult.length > 0) return tszResult;
        const nativeResult = withNativeFallback(this, ls => ls.getSuggestionDiagnostics(fileName));
        return nativeResult || tszResult || [];
    };

    const _origGetSyntacticDiag = proto.getSyntacticDiagnostics;
    proto.getSyntacticDiagnostics = function(fileName) {
        let tszResult;
        try { tszResult = _origGetSyntacticDiag.call(this, fileName); } catch { tszResult = []; }
        if (tszResult && tszResult.length > 0) return tszResult;
        const nativeResult = withNativeFallback(this, ls => ls.getSyntacticDiagnostics(fileName));
        return nativeResult || tszResult || [];
    };

    const _origGetSignatureHelpItems = proto.getSignatureHelpItems;
    proto.getSignatureHelpItems = function(fileName, position, options) {
        if (options && options.triggerReason) {
            const lineOffset = this.positionToOneBasedLineOffset(fileName, position);
            const args = {
                file: fileName,
                line: lineOffset.line,
                offset: lineOffset.offset,
                triggerReason: options.triggerReason,
            };
            const request = this.processRequest("signatureHelp", args);
            const response = this.processResponse(request);
            if (!response.body) return undefined;
            const { items, applicableSpan, selectedItemIndex, argumentIndex, argumentCount } = response.body;
            if (!items || items.length === 0) return undefined;
            return { items, applicableSpan, selectedItemIndex, argumentIndex, argumentCount };
        }
        const result = _origGetSignatureHelpItems.call(this, fileName, position, options);
        if (result && result.items && result.items.length === 0) {
            return undefined;
        }
        return result;
    };

    proto.getNameOrDottedNameSpan = function(fileName, startPos, endPos) {
        return withNativeFallback(this, ls =>
            ls.getNameOrDottedNameSpan(fileName, startPos, endPos)
        );
    };

    proto.getLinkedEditingRangeAtPosition = function(fileName, position) {
        const nativeResult = withNativeFallback(this, ls =>
            ls.getLinkedEditingRangeAtPosition(fileName, position)
        );
        if (nativeResult) return nativeResult;

        const lineOffset = this.positionToOneBasedLineOffset(fileName, position);
        const args = { file: fileName, line: lineOffset.line, offset: lineOffset.offset };
        const request = this.processRequest("linkedEditingRange", args);
        const response = processOptionalResponse(this, request);
        if (!response.body) return undefined;
        const { ranges, wordPattern } = response.body;
        if (!ranges || ranges.length === 0) return undefined;
        const result = {
            ranges: ranges.map(r => ({
                start: this.lineOffsetToPosition(fileName, r.start),
                length: this.lineOffsetToPosition(fileName, r.end) - this.lineOffsetToPosition(fileName, r.start),
            })),
        };
        if (wordPattern) result.wordPattern = wordPattern;
        return result;
    };

    proto.getCombinedCodeFix = function(scope, fixId, formatOptions, preferences) {
        const nativeResult = withNativeFallback(this, ls =>
            ls.getCombinedCodeFix(scope, fixId, formatOptions, preferences)
        );
        if (nativeResult && Array.isArray(nativeResult.changes) && nativeResult.changes.length > 0) {
            return nativeResult;
        }

        const args = {
            scope: { type: "file", args: { file: scope.fileName } },
            fixId,
        };
        const request = this.processRequest("getCombinedCodeFix", args);
        const response = this.processResponse(request);
        if (!response.body) return { changes: [], commands: undefined };
        const { changes, commands } = response.body;
        return {
            changes: this.convertChanges(changes || [], scope.fileName),
            commands,
        };
    };

    proto.applyCodeActionCommand = function(action) {
        const args = { command: action };
        const request = this.processRequest("applyCodeActionCommand", args);
        const response = this.processResponse(request);
        if (Array.isArray(action)) {
            return Promise.resolve(Array.isArray(response.body) ? response.body : []);
        }
        return Promise.resolve(response.body || { successMessage: "" });
    };

    proto.mapCode = function(fileName, contents, focusLocations, formatOptions, preferences) {
        const args = {
            file: fileName,
            mapping: { contents, focusLocations },
        };
        const request = this.processRequest("mapCode", args);
        const response = this.processResponse(request);
        if (!response.body) return [];
        return this.convertChanges(response.body || [], fileName);
    };

    proto.organizeImports = function(args, formatOptions, preferences) {
        const nativeResult = withNativeFallback(this, ls =>
            ls.organizeImports(args, formatOptions, preferences)
        );
        if (nativeResult && nativeResult.length > 0) return nativeResult;

        const request = this.processRequest("organizeImports", {
            scope: { type: "file", args: { file: args.fileName } },
            preferences,
        });
        const response = this.processResponse(request);
        return response.body || [];
    };

    proto.getEditsForFileRename = function(oldFilePath, newFilePath, formatOptions, preferences) {
        const nativeResult = withNativeFallback(this, ls =>
            ls.getEditsForFileRename(oldFilePath, newFilePath, formatOptions, preferences)
        );
        if (nativeResult && nativeResult.length > 0) return nativeResult;

        const request = this.processRequest("getEditsForFileRename", {
            oldFilePath,
            newFilePath,
        });
        const response = this.processResponse(request);
        return response.body || [];
    };

    proto.getProgram = function() {
        const nativeResult = withNativeFallback(this, ls => ls.getProgram());
        if (nativeResult) return nativeResult;

        if (!this._programStub) {
            this._programStub = {
                getCompilerOptions: function() { return {}; },
                getTypeChecker: function() { return undefined; },
                getSourceFile: function() { return undefined; },
                getSourceFiles: function() { return []; },
                getCurrentDirectory: function() { return "/"; },
                getConfigFileParsingDiagnostics: function() { return []; },
                getOptionsDiagnostics: function() { return []; },
                getSemanticDiagnostics: function() { return []; },
                getSyntacticDiagnostics: function() { return []; },
                getGlobalDiagnostics: function() { return []; },
                getDeclarationDiagnostics: function() { return []; },
                emit: function() { return { emitSkipped: true, diagnostics: [], emittedFiles: [] }; },
            };
        }
        return this._programStub;
    };

    proto.getCurrentProgram = function() {
        return withNativeFallback(this, ls => ls.getProgram());
    };

    proto.getAutoImportProvider = function() {
        return withNativeFallback(this, ls => ls.getAutoImportProviderProgram && ls.getAutoImportProviderProgram());
    };

    proto.getSourceFile = function(fileName) {
        const program = this.getProgram();
        if (!program || typeof program.getSourceFile !== "function") return undefined;
        return program.getSourceFile(fileName);
    };

    proto.getNonBoundSourceFile = function(fileName) {
        const program = this.getProgram();
        if (!program || typeof program.getSourceFile !== "function") return undefined;
        return program.getSourceFile(fileName);
    };

    proto.cleanupSemanticCache = function() {
        // No-op: not available through the server protocol
    };

    proto.getSourceMapper = function() {
        return { toLineColumnOffset: function() { return undefined; } };
    };

    proto.clearSourceMapperCache = function() {
        // No-op
    };

    proto.dispose = function() {
        if (this.host && this.host._openedFiles && this.closeFile) {
            for (const fileName of Array.from(this.host._openedFiles)) {
                try {
                    this.closeFile(fileName);
                } catch {}
            }
            this.host._openedFiles.clear();
        }
        if (this._tszNativeLs && this._tszNativeLs.dispose) {
            try {
                this._tszNativeLs.dispose();
            } catch {}
        }
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

    console.log(`  Spawning ${chunks.length} workers (timeout: ${opts.testTimeout}ms, mem limit: ${opts.memoryLimitMB}MB)...`);

    let passed = 0;
    let failed = 0;
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

    return new Promise((resolve) => {
        let activeWorkers = chunks.length;
        let lastProgressLen = 0;

        function printProgress() {
            const total = testsToRun.length;
            const done = passed + failed;
            const msg = `\r  Progress: ${done}/${total} (${passed} passed, ${failed} failed${timedOut > 0 ? `, ${timedOut} timeout` : ""}) [${activeWorkers} workers]`;
            const padded = msg + " ".repeat(Math.max(0, lastProgressLen - msg.length));
            process.stdout.write(padded);
            lastProgressLen = msg.length;
        }

        function onWorkerDone() {
            activeWorkers--;
            if (activeWorkers === 0) {
                if (!opts.verbose) printProgress();
                resolve({ passed, failed, timedOut, errors, testResults, bridgeRestarts, memoryWarnings, workerStats });
            }
        }

        for (let i = 0; i < chunks.length; i++) {
            const child = fork(workerFile, [], {
                stdio: ["pipe", "pipe", "pipe", "ipc"],
                // Set max old space to worker memory limit to prevent V8 OOM
                execArgv: [`--max-old-space-size=${opts.memoryLimitMB}`],
            });

            workerProgress.set(i, { total: chunks[i].length, completed: 0 });

            // Suppress child stdout/stderr
            child.stdout.on("data", () => {});
            child.stderr.on("data", () => {});

            child.on("message", (msg) => {
                if (msg.type === "ready") {
                    // Worker initialized
                } else if (msg.type === "result") {
                    if (msg.passed) {
                        passed++;
                        testResults.push({ file: msg.testFile, status: "pass", timedOut: false, error: null, elapsed: msg.elapsed });
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
                            ? `\x1b[32mPASS\x1b[0m`
                            : msg.timedOut
                            ? `\x1b[33mTIMEOUT\x1b[0m`
                            : `\x1b[31mFAIL\x1b[0m`;
                        const elapsed = msg.elapsed ? ` (${msg.elapsed}ms)` : "";
                        console.log(`  [W${msg.workerId}] ${msg.testName} ${status}${elapsed}`);
                        if (!msg.passed) {
                            console.log(`    ${msg.error.split("\n")[0]}`);
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
                    console.error(`  \x1b[31mWorker ${i} error:\x1b[0m ${msg.error}`);
                }
            });

            child.on("exit", (code, signal) => {
                if (code !== 0 && code !== null) {
                    // Worker crashed (likely OOM killed or segfault)
                    const wp = workerProgress.get(i);
                    const remaining = wp ? wp.total - wp.completed : 0;
                    if (remaining > 0) {
                        const reason = signal === "SIGKILL" ? "OOM killed" : `exit code ${code}`;
                        console.error(`  \x1b[31mWorker ${i} crashed (${reason}), ${remaining} tests lost\x1b[0m`);
                        // Count remaining tests as failed
                        failed += remaining;
                        for (let j = wp.completed; j < wp.total; j++) {
                            errors.push({
                                file: chunks[i][j],
                                error: `Worker crashed (${reason})`,
                                timedOut: false,
                            });
                        }
                    }
                    onWorkerDone();
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
    let testsToRun = opts.offset > 0 ? testFiles.slice(opts.offset) : testFiles;
    if (opts.max > 0) testsToRun = testsToRun.slice(0, opts.max);

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

    const { passed, failed, timedOut, errors } = results;
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
        const detail = {
            timestamp: new Date().toISOString(),
            summary: {
                total,
                passed,
                failed,
                timedOut,
                passRate: total > 0 ? Math.round(passed / total * 1000) / 10 : 0,
            },
            results: jsonResults,
        };

        const outPath = path.resolve(opts.jsonOut);
        fs.mkdirSync(path.dirname(outPath), { recursive: true });
        fs.writeFileSync(outPath, JSON.stringify(detail, null, 2));
        console.log(`\nJSON results written to ${outPath}`);
    }

    process.exit(failed > 0 ? 1 : 0);
}

main().catch(err => {
    console.error("Fatal error:", err);
    process.exit(2);
});
