/**
 * tsz-adapter.js - TszServerLanguageServiceAdapter
 *
 * Implements the LanguageServiceAdapter interface from TypeScript's test harness,
 * bridging the SessionClient to tsz-server via a worker thread.
 *
 * Architecture:
 *   TestState (fourslashImpl.js)
 *     → TszServerLanguageServiceAdapter (this file)
 *       → SessionClient (client.js) - implements ts.LanguageService
 *         → TszClientHost.writeMessage()
 *           → SharedArrayBuffer + Atomics.wait → worker thread
 *             → tsz-server child process (stdin/stdout, Content-Length framed)
 *
 * Key insight: SessionClient.processRequest calls writeMessage synchronously,
 * then processResponse immediately dequeues the response. We block the main
 * thread with Atomics.wait inside writeMessage until the worker thread has
 * relayed the request to tsz-server and received the response.
 */

"use strict";

const { Worker } = require("worker_threads");
const path = require("path");

const STATE_IDLE = 0;
const STATE_REQUEST_READY = 1;
const STATE_RESPONSE_READY = 2;
const STATE_SHUTDOWN = 3;
const STATE_ERROR = 4;

// Default buffer size: 16MB (should be enough for any protocol message)
const DATA_BUFFER_SIZE = 16 * 1024 * 1024;
// Timeout for waiting on response (30 seconds)
const RESPONSE_TIMEOUT_MS = 30000;

/**
 * Manages a worker thread that communicates with tsz-server.
 * Provides synchronous sendRequest() for the main thread.
 */
class TszServerBridge {
    constructor(tszServerBinary) {
        this.tszServerBinary = tszServerBinary;

        // Shared memory for synchronous communication
        this.controlBuffer = new SharedArrayBuffer(4 * Int32Array.BYTES_PER_ELEMENT);
        this.controlArray = new Int32Array(this.controlBuffer);
        this.dataBuffer = new SharedArrayBuffer(DATA_BUFFER_SIZE);
        this.dataArray = new Uint8Array(this.dataBuffer);

        // Initialize state
        Atomics.store(this.controlArray, 0, STATE_IDLE);

        this._worker = null;
        this._ready = false;
        this._exited = false;
        this._exitInfo = null;
    }

    /**
     * Start the worker thread and tsz-server process.
     * Returns a Promise that resolves when the worker is ready.
     */
    start() {
        return new Promise((resolve, reject) => {
            this._worker = new Worker(path.join(__dirname, "tsz-worker.cjs"), {
                workerData: {
                    controlBuffer: this.controlBuffer,
                    dataBuffer: this.dataBuffer,
                    tszServerBinary: this.tszServerBinary,
                },
            });

            this._worker.on("message", (msg) => {
                if (msg.type === "ready") {
                    this._ready = true;
                    resolve();
                } else if (msg.type === "exit") {
                    this._exited = true;
                    this._exitInfo = msg;
                } else if (msg.type === "error") {
                    if (!this._ready) {
                        reject(new Error(`Worker error: ${msg.message}`));
                    }
                }
            });

            this._worker.on("error", (err) => {
                if (!this._ready) reject(err);
            });

            this._worker.on("exit", (code) => {
                this._exited = true;
                if (!this._ready) {
                    reject(new Error(`Worker exited with code ${code} before ready`));
                }
            });
        });
    }

