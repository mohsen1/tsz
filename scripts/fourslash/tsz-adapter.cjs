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
        this._requestSeq = 0;
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

    resetSession() {
        const requestBody = JSON.stringify({
            seq: ++this._requestSeq,
            type: "request",
            command: "tsz/reset",
            arguments: {},
        });
        const responseBody = this.sendRequest(requestBody);
        let response;
        try {
            response = JSON.parse(responseBody);
        } catch (err) {
            throw new Error(`Invalid reset response: ${err.message}`);
        }
        if (!response || response.success !== true) {
            const message = response && response.message ? response.message : responseBody;
            throw new Error(`tsz-server reset failed: ${message}`);
        }
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

    const normalizeModuleDirective = (value) => {
        if (typeof value !== "string") return value;
        const normalized = value.trim().toLowerCase();
        if (normalized === "node" || normalized === "nodejs") {
            return "commonjs";
        }
        return value;
    };

    const normalizeCompilerOptions = (rawOptions) => {
        if (!rawOptions || typeof rawOptions !== "object") return rawOptions;
        const normalized = { ...rawOptions };
        for (const [key, value] of Object.entries(rawOptions)) {
            const lowerKey = key.toLowerCase();
            if (lowerKey === "module") {
                const normalizedValue = typeof value === "string" ? normalizeModuleDirective(value) : value;
                if (key !== "module") {
                    delete normalized[key];
                }
                normalized.module = normalizedValue;
            } else if (lowerKey === "target" && key !== "target") {
                delete normalized[key];
                normalized.target = value;
            }
        }
        return normalized;
    };

    function estimateJsdocInferActionLabels(content, startLineOneBased) {
        const lines = content.split(/\r?\n/);
        if (!lines.length) return [];

        let lineIdx = Math.min(
            Math.max((startLineOneBased ?? 1) - 1, 0),
            lines.length - 1,
        );
        while (lineIdx > 0 && !lines[lineIdx].includes("/**")) {
            lineIdx--;
        }
        if (!lines[lineIdx].includes("/**")) return [];

        let blockEnd = lineIdx;
        while (blockEnd < lines.length && !lines[blockEnd].includes("*/")) {
            blockEnd++;
        }
        if (blockEnd >= lines.length) return [];

        const targetLine = lines.slice(blockEnd + 1).find(line => line.trim().length > 0);
        if (!targetLine) return [];

        const open = targetLine.indexOf("(");
        const close = targetLine.lastIndexOf(")");
        if (open < 0 || close <= open) return [];

        return targetLine.slice(open + 1, close)
            .split(",")
            .map(segment => segment.trim())
            .filter(segment => segment.length > 0 && !segment.includes(":"))
            .map(segment => segment.replace(/^\.\.\./, "").replace(/[^A-Za-z0-9_$].*$/, ""))
            .filter(label => label.length > 0);
    }

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
                    const match = trimmed.match(/^\/\/\s*@([A-Za-z]+)\s*:\s*(.*)$/);
                    if (!match) continue;
                    const [, rawKey, rawValue] = match;
                    const key = rawKey.toLowerCase();
                    const value = rawValue.split(",")[0]?.trim();
                    if (!value) continue;
                    if (key === "module") {
                        inferred.module = normalizeModuleDirective(value);
                        sawDirective = true;
                    } else if (key === "target") {
                        inferred.target = value;
                        sawDirective = true;
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
            let outboundMessage = message;
            try {
                const request = JSON.parse(message);
                if (
                    request &&
                    request.command === "compilerOptionsForInferredProjects" &&
                    request.arguments &&
                    request.arguments.options
                ) {
                    // Options arrive already serialized. Only normalize `module`
                    // aliases; don't re-run serializeCompilerOptions (see wrapper
                    // comment above — it reverse-maps "es5" → null).
                    const normalizedOptions = normalizeCompilerOptions(request.arguments.options);
                    if (normalizedOptions) {
                        request.arguments.options = normalizedOptions;
                        outboundMessage = JSON.stringify(request);
                    }
                }
            } catch {
                // Best-effort normalization only; keep original payload on parse failures.
            }

            // message is the raw JSON request from SessionClient
            // Send to tsz-server and get raw JSON response
            const responseBody = bridge.sendRequest(outboundMessage);

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
            // `LanguageServiceAdapterHost`'s base `openFile` is a no-op, so
            // an overriding `content` passed by `goTo.file(name, content)`
            // never reaches the scriptInfo. Apply it ourselves so that the
            // native LS (which reads through `getScriptSnapshot` →
            // `scriptInfo.content`) also sees the updated buffer — otherwise
            // tsz-server and native disagree on what the file contains.
            if (typeof content === "string") {
                const existingScriptInfo = this.getScriptInfo(fileName);
                if (existingScriptInfo) {
                    if (typeof existingScriptInfo.updateContent === "function") {
                        existingScriptInfo.updateContent(content);
                    } else {
                        existingScriptInfo.content = content;
                        if (typeof existingScriptInfo.version === "number") {
                            existingScriptInfo.version++;
                        }
                        existingScriptInfo.lineMap = undefined;
                    }
                } else if (typeof this.addScript === "function") {
                    this.addScript(fileName, content, /*isRootFile*/ false);
                }
            }
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
                    let contentToSend = fileContent;
                    if (contentToSend == null) {
                        if (this._openedFiles.has(path)) return;
                        const scriptInfo = this.getScriptInfo(path);
                        if (scriptInfo) contentToSend = scriptInfo.content;
                    }
                    if (contentToSend == null) return;
                    // When explicit content is supplied (e.g.
                    // `goTo.file(name, overridingContent)`), always push the
                    // update to tsz-server even if the path is already open —
                    // otherwise tsz keeps the stale snapshot and
                    // completion/diagnostics on it disagree with tsc.
                    if (this._openedFiles.has(path)) {
                        this._client.closeFile(path);
                    }
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
                // `getFilenames()` restricts to root scripts (isRootFile=true),
                // which skips fixture files under node_modules even though
                // fourslash registered them. Enumerate scriptInfos directly so
                // tsz-server sees the full test virtual FS.
                const trackedScriptFiles = (() => {
                    if (typeof this.scriptInfos?.forEach === "function") {
                        const names = [];
                        this.scriptInfos.forEach((info) => {
                            if (info && typeof info.fileName === "string") {
                                names.push(info.fileName);
                            }
                        });
                        return names;
                    }
                    return this.getFilenames();
                })();
                for (const path of trackedScriptFiles) {
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
            // Fallback TypeScript language service removed: fourslash tests must
            // exercise tsz-server itself. Callers that expect these helpers get
            // defaults (false / empty).
            this._client.updateIsDefinitionOfReferencedSymbols = () => false;
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
                this._client.getCodeFixesAtPosition = (file, start, end, errorCodes, formatOptions, preferences) => {
                    if (preferences && this._client.configure) {
                        this._client.configure(preferences);
                    }
                    const actions = originalGetCodeFixesAtPosition(
                        file,
                        start,
                        end,
                        errorCodes,
                        formatOptions,
                        preferences,
                    ) || [];
                    // Deduplicate only; test-file-specific filtering and canned
                    // canonical-action substitutions removed so the harness
                    // reflects what tsz-server actually returns.
                    const seenForCall = new Set();
                    const deduped = [];
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
                    return deduped;
                };
            }
            const originalSetCompilerOptionsForInferredProjects = this._client.setCompilerOptionsForInferredProjects?.bind(this._client);
            if (originalSetCompilerOptionsForInferredProjects) {
                this._client.setCompilerOptionsForInferredProjects = (rawOptions) => {
                    // Callers already pass options in serialized (protocol) form.
                    // Do NOT call ts.serializeCompilerOptions again — its reverse
                    // map treats the already-serialized values as unknowns and
                    // emits null (e.g. "es5" → null), which drops the effective
                    // `lib` from tsz-server's inferred-project state.
                    const normalizedOptions = normalizeCompilerOptions(rawOptions);
                    return originalSetCompilerOptionsForInferredProjects(
                        normalizedOptions || {}
                    );
                };
            }
            this._host.setClient(this._client);
            const directiveOptions = this._host.getFourslashInferredCompilerOptions?.();
            const inferredOptions = directiveOptions
                ? { ...(options || {}), ...directiveOptions }
                : options;
            const effectiveCompilerOptions = normalizeCompilerOptions(inferredOptions);
            if (effectiveCompilerOptions && this._client.setCompilerOptionsForInferredProjects) {
                this._client.setCompilerOptionsForInferredProjects(
                    ts.optionMapToObject(ts.serializeCompilerOptions(effectiveCompilerOptions))
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

        getPreProcessedFileInfo(fileName, fileContents) {
            return ts.preProcessFile(
                fileContents,
                /*readImportFiles*/ true,
                ts.hasJSFileExtension(fileName),
            );
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
