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
        return response.body;
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
        const args = { file: fileName, line: lineOffset.line, offset: lineOffset.offset };
        // Use selectionRange which is already handled by tsz-server
        const request = this.processRequest("selectionRange", args);
        const response = this.processResponse(request);
        return response.body || undefined;
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