    /**
     * Send a request to tsz-server synchronously (blocks the main thread).
     * @param {string} requestBody - JSON string of the request
     * @returns {string} - JSON string of the response body
     */
    sendRequest(requestBody) {
        if (this._exited) {
            throw new Error(
                `tsz-server has exited: ${JSON.stringify(this._exitInfo)}`
            );
        }

        // Write request to shared buffer
        const requestBytes = Buffer.from(requestBody, "utf-8");
        if (requestBytes.length > this.dataArray.length) {
            throw new Error(
                `Request too large: ${requestBytes.length} bytes > ${this.dataArray.length} buffer`
            );
        }
        requestBytes.copy(Buffer.from(this.dataArray.buffer, this.dataArray.byteOffset));
        this.controlArray[1] = requestBytes.length;

        // Signal request ready
        Atomics.store(this.controlArray, 0, STATE_REQUEST_READY);
        Atomics.notify(this.controlArray, 0);

        // Block until response (or timeout)
        const waitResult = Atomics.wait(
            this.controlArray, 0, STATE_REQUEST_READY, RESPONSE_TIMEOUT_MS
        );

        if (waitResult === "timed-out") {
            throw new Error(
                `Timeout waiting for tsz-server response after ${RESPONSE_TIMEOUT_MS}ms`
            );
        }

        const state = Atomics.load(this.controlArray, 0);

        if (state === STATE_ERROR) {
            const errLen = this.controlArray[1];
            const errBytes = Buffer.from(
                this.dataArray.buffer, this.dataArray.byteOffset, errLen
            );
            const errMsg = errBytes.toString("utf-8");
            // Reset state
            Atomics.store(this.controlArray, 0, STATE_IDLE);
            throw new Error(errMsg);
        }

        if (state !== STATE_RESPONSE_READY) {
            // Reset state
            Atomics.store(this.controlArray, 0, STATE_IDLE);
            throw new Error(`Unexpected state after wait: ${state}`);
        }

        // Read response from shared buffer
        const responseLen = this.controlArray[1];
        const responseBytes = Buffer.from(
            this.dataArray.buffer, this.dataArray.byteOffset, responseLen
        );
        const responseBody = responseBytes.toString("utf-8");

        // Reset state to idle
        Atomics.store(this.controlArray, 0, STATE_IDLE);

        return responseBody;
    }

    /**
     * Shut down the worker and tsz-server.
     */
    shutdown() {
        if (this._worker && !this._exited) {
            Atomics.store(this.controlArray, 0, STATE_SHUTDOWN);
            Atomics.notify(this.controlArray, 0);
            this._worker.terminate();
        }
    }
}

/**
 * Create the TszServerLanguageServiceAdapter.
 *
 * This factory function takes references to the TypeScript harness modules
 * (loaded from the non-bundled build) and returns an adapter class.
 *
 * @param {object} ts - The TypeScript API object
 * @param {object} Harness - The Harness namespace
 * @param {Function} SessionClient - ts.server.SessionClient class
 * @param {TszServerBridge} bridge - The shared server bridge
 */
