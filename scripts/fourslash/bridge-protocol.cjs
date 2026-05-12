"use strict";

const STATE_IDLE = 0;
const STATE_REQUEST_READY = 1;
const STATE_RESPONSE_READY = 2;
const STATE_SHUTDOWN = 3;
const STATE_ERROR = 4;

// Default buffer size: 16MB (should be enough for any protocol message)
const DATA_BUFFER_SIZE = 16 * 1024 * 1024;
// Timeout for waiting on response (30 seconds)
const RESPONSE_TIMEOUT_MS = 30000;

module.exports = {
    STATE_IDLE,
    STATE_REQUEST_READY,
    STATE_RESPONSE_READY,
    STATE_SHUTDOWN,
    STATE_ERROR,
    DATA_BUFFER_SIZE,
    RESPONSE_TIMEOUT_MS,
};
