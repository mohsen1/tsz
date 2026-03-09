#!/usr/bin/env node
// tsz-server — Language Server Protocol server using the @mohsen-azimi/tsz-dev WASM module.
// Communicates over stdin/stdout using the LSP JSON-RPC protocol.

'use strict';

const path = require('path');
const { TsLanguageService, TsProgram } = require(
  path.join(__dirname, '..', 'node', 'tsz_wasm.js')
);

// ─── LSP Transport (stdin/stdout Content-Length framing) ──────────────────────
class LspTransport {
  constructor() {
    this._buf = Buffer.alloc(0);
    process.stdin.on('data', (chunk) => this._onData(chunk));
    process.stdin.on('end', () => process.exit(0));
  }

  _onData(chunk) {
    this._buf = Buffer.concat([this._buf, chunk]);
    this._processBuffer();
  }

  _processBuffer() {
    while (true) {
      const headerEnd = this._buf.indexOf('\r\n\r\n');
      if (headerEnd === -1) return;

      const header = this._buf.slice(0, headerEnd).toString('ascii');
      const match = header.match(/Content-Length:\s*(\d+)/i);
      if (!match) { this._buf = this._buf.slice(headerEnd + 4); continue; }

      const contentLength = parseInt(match[1], 10);
      const bodyStart = headerEnd + 4;
      if (this._buf.length < bodyStart + contentLength) return;

      const body = this._buf.slice(bodyStart, bodyStart + contentLength).toString('utf8');
      this._buf = this._buf.slice(bodyStart + contentLength);

      try {
        const msg = JSON.parse(body);
        this.onMessage(msg);
      } catch { /* ignore parse errors */ }
    }
  }

  send(msg) {
    const body = JSON.stringify(msg);
    const header = `Content-Length: ${Buffer.byteLength(body, 'utf8')}\r\n\r\n`;
    process.stdout.write(header + body);
  }

  // Override this
  onMessage(_msg) {}
}

// ─── Server ───────────────────────────────────────────────────────────────────
class TszServer {
  constructor() {
    this._transport = new LspTransport();
    this._transport.onMessage = (msg) => this._handle(msg);
    this._documents = new Map(); // uri -> { text, service }
    this._initialized = false;
  }

  _handle(msg) {
    const { id, method, params } = msg;

    try {
      if (method === 'initialize') {
        this._initialized = true;
        this._reply(id, {
          capabilities: {
            textDocumentSync: { openClose: true, change: 1 /* Full */ },
            completionProvider: { triggerCharacters: ['.', '"', "'", '/', '@'] },
            hoverProvider: true,
            definitionProvider: true,
            referencesProvider: true,
          },
          serverInfo: { name: 'tsz-server', version: '0.1.1' },
        });

      } else if (method === 'initialized') {
        // notification, no reply

      } else if (method === 'shutdown') {
        this._reply(id, null);

      } else if (method === 'exit') {
        process.exit(0);

      } else if (method === 'textDocument/didOpen') {
        const { textDocument } = params;
        this._openDocument(textDocument.uri, textDocument.text);
        this._publishDiagnostics(textDocument.uri);

      } else if (method === 'textDocument/didChange') {
        const { textDocument, contentChanges } = params;
        const text = contentChanges[contentChanges.length - 1].text;
        this._updateDocument(textDocument.uri, text);
        this._publishDiagnostics(textDocument.uri);

      } else if (method === 'textDocument/didClose') {
        this._documents.delete(params.textDocument.uri);
        // Clear diagnostics
        this._transport.send({
          jsonrpc: '2.0', method: 'textDocument/publishDiagnostics',
          params: { uri: params.textDocument.uri, diagnostics: [] },
        });

      } else if (method === 'textDocument/completion') {
        const result = this._completion(params);
        this._reply(id, result);

      } else if (method === 'textDocument/hover') {
        const result = this._hover(params);
        this._reply(id, result);

      } else if (method === 'textDocument/definition') {
        const result = this._definition(params);
        this._reply(id, result);

      } else if (method === 'textDocument/references') {
        const result = this._references(params);
        this._reply(id, result);

      } else if (id != null) {
        this._reply(id, null);
      }
    } catch (err) {
      if (id != null) {
        this._error(id, -32603, `Internal error: ${err.message}`);
      }
    }
  }

  _reply(id, result) {
    this._transport.send({ jsonrpc: '2.0', id, result });
  }

  _error(id, code, message) {
    this._transport.send({ jsonrpc: '2.0', id, error: { code, message } });
  }

  _getService(uri) {
    return this._documents.get(uri);
  }

  _openDocument(uri, text) {
    const fileName = this._uriToPath(uri);
    const service = new TsLanguageService(fileName, text);
    this._documents.set(uri, { text, service });
  }

