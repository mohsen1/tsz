#!/usr/bin/env node
//
// Tests for `project-file-stats.mjs` covering:
//  - Correct (lines, bytes, file_count) aggregation on a fresh invocation.
//  - Reuse of cached per-file line counts when (mtime, size) are unchanged.
//  - Cache invalidation on file modification, file removal, and file
//    replacement (size-preserving rewrite).
//  - TS7 unstable-sync config parsing, including directory-watch capture and
//    no build-info writes for incremental projects.
//  - The shell fallback `sum_ts_stats` in bench-vs-tsgo.sh (single-pass
//    traversal).
//  - File-name filters (TypeScript-family extensions, node_modules/.next
//    exclusion).
//  - Line-count edge cases: empty file, no trailing newline, CRLF.

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  aggregateProjectStats,
  cacheKeyForTsconfig,
  computeProjectFileStats,
  countNewlinesStream,
  expandWatchedDirectories,
  isLocalProjectFile,
  isTypeScriptFile,
  loadStatsCache,
  resolveTsconfigFilesCached,
  saveStatsCache,
  statFileEntry,
} from "./project-file-stats.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const BENCH_SCRIPT = path.join(SCRIPT_DIR, "bench-vs-tsgo.sh");
const BENCH_PREREQS_SCRIPT = path.join(SCRIPT_DIR, "lib", "bench-vs-tsgo-prereqs.sh");
const BENCH_RESULTS_SCRIPT = path.join(SCRIPT_DIR, "lib", "bench-vs-tsgo-results.sh");

function makeTempDir(prefix) {
  return fs.mkdtempSync(path.join(os.tmpdir(), prefix));
}

function writeFile(dir, name, contents) {
  const full = path.join(dir, name);
  fs.mkdirSync(path.dirname(full), { recursive: true });
  fs.writeFileSync(full, contents);
  return full;
}

function listAbsoluteFiles(dir, files) {
  return files.map((relative) => path.join(dir, relative));
}

