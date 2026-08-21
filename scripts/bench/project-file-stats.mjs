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
//      directory walk done by TypeScript 7's unstable sync API).
//      Invalidated by the tsconfig's own `(mtime_ns, size)` and by the
//      `mtime_ns` of every directory TypeScript enumerates while parsing this
//      config, so a file
//      added/removed/renamed anywhere in the watched tree — including under a
//      previously empty included directory — re-triggers discovery, while an
//      unchanged tree skips loading TypeScript and re-globbing entirely.
//   2. Per-file line counts, invalidated per file by `(mtime_ns, size)`.

import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath, pathToFileURL } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const TYPESCRIPT_VERSIONS_FILE = path.join(
  REPO_ROOT,
  "scripts",
  "conformance",
  "typescript-versions.json",
);

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

export function pinnedTypeScriptVersion() {
  const versions = JSON.parse(fs.readFileSync(TYPESCRIPT_VERSIONS_FILE, "utf8"));
  const version =
    versions.current && versions.mappings?.[versions.current]?.npm
      ? versions.mappings[versions.current].npm
      : versions.default?.npm;
  if (typeof version !== "string" || !/^7\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error(
      `Unable to resolve the pinned TypeScript 7 version from ${TYPESCRIPT_VERSIONS_FILE}`,
    );
  }
  return version;
}

function packageRootFromTscBinary(tscBinary) {
  const realTsc = fs.realpathSync(tscBinary);
  const packageRoot = path.resolve(path.dirname(realTsc), "..");
  const expectedEntry = path.join(packageRoot, "bin", path.basename(realTsc));
  if (path.normalize(expectedEntry) !== path.normalize(realTsc)) {
    throw new Error(
      `TSC_BIN_VALUE is not a TypeScript package launcher: ${tscBinary}`,
    );
  }
  return packageRoot;
}

export function resolvePinnedTypeScriptPackageRoot({ env = process.env } = {}) {
  const toolDir = env.TSC_TOOL_DIR_VALUE?.trim();
  const tscBinary = env.TSC_BIN_VALUE?.trim();
  const toolPackageRoot = toolDir
    ? path.resolve(toolDir, "node_modules", "typescript")
    : null;
  const binaryPackageRoot = tscBinary ? packageRootFromTscBinary(tscBinary) : null;

  if (
    toolPackageRoot &&
    binaryPackageRoot &&
    fs.realpathSync(toolPackageRoot) !== fs.realpathSync(binaryPackageRoot)
  ) {
    throw new Error(
      `Pinned TypeScript tool/binary roots disagree: ${toolPackageRoot} != ${binaryPackageRoot}`,
    );
  }

  // An explicit bench-tool selection is authoritative. Otherwise use the
  // repository-owned installation prepared by ensure-pinned-typescript.sh.
  // Never ask Node to discover an ambient/global `typescript` package.
  const packageRoot = toolPackageRoot ?? binaryPackageRoot ?? path.join(REPO_ROOT, "scripts", "node_modules", "typescript");
  const packageJsonPath = path.join(packageRoot, "package.json");
  let packageJson;
  try {
    packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
  } catch (error) {
    throw new Error(
      `Pinned TypeScript package is unavailable at ${packageJsonPath}; run scripts/setup/ensure-pinned-typescript.sh scripts (${error.message})`,
    );
  }
  const expectedVersion = pinnedTypeScriptVersion();
  if (packageJson.name !== "typescript" || packageJson.version !== expectedVersion) {
    throw new Error(
      `Pinned TypeScript package mismatch at ${packageJsonPath}: expected typescript@${expectedVersion}, found ${packageJson.name ?? "<missing>"}@${packageJson.version ?? "<missing>"}`,
    );
  }
  return fs.realpathSync(packageRoot);
}

