#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import { pathToFileURL } from "node:url";

const NUL = Buffer.from([0]);
const GIT_NAME = Buffer.from(".git");
const READ_BUFFER_BYTES = 1024 * 1024;

function updateFrame(aggregate, label, value) {
  const labelBytes = Buffer.from(label, "ascii");
  const valueBytes = Buffer.isBuffer(value) ? value : Buffer.from(String(value));
  aggregate.update(labelBytes);
  aggregate.update(NUL);
  aggregate.update(Buffer.from(String(valueBytes.length), "ascii"));
  aggregate.update(NUL);
  aggregate.update(valueBytes);
  aggregate.update(NUL);
}

function bufferEndsWith(value, suffix) {
  return value.length >= suffix.length
    && value.subarray(value.length - suffix.length).equals(suffix);
}

function isBuiltinLibName(name) {
  return name.subarray(0, 3).equals(Buffer.from("lib"))
    && bufferEndsWith(name, Buffer.from(".d.ts"));
}

function joinRaw(parent, child) {
  return Buffer.concat([parent, Buffer.from("/"), child]);
}

function readPositiveInteger(name, fallback) {
  const raw = process.env[name];
  if (raw === undefined || raw === "") return fallback;
  if (!/^[1-9][0-9]*$/.test(raw)) {
    throw new Error(`${name} must be a positive integer`);
  }
  const value = Number(raw);
  if (!Number.isSafeInteger(value)) {
    throw new Error(`${name} exceeds the safe integer range`);
  }
  return value;
}

function readPositiveBigInt(name, fallback) {
  const raw = process.env[name];
  if (raw === undefined || raw === "") return fallback;
  if (!/^[1-9][0-9]*$/.test(raw)) {
    throw new Error(`${name} must be a positive integer`);
  }
  return BigInt(raw);
}

function nodeKind(stat) {
  if (stat.isDirectory()) return "directory";
  if (stat.isFile()) return "file";
  return "unsupported";
}

function modeString(stat) {
  return Number(stat.mode & 0o7777n).toString(8).padStart(4, "0");
}

function snapshotFromStat(stat) {
  return {
    stat,
    identity: `${stat.dev}:${stat.ino}:${nodeKind(stat)}`,
    stable: [
      stat.dev,
      stat.ino,
      stat.mode,
      stat.nlink,
      stat.size,
      stat.mtimeNs,
      stat.ctimeNs,
    ].join(":"),
  };
}

function snapshot(path, follow) {
  const stat = follow
    ? fs.statSync(path, { bigint: true })
    : fs.lstatSync(path, { bigint: true });
  return snapshotFromStat(stat);
}

function requireUnchanged(path, follow, before, description) {
  const after = snapshot(path, follow);
  if (after.stable !== before.stable) {
    throw new Error(`${description} changed while its source-tree fingerprint was computed`);
  }
}

function sameRawNames(left, right) {
  return left.length === right.length
    && left.every((name, index) => name.equals(right[index]));
}

export class SourceGraphWalker {
  constructor(mode, options = {}) {
    this.mode = mode;
    this.aggregate = crypto.createHash("sha256");
    this.nodes = new Map();
    this.nextNodeId = 0;
    this.edgeCount = 0;
    this.contentBytes = 0n;
    this.directoryEntryCount = 0;
    this.verificationPathBytes = 0;
    this.builtinCount = 0;
    this.nodeVerifications = [];
    this.symlinkVerifications = [];
    this.maxNodes = readPositiveInteger("TSZ_PROJECT_SOURCE_HASH_MAX_NODES", 1_000_000);
    this.maxEdges = readPositiveInteger("TSZ_PROJECT_SOURCE_HASH_MAX_EDGES", 3_000_000);
    this.maxDepth = readPositiveInteger("TSZ_PROJECT_SOURCE_HASH_MAX_DEPTH", 1024);
    this.maxDirectoryEntries = readPositiveInteger(
      "TSZ_PROJECT_SOURCE_HASH_MAX_DIRECTORY_ENTRIES",
      2_000_000,
    );
    this.maxPathBytes = options.maxPathBytes
      ?? readPositiveInteger(
        "TSZ_PROJECT_SOURCE_HASH_MAX_PATH_BYTES",
        512 * 1024 * 1024,
      );
    if (!Number.isSafeInteger(this.maxPathBytes) || this.maxPathBytes <= 0) {
      throw new Error("source-tree retained-byte budget must be a positive safe integer");
    }
    this.maxBytes = readPositiveBigInt(
      "TSZ_PROJECT_SOURCE_HASH_MAX_BYTES",
      8n * 1024n * 1024n * 1024n,
    );
    this.now = options.now ?? (() => process.hrtime.bigint());
    const maxMilliseconds = options.maxMilliseconds
      ?? readPositiveInteger("TSZ_PROJECT_SOURCE_HASH_MAX_MILLISECONDS", 300_000);
    if (!Number.isSafeInteger(maxMilliseconds) || maxMilliseconds <= 0) {
      throw new Error("source-tree elapsed-time budget must be a positive safe integer");
    }
    this.startedAt = this.now();
    this.maxElapsedNanoseconds = BigInt(maxMilliseconds) * 1_000_000n;
    updateFrame(this.aggregate, "schema", "tsz-source-tree-graph-v3");
    updateFrame(this.aggregate, "walk-mode", mode);
  }

