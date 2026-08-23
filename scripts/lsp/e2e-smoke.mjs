#!/usr/bin/env node
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..");

function resolveBinary() {
  const configured = process.argv[2] ?? process.env.TSZ_LSP_BIN;
  if (configured) {
    return path.resolve(configured);
  }

  const name = process.platform === "win32" ? "tsz-lsp.exe" : "tsz-lsp";
  const candidates = [
    path.join(ROOT, ".target", "dist-fast", name),
    path.join(ROOT, ".target", "debug", name),
    path.join(ROOT, ".target", "release", name),
    path.join(ROOT, "target", "debug", name),
    path.join(ROOT, "target", "release", name),
  ];
  return candidates.find((candidate) => existsSync(candidate));
}

class LspClient {
  constructor(binary) {
    this.nextId = 1;
    this.pending = new Map();
    this.stdout = Buffer.alloc(0);
    this.stderr = "";
    this.notifications = [];
    this.exited = false;
    this.process = spawn(binary, ["--mode", "stdio"], {
      cwd: ROOT,
      stdio: ["pipe", "pipe", "pipe"],
    });

    this.process.stdout.on("data", (chunk) => this.onStdout(chunk));
    this.process.stderr.on("data", (chunk) => {
      this.stderr += chunk.toString("utf8");
    });
    this.exit = new Promise((resolve) => {
      this.process.on("exit", (code, signal) => {
        this.exited = true;
        resolve({ code, signal });
        for (const { reject, timer } of this.pending.values()) {
          clearTimeout(timer);
          reject(new Error(`tsz-lsp exited before response: code=${code} signal=${signal}`));
        }
        this.pending.clear();
      });
    });
    this.process.on("error", (error) => {
      for (const { reject, timer } of this.pending.values()) {
        clearTimeout(timer);
        reject(error);
      }
      this.pending.clear();
    });
  }

  onStdout(chunk) {
    this.stdout = Buffer.concat([this.stdout, chunk]);

    while (true) {
      const headerEnd = this.stdout.indexOf("\r\n\r\n");
      if (headerEnd === -1) {
        return;
      }

      const header = this.stdout.subarray(0, headerEnd).toString("ascii");
      const match = /^Content-Length:\s*(\d+)$/im.exec(header);
      assert(match, `missing Content-Length header: ${header}`);

      const length = Number.parseInt(match[1], 10);
      const bodyStart = headerEnd + 4;
      const bodyEnd = bodyStart + length;
      if (this.stdout.length < bodyEnd) {
        return;
      }

      const body = this.stdout.subarray(bodyStart, bodyEnd).toString("utf8");
      this.stdout = this.stdout.subarray(bodyEnd);
      this.onMessage(JSON.parse(body));
    }
  }

  onMessage(message) {
    if (Object.hasOwn(message, "id")) {
      const key = JSON.stringify(message.id);
      const pending = this.pending.get(key);
      if (pending) {
        clearTimeout(pending.timer);
        this.pending.delete(key);
        pending.resolve(message);
        return;
      }
    }
    this.notifications.push(message);
  }

  send(message) {
    const body = JSON.stringify(message);
    this.process.stdin.write(
      `Content-Length: ${Buffer.byteLength(body, "utf8")}\r\n\r\n${body}`,
    );
  }

  requestMessage(method, params) {
    const id = this.nextId++;
    this.send({ jsonrpc: "2.0", id, method, params });
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(JSON.stringify(id));
        reject(new Error(`timed out waiting for ${method}; stderr:\n${this.stderr}`));
      }, 15_000);
      this.pending.set(JSON.stringify(id), { resolve, reject, timer });
    });
  }

  request(method, params) {
    return this.requestMessage(method, params).then((message) => {
      if (message.error) {
        throw new Error(`${method} failed: ${JSON.stringify(message.error)}`);
      }
      return message.result;
    });
  }

  notify(method, params) {
    this.send({ jsonrpc: "2.0", method, params });
  }

  async close() {
    if (!this.process.killed && !this.exited) {
      this.process.stdin.end();
      this.process.kill();
    }
  }

  async waitForExit() {
    let timer;
    try {
      return await Promise.race([
        this.exit,
        new Promise((_, reject) => {
          timer = setTimeout(() => {
            reject(new Error(`timed out waiting for tsz-lsp exit; stderr:\n${this.stderr}`));
          }, 5_000);
        }),
      ]);
    } finally {
      clearTimeout(timer);
    }
  }
}

const binary = resolveBinary();
assert(binary, "could not find tsz-lsp; pass a binary path or set TSZ_LSP_BIN");
assert(existsSync(binary), `tsz-lsp binary does not exist: ${binary}`);

const client = new LspClient(binary);

try {
  const initialize = await client.request("initialize", {
    processId: process.pid,
    rootUri: null,
    capabilities: {},
  });
  assert.equal(initialize.serverInfo.name, "tsz-lsp");
  assert.equal(initialize.capabilities.textDocumentSync, 1);
  for (const provider of [
    "hoverProvider",
    "definitionProvider",
    "renameProvider",
    "completionProvider",
    "diagnosticProvider",
  ]) {
    assert.equal(
      Object.hasOwn(initialize.capabilities, provider),
      false,
      `rewrite foundation must not advertise unsupported ${provider}`,
    );
  }

  client.notify("initialized", {});
  const unsupported = await client.requestMessage("textDocument/hover", {
    textDocument: { uri: "file:///unsupported.ts" },
    position: { line: 0, character: 0 },
  });
  assert.equal(unsupported.jsonrpc, "2.0");
  assert.equal(unsupported.error?.code, -32601);
  assert.equal(Object.hasOwn(unsupported, "result"), false);

  await client.request("shutdown", null);
  client.notify("exit", null);
  const exit = await client.waitForExit();
  assert.equal(exit.code, 0, `expected clean exit, got ${JSON.stringify(exit)}`);
} finally {
  await client.close();
}