function unstableSyncEntry(packageRoot) {
  const packageJson = JSON.parse(
    fs.readFileSync(path.join(packageRoot, "package.json"), "utf8"),
  );
  const exported = packageJson.exports?.["./unstable/sync"];
  const relativeEntry =
    typeof exported === "string" ? exported : (exported?.import ?? exported?.default);
  if (typeof relativeEntry !== "string") return null;
  return path.resolve(packageRoot, relativeEntry);
}

async function loadTypeScriptSyncAPI() {
  const packageRoot = resolvePinnedTypeScriptPackageRoot();
  const entry = unstableSyncEntry(packageRoot);
  if (!entry || !fs.statSync(entry).isFile()) {
    throw new Error(
      `Pinned TypeScript package does not export ./unstable/sync: ${packageRoot}`,
    );
  }
  const module = await import(pathToFileURL(entry).href);
  if (typeof module.API !== "function") {
    throw new Error(
      `Pinned TypeScript ./unstable/sync entry has no API constructor: ${entry}`,
    );
  }
  return module.API;
}

const TypeScriptSyncAPI = await loadTypeScriptSyncAPI();

// Resolve the project file list and record the directories TypeScript enumerates
// while expanding the config's include/exclude patterns. The TS7 unstable sync
// API intentionally exposes only `{ options, fileNames }`, not the stable API's
// former `wildcardDirectories`. Its virtual-filesystem callback is a precise
// substitute: returning `undefined` delegates each read to the real filesystem,
// while recording every directory visited by the native config parser. Each
// visited directory is an exact, non-recursive watch point; if a file or child
// directory is later added/removed/renamed, that directory's mtime changes and
// invalidates the cached file list. Previously empty included directories are
// recorded too.
export function resolveTsconfigFiles(tsconfigAbsolutePath) {
  const enumeratedDirectories = new Set();
  const api = new TypeScriptSyncAPI({
    cwd: path.dirname(tsconfigAbsolutePath),
    fs: {
      getAccessibleEntries(directoryName) {
        const absolute = path.resolve(directoryName);
        if (!isExcludedDirectory(absolute)) enumeratedDirectories.add(absolute);
        return undefined;
      },
    },
  });
  let parsed;
  try {
    parsed = api.parseConfigFile(tsconfigAbsolutePath);
  } finally {
    api.close();
  }
  const files = [...new Set(parsed.fileNames)]
    .filter(isTypeScriptFile)
    .filter(isLocalProjectFile)
    .sort();
  const watchDirectories = [...enumeratedDirectories]
    .sort()
    .map((dir) => ({ dir, recursive: false }));
  return { files, watchDirectories };
}

export function resolveCachePath(tsconfigAbsolutePath) {
  const cacheDir = process.env.TSZ_PROJECT_FILE_STATS_CACHE_DIR;
  if (!cacheDir) return null;
  return path.join(cacheDir, cacheKeyForTsconfig(tsconfigAbsolutePath) + ".json");
}

function isExcludedDirectory(dirPath) {
  const normalized = dirPath.split(path.sep).join("/") + "/";
  return EXCLUDED_PATH_SEGMENTS.some((segment) => normalized.includes(segment));
}

// Expand the resolver's watched directories into the full set of directories
// whose `mtime` must be fingerprinted to detect a changed file list.
//
// A directory's `mtime` only advances when an entry is added, removed, or
// renamed directly inside it — not when a descendant changes. So a recursive
// watch (TypeScript descends into subdirectories) must contribute every
// existing directory in its subtree, otherwise a file added under a previously
// empty subdirectory would go unnoticed. A non-recursive watch contributes only
// the watched directory itself. `node_modules`/`.next` subtrees are skipped to
// match the project-file filter. Returned sorted for stable cache contents.
export function expandWatchedDirectories(watchDirectories) {
  const dirs = new Set();
  for (const { dir, recursive } of watchDirectories ?? []) {
    const root = path.resolve(dir);
    if (!recursive) {
      dirs.add(root);
      continue;
    }
    const stack = [root];
    while (stack.length > 0) {
      const current = stack.pop();
      if (dirs.has(current)) continue;
      let entries;
      try {
        entries = fs.readdirSync(current, { withFileTypes: true });
      } catch {
        // A vanished/unreadable directory is simply not tracked; if it had been
        // recorded previously its absence surfaces as a fingerprint mismatch.
        continue;
      }
      dirs.add(current);
      for (const entry of entries) {
        if (!entry.isDirectory()) continue;
        const child = path.join(current, entry.name);
        if (!isExcludedDirectory(child)) stack.push(child);
      }
    }
  }
  return [...dirs].sort();
}