  useEdge() {
    this.checkElapsed();
    this.edgeCount += 1;
    if (this.edgeCount > this.maxEdges) {
      throw new Error(`source-tree edge budget exceeded (${this.maxEdges})`);
    }
  }

  checkElapsed() {
    const elapsed = this.now() - this.startedAt;
    if (elapsed < 0n) {
      throw new Error("source-tree monotonic clock moved backwards");
    }
    if (elapsed > this.maxElapsedNanoseconds) {
      throw new Error(
        `source-tree elapsed-time budget exceeded (${this.maxElapsedNanoseconds / 1_000_000n}ms)`,
      );
    }
  }

  retainVerificationBytes(value) {
    this.verificationPathBytes += value.length;
    if (this.verificationPathBytes > this.maxPathBytes) {
      throw new Error(`source-tree retained-byte budget exceeded (${this.maxPathBytes})`);
    }
    return Buffer.from(value);
  }

  claimNode(observed) {
    this.checkElapsed();
    const existing = this.nodes.get(observed.identity);
    if (existing !== undefined) return { id: existing, isNew: false };
    const id = this.nextNodeId;
    this.nextNodeId += 1;
    if (this.nextNodeId > this.maxNodes) {
      throw new Error(`source-tree physical-node budget exceeded (${this.maxNodes})`);
    }
    this.nodes.set(observed.identity, id);
    return { id, isNew: true };
  }

  reference(path, follow, observed, depth) {
    const kind = nodeKind(observed.stat);
    if (kind === "unsupported") {
      throw new Error("source-tree entry resolves to an unsupported filesystem node");
    }
    const reference = this.claimNode(observed);
    updateFrame(this.aggregate, "target-node", reference.id);
    updateFrame(this.aggregate, "target-reference", reference.isNew ? "new" : "backref");
    if (reference.isNew) this.visitNode(path, follow, observed, reference.id, depth);
  }

  visitNode(path, follow, observed, id, depth) {
    if (depth > this.maxDepth) {
      throw new Error(`source-tree traversal-depth budget exceeded (${this.maxDepth})`);
    }
    const kind = nodeKind(observed.stat);
    updateFrame(this.aggregate, "node-begin", id);
    updateFrame(this.aggregate, "node-kind", kind);
    updateFrame(this.aggregate, "node-mode", modeString(observed.stat));
    this.nodeVerifications.push({
      path: this.retainVerificationBytes(path),
      follow,
      stable: observed.stable,
    });
    if (kind === "file") {
      updateFrame(this.aggregate, "content-sha256", this.hashFile(path, follow, observed));
    } else {
      this.visitDirectory(path, follow, observed, depth);
    }
    updateFrame(this.aggregate, "node-end", id);
  }

  hashFile(path, follow, observed) {
    this.checkElapsed();
    if (this.contentBytes + observed.stat.size > this.maxBytes) {
      throw new Error(`source-tree content-byte budget exceeded (${this.maxBytes})`);
    }
    const handle = fs.openSync(path, fs.constants.O_RDONLY);
    try {
      const before = snapshotFromStat(fs.fstatSync(handle, { bigint: true }));
      if (before.stable !== observed.stable) {
        throw new Error("source-tree file changed before it could be read");
      }
      const digest = crypto.createHash("sha256");
      const buffer = Buffer.allocUnsafe(READ_BUFFER_BYTES);
      let bytesRead = 0;
      for (;;) {
        this.checkElapsed();
        const count = fs.readSync(handle, buffer, 0, buffer.length, null);
        if (count === 0) break;
        digest.update(buffer.subarray(0, count));
        bytesRead += count;
      }
      this.checkElapsed();
      const after = snapshotFromStat(fs.fstatSync(handle, { bigint: true }));
      if (after.stable !== observed.stable || BigInt(bytesRead) !== observed.stat.size) {
        throw new Error("source-tree file changed while it was read");
      }
      this.contentBytes += BigInt(bytesRead);
      requireUnchanged(path, follow, observed, "source-tree file");
      return digest.digest("hex");
    } finally {
      fs.closeSync(handle);
    }
  }