// Shared awk snippet that extracts a single top-level shell function (from its
// `name()` line through the next column-0 `}`) out of the prereqs library, so a
// test can source just that function without the whole file's bench globals.
function awkExtractFunction(funcName) {
  const script = BENCH_PREREQS_SCRIPT.replace(/'/g, "'\\''");
  return `awk '/^${funcName}\\(\\)/{flag=1} flag{print} /^}/{if(flag){exit}}' '${script}'`;
}

// --------------------------------------------------------------------------
// File-name filter helpers.

assert.equal(isTypeScriptFile("foo.ts"), true);
assert.equal(isTypeScriptFile("foo.tsx"), true);
assert.equal(isTypeScriptFile("foo.d.ts"), true);
assert.equal(isTypeScriptFile("foo.mts"), true);
assert.equal(isTypeScriptFile("foo.cts"), true);
assert.equal(isTypeScriptFile("foo.d.mts"), true);
assert.equal(isTypeScriptFile("foo.js"), false);
assert.equal(isTypeScriptFile("foo.md"), false);
assert.equal(isTypeScriptFile("foo.ts.snap"), false);

assert.equal(isLocalProjectFile("/repo/src/foo.ts"), true);
assert.equal(isLocalProjectFile("/repo/node_modules/foo/index.d.ts"), false);
assert.equal(isLocalProjectFile("/repo/.next/types/foo.ts"), false);
assert.equal(
  isLocalProjectFile(path.join("repo", "node_modules", "foo", "bar.ts")),
  false,
  "platform-native path separators should be normalized before the filter",
);

// --------------------------------------------------------------------------
// countNewlinesStream edge cases.

{
  const dir = makeTempDir("tsz-stats-newlines-");
  try {
    const empty = writeFile(dir, "empty.ts", "");
    assert.equal(countNewlinesStream(empty), 0, "empty file has zero lines");

    const oneLineNoNewline = writeFile(dir, "no-trailing.ts", "export {}");
    assert.equal(
      countNewlinesStream(oneLineNoNewline),
      1,
      "missing trailing newline still counts the final line",
    );

    const oneLineWithNewline = writeFile(dir, "with-trailing.ts", "export {}\n");
    assert.equal(
      countNewlinesStream(oneLineWithNewline),
      1,
      "single line with trailing newline is one line",
    );

    const threeLines = writeFile(dir, "three.ts", "a\nb\nc\n");
    assert.equal(countNewlinesStream(threeLines), 3);

    const crlf = writeFile(dir, "crlf.ts", "a\r\nb\r\nc\r\n");
    assert.equal(
      countNewlinesStream(crlf),
      3,
      "CRLF line endings are counted by trailing LF, matching wc -l",
    );

    // File larger than a single read chunk to exercise the streaming path.
    const big = writeFile(dir, "big.ts", "a\n".repeat(200_000));
    assert.equal(countNewlinesStream(big), 200_000);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// --------------------------------------------------------------------------
// aggregateProjectStats: fresh invocation matches a naive aggregator.

{
  const dir = makeTempDir("tsz-stats-aggregate-");
  try {
    const files = [
      writeFile(dir, "a.ts", "export const a = 1;\nexport const b = 2;\n"),
      writeFile(dir, "b.ts", "// no trailing newline"),
      writeFile(dir, "nested/c.ts", "line1\nline2\nline3\nline4\n"),
    ];

    const stats = aggregateProjectStats(files);
    const expectedBytes = files.reduce((acc, f) => acc + fs.statSync(f).size, 0);
    const expectedLines = files.reduce((acc, f) => acc + countNewlinesStream(f), 0);
    assert.equal(stats.fileCount, 3);
    assert.equal(stats.bytes, expectedBytes);
    assert.equal(stats.lines, expectedLines);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// --------------------------------------------------------------------------
// aggregateProjectStats: cache reuse + invalidation.

{
  const dir = makeTempDir("tsz-stats-cache-");
  try {
    const fileA = writeFile(dir, "a.ts", "alpha\nbeta\ngamma\n");
    const fileB = writeFile(dir, "b.ts", "one\ntwo\n");
    const files = [fileA, fileB];

    const cache = { entries: {} };
    const first = aggregateProjectStats(files, { cache });
    assert.equal(first.fileCount, 2);
    assert.equal(first.lines, 5);
    assert.equal(Object.keys(cache.entries).length, 2);
    assert.equal(cache.dirty, true, "first pass over an empty cache is dirty");

    // Second pass with no changes: cache entries must be reused without
    // re-reading content. We monkey-patch the stream counter to throw if it
    // is called again, asserting the cache hit path.
    const originalOpenSync = fs.openSync;
    let openCalls = 0;
    fs.openSync = (...args) => {
      openCalls += 1;
      return originalOpenSync.apply(fs, args);
    };
    let second;
    try {
      second = aggregateProjectStats(files, { cache });
    } finally {
      fs.openSync = originalOpenSync;
    }
    assert.deepEqual(second, first, "stats must be byte-identical on a cache hit");
    assert.equal(
      openCalls,
      0,
      "cached files must not be re-opened when (mtimeNs, size) is unchanged",
    );
    assert.equal(cache.dirty, false, "steady-state pass leaves the cache clean");

    // Modify file A so cache must invalidate just that entry.
    // Wait long enough that mtime changes even on coarse-resolution fs.
    const beforeMtime = cache.entries[fileA].mtimeNs;
    while (statFileEntry(fileA).mtimeNs === beforeMtime) {
      fs.writeFileSync(fileA, "alpha\nbeta\ngamma\ndelta\n");
    }
    const third = aggregateProjectStats(files, { cache });
    assert.equal(third.fileCount, 2);
    assert.equal(third.lines, 6, "modified file A picks up the new line count");
    assert.notEqual(
      cache.entries[fileA].mtimeNs,
      beforeMtime,
      "cache entry for the modified file must be refreshed",
    );
    assert.equal(cache.dirty, true, "modification dirties the cache");

    // Remove file A from the project: its cache entry should be GC'd so the
    // cache cannot grow unbounded across many rows.
    const fourth = aggregateProjectStats([fileB], { cache });
    assert.equal(fourth.fileCount, 1);
    assert.equal(fourth.lines, 2);
    assert.equal(
      cache.entries[fileA],
      undefined,
      "removed files must be pruned from the cache",
    );
    assert.ok(cache.entries[fileB], "surviving files keep their cache entry");
    assert.equal(cache.dirty, true, "removal of a file dirties the cache");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// --------------------------------------------------------------------------
// expandWatchedDirectories: recursive watches contribute every existing
// subdirectory (including empty ones); non-recursive watches contribute only
// the watched directory; node_modules subtrees are skipped.

{
  const dir = makeTempDir("tsz-stats-watchdirs-");
  try {
    const srcDir = path.join(dir, "src");
    writeFile(srcDir, "a.ts", "alpha\n");
    fs.mkdirSync(path.join(srcDir, "empty"), { recursive: true });
    writeFile(path.join(srcDir, "nested"), "b.ts", "beta\n");
    writeFile(path.join(dir, "node_modules", "pkg"), "leak.ts", "leak\n");

    const recursive = expandWatchedDirectories([{ dir: srcDir, recursive: true }]);
    assert.ok(recursive.includes(srcDir));
    assert.ok(
      recursive.includes(path.join(srcDir, "empty")),
      "recursive watch tracks a currently empty included directory",
    );
    assert.ok(recursive.includes(path.join(srcDir, "nested")));
    assert.ok(
      !recursive.some((d) => d.split(path.sep).join("/").includes("/node_modules/")),
      "node_modules subtrees are excluded from the watched directory set",
    );

    const flat = expandWatchedDirectories([{ dir: srcDir, recursive: false }]);
    assert.deepEqual(flat, [srcDir], "a non-recursive watch tracks only the watched directory");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// --------------------------------------------------------------------------
// resolveTsconfigFilesCached: skip re-walking an unchanged tree, re-resolve on
// file-list-affecting changes.

{
  const dir = makeTempDir("tsz-stats-filelist-");
  try {
    const srcDir = path.join(dir, "src");
    const fileA = writeFile(srcDir, "a.ts", "alpha\n");
    const fileB = writeFile(srcDir, "b.ts", "beta\n");
    const tsconfig = writeFile(dir, "tsconfig.json", JSON.stringify({ include: ["src"] }));
    const watchDirectories = [{ dir: srcDir, recursive: true }];

    let resolveCalls = 0;
    let currentFiles = [fileA, fileB];
    const resolve = () => {
      resolveCalls += 1;
      return { files: currentFiles.slice().sort(), watchDirectories };
    };

    const cache = { entries: {} };
    const first = resolveTsconfigFilesCached(tsconfig, { cache, resolve });
    assert.equal(resolveCalls, 1, "cold cache resolves the file list once");
    assert.deepEqual(first, [fileA, fileB].sort());
    assert.equal(cache.fileListDirty, true, "cold resolve dirties the file-list cache");
    assert.ok(cache.fileList && cache.fileList.files.length === 2, "file list is cached");

    const second = resolveTsconfigFilesCached(tsconfig, { cache, resolve });
    assert.equal(resolveCalls, 1, "an unchanged tree reuses the cached file list");
    assert.deepEqual(second, first);
    assert.equal(cache.fileListDirty, false, "a cache hit leaves the file-list cache clean");

    // Adding a file bumps the tracked source directory's mtime. Use uniquely
    // named files so the directory mtime is guaranteed to advance even on a
    // coarse-resolution filesystem.
    const beforeDirMtime = statFileEntry(srcDir).mtimeNs;
    let fileC;
    let counter = 0;
    do {
      fileC = writeFile(srcDir, `c${counter++}.ts`, "gamma\n");
    } while (statFileEntry(srcDir).mtimeNs === beforeDirMtime);
    currentFiles = [fileA, fileB, fileC];
    const third = resolveTsconfigFilesCached(tsconfig, { cache, resolve });
    assert.equal(resolveCalls, 2, "a new file in a tracked directory re-resolves the list");
    assert.equal(third.length, 3, "the freshly discovered file appears in the list");
    assert.equal(cache.fileListDirty, true);

    // A subsequent unchanged pass is a hit again.
    resolveTsconfigFilesCached(tsconfig, { cache, resolve });
    assert.equal(resolveCalls, 2, "the refreshed file list is reused while the tree is stable");

    // Regression (review #12277): a file added under a previously EMPTY included
    // subdirectory must invalidate the cache even though no resolved file lived
    // there. The recursive watch tracks `src/empty`, so writing into it bumps a
    // fingerprinted directory.
    const emptyDir = path.join(srcDir, "empty");
    fs.mkdirSync(emptyDir, { recursive: true });
    // Re-resolve so the (now-tracked) empty directory is part of the fingerprint.
    resolveTsconfigFilesCached(tsconfig, { cache, resolve });
    const resolveCallsBeforeEmptyAdd = resolveCalls;
    const beforeEmptyMtime = statFileEntry(emptyDir).mtimeNs;
    let fileD;
    let emptyCounter = 0;
    do {
      fileD = writeFile(emptyDir, `d${emptyCounter++}.ts`, "delta\n");
    } while (statFileEntry(emptyDir).mtimeNs === beforeEmptyMtime);
    currentFiles = [fileA, fileB, fileC, fileD];
    resolveTsconfigFilesCached(tsconfig, { cache, resolve });
    assert.equal(
      resolveCalls,
      resolveCallsBeforeEmptyAdd + 1,
      "a file added under a previously empty included directory re-resolves the list",
    );

    // Editing the tsconfig (its size changes here) re-resolves even when the
    // source tree is otherwise unchanged, since include/exclude may differ.
    fs.writeFileSync(tsconfig, JSON.stringify({ include: ["src", "lib"] }));
    const resolveCallsBeforeTsconfigEdit = resolveCalls;
    resolveTsconfigFilesCached(tsconfig, { cache, resolve });
    assert.equal(
      resolveCalls,
      resolveCallsBeforeTsconfigEdit + 1,
      "editing the tsconfig re-resolves the file list",
    );
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// --------------------------------------------------------------------------
// computeProjectFileStats: the persisted file-list cache survives a fresh
// cache load (the cross-process row-mode pattern) and skips file discovery.

{
  const dir = makeTempDir("tsz-stats-filelist-stats-");
  const cacheHome = makeTempDir("tsz-stats-filelist-cache-");
  const prevCacheDir = process.env.TSZ_PROJECT_FILE_STATS_CACHE_DIR;
  try {
    const srcDir = path.join(dir, "src");
    const fileA = writeFile(srcDir, "a.ts", "alpha\nbeta\n");
    const fileB = writeFile(srcDir, "b.ts", "gamma\n");
    const tsconfig = writeFile(dir, "tsconfig.json", "{}");
    // The cache directory lives outside the project tree (as in the real
    // harness, where it sits under TMPDIR) so writing it does not perturb the
    // tracked project directory mtimes.
    process.env.TSZ_PROJECT_FILE_STATS_CACHE_DIR = cacheHome;

    let resolveCalls = 0;
    const resolve = () => {
      resolveCalls += 1;
      return { files: [fileA, fileB], watchDirectories: [{ dir: srcDir, recursive: true }] };
    };

    const first = computeProjectFileStats(tsconfig, { resolve });
    assert.equal(resolveCalls, 1, "first invocation discovers the file list");
    assert.equal(first.fileCount, 2);
    assert.equal(first.lines, 3);

    const cacheFile = path.join(cacheHome, cacheKeyForTsconfig(path.resolve(tsconfig)) + ".json");
    const persisted = JSON.parse(fs.readFileSync(cacheFile, "utf8"));
    assert.ok(persisted.fileList, "the fileList section is persisted to disk");
    assert.equal(persisted.fileList.files.length, 2);

    // Second invocation reloads the cache from disk (simulating a separate
    // process) and must not re-discover the file list.
    resolveCalls = 0;
    const second = computeProjectFileStats(tsconfig, { resolve });
    assert.equal(resolveCalls, 0, "an unchanged tree skips file discovery on cache reload");
    assert.deepEqual(second, first, "stats are identical across the cached invocations");
  } finally {
    if (prevCacheDir === undefined) {
      delete process.env.TSZ_PROJECT_FILE_STATS_CACHE_DIR;
    } else {
      process.env.TSZ_PROJECT_FILE_STATS_CACHE_DIR = prevCacheDir;
    }
    fs.rmSync(dir, { recursive: true, force: true });
    fs.rmSync(cacheHome, { recursive: true, force: true });
  }
}

// --------------------------------------------------------------------------
// loadStatsCache / saveStatsCache round-trip.

{
  const dir = makeTempDir("tsz-stats-persist-");
  try {
    const cachePath = path.join(dir, "nested", "cache.json");
    assert.equal(loadStatsCache(cachePath), null, "missing cache file loads as null");
    saveStatsCache(cachePath, {
      entries: { "/abs/foo.ts": { size: 12, mtimeNs: "100", lines: 3 } },
    });
    const reloaded = loadStatsCache(cachePath);
    assert.ok(reloaded);
    assert.equal(reloaded.entries["/abs/foo.ts"].lines, 3);

    // Corrupt cache files load as null (not throw).
    fs.writeFileSync(cachePath, "not json {");
    assert.equal(loadStatsCache(cachePath), null);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

assert.match(
  cacheKeyForTsconfig("/abs/path/to/tsconfig.json"),
  /^[0-9a-f]{24}$/,
  "cache key is a stable hex digest prefix",
);
assert.notEqual(
  cacheKeyForTsconfig("/a/tsconfig.json"),
  cacheKeyForTsconfig("/b/tsconfig.json"),
  "different tsconfig paths yield different cache keys",
);

// --------------------------------------------------------------------------
// Cross-process script invocation with the on-disk cache.

{
  const dir = makeTempDir("tsz-stats-script-");
  try {
    const srcDir = path.join(dir, "src");
    writeFile(srcDir, "a.ts", "alpha\nbeta\n");
    writeFile(srcDir, "b.ts", "gamma\n");
    // A non-TS file plus a node_modules path must be excluded by the
    // filter pipeline so they cannot contaminate the stats.
    writeFile(srcDir, "ignored.js", "noop\n");
    writeFile(dir, path.join("node_modules", "pkg", "leak.ts"), "leak\n");

    const tsconfig = path.join(dir, "tsconfig.json");
    fs.writeFileSync(
      tsconfig,
      JSON.stringify(
        {
          compilerOptions: {
            target: "es2017",
            noEmit: true,
            incremental: true,
            skipLibCheck: true,
            types: [],
          },
          include: ["src/**/*.ts"],
        },
        null,
        2,
      ),
    );

    const cacheDir = path.join(dir, "cache");
    const env = { ...process.env, TSZ_PROJECT_FILE_STATS_CACHE_DIR: cacheDir };
    const scriptPath = path.join(SCRIPT_DIR, "project-file-stats.mjs");

    const first = spawnSync(process.execPath, [scriptPath, tsconfig], {
      encoding: "utf8",
      env,
    });
    if (first.status !== 0) {
      // The TS7 unstable sync API is required by the script path; tests in this
      // workspace install it via the bench tooling, but if it is genuinely
      // absent we degrade to a warning instead of failing the whole suite.
      if (/Unable to load the TypeScript 7 unstable sync API/i.test(first.stderr || "")) {
        console.error("[skip] project-file-stats.mjs: TypeScript 7 sync API unavailable");
      } else {
        throw new Error(`project-file-stats.mjs failed: ${first.stderr}`);
      }
    } else {
      const parts = first.stdout.trim().split(/\s+/);
      assert.equal(parts.length, 3, "script must print three space-separated integers");
      const [lines, bytes, files] = parts.map((value) => Number.parseInt(value, 10));
      assert.equal(files, 2, "only the two .ts files under src/ are counted");
      assert.equal(lines, 3, "lines aggregate across both .ts files");
      const expectedBytes =
        fs.statSync(path.join(srcDir, "a.ts")).size +
        fs.statSync(path.join(srcDir, "b.ts")).size;
      assert.equal(bytes, expectedBytes);

      // Cache file must exist after the first invocation.
      const cacheKey = cacheKeyForTsconfig(path.resolve(tsconfig));
      const cacheFile = path.join(cacheDir, cacheKey + ".json");
      assert.ok(fs.existsSync(cacheFile), "cache file must be written");
      const cache = JSON.parse(fs.readFileSync(cacheFile, "utf8"));
      assert.equal(Object.keys(cache.entries).length, 2);
      assert.ok(
        cache.fileList.watch.some(
          ({ dir: watchedDir, recursive }) =>
            watchedDir === srcDir && recursive === false,
        ),
        "TS7 config parsing records the enumerated source directory as an exact watch point",
      );
      assert.equal(
        fs.existsSync(path.join(dir, "tsconfig.tsbuildinfo")),
        false,
        "config discovery must not write build info for an incremental project",
      );

      // Second invocation must produce identical output and reuse the cache.
      const second = spawnSync(process.execPath, [scriptPath, tsconfig], {
        encoding: "utf8",
        env,
      });
      assert.equal(second.status, 0);
      assert.equal(second.stdout, first.stdout, "stats must be stable across runs");

      // Adding a source changes the enumerated directory's mtime, so the next
      // process must ask TS7 to refresh the project file list.
      const beforeSrcMtime = statFileEntry(srcDir).mtimeNs;
      let addedFile;
      let addedCounter = 0;
      do {
        addedFile = writeFile(srcDir, `added-${addedCounter++}.ts`, "delta\n");
      } while (statFileEntry(srcDir).mtimeNs === beforeSrcMtime);
      const third = spawnSync(process.execPath, [scriptPath, tsconfig], {
        encoding: "utf8",
        env,
      });
      assert.equal(third.status, 0, third.stderr);
      const [thirdLines, , thirdFiles] = third.stdout
        .trim()
        .split(/\s+/)
        .map((value) => Number.parseInt(value, 10));
      assert.equal(thirdFiles, 3, "a newly enumerated TS file invalidates the cached file list");
      assert.equal(thirdLines, 4);
      assert.ok(fs.existsSync(addedFile));
    }
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// --------------------------------------------------------------------------
// Shell fallback `sum_ts_stats` performs a single-pass walk.

{
  const dir = makeTempDir("tsz-stats-shell-");
  try {
    const srcDir = path.join(dir, "src");
    writeFile(srcDir, "a.ts", "alpha\nbeta\n");
    writeFile(srcDir, "nested/b.tsx", "gamma\n");
    writeFile(srcDir, "skip.js", "noop\n");
    // A node_modules path must be excluded by the `prune` filter.
    writeFile(dir, path.join("node_modules", "pkg", "leak.ts"), "leak\n");

    const script = `set -euo pipefail
SCRIPT_DIR='${SCRIPT_DIR.replace(/'/g, "'\\''")}'
if [ -f '${BENCH_SCRIPT.replace(/'/g, "'\\''")}.helpers' ]; then
  source '${BENCH_SCRIPT.replace(/'/g, "'\\''")}.helpers'
else
  ${awkExtractFunction("sum_ts_stats")} > /tmp/tsz-sum-ts-stats-$$.sh
  source /tmp/tsz-sum-ts-stats-$$.sh
  rm -f /tmp/tsz-sum-ts-stats-$$.sh
fi
sum_ts_stats '${srcDir.replace(/'/g, "'\\''")}'
`;
    const result = spawnSync("bash", ["-c", script], { encoding: "utf8" });
    if (result.status !== 0) {
      throw new Error(`shell fallback failed: ${result.stderr}`);
    }
    const [lines, bytes, files] = result.stdout
      .trim()
      .split(/\s+/)
      .map((value) => Number.parseInt(value, 10));
    assert.equal(files, 2, ".ts and .tsx under src/ are counted; .js and node_modules excluded");
    assert.equal(lines, 3);
    const expectedBytes =
      fs.statSync(path.join(srcDir, "a.ts")).size +
      fs.statSync(path.join(srcDir, "nested", "b.tsx")).size;
    assert.equal(bytes, expectedBytes);

    // Empty source dir returns "0 0 0" rather than failing.
    const emptyDir = path.join(dir, "empty");
    fs.mkdirSync(emptyDir, { recursive: true });
    const empty = spawnSync("bash", ["-c", script.replace(srcDir, emptyDir)], {
      encoding: "utf8",
    });
    assert.equal(empty.status, 0);
    assert.equal(empty.stdout.trim(), "0 0 0");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// --------------------------------------------------------------------------
// `bench_project_file_stats_cache_dir` (bench-vs-tsgo-prereqs.sh) must resolve
// to a PERSISTENT directory. Regression for issue #10923: the cache dir
// previously defaulted to the per-run `$TEMP_DIR`, which the harness deletes on
// EXIT, so the persistence machinery above never survived a run and every row
// invocation re-read (re-line-counted) every fixture source from cold.

{
  const extract = awkExtractFunction("bench_project_file_stats_cache_dir");
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-cache-dir-resolver-"));
  const functionFile = path.join(dir, "bench-cache-dir.sh");
  const extractResult = spawnSync("bash", ["-c", `${extract} > '${functionFile.replace(/'/g, "'\\''")}'`], {
    encoding: "utf8",
  });
  assert.equal(extractResult.status, 0, extractResult.stderr);

  const runResolver = (envLine) => {
    const script = `set -euo pipefail
source '${functionFile.replace(/'/g, "'\\''")}'
${envLine}
bench_project_file_stats_cache_dir
`;
    const result = spawnSync("bash", ["-c", script], { encoding: "utf8" });
    if (result.status !== 0) {
      throw new Error(`cache-dir resolver failed: ${result.stderr}`);
    }
    return result.stdout.trim();
  };

  try {
    // With a persistent BENCH_TARGET_DIR available, the default must live under
    // it — NOT under the ephemeral per-run TEMP_DIR, even when TEMP_DIR is set.
    const persistent = runResolver(
      'BENCH_TARGET_DIR=/persist/.target-bench; TEMP_DIR=/tmp/run-XXXX; TMPDIR=/tmp; unset TSZ_PROJECT_FILE_STATS_CACHE_DIR',
    );
    assert.equal(
      persistent,
      "/persist/.target-bench/project-file-stats-cache",
      "default cache dir is anchored to the run-surviving BENCH_TARGET_DIR",
    );
    assert.ok(
      !persistent.includes("/run-XXXX"),
      "default cache dir must not live under the per-run TEMP_DIR that is deleted on exit",
    );

    // An explicit override always wins over the computed default.
    const overridden = runResolver(
      'BENCH_TARGET_DIR=/persist/.target-bench; TSZ_PROJECT_FILE_STATS_CACHE_DIR=/custom/cache',
    );
    assert.equal(
      overridden,
      "/custom/cache",
      "an explicit TSZ_PROJECT_FILE_STATS_CACHE_DIR overrides the default",
    );

    // With no persistent target known, fall back to TMPDIR rather than crashing.
    const tmpFallback = runResolver(
      'unset BENCH_TARGET_DIR; TMPDIR=/tmp/fallback; unset TSZ_PROJECT_FILE_STATS_CACHE_DIR',
    );
    assert.equal(
      tmpFallback,
      "/tmp/fallback/project-file-stats-cache",
      "without BENCH_TARGET_DIR the resolver falls back to TMPDIR",
    );
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// Fixture stats are presentation metadata, never proof of compiler admission.
// The integration suite pins the dynamic zero-display/nonzero-TSZ case; this
// source contract prevents a future edit from routing `$file_count` back into
// project compatibility records or restoring the old early-zero shortcut.
{
  const runner = fs.readFileSync(BENCH_RESULTS_SCRIPT, "utf8");
  const projectRunner = runner.slice(
    runner.indexOf("run_project_benchmark()"),
    runner.indexOf("\nJSON_EXPORTED=false"),
  );
  assert.doesNotMatch(
    projectRunner,
    /record_project_compatibility[^\n]*(?:\\\n[^\n]*){0,5}\$file_count/,
    "project compatibility files_reached must not come from fixture-side file_count",
  );
  assert.doesNotMatch(
    projectRunner,
    /file_count[^\n]*(?:-eq|==)[^\n]*0[^\n]*return/,
    "a fixture-side zero count must not short-circuit schema-v2 compiler evidence",
  );
  assert.match(
    projectRunner,
    /PROJECT_EVIDENCE_TSZ_SOURCE_FILES/,
    "project rows must record TSZ's admitted source count",
  );
}

console.log("project-file-stats tests passed");