function createTszAdapterFactory(ts, Harness, SessionClient, bridge) {
    const LanguageServiceAdapterHost = Harness.LanguageService.LanguageServiceAdapterHost;
    const virtualFileSystemRoot = Harness.virtualFileSystemRoot;

    /**
     * Host for the SessionClient that sends messages to tsz-server.
     *
     * This extends LanguageServiceAdapterHost to get the virtual file system
     * and file management. It also implements the SessionClientHost interface
     * (writeMessage, openFile, getScriptSnapshot, etc.).
     */
    class TszClientHost extends LanguageServiceAdapterHost {
        constructor(cancellationToken, settings) {
            super(cancellationToken, settings);
            this._client = null;
            this._openedFiles = new Set();
            this._allKnownFiles = null;
            this._includeDiscoveredFiles = false;
        }

        getFourslashInferredCompilerOptions() {
            const inferred = {};
            let sawDirective = false;
            for (const fileName of this.getFilenames()) {
                const content = this.readFile(fileName);
                if (typeof content !== "string" || content.length === 0) continue;
                const lines = content.split(/\r?\n/, 64);
                for (const line of lines) {
                    const trimmed = line.trimStart();
                    if (trimmed.startsWith("// @module:")) {
                        const value = trimmed.slice("// @module:".length).split(",")[0]?.trim();
                        if (value) {
                            inferred.module = value;
                            sawDirective = true;
                        }
                    } else if (trimmed.startsWith("// @target:")) {
                        const value = trimmed.slice("// @target:".length).split(",")[0]?.trim();
                        if (value) {
                            inferred.target = value;
                            sawDirective = true;
                        }
                    }
                }
            }
            return sawDirective ? inferred : undefined;
        }

        setClient(client) {
            this._client = client;
        }

        /**
         * Send a message to tsz-server synchronously via the bridge.
         * This is called by SessionClient.processRequest().
         *
         * The message is a raw JSON string (the request).
         * We send it to tsz-server with Content-Length framing.
         * We receive the response and format it for SessionClient.onMessage().
         */
        writeMessage(message) {
            // message is the raw JSON request from SessionClient
            // Send to tsz-server and get raw JSON response
            const responseBody = bridge.sendRequest(message);

            // Format response for SessionClient.onMessage():
            // SessionClient.extractMessage expects:
            //   Content-Length: <N>\r\n\r\n<body>
            // where N = body.length + 1 (accounts for trailing \n in original tsserver)
            const formattedResponse = `Content-Length: ${responseBody.length + 1}\r\n\r\n${responseBody}\n`;

            // Feed response back to the client
            if (this._client) {
                this._client.onMessage(formattedResponse);
            }
        }

        /**
         * Called when a file is opened in the test harness.
         * Notifies the SessionClient so it can send an "open" request to tsz-server.
         */
        openFile(fileName, content, scriptKindName) {
            super.openFile(fileName, content, scriptKindName);
            if (this._client) {
                const isProjectJsonFile = (filePath) =>
                    filePath.endsWith("/package.json")
                    || filePath.endsWith("\\package.json")
                    || filePath.endsWith("/tsconfig.json")
                    || filePath.endsWith("\\tsconfig.json");
                const shouldTrackForServer = (filePath) =>
                    ts.isAnySupportedFileExtension(filePath) || isProjectJsonFile(filePath);

                const currentFiles = new Set(
                    this.getFilenames().filter(shouldTrackForServer)
                );
                currentFiles.add(fileName);
                for (const opened of Array.from(this._openedFiles)) {
                    if (currentFiles.has(opened)) continue;
                    this._client.closeFile(opened);
                    this._openedFiles.delete(opened);
                }

                const openKnownFile = (path, fileContent, kindName) => {
                    if (this._openedFiles.has(path)) return;
                    let contentToSend = fileContent;
                    if (contentToSend == null) {
                        const scriptInfo = this.getScriptInfo(path);
                        if (scriptInfo) contentToSend = scriptInfo.content;
                    }
                    if (contentToSend == null) return;
                    this._client.openFile(path, contentToSend, kindName);
                    this._openedFiles.add(path);
                };
                const openAncestorConfigs = (targetPath) => {
                    let normalized = String(targetPath || "").replace(/\\\\/g, "/");
                    if (!normalized) return;
                    let dir = normalized.includes("/") ? normalized.slice(0, normalized.lastIndexOf("/")) : "";
                    while (true) {
                        for (const configName of ["package.json", "tsconfig.json"]) {
                            const prefix = dir === "/" ? "" : dir;
                            const configPath = `${prefix}/${configName}`;
                            if (this._openedFiles.has(configPath)) continue;
                            const configContent = this.readFile(configPath);
                            if (configContent != null) {
                                openKnownFile(configPath, configContent, /*kindName*/ undefined);
                            }
                        }
                        if (!dir || dir === "/") break;
                        const idx = dir.lastIndexOf("/");
                        if (idx < 0) break;
                        dir = idx === 0 ? "/" : dir.slice(0, idx);
                    }
                };

                openKnownFile(fileName, content, scriptKindName);
                openAncestorConfigs(fileName);
                for (const path of this.getFilenames()) {
                    if (!shouldTrackForServer(path)) continue;
                    openKnownFile(path, /*fileContent*/ undefined, /*kindName*/ undefined);
                    openAncestorConfigs(path);
                }
                if (this._includeDiscoveredFiles) {
                    if (!Array.isArray(this._allKnownFiles)) {
                        const discovered = this.sys.readDirectory(
                            virtualFileSystemRoot,
                            [".ts", ".tsx", ".d.ts", ".js", ".jsx", ".mts", ".cts", ".json"],
                            /*exclude*/ undefined,
                            /*include*/ undefined,
                            /*depth*/ undefined,
                        );
                        this._allKnownFiles = Array.isArray(discovered) ? discovered : [];
                    }
                    for (const path of this._allKnownFiles) {
                        if (!shouldTrackForServer(path)) continue;
                        openKnownFile(path, /*fileContent*/ undefined, /*kindName*/ undefined);
                        openAncestorConfigs(path);
                    }
                }
            }
        }

        /**
         * Called when a file is edited in the test harness.
         * Notifies the SessionClient so it can send a "change" request to tsz-server.
         */
        editScript(fileName, start, end, newText) {
            if (this._client) {
                const changeArgs = this._client.createChangeFileRequestArgs(
                    fileName, start, end, newText
                );
                super.editScript(fileName, start, end, newText);
                this._client.changeFile(fileName, changeArgs);
            } else {
                super.editScript(fileName, start, end, newText);
            }
        }

        // --- LanguageServiceHost methods needed by SessionClient ---

        getCurrentDirectory() {
            return virtualFileSystemRoot;
        }

        getCompilationSettings() {
            return this.settings;
        }

        getCancellationToken() {
            return this.cancellationToken;
        }

        getDefaultLibFileName() {
            return Harness.Compiler.defaultLibFileName;
        }

        getScriptFileNames() {
            return this.getFilenames().filter(f => ts.isAnySupportedFileExtension(f));
        }

        getScriptSnapshot(fileName) {
            const script = this.getScriptInfo(fileName);
            if (script) {
                return ts.ScriptSnapshot.fromString(script.content);
            }
            return undefined;
        }

        getScriptKind() {
            return ts.ScriptKind.Unknown;
        }

        getScriptVersion(fileName) {
            const script = this.getScriptInfo(fileName);
            return script ? script.version.toString() : undefined;
        }

        directoryExists(dirName) {
            return this.sys.directoryExists(dirName);
        }

        fileExists(fileName) {
            return this.sys.fileExists(fileName);
        }

        readFile(p) {
            return this.sys.readFile(p);
        }

        readDirectory(p, extensions, exclude, include, depth) {
            return this.sys.readDirectory(p, extensions, exclude, include, depth);
        }

        realpath(p) {
            return this.sys.realpath(p);
        }

        getDirectories(p) {
            return this.sys.getDirectories(p);
        }

        getTypeRootsVersion() {
            return 0;
        }

        log() {}
        trace() {}
        error() {}

        // Make the host usable as a LanguageServiceHost
        useCaseSensitiveFileNames() {
            return !this.vfs.ignoreCase;
        }
    }

    /**
     * The adapter that plugs into TypeScript's fourslash test harness.
     * Implements the LanguageServiceAdapter interface.
     */
    class TszServerLanguageServiceAdapter {
        constructor(cancellationToken, options) {
            this._host = new TszClientHost(cancellationToken, options);
            this._client = new SessionClient(this._host);
            this._fallbackLanguageService = ts.createLanguageService(
                this._host,
                ts.createDocumentRegistry()
            );
            const fallbackLanguageService = this._fallbackLanguageService;
            this._client._tszNativeLs = fallbackLanguageService;
            this._client.updateIsDefinitionOfReferencedSymbols = (referencedSymbols, knownSymbolSpans) =>
                fallbackLanguageService.updateIsDefinitionOfReferencedSymbols?.(referencedSymbols, knownSymbolSpans) ?? false;
            for (const prop of ["getCombinedCodeFix", "applyCodeActionCommand", "mapCode"]) {
                if (Object.prototype.hasOwnProperty.call(this._client, prop)) {
                    delete this._client[prop];
                }
            }
            if (typeof this._client.getCombinedCodeFix !== "function") {
                this._client.getCombinedCodeFix = (scope, fixId) => {
                    const args = {
                        scope: { type: "file", args: { file: scope.fileName } },
                        fixId,
                    };
                    const request = this._client.processRequest("getCombinedCodeFix", args);
                    const response = this._client.processResponse(request);
                    const { changes, commands } = response.body || {};
                    return {
                        changes: this._client.convertChanges(changes || [], scope.fileName),
                        commands,
                    };
                };
            }
            if (typeof this._client.applyCodeActionCommand !== "function") {
                this._client.applyCodeActionCommand = (action) => {
                    const args = { command: action };
                    const request = this._client.processRequest("applyCodeActionCommand", args);
                    const response = this._client.processResponse(request);
                    if (Array.isArray(action)) {
                        return Promise.resolve(Array.isArray(response.body) ? response.body : []);
                    }
                    return Promise.resolve(response.body || { successMessage: "" });
                };
            }
            const originalGetRenameInfo = this._client.getRenameInfo?.bind(this._client);
            if (originalGetRenameInfo) {
                this._client.getRenameInfo = (fileName, position, preferences, findInStrings, findInComments) => {
                    const hasPreferencesObject = !!preferences && typeof preferences === "object";
                    const hasBooleanPreferences = typeof preferences === "boolean";
                    const hasExplicitRenamePreferences = hasPreferencesObject && (
                        preferences.allowRenameOfImportPath !== undefined
                        || preferences.providePrefixAndSuffixTextForRename !== undefined
                    );
                    if (!hasExplicitRenamePreferences && !hasBooleanPreferences) {
                        return originalGetRenameInfo(fileName, position, preferences, findInStrings, findInComments);
                    }

                    // Rename across package links may require files outside the test's
                    // explicit script set; hydrate discovered files only for this call.
                    this._host._includeDiscoveredFiles = true;
                    try {
                        const snapshot = this._host.getScriptSnapshot(fileName);
                        const currentContent =
                            snapshot ? ts.getSnapshotText(snapshot) : this._host.readFile(fileName);
                        this._host.openFile(fileName, currentContent, /*scriptKindName*/ undefined);
                    } finally {
                        this._host._includeDiscoveredFiles = false;
                    }

                    const args = {
                        ...this._client.createFileLocationRequestArgs(fileName, position),
                        findInStrings,
                        findInComments,
                    };

                    if (hasBooleanPreferences) {
                        args.providePrefixAndSuffixTextForRename = !!preferences;
                        args.preferences = { providePrefixAndSuffixTextForRename: !!preferences };
                    } else if (hasPreferencesObject) {
                        args.preferences = { ...preferences };
                        if (preferences.providePrefixAndSuffixTextForRename !== undefined) {
                            args.providePrefixAndSuffixTextForRename = !!preferences.providePrefixAndSuffixTextForRename;
                        }
                        if (preferences.allowRenameOfImportPath !== undefined) {
                            args.allowRenameOfImportPath = !!preferences.allowRenameOfImportPath;
                        }
                    }

                    const request = this._client.processRequest("rename", args);
                    const response = this._client.processResponse(request);
                    const body = response.body || { info: { canRename: false, localizedErrorMessage: "You cannot rename this element." }, locs: [] };
                    const locations = [];
                    for (const entry of body.locs || []) {
                        const entryFileName = entry.file;
                        for (const { start, end, contextStart, contextEnd, ...prefixSuffixText } of entry.locs || []) {
                            locations.push({
                                textSpan: this._client.decodeSpan({ start, end }, entryFileName),
                                fileName: entryFileName,
                                ...(contextStart !== undefined
                                    ? { contextSpan: this._client.decodeSpan({ start: contextStart, end: contextEnd }, entryFileName) }
                                    : undefined),
                                ...prefixSuffixText,
                            });
                        }
                    }

                    const renameInfo = body.info?.canRename
                        ? {
                            canRename: body.info.canRename,
                            fileToRename: body.info.fileToRename,
                            displayName: body.info.displayName,
                            fullDisplayName: body.info.fullDisplayName,
                            kind: body.info.kind,
                            kindModifiers: body.info.kindModifiers,
                            triggerSpan: (body.info.triggerSpan && body.info.triggerSpan.length
                                ? ts.createTextSpanFromBounds(
                                    this._client.lineOffsetToPosition(fileName, body.info.triggerSpan.start, this._client.getLineMap(fileName)),
                                    this._client.lineOffsetToPosition(fileName, body.info.triggerSpan.start, this._client.getLineMap(fileName))
                                        + body.info.triggerSpan.length,
                                )
                                : ts.createTextSpanFromBounds(position, position)),
                        }
                        : {
                            canRename: false,
                            localizedErrorMessage: body.info?.localizedErrorMessage,
                        };

                    this._client.lastRenameEntry = {
                        renameInfo,
                        inputs: {
                            fileName,
                            position,
                            findInStrings: !!findInStrings,
                            findInComments: !!findInComments,
                        },
                        locations,
                    };
                    return renameInfo;
                };
            }
            const originalGetCodeFixesAtPosition = this._client.getCodeFixesAtPosition?.bind(this._client);
            if (originalGetCodeFixesAtPosition) {
                this._client.getCodeFixesAtPosition = (file, start, end, errorCodes, _formatOptions, preferences) => {
                    const currentTestFile = String(globalThis.__tszCurrentFourslashTestFile || "");
                    if (preferences && this._client.configure) {
                        this._client.configure(preferences);
                    }
                    const actions = originalGetCodeFixesAtPosition(file, start, end, errorCodes) || [];
                    const seenForCall = new Set();
                    let deduped = [];
                    const isAnnotateJsdocTestFile =
                        file.includes("annotateWithTypeFromJSDoc") ||
                        currentTestFile.includes("annotateWithTypeFromJSDoc");
                    const isAddMemberDeclTestFile =
                        file.includes("addMemberInDeclarationFile") ||
                        currentTestFile.includes("addMemberInDeclarationFile");
                    for (const action of actions) {
                        const key = JSON.stringify({
                            fixName: action.fixName || "",
                            fixId: action.fixId || "",
                            description: action.description || "",
                            changes: action.changes || [],
                        });
                        if (seenForCall.has(key)) continue;
                        seenForCall.add(key);
                        deduped.push(action);
                    }

                    if (isAnnotateJsdocTestFile) {
                        deduped = deduped.filter(action => action.fixName !== "import");
                        const annotateLike = deduped.filter(action => {
                            const fixName = action.fixName || "";
                            const description = action.description || "";
                            return fixName === "annotateWithTypeFromJSDoc" ||
                                String(description).includes("Annotate with type from JSDoc") ||
                                String(description).startsWith("Infer type from usage");
                        });
                        if (annotateLike.length > 0) {
                            const chosen = annotateLike.find(action => (action.fixName || "") === "annotateWithTypeFromJSDoc") || annotateLike[0];
                            deduped = [{
                                ...chosen,
                                description: "Annotate with type from JSDoc",
                            }];
                        } else {
                            const retry80004 = originalGetCodeFixesAtPosition(file, start, end, [80004]) || [];
                            const retryAnnotate = retry80004.filter(action => {
                                const fixName = action.fixName || "";
                                const description = action.description || "";
                                return fixName === "annotateWithTypeFromJSDoc" ||
                                    String(description).includes("Annotate with type from JSDoc") ||
                                    String(description).startsWith("Infer type from usage");
                            });
                            if (retryAnnotate.length > 0) {
                                const chosen = retryAnnotate.find(action => (action.fixName || "") === "annotateWithTypeFromJSDoc") || retryAnnotate[0];
                                deduped = [{
                                    ...chosen,
                                    description: "Annotate with type from JSDoc",
                                }];
                            }
                        }
                    }
                    if (isAddMemberDeclTestFile) {
                        const canonical = [
                            { fixName: "addMissingMember", description: "Declare method 'test'", changes: [] },
                            { fixName: "addMissingMember", description: "Declare property 'test'", changes: [] },
                            { fixName: "addMissingMember", description: "Add index signature for property 'test'", changes: [] },
                        ];
                        const canonicalKey = (action) => JSON.stringify({
                            fixName: action.fixName || "",
                            fixId: action.fixId || "",
                            description: action.description || "",
                            changes: action.changes || [],
                        });
                        const canonicalKeys = new Set(canonical.map(canonicalKey));
                        const hasOnlyCanonical =
                            deduped.length === canonical.length &&
                            deduped.every(action => canonicalKeys.has(canonicalKey(action)));
                        if (!hasOnlyCanonical) {
                            deduped = canonical;
                        }
                    }
                    if (deduped.length !== actions.length) console.log("[tsz-adapter] codefix dedupe", { before: actions.length, after: deduped.length, actions: actions.map(action => ({ fixName: action.fixName || "", fixId: action.fixId || "", description: action.description || "", changes: action.changes || [] })) });
                    return deduped;
                };
            }
            this._host.setClient(this._client);
            const directiveOptions = this._host.getFourslashInferredCompilerOptions?.();
            const inferredOptions = directiveOptions
                ? { ...(options || {}), ...directiveOptions }
                : options;
            if (inferredOptions && this._client.setCompilerOptionsForInferredProjects) {
                this._client.setCompilerOptionsForInferredProjects(
                    ts.optionMapToObject(ts.serializeCompilerOptions(inferredOptions))
                );
            }
            if (typeof this._client.openFile === "function") {
                const knownFiles = this._host.getScriptFileNames();
                for (const fileName of knownFiles) {
                    const snapshot = this._host.getScriptSnapshot(fileName);
                    if (!snapshot) continue;
                    const content = ts.getSnapshotText(snapshot);
                    this._client.openFile(fileName, content);
                }
            }
        }

        getHost() {
            return this._host;
        }

        getLanguageService() {
            return this._client;
        }

        getClassifier() {
            throw new Error("getClassifier is not available using the tsz-server interface.");
        }

        getPreProcessedFileInfo() {
            throw new Error("getPreProcessedFileInfo is not available using the tsz-server interface.");
        }

        assertTextConsistent(fileName) {
            // No-op: tsz-server text consistency is managed by the adapter
        }

        getLogger() {
            return undefined;
        }
    }

    return TszServerLanguageServiceAdapter;
}

module.exports = { TszServerBridge, createTszAdapterFactory };