  readDirectoryNames(path, limit) {
    const names = [];
    const directory = fs.opendirSync(path, { encoding: "buffer" });
    try {
      for (;;) {
        this.checkElapsed();
        const entry = directory.readSync();
        if (entry === null) break;
        if (names.length >= limit) {
          throw new Error(`source-tree directory-entry budget exceeded (${this.maxDirectoryEntries})`);
        }
        names.push(Buffer.from(entry.name));
      }
    } finally {
      directory.closeSync();
    }
    names.sort(Buffer.compare);
    return names;
  }

  visitDirectory(path, follow, observed, depth) {
    this.checkElapsed();
    const remainingEntries = this.maxDirectoryEntries - this.directoryEntryCount;
    const names = this.readDirectoryNames(path, remainingEntries);
    this.directoryEntryCount += names.length;
    for (const name of names) {
      this.checkElapsed();
      if (name.equals(GIT_NAME)) continue;
      const entryPath = joinRaw(path, name);
      const entry = snapshot(entryPath, false);
      if (entry.stat.isSymbolicLink()) {
        // Source mode retains every symlink edge, even when a file link's
        // lexical name has no source suffix. Directory aliases expose source
        // descendants, and conservatively binding file aliases ensures a
        // retarget cannot replay evidence from a different dependency graph.
        // Ordinary non-source files remain excluded below.
        if (this.mode === "builtins" && !isBuiltinLibName(name)) continue;
        this.visitSymlink(entryPath, name, entry, depth + 1);
      } else if (entry.stat.isDirectory()) {
        if (this.mode === "builtins") continue;
        this.useEdge();
        updateFrame(this.aggregate, "edge-name", name);
        updateFrame(this.aggregate, "edge-kind", "directory");
        this.reference(entryPath, false, entry, depth + 1);
      } else if (entry.stat.isFile()) {
        // TypeScript's allowNonTsExtensions option admits explicit roots with
        // arbitrary or absent suffixes. The graph walker deliberately has no
        // config-policy parser, so source mode hashes every ordinary file. This
        // can create conservative cache misses for assets, but it cannot replay
        // a stale compile result after an arbitrary-extension input changes.
        const selected = this.mode !== "builtins" || isBuiltinLibName(name);
        if (!selected) continue;
        if (this.mode === "builtins") this.builtinCount += 1;
        this.useEdge();
        updateFrame(this.aggregate, "edge-name", name);
        updateFrame(this.aggregate, "edge-kind", "file");
        this.reference(entryPath, false, entry, depth + 1);
      }
    }
    const namesAfter = this.readDirectoryNames(path, names.length);
    if (!sameRawNames(names, namesAfter)) {
      throw new Error("source-tree directory entries changed during traversal");
    }
    requireUnchanged(path, follow, observed, "source-tree directory");
  }

  visitSymlink(path, name, linkBefore, depth) {
    this.checkElapsed();
    const rawTarget = fs.readlinkSync(path, { encoding: "buffer" });
    this.symlinkVerifications.push({
      path: this.retainVerificationBytes(path),
      stable: linkBefore.stable,
      rawTarget: this.retainVerificationBytes(rawTarget),
    });
    const target = snapshot(path, true);
    const targetKind = nodeKind(target.stat);
    if (targetKind === "unsupported") {
      throw new Error("source-tree symlink resolves to an unsupported filesystem node");
    }
    if (this.mode === "builtins" && targetKind !== "file") return;
    if (this.mode === "builtins") this.builtinCount += 1;
    this.useEdge();
    updateFrame(this.aggregate, "edge-name", name);
    updateFrame(this.aggregate, "edge-kind", "symlink");
    updateFrame(this.aggregate, "symlink-mode", modeString(linkBefore.stat));
    updateFrame(this.aggregate, "symlink-target", rawTarget);
    this.reference(path, true, target, depth);
    requireUnchanged(path, false, linkBefore, "source-tree symlink");
    const targetAfter = fs.readlinkSync(path, { encoding: "buffer" });
    if (!targetAfter.equals(rawTarget)) {
      throw new Error("source-tree symlink target changed during traversal");
    }
    this.checkElapsed();
  }

  verifyFinalState() {
    for (const verification of this.nodeVerifications) {
      this.checkElapsed();
      requireUnchanged(
        verification.path,
        verification.follow,
        verification,
        "source-tree physical node",
      );
    }
    for (const verification of this.symlinkVerifications) {
      this.checkElapsed();
      requireUnchanged(
        verification.path,
        false,
        verification,
        "source-tree symlink",
      );
      const target = fs.readlinkSync(verification.path, { encoding: "buffer" });
      if (!target.equals(verification.rawTarget)) {
        throw new Error("source-tree symlink target changed during traversal");
      }
    }
  }

