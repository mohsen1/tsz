#!/usr/bin/env node
//
// Aggregate (lines, bytes, file_count) statistics for the TypeScript-family
// files belonging to a tsconfig project. The bench harness calls this once per
// project row; it is also exported as a module so unit tests can exercise the
// stat aggregation and cache logic without spawning a subprocess.
//
// Performance contract: when the same tsconfig is used across multiple
// invocations within a single bench run (the project-row harness pattern),
// unchanged files must not be re-opened or re-read. Set the
// `TSZ_PROJECT_FILE_STATS_CACHE_DIR` env var (a writable directory) to enable
// the on-disk cache. The cache key is the absolute tsconfig path; cache
// entries are invalidated per file by `(mtime_ns, size)`.

import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);

const TS_FILE_RE = /\.(d\.)?[cm]?tsx?$/;
const EXCLUDED_PATH_SEGMENTS = ["/node_modules/", "/.next/"];
const LINE_COUNT_CHUNK_BYTES = 64 * 1024;
const NEWLINE_BYTE = 0x0a;

export function isTypeScriptFile(fileName) {
  return TS_FILE_RE.test(fileName);
}

export function isLocalProjectFile(fileName) {
  const normalized = fileName.split(path.sep).join("/");
  return !EXCLUDED_PATH_SEGMENTS.some((segment) => normalized.includes(segment));
}

// Counts lines (LF) in a file without loading the entire file into memory.
// Treats a missing trailing newline as a final partial line, matching the
// previous in-memory counter and `wc -l + 1 if !endsWith("\n")` style used in
// the bench harness shell fallback.
export function countNewlinesStream(file) {
  let fd;
  try {
    fd = fs.openSync(file, "r");
  } catch {
    return null;
  }
  const buffer = Buffer.allocUnsafe(LINE_COUNT_CHUNK_BYTES);
  let total = 0;
  let lastByte = -1;
  try {
    let read;
    do {
      read = fs.readSync(fd, buffer, 0, buffer.length, null);
      for (let i = 0; i < read; i += 1) {
        if (buffer[i] === NEWLINE_BYTE) total += 1;
      }
      if (read > 0) lastByte = buffer[read - 1];
    } while (read === buffer.length);
  } finally {
    fs.closeSync(fd);
  }
  if (lastByte === -1) return 0;
  if (lastByte !== NEWLINE_BYTE) total += 1;
  return total;
}

// `statFileEntry` is exported for tests that need to peek at the file
// `(size, mtimeNs)` shape used for cache invalidation. The aggregator below
// does not call it — it inlines the stat to avoid one object allocation per
// file on the cache-hit hot path.
export function statFileEntry(file) {
  try {
    const stat = fs.statSync(file);
    return { size: stat.size, mtimeNs: mtimeNsKey(stat) };
  } catch {
    return null;
  }
}

// mtimeNs is more precise than mtimeMs and avoids cache collisions when files
// are rewritten within the same millisecond (common with generators like
// type-challenges-solutions-manifest.mjs).
function mtimeNsKey(stat) {
  return typeof stat.mtimeNs === "bigint" ? stat.mtimeNs.toString() : String(stat.mtimeMs ?? 0);
}

// 24 hex chars (96 bits) is plenty to avoid filename collisions among the
// few tsconfig paths a single bench run sees, while keeping the per-row
// cache filenames short enough to scan by eye.
const CACHE_KEY_HEX_LENGTH = 24;

export function cacheKeyForTsconfig(tsconfigAbsolutePath) {
  return crypto
    .createHash("sha256")
    .update(tsconfigAbsolutePath)
    .digest("hex")
    .slice(0, CACHE_KEY_HEX_LENGTH);
}

export function loadStatsCache(cachePath) {
  if (!cachePath) return null;
  try {
    const data = JSON.parse(fs.readFileSync(cachePath, "utf8"));
    if (data && data.entries) return data;
  } catch {
    // Treat any cache read failure (missing, corrupted, permission) as cold.
  }
  return null;
}

export function saveStatsCache(cachePath, cache) {
  if (!cachePath || !cache) return;
  try {
    fs.mkdirSync(path.dirname(cachePath), { recursive: true });
    const tmp = cachePath + ".tmp." + process.pid;
    fs.writeFileSync(tmp, JSON.stringify(cache));
    fs.renameSync(tmp, cachePath);
  } catch {
    // Cache persistence is best-effort; never let it break the bench run.
  }
}

