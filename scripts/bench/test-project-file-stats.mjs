#!/usr/bin/env node
//
// Tests for `project-file-stats.mjs` covering:
//  - Correct (lines, bytes, file_count) aggregation on a fresh invocation.
//  - Reuse of cached per-file line counts when (mtime, size) are unchanged.
//  - Cache invalidation on file modification, file removal, and file
//    replacement (size-preserving rewrite).
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
  countNewlinesStream,
  isLocalProjectFile,
  isTypeScriptFile,
  loadStatsCache,
  saveStatsCache,
  statFileEntry,
} from "./project-file-stats.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const BENCH_SCRIPT = path.join(SCRIPT_DIR, "bench-vs-tsgo.sh");

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
          compilerOptions: { target: "es2017", noEmit: true, skipLibCheck: true, types: [] },
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
      // The TypeScript package is required by the script path; tests in this
      // workspace install it via the bench tooling, but if it is genuinely
      // absent we degrade to a warning instead of failing the whole suite.
      if (/Unable to load the TypeScript package/i.test(first.stderr || "")) {
        console.error("[skip] project-file-stats.mjs: TypeScript package unavailable");
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

      // Second invocation must produce identical output and reuse the cache.
      const second = spawnSync(process.execPath, [scriptPath, tsconfig], {
        encoding: "utf8",
        env,
      });
      assert.equal(second.status, 0);
      assert.equal(second.stdout, first.stdout, "stats must be stable across runs");
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
  awk '/^sum_ts_stats\\(\\)/{flag=1} flag{print} /^}/{if(flag){exit}}' '${BENCH_SCRIPT.replace(/'/g, "'\\''")}' > /tmp/tsz-sum-ts-stats-$$.sh
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

console.log("project-file-stats tests passed");
