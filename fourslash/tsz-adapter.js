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
            this._worker = new Worker(path.join(__dirname, "tsz-worker.js"), {
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
                // If content is not provided, get it from the host's stored scripts
                // This is needed because the virtual file system paths don't exist on disk,
                // and tsz-server would fall back to reading from disk (which would fail).
                let fileContent = content;
                if (fileContent == null) {
                    const scriptInfo = this.getScriptInfo(fileName);
                    if (scriptInfo) {
                        fileContent = scriptInfo.content;
                    }
                }
                this._client.openFile(fileName, fileContent, scriptKindName);
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
            this._host.setClient(this._client);
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