// Computes aggregate (lines, bytes, fileCount) for a list of absolute file
// paths. When a cache object is provided, files whose `(mtimeNs, size)` match
// the cached entry reuse the cached line count. The cache is replaced in
// place with a fresh entry map plus a `dirty` flag so callers can skip
// rewriting an unchanged cache file.
export function aggregateProjectStats(files, { cache } = {}) {
  let lines = 0;
  let bytes = 0;
  let fileCount = 0;
  const prevEntries = cache && cache.entries ? cache.entries : null;
  const nextEntries = cache ? {} : null;
  let dirty = !prevEntries;

  for (const file of files) {
    let stat;
    try {
      stat = fs.statSync(file);
    } catch {
      continue;
    }
    const size = stat.size;
    const mtimeNs = mtimeNsKey(stat);

    let entry = prevEntries ? prevEntries[file] : null;
    if (!entry || entry.size !== size || entry.mtimeNs !== mtimeNs) {
      const computed = countNewlinesStream(file);
      if (computed === null) continue;
      entry = { size, mtimeNs, lines: computed };
      dirty = true;
    }

    if (nextEntries) nextEntries[file] = entry;
    lines += entry.lines;
    bytes += size;
    fileCount += 1;
  }

  if (cache) {
    // Any difference in cardinality means at least one file was removed
    // (additions/modifications are caught by the per-file dirty above).
    if (prevEntries && Object.keys(prevEntries).length !== Object.keys(nextEntries).length) {
      dirty = true;
    }
    cache.entries = nextEntries;
    cache.dirty = dirty;
  }

  return { lines, bytes, fileCount };
}

function candidateTypeScriptModules() {
  const candidates = [];
  if (process.env.TSC_TOOL_DIR_VALUE) {
    candidates.push(path.join(process.env.TSC_TOOL_DIR_VALUE, "node_modules", "typescript"));
  }
  if (process.env.TSC_BIN_VALUE) {
    try {
      const realTsc = fs.realpathSync(process.env.TSC_BIN_VALUE);
      candidates.push(path.resolve(path.dirname(realTsc), ".."));
    } catch {
      // Fall back to the default module resolution candidates below.
    }
  }
  candidates.push("typescript");
  return candidates;
}

function loadTypeScript() {
  for (const candidate of candidateTypeScriptModules()) {
    try {
      return require(candidate);
    } catch {
      // Try the next candidate.
    }
  }
  throw new Error("Unable to load the TypeScript package for tsconfig parsing");
}

export function resolveTsconfigFiles(tsconfigAbsolutePath) {
  const ts = loadTypeScript();
  const config = ts.readConfigFile(tsconfigAbsolutePath, ts.sys.readFile);
  if (config.error) {
    throw new Error(ts.flattenDiagnosticMessageText(config.error.messageText, "\n"));
  }
  const parsed = ts.parseJsonConfigFileContent(
    config.config,
    ts.sys,
    path.dirname(tsconfigAbsolutePath),
    {},
    tsconfigAbsolutePath,
  );
  return [...new Set(parsed.fileNames)]
    .filter(isTypeScriptFile)
    .filter(isLocalProjectFile)
    .sort();
}

export function resolveCachePath(tsconfigAbsolutePath) {
  const cacheDir = process.env.TSZ_PROJECT_FILE_STATS_CACHE_DIR;
  if (!cacheDir) return null;
  return path.join(cacheDir, cacheKeyForTsconfig(tsconfigAbsolutePath) + ".json");
}

export function computeProjectFileStats(tsconfigAbsolutePath) {
  const files = resolveTsconfigFiles(tsconfigAbsolutePath);
  const cachePath = resolveCachePath(tsconfigAbsolutePath);
  const cache = cachePath ? (loadStatsCache(cachePath) ?? { entries: {} }) : null;
  const stats = aggregateProjectStats(files, { cache });
  if (cache && cache.dirty) saveStatsCache(cachePath, cache);
  return stats;
}

function main() {
  const tsconfig = process.argv[2] ? path.resolve(process.argv[2]) : "";
  if (!tsconfig) {
    console.error("usage: project-file-stats.mjs <tsconfig>");
    process.exit(2);
  }
  let stats;
  try {
    stats = computeProjectFileStats(tsconfig);
  } catch (err) {
    console.error(err instanceof Error ? err.message : String(err));
    process.exit(1);
  }
  console.log(`${stats.lines} ${stats.bytes} ${stats.fileCount}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
