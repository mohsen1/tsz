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

  // Convert a UTF-16 offset into the source text into LSP-style
  // {line, character}. The WASM API returns diagnostics as
  // {start, length, ...}; older versions of this wrapper read non-existent
  // line/character fields and defaulted them all to 0. Issue #3525.
  _utf16OffsetToPosition(text, offset) {
    if (!text) return { line: 0, character: 0 };
    const clamped = Math.max(0, Math.min(offset >>> 0, text.length));
    let line = 0;
    let lineStart = 0;
    for (let i = 0; i < clamped; i++) {
      const ch = text.charCodeAt(i);
      if (ch === 10 /* \n */) {
        line += 1;
        lineStart = i + 1;
      } else if (ch === 13 /* \r */) {
        line += 1;
        lineStart = (text.charCodeAt(i + 1) === 10) ? i + 2 : i + 1;
        if (text.charCodeAt(i + 1) === 10) i += 1;
      } else if (ch === 0x2028 || ch === 0x2029) {
        line += 1;
        lineStart = i + 1;
      }
    }
    return { line, character: clamped - lineStart };
  }

  _toLspDiagnostic(d, text) {
    // WASM diagnostics carry `start`/`length` (UTF-16 units) and a numeric
    // `category` (1=error, 0=warning, 2=suggestion, 3=message). Translate
    // both into LSP shape.
    const start = d.start != null ? d.start >>> 0 : 0;
    const length = d.length != null ? d.length >>> 0 : 1;
    const startPos = this._utf16OffsetToPosition(text, start);
    const endPos = this._utf16OffsetToPosition(text, start + length);

    // Numeric category: error=1, warning=0, suggestion=2, message=3.
    // LSP severity: error=1, warning=2, info=3, hint=4.
    const numericMap = { 0: 2, 1: 1, 2: 3, 3: 4 };
    const stringMap = { error: 1, warning: 2, suggestion: 3, message: 4 };
    const severity =
      typeof d.category === 'number'
        ? numericMap[d.category] || 1
        : stringMap[String(d.category || 'error').toLowerCase()] || 1;

    return {
      range: { start: startPos, end: endPos },
      severity,
      code: d.code,
      source: 'tsz',
      message: d.messageText || d.message || '',
    };
  }

  _uriToPath(uri) {
    return uri.replace(/^file:\/\//, '').replace(/%20/g, ' ');
  }

  // Convert a `{start, length}` UTF-16 text-span (returned by the WASM
  // language-service APIs alongside hover/definition/reference results)
  // into an LSP `{start, end}` range using `_utf16OffsetToPosition`. Issue #3525.
  _textSpanToRange(text, span) {
    if (!span || typeof span.start !== 'number' || typeof span.length !== 'number') {
      const zero = { line: 0, character: 0 };
      return { start: zero, end: zero };
    }
    return {
      start: this._utf16OffsetToPosition(text, span.start),
      end: this._utf16OffsetToPosition(text, span.start + span.length),
    };
  }

  _completion(params) {
    const { textDocument, position } = params;
    const doc = this._getService(textDocument.uri);
    if (!doc) return { isIncomplete: false, items: [] };

    // WASM API: `getCompletionsAtPosition(line, character)` returns a JSON
    // array of `{label, kind, detail, documentation}`. Older versions of
    // this wrapper passed a single offset and read `raw.entries`. Issue #3525.
    try {
      const raw = JSON.parse(
        doc.service.getCompletionsAtPosition(position.line, position.character),
      );
      if (!Array.isArray(raw)) return { isIncomplete: false, items: [] };
      return {
        isIncomplete: false,
        items: raw.map((e) => ({
          label: e.label,
          kind: typeof e.kind === 'number' ? e.kind : 1,
          detail: e.detail,
          documentation: e.documentation,
        })),
      };
    } catch { return { isIncomplete: false, items: [] }; }
  }

  _hover(params) {
    const { textDocument, position } = params;
    const doc = this._getService(textDocument.uri);
    if (!doc) return null;

    // WASM API: `getQuickInfoAtPosition(line, character)` returns
    // `{display_parts, documentation, text_span: {start, length}}` or null.
    try {
      const raw = JSON.parse(
        doc.service.getQuickInfoAtPosition(position.line, position.character),
      );
      if (!raw) return null;
      const parts = Array.isArray(raw.display_parts) ? raw.display_parts : [];
      const value = parts.map((p) => p && p.text ? p.text : '').join('');
      if (!value) return null;
      return {
        contents: { kind: 'markdown', value: '```typescript\n' + value + '\n```' },
        range: raw.text_span ? this._textSpanToRange(doc.text, raw.text_span) : undefined,
      };
    } catch { return null; }
  }

  _definition(params) {
    const { textDocument, position } = params;
    const doc = this._getService(textDocument.uri);
    if (!doc) return null;

    // WASM API: `getDefinitionAtPosition(line, character)` returns
    // `[{file_name, text_span: {start, length}}]`.
    try {
      const raw = JSON.parse(
        doc.service.getDefinitionAtPosition(position.line, position.character),
      );
      if (!Array.isArray(raw) || raw.length === 0) return null;
      return raw.map((def) => ({
        uri: 'file://' + (def.file_name || ''),
        range: this._textSpanToRange(doc.text, def.text_span),
      }));
    } catch { return null; }
  }

  _references(params) {
    const { textDocument, position } = params;
    const doc = this._getService(textDocument.uri);
    if (!doc) return [];

    // WASM API: `getReferencesAtPosition(line, character)` returns
    // `[{file_name, text_span: {start, length}}]`.
    try {
      const raw = JSON.parse(
        doc.service.getReferencesAtPosition(position.line, position.character),
      );
      if (!Array.isArray(raw)) return [];
      return raw.map((ref) => ({
        uri: 'file://' + (ref.file_name || ''),
        range: this._textSpanToRange(doc.text, ref.text_span),
      }));
    } catch { return []; }
  }
}

// Start
new TszServer();
