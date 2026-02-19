/**
 * tsz-worker.js - Worker thread for synchronous communication with tsz-server.
 *
 * This worker bridges the gap between the synchronous SessionClient (which
 * expects writeMessage/processResponse to be synchronous) and the async
 * child process I/O with tsz-server.
 *
 * Protocol:
 *   SharedArrayBuffer layout:
 *     controlArray (Int32Array, 4 elements):
 *       [0] state:  0=idle, 1=request_ready, 2=response_ready, 3=shutdown, 4=error
 *       [1] data length (bytes) for request or response
 *       [2] reserved
 *       [3] reserved
 *     dataBuffer (Uint8Array): request/response body (shared)
 *
 *   Main thread sends request:
 *     1. Writes request bytes to dataBuffer
 *     2. Sets controlArray[1] = byte length
 *     3. Atomics.store(controlArray, 0, 1)  // state = request_ready
 *     4. Atomics.notify(controlArray, 0)
 *     5. Atomics.wait(controlArray, 0, 1)   // blocks until state != 1
 *
 *   Worker processes:
 *     1. Atomics.wait(controlArray, 0, 0)   // blocks until state != 0
 *     2. Reads request from dataBuffer
 *     3. Sends Content-Length framed request to tsz-server stdin
 *     4. Reads Content-Length framed response from tsz-server stdout
 *     5. Writes response body to dataBuffer
 *     6. Sets controlArray[1] = response byte length
 *     7. Atomics.store(controlArray, 0, 2)  // state = response_ready
 *     8. Atomics.notify(controlArray, 0)
 */

"use strict";

const { workerData, parentPort } = require("worker_threads");
const { spawn } = require("child_process");

const STATE_IDLE = 0;
const STATE_REQUEST_READY = 1;
const STATE_RESPONSE_READY = 2;
const STATE_SHUTDOWN = 3;
const STATE_ERROR = 4;

const { controlBuffer, dataBuffer, tszServerBinary } = workerData;
const controlArray = new Int32Array(controlBuffer);
const dataArray = new Uint8Array(dataBuffer);

// Spawn tsz-server
const serverProcess = spawn(tszServerBinary, [], {
    stdio: ["pipe", "pipe", "pipe"],
});

// Collect stderr for debugging
let stderrChunks = [];
serverProcess.stderr.on("data", (chunk) => {
    stderrChunks.push(chunk);
});

serverProcess.on("error", (err) => {
    if (parentPort) parentPort.postMessage({ type: "error", message: err.message });
});

serverProcess.on("exit", (code, signal) => {
    if (parentPort) {
        parentPort.postMessage({
            type: "exit",
            code,
            signal,
            stderr: Buffer.concat(stderrChunks).toString("utf-8"),
        });
    }
});

/**
 * Read a Content-Length framed message from a stream.
 * Returns a Promise<string> with the message body.
 */
