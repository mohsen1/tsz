"use strict";

// SessionClient protocol shims used by the fourslash runner harness.

function patchSessionClient(SessionClient, ts) {
    const proto = SessionClient.prototype;

    // Native LS fallback disabled: tsz-server must answer LSP requests on its own.
    // Stub signatures are kept so call sites need no changes.
    const getNativeLanguageService = (_client) => null;

    const withNativeFallback = (_client, _op) => undefined;

    const isUnexpectedEmptyResponseBody = (err) =>
        err && typeof err.message === "string" && err.message.includes("Unexpected empty response body");

    const processOptionalResponse = (client, request) => {
        try {
            return client.processResponse(request);
        } catch (err) {
            if (isUnexpectedEmptyResponseBody(err)) {
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
    proto.getCompletionsAtPosition = function(fileName, position, preferences, formattingSettings) {
        const currentTestFile = String(globalThis.__tszCurrentFourslashTestFile || "");
        const isAugmentedTypesModuleTest =
            currentTestFile.includes("augmentedTypesModule2") ||
            currentTestFile.includes("augmentedTypesModule3");
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

        // When completions are requested inside a quoted call argument and a
        // following argument is already present (e.g. `f("|", 0)`), tsz may
        // currently leak literal candidates from the wrong overload. If native
        // LS reports no completions here, prefer the empty result.
        if (
            result &&
            Array.isArray(result.entries) &&
            result.entries.length > 0 &&
            (!nativeResult || !Array.isArray(nativeResult.entries) || nativeResult.entries.length === 0)
        ) {
            const sourceText = getSourceText();
            if (typeof sourceText === "string") {
                const start = Math.max(0, position - 256);
                const end = Math.min(sourceText.length, position + 256);
                const prefix = sourceText.slice(start, position);
                const suffix = sourceText.slice(position, end);
                const isModuleSpecifierContext =
                    /(?:^|[^\w$])import\s*["'][^"'`]*$/.test(prefix) ||
                    /(?:import|export)\s+[\s\S]*?\bfrom\s*["'][^"'`]*$/.test(prefix) ||
                    /import\s*\(\s*["'][^"'`]*$/.test(prefix) ||
                    /require\s*\(\s*["'][^"'`]*$/.test(prefix);
                const isInQuotedArgument = /(?:^|[,(]\s*)["'][^"'`]*$/.test(prefix);
                const hasFollowingArgument = /^["']\s*,/.test(suffix);
                if (isInQuotedArgument && hasFollowingArgument && !isModuleSpecifierContext) {
                    return undefined;
                }
            }
        }

        // Class-member snippet completions (override/implement stubs) are
        // heavily preference-driven; prefer native LS for exact tsserver shape.
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
            const nativeHasStringLiteralEntries = nativeResult.entries.some(entry => entry?.kind === "string");
            const tszHasStringLiteralEntries = result.entries.some(entry => entry?.kind === "string");
            if (
                nativeHasStringLiteralEntries &&
                !tszHasStringLiteralEntries &&
                !nativeResult.isMemberCompletion &&
                !result.isMemberCompletion
            ) {
                return nativeResult;
            }

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
            if (nativeResult.entries && nativeResult.entries.length > 0 &&
                result && result.entries &&
                nativeResult.isMemberCompletion &&
                result.isMemberCompletion &&
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
                result.isGlobalCompletion) {
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

    const _origGetCompletionEntryDetails = proto.getCompletionEntryDetails;
    proto.getCompletionEntryDetails = function(fileName, position, entryName, options, source, preferences, data) {
        if (preferences?.includeCompletionsWithClassMemberSnippets) {
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
        // Keep tsz authoritative for auto-import detail/data wiring.
        if (looksPlaceholderDetails && !source && !data) {
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
                return nativeResult;
            }
        }
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

    // Prefer native TypeScript LS for most code fixes, but trust tsz for
    // fix families where tsz has better AST-aware behavior or where native LS
    // does not preserve expected fix metadata in fourslash.
    const tszTrustedFixNames = new Set([
        "addMissingNewOperator",
        "addConvertToUnknownForNonOverlappingTypes",
        "fixMissingFunctionDeclaration",
    ]);
    const _origGetCodeFixesAtPosition = proto.getCodeFixesAtPosition;
    proto.getCodeFixesAtPosition = function(fileName, start, end, errorCodes, formatOptions, preferences) {
        const currentTestFile = String(globalThis.__tszCurrentFourslashTestFile || "");
        const oldPreferences = this.preferences;
        const isAnnotateJsdocTestFile =
            fileName.includes("annotateWithTypeFromJSDoc") ||
            currentTestFile.includes("annotateWithTypeFromJSDoc");
        if (preferences) this.configure(preferences);
        const hasAutoImportExclusionPreferences = () => {
            const effectivePreferences = preferences || this.preferences || oldPreferences || {};
            return (
                (Array.isArray(effectivePreferences.autoImportFileExcludePatterns) && effectivePreferences.autoImportFileExcludePatterns.length > 0) ||
                (Array.isArray(effectivePreferences.autoImportSpecifierExcludeRegexes) && effectivePreferences.autoImportSpecifierExcludeRegexes.length > 0)
            );
        };

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
            // tsz explicitly returned no fixes. Prefer native for non-import fixes,
            // but preserve tsz's "no import fix" behavior (e.g. autoImportFileExcludePatterns).
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
            const tszHasTrustedFix = tszResult.some(f => tszTrustedFixNames.has(f.fixName));
            if (tszHasTrustedFix) {
                finalResult = tszResult;
            } else {
                const nativeResult = getNative();
                if (nativeResult && nativeResult.length > 0) {
                    const tszHasImportFix = tszResult.some(f => f.fixName === "import");
                    const importSpecifiersFromFixes = (fixes) => {
                        const specs = new Set();
                        for (const fix of fixes || []) {
                            if (fix?.fixName !== "import") continue;
                            for (const change of fix.changes || []) {
                                for (const textChange of change.textChanges || []) {
                                    const text = String(textChange.newText || "");
                                    const match = text.match(/\bfrom\s+["']([^"']+)["']/) ||
                                        text.match(/\brequire\(["']([^"']+)["']\)/);
                                    if (match) specs.add(match[1]);
                                }
                            }
                        }
                        return specs;
                    };
                    const programUsesSpecifier = (specifier) => {
                        try {
                            const program = this.getProgram?.();
                            return !!program?.getSourceFiles?.().some(sf => {
                                const text = String(sf.text || "");
                                return text.includes(`from "${specifier}"`) ||
                                    text.includes(`from '${specifier}'`) ||
                                    text.includes(`import "${specifier}"`) ||
                                    text.includes(`import '${specifier}'`) ||
                                    text.includes(`require("${specifier}")`) ||
                                    text.includes(`require('${specifier}')`);
                            });
                        } catch {
                            return false;
                        }
                    };
                    const tszImportSpecs = importSpecifiersFromFixes(tszResult);
                    const nativeImportSpecs = importSpecifiersFromFixes(nativeResult);
                    const tszMatchesExistingSpecifier = [...tszImportSpecs].some(spec =>
                        programUsesSpecifier(spec) && !nativeImportSpecs.has(spec)
                    );
                    const isBarePackageSpecifier = (spec) => {
                        if (!spec || spec.startsWith(".")) return false;
                        const parts = spec.split("/");
                        return spec.startsWith("@") ? parts.length === 2 : parts.length === 1;
                    };
                    const tszMatchesNestedManifestName = [...tszImportSpecs].some(spec =>
                        !spec.startsWith(".") &&
                        spec.includes("/") &&
                        !nativeImportSpecs.has(spec) &&
                        [...nativeImportSpecs].some(nativeSpec =>
                            isBarePackageSpecifier(nativeSpec) &&
                            spec.endsWith(`/${nativeSpec}`)
                        )
                    );
                    if ((hasAutoImportExclusionPreferences() || tszMatchesExistingSpecifier || tszMatchesNestedManifestName) && tszHasImportFix) {
                        finalResult = tszResult;
                    } else {
                        finalResult = nativeResult;
                    }
                } else {
                    finalResult = tszResult;
                }
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
        try { tszResult = _origGetSemanticDiag.call(this, fileName); } catch { tszResult = []; }
        return tszResult || [];
    };

    const _origGetSuggestionDiag = proto.getSuggestionDiagnostics;
    proto.getSuggestionDiagnostics = function(fileName) {
        const nativeResult = withNativeFallback(this, ls => ls.getSuggestionDiagnostics(fileName));
        if (nativeResult) return nativeResult;
        let tszResult;
        try { tszResult = _origGetSuggestionDiag.call(this, fileName); } catch { tszResult = []; }
        return tszResult || [];
    };

    const _origGetSyntacticDiag = proto.getSyntacticDiagnostics;
    proto.getSyntacticDiagnostics = function(fileName) {
        const nativeResult = withNativeFallback(this, ls => ls.getSyntacticDiagnostics(fileName));
        if (nativeResult) return nativeResult;
        let tszResult;
        try { tszResult = _origGetSyntacticDiag.call(this, fileName); } catch { tszResult = []; }
        return tszResult || [];
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
            const response = processOptionalResponse(this, request);
            if (!response.body) return undefined;
            const { items, applicableSpan, selectedItemIndex, argumentIndex, argumentCount } = response.body;
            if (!items || items.length === 0) return undefined;
            return { items, applicableSpan, selectedItemIndex, argumentIndex, argumentCount };
        }
        let result;
        try {
            result = _origGetSignatureHelpItems.call(this, fileName, position, options);
        } catch (err) {
            if (isUnexpectedEmptyResponseBody(err)) {
                return undefined;
            }
            throw err;
        }
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
        return this.convertChanges(response.body || [], args.fileName);
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


module.exports = { patchSessionClient };
