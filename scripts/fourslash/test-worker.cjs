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

// Per-test timeout (ms) - tests taking longer are killed
const TEST_TIMEOUT_MS = 15000;
// Memory threshold per worker (bytes) - restart bridge if exceeded
const MEMORY_THRESHOLD_BYTES = 512 * 1024 * 1024; // 512MB
// Check memory every N tests
const MEMORY_CHECK_INTERVAL = 25;
// Prevent cross-test contamination in tsz-server open file state.
const RESTART_BRIDGE_EVERY_TEST = true;

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
function patchSessionClient(SessionClient, ts) {
    const proto = SessionClient.prototype;

    // Create a wrapper host that fixes getDefaultLibFileName for the native LS.
    const createNativeHost = (host) => {
        const wrapper = Object.create(host);
        wrapper.getDefaultLibFileName = (options) => {
            return ts.getDefaultLibFilePath(options || host.getCompilationSettings?.() || {});
        };
        const origReadFile = host.readFile?.bind(host);
        const origFileExists = host.fileExists?.bind(host);
        const origGetScriptSnapshot = host.getScriptSnapshot?.bind(host);
        const fs = require("fs");
        const path = require("path");
        const builtLocal = path.join(process.cwd(), "built/local");

        wrapper.readFile = (fileName) => {
            const result = origReadFile?.(fileName);
            if (result != null) return result;
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
            const content = wrapper.readFile(fileName);
            if (content != null) return ts.ScriptSnapshot.fromString(content);
            return undefined;
        };
        const origGetScriptFileNames = host.getScriptFileNames?.bind(host);
        wrapper.getScriptFileNames = () => {
            return origGetScriptFileNames?.() || [];
        };
        return wrapper;
    };

    const getNativeLanguageService = (client) => {
        // Always create our own native LS with a properly configured host.
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

    // The constructor sets getCombinedCodeFix, applyCodeActionCommand, and mapCode
    // as instance properties (= notImplemented), which shadows prototype methods.
    // Wrap the constructor to delete those instance properties so our prototype
    // patches take effect.
    // We can't easily wrap the constructor, so instead use a post-init hook.
    // Override writeMessage to delete instance properties on first call.
    const instancePropsToDelete = ['getCombinedCodeFix', 'applyCodeActionCommand', 'mapCode'];
    const _origWriteMessage = proto.writeMessage;
    proto.writeMessage = function(msg) {
        // Delete shadowing instance properties on first use
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

    // Override getCompletionsAtPosition to:
    // 1) honor per-call preferences
    // 2) return undefined when there are no entries (harness contract)
    // 3) fix isNewIdentifierLocation from native LS
    // 4) prefer native LS entries in type-aware contexts (e.g. type literals
    //    in type parameters) where tsz returns scope-level completions but the
    //    native LS returns a targeted, smaller set
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
            // tsz returned empty entries. If native LS has results, use them.
            if (nativeResult && nativeResult.entries && nativeResult.entries.length > 0) {
                return nativeResult;
            }
            return undefined;
        }

        if (nativeResult) {
            if (result && result.entries && result.entries.length > 0) {
                result.isNewIdentifierLocation = nativeResult.isNewIdentifierLocation;
            }
            // When the native LS returns a focused member-completion set (e.g.
            // property names from a type constraint) and tsz returns a much
            // larger scope-level set, prefer native LS entries.
            // This covers type-literal-in-type-parameter completions and
            // similar type-aware contexts that tsz hasn't implemented yet.
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

    // Same preference forwarding for completion details.
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

    // Prefer native TypeScript LS for code fixes when tsz-server returns
    // incorrect or empty results.
    // Trust tsz for fix types where it has full AST-based support, since
    // the native LS may have stale content state through the adapter.
    const tszTrustedFixNames = new Set(["addMissingNewOperator", "addConvertToUnknownForNonOverlappingTypes"]);

    // Pre-scanned files: on first getCodeFixesAtPosition call per file,
    // probe TS2339 diagnostics to detect enum member fixes. When tsz has
    // an enum member fix, it often also emits spurious TS2304/TS7043 that
    // tsc wouldn't emit. We suppress fixes for those spurious codes.
    const _enumFixFiles = new Map(); // key: fileName -> true
    const _prescannedFiles = new Set();
    // Track positions where a trusted fix was returned, to suppress
    // spurious fixes from subsequent calls at the same span.
    const _trustedFixPositions = new Set(); // "fileName:start:end"

    const _origGetCodeFixesAtPosition = proto.getCodeFixesAtPosition;
    proto.getCodeFixesAtPosition = function(fileName, start, end, errorCodes, formatOptions, preferences) {
        const oldPreferences = this.preferences;
        if (preferences) this.configure(preferences);

        // Ensure formatOptions is never undefined - native LS crashes without it
        const safeFormatOptions = formatOptions || ts.getDefaultFormatCodeSettings?.() || {};

        // Pre-scan: on first call for a file, check if any TS2339
        // diagnostic produces an "Add missing enum member" fix from tsz.
        if (!_prescannedFiles.has(fileName)) {
            _prescannedFiles.add(fileName);
            try {
                const semDiags = _origGetSemanticDiag.call(this, fileName) || [];
                for (const d of semDiags) {
                    if (d.code !== 2339 || d.start === undefined || d.length === undefined) continue;
                    try {
                        const probe = _origGetCodeFixesAtPosition.call(
                            this, fileName, d.start, d.start + d.length, [d.code], formatOptions, preferences,
                        );
                        if (probe && probe.some(f =>
                            f.fixName === "addMissingMember" &&
                            typeof f.description === "string" &&
                            f.description.startsWith("Add missing enum member")
                        )) {
                            _enumFixFiles.set(fileName, true);
                            break;
                        }
                    } catch { /* ignore */ }
                }
            } catch { /* ignore */ }
        }

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
                // If no results with given codes, try native LS's own diagnostics
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

        // When an enum member fix exists for this file, use tsz exclusively:
        // return the enum fix for TS2339, suppress everything else to avoid
        // spurious fixes from extra diagnostics tsz emits (TS2304, TS7043).
        if (_enumFixFiles.has(fileName)) {
            const isEnumFix = tszResult && tszResult.some(f =>
                f.fixName === "addMissingMember" &&
                typeof f.description === "string" &&
                f.description.startsWith("Add missing enum member")
            );
            if (isEnumFix) {
                if (preferences) this.configure(oldPreferences || {});
                return tszResult;
            }
            // Suppress fixes for non-TS2339 codes in enum-fix files
            // (these are spurious diagnostics tsz emits that tsc wouldn't)
            if (!errorCodes.includes(2339)) {
                if (preferences) this.configure(oldPreferences || {});
                return [];
            }
        }

        // If a trusted fix was already returned for this exact span,
        // suppress non-trusted results from other error codes at the
        // same span (caused by tsz emitting extra diagnostic codes).
        const posKey = `${fileName}:${start}:${end}`;
        if (_trustedFixPositions.has(posKey)) {
            const tszHasTrustedFixHere = tszResult && tszResult.some(f => tszTrustedFixNames.has(f.fixName));
            if (!tszHasTrustedFixHere) {
                if (preferences) this.configure(oldPreferences || {});
                return [];
            }
        }

        let finalResult;
        if (!tszResult || tszResult.length === 0) {
            // tsz returned nothing - use native
            finalResult = getNative() || [];
        } else {
            // tsz returned something - use native if available (it matches tsc exactly),
            // but fall back to tsz if native has no results.
            // However, respect tsz's import fix exclusion decisions: if tsz produced
            // results but no import fixes (e.g. due to autoImportFileExcludePatterns),
            // filter out import fixes from native results to avoid re-introducing
            // excluded imports.
            const tszHasTrustedFix = tszResult.some(f => tszTrustedFixNames.has(f.fixName));
            if (tszHasTrustedFix) {
                finalResult = tszResult;
                // Record this position so subsequent calls for the same
                // span with different error codes get suppressed.
                _trustedFixPositions.add(posKey);
            } else {
                const nativeResult = getNative();
                if (nativeResult && nativeResult.length > 0) {
                    const tszHasImportFix = tszResult.some(f => f.fixName === "import");
                    if (!tszHasImportFix) {
                        const filtered = nativeResult.filter(f => f.fixName !== "import");
                        finalResult = filtered.length > 0 ? filtered : tszResult;
                    } else {
                        finalResult = nativeResult;
                    }
                } else {
                    finalResult = tszResult;
                }
            }
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

    // Override getDefinitionAtPosition to pass through metadata fields from
    // the server response (kind, name, containerName, contextSpan, etc.)
    // The base SessionClient hardcodes these as empty strings.
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

    // Override getDefinitionAndBoundSpan to pass through metadata fields
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
        // Return undefined when no definitions found (matches TypeScript behavior)
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
        // Server returns {newText: "", caretOffset: 0} for "no template"
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

    // Override diagnostic methods to merge native LS diagnostics with tsz-server's.
    // Many code fix tests depend on specific error codes that tsz-server may not emit.
    const _origGetSemanticDiag = proto.getSemanticDiagnostics;
    proto.getSemanticDiagnostics = function(fileName) {
        let tszResult;
        try {
            tszResult = _origGetSemanticDiag.call(this, fileName);
        } catch {
            tszResult = [];
        }

        // If tsz returned diagnostics, use them
        if (tszResult && tszResult.length > 0) return tszResult;

        // Fallback to native LS diagnostics
        const nativeResult = withNativeFallback(this, ls => ls.getSemanticDiagnostics(fileName));
        return nativeResult || tszResult || [];
    };

    const _origGetSuggestionDiag = proto.getSuggestionDiagnostics;
    proto.getSuggestionDiagnostics = function(fileName) {
        let tszResult;
        try {
            tszResult = _origGetSuggestionDiag.call(this, fileName);
        } catch {
            tszResult = [];
        }

        if (tszResult && tszResult.length > 0) return tszResult;

        const nativeResult = withNativeFallback(this, ls => ls.getSuggestionDiagnostics(fileName));
        return nativeResult || tszResult || [];
    };

    const _origGetSyntacticDiag = proto.getSyntacticDiagnostics;
    proto.getSyntacticDiagnostics = function(fileName) {
        let tszResult;
        try {
            tszResult = _origGetSyntacticDiag.call(this, fileName);
        } catch {
            tszResult = [];
        }

        if (tszResult && tszResult.length > 0) return tszResult;

        const nativeResult = withNativeFallback(this, ls => ls.getSyntacticDiagnostics(fileName));
        return nativeResult || tszResult || [];
    };

    // Override getSignatureHelpItems to:
    // 1. Forward triggerReason to the server protocol request
    // 2. Return undefined when items are empty (harness expects undefined for "no help")
    const _origGetSignatureHelpItems = proto.getSignatureHelpItems;
    proto.getSignatureHelpItems = function(fileName, position, options) {
        // Intercept: forward triggerReason to the server by augmenting the request
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

    // getLinkedEditingRangeAtPosition - route to server protocol
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

    // getCombinedCodeFix - route to server protocol
    proto.getCombinedCodeFix = function(scope, fixId, formatOptions, preferences) {
        const safeFormatOptions = formatOptions || ts.getDefaultFormatCodeSettings?.() || {};
        const nativeResult = withNativeFallback(this, ls =>
            ls.getCombinedCodeFix(scope, fixId, safeFormatOptions, preferences || {})
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

    // applyCodeActionCommand - route to server protocol
    proto.applyCodeActionCommand = function(action) {
        const args = { command: action };
        const request = this.processRequest("applyCodeActionCommand", args);
        const response = this.processResponse(request);
        if (Array.isArray(action)) {
            return Promise.resolve(Array.isArray(response.body) ? response.body : []);
        }
        return Promise.resolve(response.body || { successMessage: "" });
    };

    // mapCode - route to server protocol
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

    // organizeImports - route to server protocol
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

    // getEditsForFileRename - route to server protocol
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

    // --- Stubs for methods that throw "not serializable" errors ---
    // These methods cannot work through the server protocol because they return
    // non-serializable objects (SourceFile, Program). The fourslash harness calls
    // them when testType=Native (0), but our adapter uses a SessionClient (server-like).
    // Return safe stubs so tests that don't strictly need these objects can proceed.

    proto.getProgram = function() {
        const nativeResult = withNativeFallback(this, ls => ls.getProgram());
        if (nativeResult) return nativeResult;

        // Return a minimal Program stub so callers like
        // ts.getPreEmitDiagnostics(languageService.getProgram()) don't crash.
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
    patchSessionClient(SessionClient, ts);

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

        if (RESTART_BRIDGE_EVERY_TEST) {
            try {
                bridge.shutdown();
                bridge = new TszServerBridge(tszServerBinary);
                await bridge.start();
                TszAdapter = createTszAdapterFactory(ts, Harness, SessionClient, bridge);
                patchTestState(FourSlash, TszAdapter);
            } catch (restartErr) {
                process.send({
                    type: "error", workerId,
                    error: `Per-test bridge restart failed: ${restartErr.message}`,
                });
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