function readContentLengthMessage(stream) {
    return new Promise((resolve, reject) => {
        let headerBuf = "";
        let contentLength = -1;
        let bodyBuf = Buffer.alloc(0);
        let bodyBytesRead = 0;
        let phase = "header"; // "header" | "body"

        function onData(chunk) {
            try {
                if (phase === "header") {
                    headerBuf += chunk.toString("utf-8");

                    // Look for the double-CRLF separator (or double-LF)
                    const sepIdx = headerBuf.indexOf("\r\n\r\n");
                    const sepLfIdx = headerBuf.indexOf("\n\n");
                    let sepEnd = -1;
                    let sepLen = 0;

                    if (sepIdx !== -1) {
                        sepEnd = sepIdx;
                        sepLen = 4; // \r\n\r\n
                    } else if (sepLfIdx !== -1) {
                        sepEnd = sepLfIdx;
                        sepLen = 2; // \n\n
                    }

                    if (sepEnd === -1) return; // Need more data

                    // Parse Content-Length from headers
                    const headerStr = headerBuf.substring(0, sepEnd);
                    const match = headerStr.match(/Content-Length:\s*(\d+)/i);
                    if (!match) {
                        cleanup();
                        reject(new Error("No Content-Length header found: " + headerStr));
                        return;
                    }
                    contentLength = parseInt(match[1], 10);
                    bodyBuf = Buffer.alloc(contentLength);
                    bodyBytesRead = 0;
                    phase = "body";

                    // Any remaining data after the separator is part of the body
                    const remainder = headerBuf.substring(sepEnd + sepLen);
                    if (remainder.length > 0) {
                        const remainderBytes = Buffer.from(remainder, "utf-8");
                        const toCopy = Math.min(remainderBytes.length, contentLength);
                        remainderBytes.copy(bodyBuf, 0, 0, toCopy);
                        bodyBytesRead += toCopy;

                        if (bodyBytesRead >= contentLength) {
                            cleanup();
                            resolve(bodyBuf.toString("utf-8"));
                            return;
                        }
                    }
                } else {
                    // phase === "body"
                    const toCopy = Math.min(chunk.length, contentLength - bodyBytesRead);
                    chunk.copy(bodyBuf, bodyBytesRead, 0, toCopy);
                    bodyBytesRead += toCopy;

                    if (bodyBytesRead >= contentLength) {
                        cleanup();
                        resolve(bodyBuf.toString("utf-8"));
                    }
                }
            } catch (err) {
                cleanup();
                reject(err);
            }
        }

        function onError(err) {
            cleanup();
            reject(err);
        }

        function onClose() {
            cleanup();
            reject(new Error("Stream closed before complete message was read"));
        }

        function cleanup() {
            stream.removeListener("data", onData);
            stream.removeListener("error", onError);
            stream.removeListener("close", onClose);
        }

        stream.on("data", onData);
        stream.on("error", onError);
        stream.on("close", onClose);
    });
}

/**
 * Send a Content-Length framed message to tsz-server stdin.
 */
function writeContentLengthMessage(requestBody) {
    const bodyBytes = Buffer.byteLength(requestBody, "utf-8");
    const header = `Content-Length: ${bodyBytes}\r\n\r\n`;
    serverProcess.stdin.write(header + requestBody);
}

/**
 * Main loop: wait for requests from main thread, relay to tsz-server.
 */
async function mainLoop() {
    while (true) {
        // Wait for the main thread to signal a request (or shutdown)
        const waitResult = Atomics.wait(controlArray, 0, STATE_IDLE);
        // waitResult is "ok" if woken by notify, "not-equal" if value already changed

        const state = Atomics.load(controlArray, 0);

        if (state === STATE_SHUTDOWN) {
            // Graceful shutdown
            serverProcess.stdin.end();
            serverProcess.kill("SIGTERM");
            break;
        }

        if (state !== STATE_REQUEST_READY) {
            // Spurious wakeup or unexpected state, loop back
            continue;
        }

        try {
            // Read the request from shared buffer
            const requestLen = controlArray[1];
            const requestBytes = Buffer.from(
                dataArray.buffer,
                dataArray.byteOffset,
                requestLen
            );
            const requestBody = requestBytes.toString("utf-8");

            // Send to tsz-server
            writeContentLengthMessage(requestBody);

            // Read response from tsz-server
            const responseBody = await readContentLengthMessage(serverProcess.stdout);

            // Write response to shared buffer
            const responseBytes = Buffer.from(responseBody, "utf-8");
            if (responseBytes.length > dataArray.length) {
                throw new Error(
                    `Response too large: ${responseBytes.length} bytes > ${dataArray.length} buffer`
                );
            }
            responseBytes.copy(Buffer.from(dataArray.buffer, dataArray.byteOffset));
            controlArray[1] = responseBytes.length;

            // Signal response ready
            Atomics.store(controlArray, 0, STATE_RESPONSE_READY);
            Atomics.notify(controlArray, 0);
        } catch (err) {
            // Write error message to shared buffer
            const errMsg = `tsz-worker error: ${err.message}`;
            const errBytes = Buffer.from(errMsg, "utf-8");
            const copyLen = Math.min(errBytes.length, dataArray.length);
            errBytes.copy(Buffer.from(dataArray.buffer, dataArray.byteOffset), 0, 0, copyLen);
            controlArray[1] = copyLen;

            // Signal error
            Atomics.store(controlArray, 0, STATE_ERROR);
            Atomics.notify(controlArray, 0);
        }
    }
}

// Signal to parent that we're ready
if (parentPort) parentPort.postMessage({ type: "ready" });

mainLoop().catch((err) => {
    if (parentPort) parentPort.postMessage({ type: "error", message: err.message });
    process.exit(1);
});
