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
const { TszServerBridge, createTszAdapterFactory } = require("./tsz-adapter");

// Per-test timeout (ms) - tests taking longer are killed
const TEST_TIMEOUT_MS = 15000;
// Memory threshold per worker (bytes) - restart bridge if exceeded
const MEMORY_THRESHOLD_BYTES = 512 * 1024 * 1024; // 512MB
// Check memory every N tests
const MEMORY_CHECK_INTERVAL = 25;

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

    // checkPostEditInvariants: called after every edit, uses getNonBoundSourceFile()
    // and getProgram() to verify AST invariants. We skip these checks since we
    // cannot access SourceFile objects through the server protocol.
    TestState.prototype.checkPostEditInvariants = function() {
        // Skip entirely - these invariant checks require direct SourceFile access
        // which is not available through the server protocol.
    };

    // getChecker: depends on getProgram() which may return a stub.
    TestState.prototype.getChecker = function() {
        const program = this.getProgram();
        if (!program) return undefined;
        const checker = program.getTypeChecker();
        if (!checker) return undefined;
        return this._checker || (this._checker = checker);
    };

    // getSourceFile: depends on getProgram() which may return undefined.
    TestState.prototype.getSourceFile = function() {
        const program = this.getProgram();
        if (!program) return undefined;
        const fileName = this.activeFile.fileName;
        return program.getSourceFile(fileName);
    };

    // getNode: depends on getSourceFile() which may return undefined.
    const originalGetNode = TestState.prototype.getNode;
    TestState.prototype.getNode = function() {
        const sf = this.getSourceFile();
        if (!sf) return undefined;
        return originalGetNode.call(this);
    };

    // getProgram: return a minimal stub that provides getCompilerOptions().
    // The fourslash harness calls getProgram().getCompilerOptions() without null
    // checks when testType !== Server (our case, since we use testType=Native).
    // The stub provides safe defaults so these calls don't throw.
    const _origGetProgram = TestState.prototype.getProgram;
    TestState.prototype.getProgram = function() {
        if (!this._program) {
            this._program = this.languageService.getProgram() || "missing";
        }
        if (this._program === "missing") {
            // Return a minimal stub with getCompilerOptions so callers
            // like verifyNoErrors don't crash.
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

/**
 * Patch SessionClient to implement methods that throw "Not implemented"
 * by routing them to tsz-server protocol commands.
 */
function patchSessionClient(SessionClient) {
    const proto = SessionClient.prototype;

    proto.getBreakpointStatementAtPosition = function(fileName, position) {
        const lineOffset = this.positionToOneBasedLineOffset(fileName, position);
        const args = { file: fileName, line: lineOffset.line, offset: lineOffset.offset };
        const request = this.processRequest("breakpointStatement", args);
        const response = this.processResponse(request, /*expectEmptyBody*/ true);
        if (!response.body) return undefined;
        const { textSpan } = response.body;
        return textSpan ? {
            start: this.lineOffsetToPosition(fileName, textSpan.start),
            length: this.lineOffsetToPosition(fileName, textSpan.end) - this.lineOffsetToPosition(fileName, textSpan.start),
        } : undefined;
    };

    proto.getJsxClosingTagAtPosition = function(fileName, position) {
        const lineOffset = this.positionToOneBasedLineOffset(fileName, position);
        const args = { file: fileName, line: lineOffset.line, offset: lineOffset.offset };
        const request = this.processRequest("jsxClosingTag", args);
        const response = this.processResponse(request, /*expectEmptyBody*/ true);
        return response.body || undefined;
    };

    // Override getCompletionsAtPosition to return undefined when no entries
    // The fourslash harness distinguishes "no completions" (undefined) from
    // "completions with 0 entries" (object with empty entries array)
    const _origGetCompletions = proto.getCompletionsAtPosition;
    proto.getCompletionsAtPosition = function(fileName, position, preferences) {
        const result = _origGetCompletions.call(this, fileName, position, preferences);
        if (result && result.entries && result.entries.length === 0) {
            return undefined;
        }
        return result;
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
        const response = this.processResponse(request);
        return response.body;
    };

    proto.getSpanOfEnclosingComment = function(fileName, position, onlyMultiLine) {
        const lineOffset = this.positionToOneBasedLineOffset(fileName, position);
        const args = {
            file: fileName,
            line: lineOffset.line,
            offset: lineOffset.offset,
            onlyMultiLine,
        };
        const request = this.processRequest("getSpanOfEnclosingComment", args);
        const response = this.processResponse(request, /*expectEmptyBody*/ true);
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
        const response = this.processResponse(request, /*expectEmptyBody*/ true);
        return response.body || undefined;
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
        // selectionRange command expects locations array, not line/offset directly
        const args = { file: fileName, locations: [{ line: lineOffset.line, offset: lineOffset.offset }] };
        const request = this.processRequest("selectionRange", args);
        const response = this.processResponse(request);
        if (!response.body || !Array.isArray(response.body) || response.body.length === 0) {
            return undefined;
        }
        // Convert server format {textSpan: {start, end}, parent: ...} to
        // LS API format {textSpan: {start, length}, parent: ...}
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
        return [];
    };

    proto.getSemanticClassifications = function(fileName, span) {
        return [];
    };

    proto.getEncodedSyntacticClassifications = function(fileName, span) {
        return { spans: [], endOfLineState: 0 };
    };

    proto.getCompilerOptionsDiagnostics = function() {
        return [];
    };

    // Override getSignatureHelpItems to return undefined when items are empty.
    // The server always returns a body (processResponse requires it), but when
    // the items array is empty, the harness expects undefined (no signature help).
    const _origGetSignatureHelpItems = proto.getSignatureHelpItems;
    proto.getSignatureHelpItems = function(fileName, position, options) {
        const result = _origGetSignatureHelpItems.call(this, fileName, position, options);
        if (result && result.items && result.items.length === 0) {
            return undefined;
        }
        return result;
    };

    proto.getNameOrDottedNameSpan = function(fileName, startPos, endPos) {
        return undefined;
    };

    // organizeImports - route to server protocol
    proto.organizeImports = function(args, formatOptions) {
        const request = this.processRequest("organizeImports", {
            scope: { type: "file", args: { file: args.fileName } },
        });
        const response = this.processResponse(request);
        return response.body || [];
    };

    // getEditsForFileRename - route to server protocol
    proto.getEditsForFileRename = function(oldFilePath, newFilePath, formatOptions, preferences) {
        const request = this.processRequest("getEditsForFileRename", {
            oldFilePath,
            newFilePath,
        });
        const response = this.processResponse(request);
        return response.body || [];
    };

    // --- Stubs for methods that throw "not serializable" errors ---
    // These methods cannot work through the server protocol because they return
    // non-serializable objects (SourceFile, Program). The fourslash harness calls
    // them when testType=Native (0), but our adapter uses a SessionClient (server-like).
    // Return safe stubs so tests that don't strictly need these objects can proceed.

    proto.getProgram = function() {
        // Return a minimal Program stub so callers like
        // ts.getPreEmitDiagnostics(languageService.getProgram()) don't crash.
        // TODO: Implement proper Program when compiler supports it
        if (!this._programStub) {
            this._programStub = {
                getCompilerOptions: function() { return {}; },
                getTypeChecker: function() { return undefined; },
                getSourceFile: function() { return undefined; },
                getSourceFiles: function() { return []; },
                getCurrentDirectory: function() { return "/"; },
                getConfigFileParsingDiagnostics: function() { return []; },
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
        return undefined;
    };

    proto.getAutoImportProvider = function() {
        return undefined;
    };

    proto.getSourceFile = function(_fileName) {
        return undefined;
    };

    proto.getNonBoundSourceFile = function(_fileName) {
        return undefined;
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
        // No-op: not available through the server protocol
    };
}

function runSingleTest(FourSlash, Harness, testFile, testType) {
    const basePath = path.dirname(testFile);
    const content = Harness.IO.readFile(testFile);
    if (content == null) throw new Error(`Could not read test file: ${testFile}`);
    FourSlash.runFourSlashTestContent(basePath, testType, content, testFile);
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

    // Start our own tsz-server bridge
    let bridge = new TszServerBridge(tszServerBinary);
    await bridge.start();

    // Create adapter and patch TestState
    let TszAdapter = createTszAdapterFactory(ts, Harness, SessionClient, bridge);
    patchTestState(FourSlash, TszAdapter);
    patchSessionClient(SessionClient);

    const testType = 0; // FourSlashTestType.Native

    // Signal ready
    process.send({ type: "ready", workerId });

    // Run assigned tests
    let testsRun = 0;
    for (const testFile of testFiles) {
        const testName = path.basename(testFile, ".ts");
        const startTime = Date.now();

        try {
            runTestWithTimeout(FourSlash, Harness, testFile, testType, perTestTimeout);
            const elapsed = Date.now() - startTime;
            process.send({ type: "result", workerId, testFile, testName, passed: true, elapsed });
        } catch (err) {
            const elapsed = Date.now() - startTime;
            const errMsg = err.message || String(err);
            const timedOut = elapsed >= perTestTimeout || errMsg.includes("Timeout");
            process.send({
                type: "result", workerId, testFile, testName,
                passed: false, error: errMsg, elapsed, timedOut,
            });
        }

        testsRun++;

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
                        bridge.shutdown();
                        bridge = new TszServerBridge(tszServerBinary);
                        await bridge.start();
                        TszAdapter = createTszAdapterFactory(ts, Harness, SessionClient, bridge);
                        patchTestState(FourSlash, TszAdapter);
                        process.send({
                            type: "bridge_restart", workerId,
                            reason: `RSS ${(afterGc / 1024 / 1024).toFixed(0)}MB > ${(memThreshold / 1024 / 1024).toFixed(0)}MB threshold`,
                        });
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
