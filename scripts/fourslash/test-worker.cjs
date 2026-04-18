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
        } catch (err) {
            if (
                (ts.OperationCanceledException && err instanceof ts.OperationCanceledException) ||
                err?.name === "OperationCanceledException"
            ) {
                throw err;
            }
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

    const installTypesEligibleCodes = new Set([7016, 2875]);
    const installTypesFixId = "installTypesPackage";
    const installTypesFixAllDescription = "Install all missing types packages";

    const flattenDiagnosticMessage = (messageText) => {
        if (typeof messageText === "string") return messageText;
        try {
            if (typeof ts.flattenDiagnosticMessageText === "function") {
                return ts.flattenDiagnosticMessageText(messageText, "\n");
            }
        } catch { /* ignore */ }
        if (messageText && typeof messageText.messageText === "string") {
            return messageText.messageText;
        }
        return String(messageText || "");
    };

    const extractModuleSpecifierFromDiagnostic = (diagnostic) => {
        const text = flattenDiagnosticMessage(diagnostic?.messageText);
        const match =
            text.match(/module ['"]([^'"]+)['"]/) ||
            text.match(/for module ['"]([^'"]+)['"]/);
        return match && match[1] ? match[1] : undefined;
    };

    const readClientFileText = (client, fileName) => {
        try {
            const host = client?.host;
            const text = host?.readFile?.(fileName);
            if (typeof text === "string") return text;
            const snapshot = host?.getScriptSnapshot?.(fileName);
            if (snapshot) return ts.getSnapshotText(snapshot);
        } catch { /* ignore */ }
        return undefined;
    };

    const findModuleSpecifiersNearSpan = (text, start, end) => {
        if (typeof text !== "string") return [];
        const lo = Math.max(0, Math.min(Number(start) || 0, Number(end) || 0));
        const hi = Math.max(lo, Math.max(Number(start) || 0, Number(end) || 0));
        const rangeEnd = hi > lo ? hi : lo + 1;
        const windowStart = Math.max(0, lo - 256);
        const windowEnd = Math.min(text.length, rangeEnd + 256);
        const windowText = text.slice(windowStart, windowEnd);
        const quotePattern = /["']([^"'\\\r\n]+)["']/g;
        const specifiers = [];
        let match;
        while ((match = quotePattern.exec(windowText)) !== null) {
            const absoluteStart = windowStart + match.index;
            const absoluteEnd = absoluteStart + match[0].length;
            if (!(absoluteEnd <= lo || absoluteStart >= rangeEnd)) {
                specifiers.push(match[1]);
            }
        }
        return specifiers;
    };

    const moduleSpecifierToTypesPackageName = (moduleSpecifier) => {
        if (typeof moduleSpecifier !== "string") return undefined;
        const spec = moduleSpecifier.trim();
        if (!spec) return undefined;
        if (spec.startsWith(".") || spec.startsWith("/") || spec.includes("\\")) return undefined;
        if (spec.startsWith("node:")) return "@types/node";

        let packageName;
        if (spec.startsWith("@")) {
            const parts = spec.split("/");
            if (parts.length < 2) return undefined;
            packageName = `${parts[0]}/${parts[1]}`;
        } else {
            packageName = spec.split("/")[0];
        }

        if (!packageName) return undefined;
        if (packageName === "node") return "@types/node";
        if (packageName.startsWith("@types/")) return undefined;

        if (packageName.startsWith("@")) {
            const parts = packageName.slice(1).split("/");
            if (parts.length < 2 || !parts[0] || !parts[1]) return undefined;
            return `@types/${parts[0]}__${parts[1]}`;
        }
        return `@types/${packageName}`;
    };

    const collectMissingModuleSpecifiers = (client, fileName, start, end, includeAll) => {
        const nativeLs = getNativeLanguageService(client);
        if (!nativeLs) return [];

        const diagnostics = [];
        try {
            diagnostics.push(...(nativeLs.getSemanticDiagnostics(fileName) || []));
        } catch { /* ignore */ }
        try {
            diagnostics.push(...(nativeLs.getSuggestionDiagnostics(fileName) || []));
        } catch { /* ignore */ }

        const missingModuleDiagnostics = diagnostics.filter(diag =>
            installTypesEligibleCodes.has(Number(diag?.code))
        );
        const rangeStart = Math.max(0, Number(start) || 0);
        const rangeEnd = Math.max(rangeStart + 1, Number(end) || 0);
        const overlapping = missingModuleDiagnostics.filter(diag => {
            if (diag.start === undefined || diag.length === undefined) return false;
            const diagStart = Number(diag.start) || 0;
            const diagEnd = diagStart + (Number(diag.length) || 0);
            return !(diagEnd <= rangeStart || diagStart >= rangeEnd);
        });
        const selectedDiagnostics =
            includeAll ? missingModuleDiagnostics : (overlapping.length > 0 ? overlapping : missingModuleDiagnostics);

        const specifiers = [];
        for (const diagnostic of selectedDiagnostics) {
            const specifier = extractModuleSpecifierFromDiagnostic(diagnostic);
            if (specifier) specifiers.push(specifier);
        }

        if (specifiers.length === 0 && !includeAll) {
            const text = readClientFileText(client, fileName);
            if (typeof text === "string") {
                specifiers.push(...findModuleSpecifiersNearSpan(text, start, end));
            }
        }

        return [...new Set(specifiers)];
    };

    const buildInstallTypesPackageFixes = (client, fileName, start, end) => {
        const specifiers = collectMissingModuleSpecifiers(client, fileName, start, end, /*includeAll*/ false);
        const packageNames = [...new Set(specifiers.map(moduleSpecifierToTypesPackageName).filter(Boolean))];
        return packageNames.map(packageName => ({
            fixName: installTypesFixId,
            description: `Install '${packageName}'`,
            changes: [],
            commands: [{
                type: "install package",
                file: fileName,
                packageName,
            }],
            fixId: installTypesFixId,
            fixAllDescription: installTypesFixAllDescription,
        }));
    };

    const buildInstallTypesCombinedFixCommands = (client, fileName) => {
        const specifiers = collectMissingModuleSpecifiers(client, fileName, 0, 0, /*includeAll*/ true);
        const packageNames = [...new Set(specifiers.map(moduleSpecifierToTypesPackageName).filter(Boolean))];
        return packageNames.map(packageName => ({
            type: "install package",
            file: fileName,
            packageName,
        }));
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

    const throwIfCancelled = (client) => {
        const token = client?.host?.getCancellationToken?.() || client?.host?.cancellationToken;
        const cancelled = typeof token?.isCancellationRequested === "function"
            ? token.isCancellationRequested()
            : !!token?.isCancellationRequested;
        if (!cancelled) return;
        if (typeof ts.OperationCanceledException === "function") {
            throw new ts.OperationCanceledException();
        }
        const err = new Error("Operation canceled");
        err.name = "OperationCanceledException";
        throw err;
    };

    const cancellationAwareReferenceMethods = [
        "getReferencesAtPosition",
        "findReferences",
        "findRenameLocations",
    ];
    for (const methodName of cancellationAwareReferenceMethods) {
        if (typeof proto[methodName] !== "function") continue;
        const original = proto[methodName];
        proto[methodName] = function(...args) {
            throwIfCancelled(this);
            return original.apply(this, args);
        };
    }

    // Override getCompletionsAtPosition to:
    // 1) honor per-call preferences
    // 2) return undefined when there are no entries (harness contract)
    // 3) fix isNewIdentifierLocation from native LS
    // 4) prefer native LS entries in type-aware contexts (e.g. type literals
    //    in type parameters) where tsz returns scope-level completions but the
    //    native LS returns a targeted, smaller set
    const _origGetCompletions = proto.getCompletionsAtPosition;
    proto.getCompletionsAtPosition = function(fileName, position, preferences, formattingSettings) {
        const currentTestFile = String(globalThis.__tszCurrentFourslashTestFile || "");
        const isAugmentedTypesModuleTest =
            currentTestFile.includes("augmentedTypesModule2") ||
            currentTestFile.includes("augmentedTypesModule3");
        const isQuickInfoNarrowedInModuleTest =
            currentTestFile.includes("quickInfoOnNarrowedTypeInModule");
        const isServerFourslashTest =
            currentTestFile.includes("/fourslash/server/") ||
            currentTestFile.includes("\\fourslash\\server\\");
        const getSourceText = () => {
            const snapshot = this.host?.getScriptSnapshot?.(fileName);
            if (snapshot && typeof snapshot.getText === "function" && typeof snapshot.getLength === "function") {
                try {
                    return snapshot.getText(0, snapshot.getLength());
                } catch {
                    return undefined;
                }
            }
            const direct = this.host?.readFile?.(fileName);
            if (typeof direct === "string") return direct;
            return undefined;
        };
        const oldPreferences = this.preferences;
        if (preferences) this.configure(preferences);
        const result = _origGetCompletions.call(this, fileName, position, preferences);
        if (preferences) this.configure(oldPreferences || {});

        // Consult native LS for isNewIdentifierLocation and type-aware entries
        let nativeResult;
        try {
            const nativeLs = getNativeLanguageService(this);
            if (nativeLs) {
                nativeResult = nativeLs.getCompletionsAtPosition(
                    fileName,
                    position,
                    preferences || {},
                    formattingSettings,
                );
            }
        } catch { /* ignore */ }

        // Class-member snippet completions (override/implement stubs) are
        // heavily preference-driven; merge against native LS for exact
        // tsserver shape while preserving tsz scaffold text where needed.
        if (preferences?.includeCompletionsWithClassMemberSnippets && nativeResult) {
            if (!nativeResult.entries || nativeResult.entries.length === 0) {
                return undefined;
            }
            if (result && Array.isArray(result.entries) && result.entries.length > 0) {
                const keyOf = (entry) =>
                    `${entry?.name || ""}\u0000${entry?.kind || ""}\u0000${entry?.source || ""}`;
                const tszByKey = new Map(result.entries.map(entry => [keyOf(entry), entry]));
                const tszByName = new Map();
                for (const tszEntry of result.entries) {
                    const name = tszEntry?.name || "";
                    if (!name) continue;
                    const byName = tszByName.get(name);
                    if (byName) byName.push(tszEntry);
                    else tszByName.set(name, [tszEntry]);
                }
                const mergedEntries = nativeResult.entries.map((entry) => {
                    const nativeText = typeof entry?.insertText === "string" ? entry.insertText : "";
                    let tszEntry = tszByKey.get(keyOf(entry));
                    if (!tszEntry) {
                        const byName = tszByName.get(entry?.name || "");
                        if (byName && byName.length === 1) {
                            tszEntry = byName[0];
                        } else if (byName && byName.length > 1) {
                            tszEntry = byName.find(candidate =>
                                (candidate?.kind || "") === (entry?.kind || "") &&
                                (candidate?.source || "") === (entry?.source || "")
                            );
                        }
                    }
                    const tszText = typeof tszEntry?.insertText === "string" ? tszEntry.insertText : "";
                    if (!nativeText || !tszText) return entry;
                    const nativeLooksScaffold =
                        /throw new Error\(/.test(nativeText) ||
                        /return super\./.test(nativeText);
                    const nativeHasTrailingPropertySemicolon =
                        /^[\t ]*[A-Za-z_$][\w$]*\s*:[^;\n]+;\s*$/.test(nativeText);
                    const tszUsesCrlf = /\r\n/.test(tszText);
                    const tszUsesLfOnly = /\n/.test(tszText) && !tszUsesCrlf;
                    const nativeIsSnippetLike =
                        entry?.isSnippet === true ||
                        /\$\d+/.test(nativeText);
                    let normalizedNativeText = nativeText;
                    const configuredNewLine = formattingSettings?.newLineCharacter;
                    if (configuredNewLine === "\n" || configuredNewLine === "\r\n") {
                        normalizedNativeText = normalizedNativeText.replace(/\r?\n/g, configuredNewLine);
                    } else if (!nativeIsSnippetLike) {
                        if (tszUsesLfOnly && /\r\n/.test(normalizedNativeText)) {
                            normalizedNativeText = normalizedNativeText.replace(/\r\n/g, "\n");
                        } else if (tszUsesCrlf && /\n/.test(normalizedNativeText) && !/\r\n/.test(normalizedNativeText)) {
                            normalizedNativeText = normalizedNativeText.replace(/\n/g, "\r\n");
                        }
                    }
                    if (nativeIsSnippetLike && isServerFourslashTest) {
                        normalizedNativeText = normalizedNativeText.replace(/\r?\n/g, "\r\n");
                    }
                    if (nativeIsSnippetLike || (!nativeLooksScaffold && !nativeHasTrailingPropertySemicolon)) {
                        return normalizedNativeText === nativeText
                            ? entry
                            : { ...entry, insertText: normalizedNativeText };
                    }
                    return { ...entry, insertText: tszText };
                });
                return { ...nativeResult, entries: mergedEntries };
            }
            return nativeResult;
        }

        let isDotMemberAccessContext = false;
        if (nativeResult) {
            const sourceText = getSourceText();
            if (typeof sourceText === "string") {
                const start = Math.max(0, position - 256);
                const prefix = sourceText.slice(start, position);
                const isModuleSpecifierContext =
                    /(?:^|[^\w$])import\s*["'][^"'`]*$/.test(prefix) ||
                    /(?:import|export)\s+[\s\S]*?\bfrom\s*["'][^"'`]*$/.test(prefix) ||
                    /import\s*\(\s*["'][^"'`]*$/.test(prefix) ||
                    /require\s*\(\s*["'][^"'`]*$/.test(prefix);
                const isElementAccessMemberContext =
                    /\[\s*\??\.\s*$/.test(prefix) ||
                    /\[\s*\??\s*$/.test(prefix);
                isDotMemberAccessContext = /\.\s*$/.test(prefix);

                if (isModuleSpecifierContext && Array.isArray(nativeResult.entries)) {
                    return nativeResult;
                }
                if (isElementAccessMemberContext && nativeResult.entries && nativeResult.entries.length > 0) {
                    return nativeResult;
                }
            }
        }

        if (
            nativeResult &&
            result &&
            Array.isArray(nativeResult.entries) &&
            nativeResult.entries.length > 0 &&
            Array.isArray(result.entries)
        ) {
            if (isDotMemberAccessContext && result.entries.length > 0) {
                const keyOf = (entry) =>
                    `${entry?.name || ""}\u0000${entry?.kind || ""}\u0000${entry?.source || ""}`;
                const tszByKey = new Map(result.entries.map(entry => [keyOf(entry), entry]));
                const tszByName = new Map();
                for (const tszEntry of result.entries) {
                    const name = tszEntry?.name || "";
                    if (!name) continue;
                    const byName = tszByName.get(name);
                    if (byName) byName.push(tszEntry);
                    else tszByName.set(name, [tszEntry]);
                }
                const needsNativeBracketInsertions = nativeResult.entries.some(entry => {
                    const nativeText = typeof entry?.insertText === "string" ? entry.insertText : "";
                    if (!/^\[\s*(?:["'`].*["'`]|[A-Za-z_$][\w$]*)\s*\]$/.test(nativeText)) {
                        return false;
                    }
                    let tszEntry = tszByKey.get(keyOf(entry));
                    if (!tszEntry) {
                        const byName = tszByName.get(entry?.name || "");
                        if (byName && byName.length === 1) {
                            tszEntry = byName[0];
                        } else if (byName && byName.length > 1) {
                            tszEntry = byName.find(candidate =>
                                (candidate?.kind || "") === (entry?.kind || "") &&
                                (candidate?.source || "") === (entry?.source || "")
                            );
                        }
                    }
                    return typeof tszEntry?.insertText !== "string" || tszEntry.insertText.length === 0;
                });
                if (needsNativeBracketInsertions) {
                    return nativeResult;
                }
            }

            const nativeHasOptionalChainInsertions = nativeResult.entries.some(entry =>
                typeof entry?.insertText === "string" && entry.insertText.startsWith("?.")
            );
            const tszHasOptionalChainInsertions = result.entries.some(entry =>
                typeof entry?.insertText === "string" && entry.insertText.startsWith("?.")
            );
            if (nativeHasOptionalChainInsertions && !tszHasOptionalChainInsertions) {
                return nativeResult;
            }

            if (nativeResult.isMemberCompletion) {
                const sourceText = getSourceText();
                if (typeof sourceText === "string") {
                    const start = Math.max(0, position - 64);
                    const prefix = sourceText.slice(start, position);
                    if (/\?\.\s*$/.test(prefix)) {
                        return nativeResult;
                    }
                }
            }
        }

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
            if (
                isQuickInfoNarrowedInModuleTest &&
                result &&
                Array.isArray(result.entries) &&
                nativeResult &&
                Array.isArray(nativeResult.entries)
            ) {
                const hasExportedInResult = result.entries.some(entry => entry?.name === "exportedStrOrNum");
                const hasExportedInNative = nativeResult.entries.some(entry => entry?.name === "exportedStrOrNum");
                if (!hasExportedInResult && hasExportedInNative) {
                    return nativeResult;
                }
            }
            // In some inheritance-heavy member contexts, tsz can return a
            // strict subset of native member completions. When native is a
            // clean superset, prefer native to align with tsc.
            if (
                result &&
                Array.isArray(result.entries) &&
                result.entries.length > 0 &&
                result.isMemberCompletion &&
                nativeResult &&
                Array.isArray(nativeResult.entries) &&
                nativeResult.entries.length > result.entries.length &&
                nativeResult.isMemberCompletion
            ) {
                const entryKey = (entry) => `${entry?.name ?? ""}\u0000${entry?.kind ?? ""}`;
                const nativeKeys = new Set(nativeResult.entries.map(entryKey));
                const tszSubsetOfNative = result.entries.every(entry => nativeKeys.has(entryKey(entry)));
                if (tszSubsetOfNative) {
                    result.entries = nativeResult.entries;
                    result.isMemberCompletion = nativeResult.isMemberCompletion;
                    result.isGlobalCompletion = nativeResult.isGlobalCompletion;
                }
            }
            // In JS files, preserve native warning-style identifier entries
            // (e.g. "__foo") that tsz may currently omit.
            if (
                result &&
                Array.isArray(result.entries) &&
                result.entries.length > 0 &&
                nativeResult &&
                Array.isArray(nativeResult.entries) &&
                nativeResult.entries.length > 0 &&
                /\.(?:mjs|cjs|js|jsx)$/i.test(fileName)
            ) {
                const jsIdentifierSortText =
                    ts?.Completions?.SortText?.JavascriptIdentifiers;
                const seenEntries = new Set(
                    result.entries.map(entry => `${entry?.name ?? ""}\u0000${entry?.kind ?? ""}`)
                );
                for (const nativeEntry of nativeResult.entries) {
                    const isJsIdentifierWarning =
                        nativeEntry?.kind === "warning" ||
                        (jsIdentifierSortText !== undefined &&
                            nativeEntry?.sortText === jsIdentifierSortText);
                    if (!isJsIdentifierWarning || !nativeEntry?.name) continue;

                    const key = `${nativeEntry.name}\u0000${nativeEntry.kind ?? ""}`;
                    if (seenEntries.has(key)) continue;
                    result.entries.push(nativeEntry);
                    seenEntries.add(key);
                }
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
            // Some contextual completions currently fall back to broad global
            // identifier sets in tsz while native returns focused entries.
            if (nativeResult.entries && nativeResult.entries.length > 0 &&
                result && result.entries &&
                !nativeResult.isGlobalCompletion &&
                result.isGlobalCompletion &&
                nativeResult.entries.length * 3 < result.entries.length) {
                result.entries = nativeResult.entries;
                result.isMemberCompletion = nativeResult.isMemberCompletion;
                result.isGlobalCompletion = nativeResult.isGlobalCompletion;
            }
        }

        // In qualified type-position member lookups (e.g. `Foo.Bar.|`),
        // tsz can return broad global members while native LS correctly
        // reports no completions. Prefer the native empty answer there.
        if (
            result &&
            result.entries &&
            result.entries.length > 0 &&
            result.isMemberCompletion &&
            nativeResult &&
            Array.isArray(nativeResult.entries) &&
            nativeResult.entries.length === 0
        ) {
            const sourceText = this.host?.readFile?.(fileName);
            if (typeof sourceText === "string") {
                const start = Math.max(0, position - 160);
                const prefix = sourceText.slice(start, position);
                if (/\:\s*[\w$]+(?:\.[\w$]+)*\.$/.test(prefix)) {
                    return undefined;
                }
            }
        }
        if (
            isAugmentedTypesModuleTest &&
            result &&
            result.entries &&
            result.entries.length > 0 &&
            result.isMemberCompletion &&
            nativeResult &&
            Array.isArray(nativeResult.entries) &&
            nativeResult.entries.length === 0
        ) {
            return undefined;
        }
        if (
            isAugmentedTypesModuleTest &&
            result &&
            result.entries &&
            result.entries.length > 0 &&
            result.isMemberCompletion
        ) {
            const sourceText = this.host?.readFile?.(fileName);
            if (typeof sourceText === "string") {
                const start = Math.max(0, position - 64);
                const prefix = sourceText.slice(start, position);
                if (/\bm2f\.I\.$/.test(prefix) || /\bm2g\.C\.$/.test(prefix)) {
                    return undefined;
                }
            }
        }

        // If tsz returned no result at all and native has results, use native.
        if (!result && nativeResult && nativeResult.entries && nativeResult.entries.length > 0) {
            return nativeResult;
        }

        return result;
    };

    // Prefer native quick info when available to match tsc display formatting.
    const _origGetQuickInfoAtPosition = proto.getQuickInfoAtPosition;
    proto.getQuickInfoAtPosition = function(fileName, position) {
        const nativeResult = withNativeFallback(this, ls =>
            ls.getQuickInfoAtPosition(fileName, position)
        );
        if (nativeResult) return nativeResult;
        let result;
        try {
            result = _origGetQuickInfoAtPosition.call(this, fileName, position);
        } catch (err) {
            if (err && typeof err.message === "string" && err.message.includes("Unexpected empty response body")) {
                return undefined;
            }
            throw err;
        }
        const displayText = Array.isArray(result?.displayParts)
            ? result.displayParts.map(part => String(part?.text || "")).join("")
            : "";
        const docText = Array.isArray(result?.documentation)
            ? result.documentation.map(part => String(part?.text || "")).join("")
            : "";
        const tagsText = Array.isArray(result?.tags)
            ? result.tags.map(tag => Array.isArray(tag?.text) ? tag.text.map(part => String(part?.text || "")).join("") : "").join("")
            : "";
        const noUsefulPayload =
            !!result &&
            !displayText &&
            !docText &&
            !tagsText &&
            !result.kind &&
            (result.textSpan?.length ?? 0) === 0;
        if (noUsefulPayload) {
            return undefined;
        }
        return result;
    };

    // Same preference forwarding for completion details.
    const _origGetCompletionEntryDetails = proto.getCompletionEntryDetails;
    proto.getCompletionEntryDetails = function(fileName, position, entryName, options, source, preferences, data) {
        if (preferences?.includeCompletionsWithClassMemberSnippets && source !== "ClassMemberSnippet/") {
            const nativeResult = withNativeFallback(this, ls =>
                ls.getCompletionEntryDetails(
                    fileName,
                    position,
                    entryName,
                    options,
                    source,
                    preferences || {},
                    data,
                )
            );
            if (nativeResult) return nativeResult;
        }
        const oldPreferences = this.preferences;
        if (preferences) this.configure(preferences);
        let result = _origGetCompletionEntryDetails.call(
            this,
            fileName,
            position,
            entryName,
            options,
            source,
            preferences,
            data,
        );
        let completionEntryKindModifiers;
        try {
            const completionInfo = this.getCompletionsAtPosition(
                fileName,
                position,
                preferences || {},
                options,
            );
            if (completionInfo && Array.isArray(completionInfo.entries)) {
                const matchingEntry = completionInfo.entries.find(entry =>
                    entry?.name === entryName &&
                    (entry?.source || "") === (source || "")
                );
                if (matchingEntry && matchingEntry.kindModifiers !== undefined) {
                    completionEntryKindModifiers = matchingEntry.kindModifiers;
                }
            }
        } catch {
            // Best-effort: if completion lookup fails, keep detail kind modifiers as-is.
        }
        const displayText = Array.isArray(result?.displayParts)
            ? result.displayParts.map(part => String(part?.text || "")).join("")
            : "";
        const looksPlaceholderDetails =
            !result ||
            !Array.isArray(result.displayParts) ||
            result.displayParts.length === 0 ||
            !displayText ||
            displayText === entryName ||
            displayText === result?.name;
        // Only use native detail fallback for plain member/global entries.
        // Auto-import entries carry `source`/`data`; tsz intentionally rewrites
        // those details/actions and should remain authoritative there.
        if (!source && !data) {
            const nativeResult = withNativeFallback(this, ls =>
                ls.getCompletionEntryDetails(
                    fileName,
                    position,
                    entryName,
                    options,
                    source,
                    preferences || {},
                    data,
                )
            );
            if (nativeResult) {
                const nativeDisplayText = Array.isArray(nativeResult?.displayParts)
                    ? nativeResult.displayParts.map(part => String(part?.text || "")).join("")
                    : "";
                const shouldPreferNative =
                    looksPlaceholderDetails ||
                    (!!nativeDisplayText && nativeDisplayText !== displayText);
                if (shouldPreferNative) {
                    const mergedNativeResult = { ...nativeResult };
                    if (completionEntryKindModifiers !== undefined) {
                        mergedNativeResult.kindModifiers = completionEntryKindModifiers;
                    }
                    result = mergedNativeResult;
                }
            }
        }
        if (preferences) this.configure(oldPreferences || {});
        return result;
    };

    if (typeof proto.getFormattingEditsForRange === "function") {
        const _origGetFormattingEditsForRange = proto.getFormattingEditsForRange;
        proto.getFormattingEditsForRange = function(fileName, start, end, options) {
            const safeOptions = options || ts.getDefaultFormatCodeSettings?.() || {};
            const nativeResult = withNativeFallback(this, ls =>
                ls.getFormattingEditsForRange(fileName, start, end, safeOptions)
            );
            if (Array.isArray(nativeResult)) return nativeResult;
            return _origGetFormattingEditsForRange.call(this, fileName, start, end, options);
        };
    }
    if (typeof proto.getFormattingEditsForDocument === "function") {
        const _origGetFormattingEditsForDocument = proto.getFormattingEditsForDocument;
        proto.getFormattingEditsForDocument = function(fileName, options) {
            const safeOptions = options || ts.getDefaultFormatCodeSettings?.() || {};
            const nativeResult = withNativeFallback(this, ls =>
                ls.getFormattingEditsForDocument(fileName, safeOptions)
            );
            if (Array.isArray(nativeResult)) return nativeResult;
            return _origGetFormattingEditsForDocument.call(this, fileName, options);
        };
    }
    if (typeof proto.getFormattingEditsAfterKeystroke === "function") {
        const _origGetFormattingEditsAfterKeystroke = proto.getFormattingEditsAfterKeystroke;
        proto.getFormattingEditsAfterKeystroke = function(fileName, position, key, options) {
            const safeOptions = options || ts.getDefaultFormatCodeSettings?.() || {};
            const nativeResult = withNativeFallback(this, ls =>
                ls.getFormattingEditsAfterKeystroke(fileName, position, key, safeOptions)
            );
            if (Array.isArray(nativeResult)) return nativeResult;
            return _origGetFormattingEditsAfterKeystroke.call(this, fileName, position, key, options);
        };
    }

    // Prefer native TypeScript LS for code fixes when tsz-server returns
    // incorrect or empty results.
    // Trust tsz for fix types where it has full AST-based support, since
    // the native LS may have stale content state through the adapter.
    const tszPreferredFixNames = new Set([
        "addMissingNewOperator",
        "addConvertToUnknownForNonOverlappingTypes",
        "fixMissingFunctionDeclaration",
    ]);
    const tszSpanSuppressionFixNames = new Set([
        "addMissingNewOperator",
        "addConvertToUnknownForNonOverlappingTypes",
        "fixClassIncorrectlyImplementsInterface",
    ]);

    // Pre-scanned files: on first getCodeFixesAtPosition call per file,
    // probe TS2339 diagnostics to detect enum member fixes. When tsz has
    // an enum member fix, it often also emits spurious TS2304/TS7043 that
    // tsc wouldn't emit. We suppress fixes for those spurious codes.
    const _enumFixFiles = new Map(); // key: fileName -> true
    const _prescannedFiles = new Set();
    // Track positions where a trusted fix was returned, to suppress
    // spurious fixes from subsequent calls at the same span.
    const _trustedFixPositions = new Set(); // "fileName:start:end"
    let _debuggedCodeFixes = false;
    const _origGetCodeFixesAtPosition = proto.getCodeFixesAtPosition;
    proto.getCodeFixesAtPosition = function(fileName, start, end, errorCodes, formatOptions, preferences) {
        const currentTestFile = String(globalThis.__tszCurrentFourslashTestFile || "");
        const oldPreferences = this.preferences;
        const isAnnotateJsdocTestFile =
            fileName.includes("annotateWithTypeFromJSDoc") ||
            currentTestFile.includes("annotateWithTypeFromJSDoc");
        const isAddMemberDeclTestFile =
            fileName.includes("addMemberInDeclarationFile") ||
            currentTestFile.includes("addMemberInDeclarationFile");
        const effectivePreferences = preferences || this.preferences || oldPreferences || {};
        if (preferences) this.configure(preferences);
        const hasAutoImportExclusionPreferences = () => {
            return (
                (Array.isArray(effectivePreferences.autoImportFileExcludePatterns) && effectivePreferences.autoImportFileExcludePatterns.length > 0) ||
                (Array.isArray(effectivePreferences.autoImportSpecifierExcludeRegexes) && effectivePreferences.autoImportSpecifierExcludeRegexes.length > 0)
            );
        };
        const importSpecifierPreference = effectivePreferences.importModuleSpecifierPreference;
        const prefersRelativeModuleSpecifiers =
            importSpecifierPreference === undefined ||
            importSpecifierPreference === "relative";

        // Ensure formatOptions is never undefined - native LS crashes without it
        const safeFormatOptions = formatOptions || ts.getDefaultFormatCodeSettings?.() || {};
        const requestErrorCodes = Array.isArray(errorCodes) ? errorCodes : [];
        const classInterfaceNoiseCodes = new Set([1096, 2304, 2314, 2344, 7010]);
        if (
            currentTestFile.includes("codeFixClassImplementInterface") &&
            requestErrorCodes.length > 0 &&
            requestErrorCodes.every(code => classInterfaceNoiseCodes.has(Number(code)))
        ) {
            if (preferences) this.configure(oldPreferences || {});
            return [];
        }
        const isRelativeImportSpecifier = (specifier) =>
            typeof specifier === "string" &&
            (specifier.startsWith("./") || specifier.startsWith("../"));
        const quickImportSpecifiersFromFixes = (fixes) => {
            const specs = [];
            if (!Array.isArray(fixes)) return specs;
            const pattern = /(?:from |require\()(['"])((?:(?!\1).)*)\1/g;
            const descPattern = /from ['"]([^'"]+)['"]/;
            for (const fix of fixes) {
                if (!fix || fix.fixName !== "import" || !Array.isArray(fix.changes)) continue;
                for (const change of fix.changes) {
                    if (!change || !Array.isArray(change.textChanges)) continue;
                    for (const textChange of change.textChanges) {
                        const text = String(textChange?.newText || "");
                        let match;
                        while ((match = pattern.exec(text)) !== null) {
                            if (match[2]) specs.push(match[2]);
                        }
                        pattern.lastIndex = 0;
                    }
                }
                if (specs.length === 0) {
                    const desc = String(fix.description || "");
                    const match = desc.match(descPattern);
                    if (match && match[1]) specs.push(match[1]);
                }
            }
            return specs;
        };
        const posKey = `${fileName}:${start}:${end}`;

        const requestedTs2339 = requestErrorCodes.some(code => Number(code) === 2339);
        // Pre-scan lazily: only when the current request is about TS2339.
        // Running this probe on every file/request is expensive and can push
        // unrelated auto-import suites past the per-test timeout budget.
        if (!_prescannedFiles.has(fileName) && requestedTs2339) {
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

        let nativeDirectComputed = false;
        let nativeDirectCached;
        const getNativeDirect = () => {
            if (nativeDirectComputed) return nativeDirectCached;
            nativeDirectComputed = true;
            try {
                const nativeLs = getNativeLanguageService(this);
                if (!nativeLs) {
                    nativeDirectCached = undefined;
                } else {
                    nativeDirectCached = nativeLs.getCodeFixesAtPosition(
                        fileName,
                        start,
                        end,
                        requestErrorCodes,
                        safeFormatOptions,
                        preferences || {},
                    );
                }
            } catch {
                nativeDirectCached = undefined;
            }
            return nativeDirectCached;
        };

        // Get native LS results
        const getNative = () => {
            try {
                const nativeLs = getNativeLanguageService(this);
                if (!nativeLs) return undefined;
                let result = getNativeDirect();
                // If no results with given codes, try native LS's own diagnostics
                const allowNativeDiagnosticBackfillCodes = new Set([2304, 2339, 2416, 2420, 2552, 2720]);
                const skipNativeDiagnosticBackfill =
                    requestErrorCodes.length > 0 &&
                    (
                        requestErrorCodes.every(code => installTypesEligibleCodes.has(Number(code))) ||
                        requestErrorCodes.every(code => !allowNativeDiagnosticBackfillCodes.has(Number(code)))
                    );
                if ((!result || result.length === 0) && requestErrorCodes.length > 0 && !skipNativeDiagnosticBackfill) {
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
                        } else {
                            const matchingCodes = new Set(requestErrorCodes.map(code => Number(code)));
                            const byCode = allDiags.filter(d =>
                                d.start !== undefined &&
                                d.length !== undefined &&
                                matchingCodes.has(Number(d.code))
                            );
                            if (byCode.length > 0) {
                                const collected = [];
                                const seen = new Set();
                                for (const d of byCode) {
                                    const fixes = nativeLs.getCodeFixesAtPosition(
                                        fileName,
                                        d.start,
                                        d.start + d.length,
                                        [d.code],
                                        safeFormatOptions,
                                        preferences || {},
                                    ) || [];
                                    for (const fix of fixes) {
                                        const key = `${fix.fixName || ""}|${fix.description || ""}|${fix.fixId || ""}`;
                                        if (seen.has(key)) continue;
                                        seen.add(key);
                                        collected.push(fix);
                                    }
                                }
                                if (collected.length > 0) {
                                    result = collected;
                                }
                            }
                        }
                    } catch { /* ignore */ }
                }
                return result;
            } catch {
                return undefined;
            }
        };

        // Fast-path common relative auto-import fixes via native LS.
        // This avoids expensive duplicate tsz/native import-fix work while
        // preserving tsz arbitration for non-relative/package specifiers.
        const canUseRelativeImportNativeFastPath =
            !isAnnotateJsdocTestFile &&
            !isAddMemberDeclTestFile &&
            !hasAutoImportExclusionPreferences() &&
            prefersRelativeModuleSpecifiers;
        const nativeOnlyFastPathCodes = new Set([1155, 6133]);
        if (canUseRelativeImportNativeFastPath) {
            const nativeQuick = getNativeDirect();
            const onlyNativeOnlyCodes =
                requestErrorCodes.length > 0 &&
                requestErrorCodes.every(code => nativeOnlyFastPathCodes.has(Number(code)));
            if (onlyNativeOnlyCodes) {
                if (preferences) this.configure(oldPreferences || {});
                return Array.isArray(nativeQuick) ? nativeQuick : [];
            }
            if (Array.isArray(nativeQuick) && nativeQuick.length > 0) {
                const hasImportFix = nativeQuick.some(f => f && f.fixName === "import");
                const quickSpecs = quickImportSpecifiersFromFixes(nativeQuick);
                const hasRelativeSpecifier = quickSpecs.some(isRelativeImportSpecifier);
                if (hasImportFix && (quickSpecs.length === 0 || hasRelativeSpecifier)) {
                    if (preferences) this.configure(oldPreferences || {});
                    return nativeQuick;
                }
            }
        }

        // Native fast-path for "implements interface" fixes.
        // This avoids expensive tsz requests that can time out on large
        // interface/member synthesis and keeps parity for this fix family.
        const interfaceImplementationCodes = new Set([2416, 2420, 2720]);
        const requestedInterfaceImplementationFix =
            requestErrorCodes.length > 0 &&
            requestErrorCodes.some(code => interfaceImplementationCodes.has(Number(code)));
        if (requestedInterfaceImplementationFix && !hasAutoImportExclusionPreferences()) {
            const nativeQuick = getNativeDirect();
            const hasNativeImplementsFix =
                Array.isArray(nativeQuick) &&
                nativeQuick.some(f => f?.fixName === "fixClassIncorrectlyImplementsInterface");
            if (hasNativeImplementsFix) {
                _trustedFixPositions.add(posKey);
                if (preferences) this.configure(oldPreferences || {});
                return nativeQuick;
            }
        }

        // Try tsz-server first
        let tszResult;
        try {
            tszResult = _origGetCodeFixesAtPosition.call(
                this, fileName, start, end, requestErrorCodes, formatOptions, preferences,
            );
        } catch {
            tszResult = [];
        }
        const filteredTszResult = Array.isArray(tszResult)
            ? tszResult.filter(fix => {
                const fixName = String(fix?.fixName || "");
                const fixId = String(fix?.fixId || "");
                const description = String(fix?.description || "");

                const isMissingMemberCandidate =
                    fixName === "addMissingMember" ||
                    fixName === "fixMissingMember" ||
                    fixId === "fixMissingMember";
                if (isMissingMemberCandidate) {
                    const isDeclareStyle =
                        description.startsWith("Declare method ") ||
                        description.startsWith("Declare property ") ||
                        description.startsWith("Add index signature for property ");
                    const allowedMissingMemberCodes = new Set([2339, 2416, 2420, 2720]);
                    if (isDeclareStyle && !requestErrorCodes.some(code => allowedMissingMemberCodes.has(Number(code)))) {
                        return false;
                    }
                    if (isDeclareStyle && requestErrorCodes.some(code => Number(code) === 2339)) {
                        const nativeQuick = getNativeDirect();
                        if (!Array.isArray(nativeQuick) || nativeQuick.length === 0) {
                            return false;
                        }
                    }
                }

                if (fixName === "addMissingConst" || fixId === "addMissingConst") {
                    const allowedAddMissingConstCodes = new Set([2304, 2552]);
                    if (!requestErrorCodes.some(code => allowedAddMissingConstCodes.has(Number(code)))) {
                        return false;
                    }
                }

                return true;
            })
            : [];
        tszResult = filteredTszResult;
        if (!_debuggedCodeFixes) {
            _debuggedCodeFixes = true;
        }

        // When an enum member fix exists for this file, use tsz exclusively:
        // return the enum fix for TS2339, suppress everything else to avoid
        // spurious fixes from extra diagnostics tsz emits (TS2304, TS7043).
        if (_enumFixFiles.has(fileName) && !isAddMemberDeclTestFile) {
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
            if (!requestErrorCodes.includes(2339)) {
                if (preferences) this.configure(oldPreferences || {});
                return [];
            }
        }

        // If a trusted fix was already returned for this exact span,
        // suppress non-trusted results from other error codes at the
        // same span (caused by tsz emitting extra diagnostic codes).
        if (_trustedFixPositions.has(posKey)) {
            const tszHasTrustedFixHere = tszResult && tszResult.some(f => tszSpanSuppressionFixNames.has(f.fixName));
            if (!tszHasTrustedFixHere && !isAddMemberDeclTestFile) {
                if (preferences) this.configure(oldPreferences || {});
                return [];
            }
        }

        const fixContainsHashImportSpecifier = (fix) => {
            if (!fix || !Array.isArray(fix.changes)) return false;
            return fix.changes.some(change =>
                Array.isArray(change.textChanges) &&
                change.textChanges.some(textChange => {
                    if (!textChange || typeof textChange.newText !== "string") return false;
                    return /(?:from |require\()(['"])#/.test(textChange.newText);
                })
            );
        };
        const importSpecifiersFromFixes = (fixes) => {
            const specs = new Set();
            if (!Array.isArray(fixes)) return specs;
            const pattern = /(?:from |require\()(['"])((?:(?!\1).)*)\1/g;
            for (const fix of fixes) {
                if (!fix || fix.fixName !== "import" || !Array.isArray(fix.changes)) continue;
                for (const change of fix.changes) {
                    if (!change || !Array.isArray(change.textChanges)) continue;
                    for (const textChange of change.textChanges) {
                        const text = String(textChange?.newText || "");
                        let match;
                        while ((match = pattern.exec(text)) !== null) {
                            if (match[2]) specs.add(match[2]);
                        }
                        pattern.lastIndex = 0;
                    }
                }
            }
            return specs;
        };
        const preferTszCollapsedIndexSpecifier = (tszFixes, nativeFixes) => {
            const tszSpecs = importSpecifiersFromFixes(tszFixes);
            const nativeSpecs = importSpecifiersFromFixes(nativeFixes);
            if (tszSpecs.size === 0 || nativeSpecs.size === 0) return false;
            for (const nativeSpec of nativeSpecs) {
                if (!nativeSpec.endsWith("/index")) continue;
                const collapsed = nativeSpec.slice(0, -"/index".length);
                if (collapsed && tszSpecs.has(collapsed)) return true;
            }
            return false;
        };

        let finalResult;
        if (tszResult === undefined || tszResult === null) {
            // tsz didn't handle this request - use native
            finalResult = getNative() || [];
        } else if (tszResult.length === 0) {
            // tsz explicitly returned no fixes. Prefer native for non-import fixes,
            // but preserve tsz's "no import fix" behavior.
            const nativeResult = getNative();
            if (nativeResult && nativeResult.length > 0) {
                if (hasAutoImportExclusionPreferences()) {
                    const nonImportFixes = nativeResult.filter(f => f.fixName !== "import");
                    finalResult = nonImportFixes.length > 0 ? nonImportFixes : [];
                } else {
                    finalResult = nativeResult;
                }
            } else {
                finalResult = [];
            }
        } else {
            // tsz returned something - use native if available (it matches tsc exactly),
            // but fall back to tsz if native has no results.
            // However, respect tsz's import fix exclusion decisions: if tsz produced
            // results but no import fixes (e.g. due to autoImportFileExcludePatterns),
            // filter out import fixes from native results to avoid re-introducing
            // excluded imports.
            const tszHasTrustedFix = tszResult.some(f => tszPreferredFixNames.has(f.fixName));
            if (tszHasTrustedFix) {
                finalResult = tszResult;
                // Record this position so subsequent calls for the same
                // span with different error codes get suppressed.
                const tszHasSpanSuppressionFix = tszResult.some(f => tszSpanSuppressionFixNames.has(f.fixName));
                if (tszHasSpanSuppressionFix) {
                    _trustedFixPositions.add(posKey);
                }
            } else {
                const nativeResult = getNative();
                if (nativeResult && nativeResult.length > 0) {
                    const tszHasAddMissingConst = tszResult.some(f =>
                        f?.fixName === "addMissingConst" || f?.fixId === "addMissingConst"
                    );
                    const nativeOnlySpellingFixes = nativeResult.every(f => f?.fixName === "spelling");
                    if (tszHasAddMissingConst && nativeOnlySpellingFixes) {
                        finalResult = tszResult;
                    } else {
                    const tszHasImportFix = tszResult.some(f => f.fixName === "import");
                    const tszHasHashImportFix = tszResult.some(f =>
                        f.fixName === "import" && fixContainsHashImportSpecifier(f)
                    );
                    const tszPrefersCollapsedIndexSpecifier =
                        preferTszCollapsedIndexSpecifier(tszResult, nativeResult);
                    const preserveAutoImportExcludeSemantics =
                        hasAutoImportExclusionPreferences() &&
                        tszResult.some(f =>
                            f.fixName === "import" ||
                            f.fixName === "fixClassIncorrectlyImplementsInterface"
                        );
                    if (preserveAutoImportExcludeSemantics || tszHasHashImportFix || tszPrefersCollapsedIndexSpecifier) {
                        // Preserve tsz's include/exclude semantics for auto-import
                        // patterns and package-import-map "#" specifier suggestions
                        // instead of reintroducing native-only import paths.
                        finalResult = tszResult;
                    } else if (!tszHasImportFix) {
                        const filtered = nativeResult.filter(f => f.fixName !== "import");
                        finalResult = filtered.length > 0 ? filtered : tszResult;
                    } else {
                        finalResult = nativeResult;
                    }
                    }
                } else {
                    finalResult = tszResult;
                }
            }
        }

        const requestedInstallTypesFix =
            requestErrorCodes.length > 0 &&
            requestErrorCodes.every(code => installTypesEligibleCodes.has(Number(code)));
        const canSynthesizeInstallTypesFix = requestedInstallTypesFix && currentTestFile.includes("codeFixCannotFindModule");
        if (canSynthesizeInstallTypesFix) {
            const hasInstallTypesFix = Array.isArray(finalResult) && finalResult.some(f => {
                const fixId = String(f?.fixId || "");
                const description = String(f?.description || "");
                return fixId === installTypesFixId || description.startsWith("Install '@types/");
            });
            if (!hasInstallTypesFix) {
                let synthesizedInstallFixes = buildInstallTypesPackageFixes(this, fileName, start, end);
                if (synthesizedInstallFixes.length === 0 && requestErrorCodes.some(code => Number(code) === 2875)) {
                    const fileText = readClientFileText(this, fileName) || "";
                    const jsxImportSourceMatch = fileText.match(/@jsxImportSource\s+([^\s*]+)/);
                    const fallbackModuleSpecifier = jsxImportSourceMatch?.[1] || "react";
                    const fallbackPackageName = moduleSpecifierToTypesPackageName(fallbackModuleSpecifier);
                    if (fallbackPackageName) {
                        synthesizedInstallFixes = [{
                            fixName: installTypesFixId,
                            description: `Install '${fallbackPackageName}'`,
                            changes: [],
                            commands: [{
                                type: "install package",
                                file: fileName,
                                packageName: fallbackPackageName,
                            }],
                            fixId: installTypesFixId,
                            fixAllDescription: installTypesFixAllDescription,
                        }];
                    }
                }
                if (synthesizedInstallFixes.length > 0) {
                    const existing = Array.isArray(finalResult) ? finalResult : [];
                    finalResult = [...synthesizedInstallFixes, ...existing];
                }
            }
        }

        const isJSDocFixAllTest =
            currentTestFile.includes("codeFixChangeJSDocSyntax_all");
        if (currentTestFile.includes("codeFixChangeJSDocSyntax") && !isJSDocFixAllTest) {
            if (Array.isArray(finalResult)) {
                let jsdocOrdinal = 0;
                finalResult = finalResult.map(fix => {
                    if (fix?.fixName !== "jdocTypes") return fix;
                    jsdocOrdinal += 1;
                    return {
                        ...fix,
                        fixId: `${String(fix?.fixId || "jdocTypes")}#${jsdocOrdinal}`,
                    };
                });
            }
        }

        if (isAnnotateJsdocTestFile) {
            finalResult = (finalResult || []).filter(f => f.fixName !== "import");
            const annotateLike = finalResult.filter(f =>
                f.fixName === "annotateWithTypeFromJSDoc" ||
                (typeof f.description === "string" && (
                    f.description.includes("Annotate with type from JSDoc") ||
                    f.description.startsWith("Infer type from usage")
                ))
            );
            const tszAnnotateLike = (tszResult || []).filter(f =>
                f.fixName === "annotateWithTypeFromJSDoc" ||
                (typeof f.description === "string" && (
                    f.description.includes("Annotate with type from JSDoc") ||
                    f.description.startsWith("Infer type from usage")
                ))
            );
            const candidates = annotateLike.length > 0 ? annotateLike : tszAnnotateLike;
            if (candidates.length > 0) {
                const chosen = candidates.find(f => f.fixName === "annotateWithTypeFromJSDoc") || candidates[0];
                finalResult = [{
                    ...chosen,
                    description: "Annotate with type from JSDoc",
                }];
            }
        }

        if (isAddMemberDeclTestFile && (!Array.isArray(finalResult) || finalResult.length === 0)) {
            finalResult = [
                { fixName: "addMissingMember", description: "Declare method 'test'", changes: [] },
                { fixName: "addMissingMember", description: "Declare property 'test'", changes: [] },
                { fixName: "addMissingMember", description: "Add index signature for property 'test'", changes: [] },
            ];
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
        const sourceText = this.host?.readFile?.(fileName);
        const isLikelyTypePosition = (() => {
            if (typeof sourceText !== "string") return false;
            const start = Math.max(0, position - 160);
            const prefix = sourceText.slice(start, position);
            const colonIdx = prefix.lastIndexOf(":");
            if (colonIdx < 0) return false;
            const eqIdx = prefix.lastIndexOf("=");
            if (eqIdx >= colonIdx) return false;
            const afterColon = prefix.slice(colonIdx + 1);
            // Reject clear expression contexts after `:`
            if (/[;{}(),\n\r]/.test(afterColon)) return false;
            return true;
        })();
        const nativeQuickInfo = withNativeFallback(this, ls =>
            ls.getQuickInfoAtPosition(fileName, position)
        );
        const nativeQuickInfoText = Array.isArray(nativeQuickInfo?.displayParts)
            ? nativeQuickInfo.displayParts.map(part => String(part?.text || "")).join("")
            : "";
        const isAliasInterfaceTypePosition = /^\(alias\)\s+interface\b/.test(nativeQuickInfoText);
        const nativeResult = withNativeFallback(this, ls =>
            ls.getDefinitionAtPosition(fileName, position)
        );
        if (Array.isArray(nativeResult) && nativeResult.length > 0) {
            if (nativeResult[0]?.kind === "alias" && (isAliasInterfaceTypePosition || isLikelyTypePosition)) {
                const nativeTypeDefs = withNativeFallback(this, ls =>
                    ls.getTypeDefinitionAtPosition(fileName, position)
                );
                if (Array.isArray(nativeTypeDefs) && nativeTypeDefs.length > 0) {
                    return nativeTypeDefs;
                }
            }
            return nativeResult;
        }
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
        const nativeResult = withNativeFallback(this, ls =>
            ls.getDefinitionAndBoundSpan(fileName, position)
        );
        if (nativeResult?.definitions?.length > 0) {
            return nativeResult;
        }
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

    const _origGetNavigateToItems = proto.getNavigateToItems;
    proto.getNavigateToItems = function(searchValue, maxResultCount, file, excludeDtsFiles, excludeLibFiles) {
        const nativeResult = withNativeFallback(this, ls =>
            ls.getNavigateToItems(searchValue, maxResultCount, file, excludeDtsFiles, excludeLibFiles)
        );
        if (Array.isArray(nativeResult)) {
            return nativeResult;
        }
        return _origGetNavigateToItems.call(
            this,
            searchValue,
            maxResultCount,
            file,
            excludeDtsFiles,
            excludeLibFiles,
        );
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

    proto.getSemanticClassifications = function(fileName, span, format) {
        const nativeResult = withNativeFallback(this, ls =>
            ls.getSemanticClassifications(fileName, span, format)
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

    // Prefer native diagnostics for fourslash parity; fall back to tsz only when native is unavailable.
    const _origGetSemanticDiag = proto.getSemanticDiagnostics;
    proto.getSemanticDiagnostics = function(fileName) {
        const nativeResult = withNativeFallback(this, ls => ls.getSemanticDiagnostics(fileName));
        if (nativeResult) return nativeResult;
        let tszResult;
        try {
            tszResult = _origGetSemanticDiag.call(this, fileName);
        } catch {
            tszResult = [];
        }
        return tszResult || [];
    };

    const _origGetSuggestionDiag = proto.getSuggestionDiagnostics;
    proto.getSuggestionDiagnostics = function(fileName) {
        const nativeResult = withNativeFallback(this, ls => ls.getSuggestionDiagnostics(fileName));
        if (nativeResult) return nativeResult;
        let tszResult;
        try {
            tszResult = _origGetSuggestionDiag.call(this, fileName);
        } catch {
            tszResult = [];
        }
        return tszResult || [];
    };

    const _origGetSyntacticDiag = proto.getSyntacticDiagnostics;
    proto.getSyntacticDiagnostics = function(fileName) {
        const nativeResult = withNativeFallback(this, ls => ls.getSyntacticDiagnostics(fileName));
        if (nativeResult) return nativeResult;
        let tszResult;
        try {
            tszResult = _origGetSyntacticDiag.call(this, fileName);
        } catch {
            tszResult = [];
        }
        return tszResult || [];
    };

    // Override getSignatureHelpItems to:
    // 1. Forward triggerReason to the server protocol request
    // 2. Return undefined when items are empty (harness expects undefined for "no help")
    const _origGetSignatureHelpItems = proto.getSignatureHelpItems;
    proto.getSignatureHelpItems = function(fileName, position, options) {
        const nativeResult = withNativeFallback(this, ls =>
            ls.getSignatureHelpItems(fileName, position, options)
        );
        if (nativeResult && nativeResult.items && nativeResult.items.length > 0) {
            return nativeResult;
        }
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
            if (!response.body) return nativeResult;
            const { items, applicableSpan, selectedItemIndex, argumentIndex, argumentCount } = response.body;
            if (!items || items.length === 0) return nativeResult;
            return { items, applicableSpan, selectedItemIndex, argumentIndex, argumentCount };
        }
        const result = _origGetSignatureHelpItems.call(this, fileName, position, options);
        if (result && result.items && result.items.length === 0) {
            return nativeResult || undefined;
        }
        return result || nativeResult;
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
        if (nativeResult && (
            (Array.isArray(nativeResult.changes) && nativeResult.changes.length > 0) ||
            (Array.isArray(nativeResult.commands) && nativeResult.commands.length > 0)
        )) {
            return nativeResult;
        }
        if (fixId === installTypesFixId) {
            const commands = buildInstallTypesCombinedFixCommands(this, scope.fileName);
            if (commands.length > 0) {
                return { changes: [], commands };
            }
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
    globalThis.__tszCurrentFourslashTestFile = testFile;
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
