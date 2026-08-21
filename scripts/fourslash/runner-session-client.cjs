"use strict";

// The pinned TypeScript SessionClient already translates language-service
// calls into tsserver protocol requests. Keep that implementation authoritative:
// a canonical fourslash result must be derived from the response sent by
// tsz-server, never from an in-process TypeScript language service or a
// fixture-specific substitute.

function patchSessionClient(SessionClient) {
    const proto = SessionClient?.prototype;
    if (!proto) throw new TypeError("SessionClient constructor is required");
    // Deliberately do not override the pinned protocol client. The function is
    // retained as an explicit integration boundary for both runner modes.
}

module.exports = { patchSessionClient };
