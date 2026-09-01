"use strict";

// The pinned TypeScript SessionClient translates language-service calls into
// tsserver protocol requests. Its simplified definition decoders intentionally
// discard metadata and context spans that the in-process language-service API
// retains, though. Fourslash baselines exercise that richer public API, so keep
// the protocol request authoritative while decoding every field returned by
// tsz-server. Never consult an in-process TypeScript language service or a
// fixture-specific substitute.

const patchedClients = new WeakSet();

function decodeDefinition(client, entry) {
    if (!entry || typeof entry !== "object" || Array.isArray(entry)) {
        throw new Error("Malformed definition response from tsz-server");
    }
    if (typeof entry.file !== "string" || !entry.start || !entry.end) {
        throw new Error("Malformed definition location from tsz-server");
    }

    const definition = {
        fileName: entry.file,
        textSpan: client.decodeSpan(entry, entry.file),
        kind: entry.kind ?? "",
        name: entry.name ?? "",
        containerKind: entry.containerKind ?? "",
        containerName: entry.containerName ?? "",
        isLocal: entry.isLocal ?? false,
        isAmbient: entry.isAmbient ?? false,
        unverified: entry.unverified ?? false,
        failedAliasResolution: entry.failedAliasResolution ?? false,
    };
    if (entry.contextStart !== undefined || entry.contextEnd !== undefined) {
        if (entry.contextStart === undefined || entry.contextEnd === undefined) {
            throw new Error("Malformed definition context span from tsz-server");
        }
        definition.contextSpan = client.decodeSpan({
            start: entry.contextStart,
            end: entry.contextEnd,
        }, entry.file);
    }
    return definition;
}

function definitionRequest(client, command, fileName, position) {
    const args = client.createFileLocationRequestArgs(fileName, position);
    const request = client.processRequest(command, args);
    const response = client.processResponse(request);
    if (!Array.isArray(response?.body)) {
        throw new Error(`Malformed ${command} response from tsz-server`);
    }
    return response.body.map(entry => decodeDefinition(client, entry));
}

function patchSessionClient(SessionClient) {
    const proto = SessionClient?.prototype;
    if (!proto) throw new TypeError("SessionClient constructor is required");
    if (patchedClients.has(SessionClient)) return;
    patchedClients.add(SessionClient);

    proto.getDefinitionAtPosition = function(fileName, position) {
        return definitionRequest(this, "definition", fileName, position);
    };
    proto.getTypeDefinitionAtPosition = function(fileName, position) {
        return definitionRequest(this, "typeDefinition", fileName, position);
    };
    proto.getDefinitionAndBoundSpan = function(fileName, position) {
        const args = this.createFileLocationRequestArgs(fileName, position);
        const request = this.processRequest("definitionAndBoundSpan", args);
        const response = this.processResponse(request);
        const body = response?.body;
        if (!body || typeof body !== "object" || !Array.isArray(body.definitions)) {
            throw new Error("Malformed definitionAndBoundSpan response from tsz-server");
        }
        if (body.definitions.length === 0) return undefined;
        return {
            definitions: body.definitions.map(entry => decodeDefinition(this, entry)),
            textSpan: body.textSpan === undefined
                ? undefined
                : this.decodeSpan(body.textSpan, request.arguments.file),
        };
    };
}

module.exports = { patchSessionClient };