  hash(tree) {
    this.checkElapsed();
    const root = Buffer.from(tree.endsWith("/") ? tree.slice(0, -1) : tree);
    const rootObserved = snapshot(root, false);
    if (!rootObserved.stat.isDirectory()) {
      throw new Error("source-tree root is not a directory");
    }
    const rootReference = this.claimNode(rootObserved);
    updateFrame(this.aggregate, "root-node", rootReference.id);
    this.visitNode(root, false, rootObserved, rootReference.id, 0);
    if (this.mode === "builtins" && this.builtinCount === 0) {
      throw new Error("builtin declaration tree contains no lib*.d.ts files");
    }
    this.verifyFinalState();
    this.checkElapsed();
    return this.aggregate.digest("hex");
  }
}

export function sourceGraphHash(tree, mode = "source") {
  if (mode !== "source" && mode !== "builtins") {
    throw new Error(`unsupported source-tree graph mode: ${mode}`);
  }
  return new SourceGraphWalker(mode).hash(tree);
}

function listedPathParts(tree, listed) {
  if (listed.length === 0) throw new Error("source-tree listing contains an empty path");
  const treeBytes = Buffer.from(tree.endsWith("/") ? tree.slice(0, -1) : tree);
  const prefix = Buffer.concat([treeBytes, Buffer.from("/")]);
  if (listed.subarray(0, prefix.length).equals(prefix)) {
    return { file: listed, relative: listed.subarray(prefix.length) };
  }
  const relative = listed.subarray(0, 2).equals(Buffer.from("./"))
    ? listed.subarray(2)
    : listed;
  return { file: Buffer.concat([prefix, relative]), relative };
}

function hashListedPath(aggregate, tree, listed, untrackedOnly) {
  const { file, relative } = listedPathParts(tree, listed);
  const entry = fs.lstatSync(file, { bigint: true });
  const mode = modeString(entry);

  updateFrame(aggregate, "path", relative);
  updateFrame(aggregate, "mode", mode);
  if (entry.isSymbolicLink()) {
    updateFrame(aggregate, "kind", "symlink");
    updateFrame(aggregate, "target", fs.readlinkSync(file, { encoding: "buffer" }));
    if (!untrackedOnly) {
      const target = fs.statSync(file, { bigint: true });
      if (!target.isFile()) {
        throw new Error(`listed symlink target is not a file: ${relative.toString()}`);
      }
      updateFrame(aggregate, "target-mode", modeString(target));
      updateFrame(
        aggregate,
        "content-sha256",
        crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex"),
      );
    }
  } else if (entry.isFile()) {
    updateFrame(aggregate, "kind", "file");
    updateFrame(
      aggregate,
      "content-sha256",
      crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex"),
    );
  } else {
    throw new Error(`listed source-tree entry is neither file nor symlink: ${relative.toString()}`);
  }
}

async function readNulPaths(listing) {
  const paths = [];
  let pending = Buffer.alloc(0);

  for await (const chunk of fs.createReadStream(listing)) {
    const input = pending.length === 0 ? chunk : Buffer.concat([pending, chunk]);
    let start = 0;
    for (let nul = input.indexOf(0, start); nul !== -1; nul = input.indexOf(0, start)) {
      if (nul > start) paths.push(Buffer.from(input.subarray(start, nul)));
      start = nul + 1;
    }
    pending = Buffer.from(input.subarray(start));
  }
  if (pending.length !== 0) {
    throw new Error("source-tree listing is not NUL terminated");
  }
  paths.sort(Buffer.compare);
  return paths;
}

async function listedTreeHash(tree, listing, untrackedOnly) {
  const aggregate = crypto.createHash("sha256");
  updateFrame(aggregate, "schema", "tsz-listed-tree-v3");
  for (const listed of await readNulPaths(listing)) {
    hashListedPath(aggregate, tree, listed, untrackedOnly);
  }
  return aggregate.digest("hex");
}

async function main(args) {
  const [tree, operation] = args;
  if (!tree) {
    throw new Error(
      "usage: project-source-tree-hash.mjs <tree> <--source-tree|--builtin-libs|<nul-listing> --untracked>",
    );
  }
  if (operation === "--source-tree" && args.length === 2) {
    return sourceGraphHash(tree, "source");
  }
  if (operation === "--builtin-libs" && args.length === 2) {
    return sourceGraphHash(tree, "builtins");
  }
  if (operation && args[2] === "--untracked" && args.length === 3) {
    return listedTreeHash(tree, operation, true);
  }
  throw new Error(
    "usage: project-source-tree-hash.mjs <tree> <--source-tree|--builtin-libs|<nul-listing> --untracked>",
  );
}

if (process.argv[1]
  && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    process.stdout.write(await main(process.argv.slice(2)));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