  _updateDocument(uri, text) {
    const doc = this._documents.get(uri);
    if (doc) {
      doc.text = text;
      doc.service.updateSource(text);
    } else {
      this._openDocument(uri, text);
    }
  }

  _publishDiagnostics(uri) {
    const doc = this._getService(uri);
    if (!doc) return;

    // Use TsProgram for diagnostics (more accurate than language service)
    const fileName = this._uriToPath(uri);
    const program = new TsProgram();
    program.addSourceFile(fileName, doc.text);
    let diags = [];
    try {
      const raw = JSON.parse(program.getSemanticDiagnosticsJson(undefined));
      diags = raw.map((d) => this._toLspDiagnostic(d, doc.text));
    } catch { /* ignore */ }

    this._transport.send({
      jsonrpc: '2.0',
      method: 'textDocument/publishDiagnostics',
      params: { uri, diagnostics: diags },
    });
  }

  _toLspDiagnostic(d, text) {
    const lines = text.split('\n');
    const line = d.line != null ? d.line : 0;
    const char = d.character != null ? d.character : 0;
    const endLine = d.endLine != null ? d.endLine : line;
    const endChar = d.endCharacter != null ? d.endCharacter : char + (d.length || 1);

    const severityMap = { error: 1, warning: 2, suggestion: 3, message: 4 };
    const severity = severityMap[String(d.category || 'error').toLowerCase()] || 1;

    return {
      range: {
        start: { line, character: char },
        end: { line: endLine, character: endChar },
      },
      severity,
      code: d.code,
      source: 'tsz',
      message: d.messageText || d.message || '',
    };
  }

  _uriToPath(uri) {
    return uri.replace(/^file:\/\//, '').replace(/%20/g, ' ');
  }

  _positionToOffset(text, line, character) {
    const lines = text.split('\n');
    let offset = 0;
    for (let i = 0; i < Math.min(line, lines.length - 1); i++) {
      offset += lines[i].length + 1;
    }
    return offset + character;
  }

  _completion(params) {
    const { textDocument, position } = params;
    const doc = this._getService(textDocument.uri);
    if (!doc) return { isIncomplete: false, items: [] };

    const offset = this._positionToOffset(doc.text, position.line, position.character);
    try {
      const raw = JSON.parse(doc.service.getCompletionsAtPosition(offset));
      if (!raw || !raw.entries) return { isIncomplete: false, items: [] };
      return {
        isIncomplete: false,
        items: raw.entries.map((e) => ({
          label: e.name,
          kind: this._completionKind(e.kind),
          detail: e.kindModifiers,
        })),
      };
    } catch { return { isIncomplete: false, items: [] }; }
  }

  _completionKind(kind) {
    const map = {
      function: 3, method: 2, property: 10, field: 5, variable: 6,
      class: 7, interface: 8, module: 9, keyword: 14, text: 1,
    };
    return map[kind] || 1;
  }

  _hover(params) {
    const { textDocument, position } = params;
    const doc = this._getService(textDocument.uri);
    if (!doc) return null;

    const offset = this._positionToOffset(doc.text, position.line, position.character);
    try {
      const raw = JSON.parse(doc.service.getQuickInfoAtPosition(offset));
      if (!raw || !raw.displayString) return null;
      return {
        contents: { kind: 'markdown', value: '```typescript\n' + raw.displayString + '\n```' },
      };
    } catch { return null; }
  }

  _definition(params) {
    const { textDocument, position } = params;
    const doc = this._getService(textDocument.uri);
    if (!doc) return null;

    const offset = this._positionToOffset(doc.text, position.line, position.character);
    try {
      const raw = JSON.parse(doc.service.getDefinitionAtPosition(offset));
      if (!raw || !raw.length) return null;
      return raw.map((def) => ({
        uri: 'file://' + def.fileName,
        range: {
          start: { line: def.line || 0, character: def.character || 0 },
          end: { line: def.endLine || def.line || 0, character: def.endCharacter || def.character || 0 },
        },
      }));
    } catch { return null; }
  }

  _references(params) {
    const { textDocument, position } = params;
    const doc = this._getService(textDocument.uri);
    if (!doc) return [];

    const offset = this._positionToOffset(doc.text, position.line, position.character);
    try {
      const raw = JSON.parse(doc.service.getReferencesAtPosition(offset));
      if (!raw || !raw.length) return [];
      return raw.map((ref) => ({
        uri: 'file://' + ref.fileName,
        range: {
          start: { line: ref.line || 0, character: ref.character || 0 },
          end: { line: ref.endLine || ref.line || 0, character: ref.endCharacter || ref.character || 0 },
        },
      }));
    } catch { return []; }
  }
}

// Start
new TszServer();
