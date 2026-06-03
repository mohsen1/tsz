#!/usr/bin/env node
//
// Aggregate (lines, bytes, file_count) statistics for the TypeScript-family
// files belonging to a tsconfig project. The bench harness calls this once per
// project row; it is also exported as a module so unit tests can exercise the
// stat aggregation and cache logic without spawning a subprocess.
//
// Performance contract: when the same tsconfig is used across multiple
// invocations within a single bench run (the project-row harness pattern),
// unchanged files must not be re-opened or re-read, AND the project file list
// must not be re-discovered by re-walking the whole source tree. Set the
// `TSZ_PROJECT_FILE_STATS_CACHE_DIR` env var (a writable directory) to enable
// the on-disk cache. The cache key is the absolute tsconfig path. Two layers
// are cached:
//   1. The resolved project file list (the recursive `include`/`exclude`
//      directory walk done by TypeScript's `parseJsonConfigFileContent`).
//      Invalidated by the tsconfig's own `(mtime_ns, size)` and by the
//      `mtime_ns` of every directory that spans the resolved files, so a file
//      added/removed/renamed anywhere in the tree re-triggers discovery while
//      an unchanged tree skips loading TypeScript entirely.
//   2. Per-file line counts, invalidated per file by `(mtime_ns, size)`.

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

// True when `child` is `parent` itself or nested somewhere beneath it. Uses a
// relative-path probe so it is robust to mixed separators and trailing
// slashes without touching the filesystem.
function isPathUnder(child, parent) {
  const relative = path.relative(parent, child);
  return relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative));
}

// The set of directories whose `mtime` governs whether the resolved file list
// is still current: the parent directory of every file, plus — for files that
// live under the tsconfig directory — every intermediate directory up to and
// including the tsconfig directory. A file added, removed, or renamed in any
// of these directories bumps that directory's `mtime`, which lets us detect a
// stale file list without re-walking the tree. Returned sorted for stable
// cache contents.
export function contributingDirectories(files, rootDir) {
  const root = path.resolve(rootDir);
  const dirs = new Set();
  for (const file of files) {
    let dir = path.dirname(path.resolve(file));
    dirs.add(dir);
    // Walk up to `root` for in-tree files; `root` itself is added by the loop
    // when descending from a deeper file, or by the `dirs.add(dir)` above when
    // the file sits directly in it.
    if (isPathUnder(dir, root)) {
      while (dir !== root) {
        const parent = path.dirname(dir);
        if (parent === dir) break; // reached the filesystem root
        dir = parent;
        dirs.add(dir);
      }
    }
  }
  return [...dirs].sort();
}

// Stat every directory in `dirs` and return a `{ dir: mtimeNs }` map, or `null`
// if any directory cannot be stat'd (a vanished directory means the file list
// is stale and must be re-resolved).
export function directoryFingerprint(dirs) {
  const fingerprint = {};
  for (const dir of dirs) {
    let stat;
    try {
      stat = fs.statSync(dir);
    } catch {
      return null;
    }
    fingerprint[dir] = mtimeNsKey(stat);
  }
  return fingerprint;
}

// True when the cached file list is still valid: the tsconfig's own
// `(mtimeNs, size)` is unchanged (catches `include`/`exclude`/config edits) and
// every recorded directory `mtime` is unchanged (catches file additions,
// removals, and renames anywhere the file list spans).
export function fileListCacheValid(cache, tsconfigAbsolutePath) {
  const fileList = cache && cache.fileList;
  if (!fileList || !fileList.tsconfig || !fileList.dirs || !Array.isArray(fileList.files)) {
    return false;
  }
  let tsconfigStat;
  try {
    tsconfigStat = fs.statSync(tsconfigAbsolutePath);
  } catch {
    return false;
  }
  if (
    fileList.tsconfig.size !== tsconfigStat.size ||
    fileList.tsconfig.mtimeNs !== mtimeNsKey(tsconfigStat)
  ) {
    return false;
  }
  // Re-stat the recorded directories through the same helper used to build the
  // fingerprint, so the stat-and-compare logic lives in exactly one place.
  const current = directoryFingerprint(Object.keys(fileList.dirs));
  if (current === null) return false;
  for (const [dir, mtimeNs] of Object.entries(fileList.dirs)) {
    if (current[dir] !== mtimeNs) return false;
  }
  return true;
}

// Resolve the project file list, reusing a cached list when the tsconfig and
// the directories spanning the project are unchanged. On a cache hit this
// skips loading TypeScript and re-walking the source tree entirely; on a miss
// it records a fresh fingerprint and sets `cache.fileListDirty` so the caller
// persists the updated cache. `resolve` is injectable for tests so the file
// discovery can be exercised without the TypeScript package installed.
export function resolveTsconfigFilesCached(tsconfigAbsolutePath, { cache, resolve } = {}) {
  const resolveFiles = resolve ?? resolveTsconfigFiles;
  if (cache && fileListCacheValid(cache, tsconfigAbsolutePath)) {
    cache.fileListDirty = false;
    return cache.fileList.files.slice();
  }

  const files = resolveFiles(tsconfigAbsolutePath);
  if (cache) {
    const rootDir = path.dirname(path.resolve(tsconfigAbsolutePath));
    const dirFingerprint = directoryFingerprint(contributingDirectories(files, rootDir));
    let tsconfigStat = null;
    try {
      const stat = fs.statSync(tsconfigAbsolutePath);
      tsconfigStat = { size: stat.size, mtimeNs: mtimeNsKey(stat) };
    } catch {
      // Leave null; without a tsconfig stat the list cannot be cached safely.
    }
    if (dirFingerprint && tsconfigStat) {
      cache.fileList = { tsconfig: tsconfigStat, dirs: dirFingerprint, files: files.slice() };
    } else {
      delete cache.fileList;
    }
    cache.fileListDirty = true;
  }
  return files;
}

export function computeProjectFileStats(tsconfigAbsolutePath, { resolve } = {}) {
  const cachePath = resolveCachePath(tsconfigAbsolutePath);
  const cache = cachePath ? (loadStatsCache(cachePath) ?? { entries: {} }) : null;
  const files = resolveTsconfigFilesCached(tsconfigAbsolutePath, { cache, resolve });
  const fileListDirty = cache?.fileListDirty === true;
  const stats = aggregateProjectStats(files, { cache });
  if (cache && (cache.dirty || fileListDirty)) saveStatsCache(cachePath, cache);
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