function fingerprintsMatch(a, b) {
  const aKeys = Object.keys(a);
  if (aKeys.length !== Object.keys(b).length) return false;
  for (const key of aKeys) {
    if (a[key] !== b[key]) return false;
  }
  return true;
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

// Stat the tsconfig and return its `{ size, mtimeNs }` invalidation key, or
// null when it cannot be stat'd. Mirrors `statFileEntry` for the file list's
// own config key, keeping the stat-to-key shape in one place.
function tsconfigStatKey(tsconfigAbsolutePath) {
  try {
    const stat = fs.statSync(tsconfigAbsolutePath);
    return { size: stat.size, mtimeNs: mtimeNsKey(stat) };
  } catch {
    return null;
  }
}

// True when the cached file list is still valid: the tsconfig's own
// `(mtimeNs, size)` is unchanged (catches `include`/`exclude`/config edits) and
// re-fingerprinting the recorded watched directories yields an identical map
// (catches file additions, removals, renames, and new/removed subdirectories
// anywhere the watched tree spans, including previously empty directories).
export function fileListCacheValid(cache, tsconfigAbsolutePath) {
  const fileList = cache && cache.fileList;
  if (
    !fileList ||
    !fileList.tsconfig ||
    !fileList.dirs ||
    !Array.isArray(fileList.files) ||
    !Array.isArray(fileList.watch)
  ) {
    return false;
  }
  const tsconfigStat = tsconfigStatKey(tsconfigAbsolutePath);
  if (
    tsconfigStat === null ||
    fileList.tsconfig.size !== tsconfigStat.size ||
    fileList.tsconfig.mtimeNs !== tsconfigStat.mtimeNs
  ) {
    return false;
  }
  // Re-expand and re-stat the watched directories. Expanding from the recorded
  // watch roots rediscovers any newly created subdirectory; comparing the full
  // map then catches both mtime changes and added/removed directories.
  const current = directoryFingerprint(expandWatchedDirectories(fileList.watch));
  return current !== null && fingerprintsMatch(fileList.dirs, current);
}

// Resolve the project file list, reusing a cached list when the tsconfig and
// the watched directory tree are unchanged. On a cache hit this skips loading
// TypeScript and re-globbing the source tree (only a directory-only walk runs);
// on a miss it records a fresh fingerprint and sets `cache.fileListDirty` so
// the caller persists the updated cache. `resolve` is injectable for tests so
// file discovery can be exercised without the TypeScript package installed; it
// returns `{ files, watchDirectories }`.
export function resolveTsconfigFilesCached(tsconfigAbsolutePath, { cache, resolve } = {}) {
  const resolveFiles = resolve ?? resolveTsconfigFiles;
  if (cache && fileListCacheValid(cache, tsconfigAbsolutePath)) {
    cache.fileListDirty = false;
    return cache.fileList.files.slice();
  }

  const { files, watchDirectories = [] } = resolveFiles(tsconfigAbsolutePath);
  if (cache) {
    const dirFingerprint = directoryFingerprint(expandWatchedDirectories(watchDirectories));
    // Null when a watched directory or the tsconfig vanished mid-resolve;
    // without a complete fingerprint the list cannot be cached safely, so fall
    // through to `delete cache.fileList`.
    const tsconfigStat = tsconfigStatKey(tsconfigAbsolutePath);
    if (dirFingerprint && tsconfigStat) {
      cache.fileList = {
        tsconfig: tsconfigStat,
        watch: watchDirectories,
        dirs: dirFingerprint,
        files: files.slice(),
      };
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
