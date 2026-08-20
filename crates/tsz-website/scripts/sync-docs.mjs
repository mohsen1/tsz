#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const WEBSITE = path.resolve(import.meta.dirname, "..");
const ROOT = path.resolve(WEBSITE, "..", "..");
const DOCS = path.join(ROOT, "docs");
const SRC = path.join(WEBSITE, "src");
const TARGET_DOCS = path.join(SRC, "docs");
const DOCS_ALLOWLIST = [
  "site",
  "architecture",
  "specs",
  "DEVELOPMENT.md",
  "HOW_TO_CODE.md",
];
const WATCH_DEBOUNCE_MS = 100;

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function copyFileIfChanged(srcPath, destPath) {
  if (
    fs.existsSync(destPath)
    && fs.readFileSync(srcPath).equals(fs.readFileSync(destPath))
  ) {
    return;
  }

  ensureDir(path.dirname(destPath));
  fs.copyFileSync(srcPath, destPath);
}

function pruneTree(rootDir, expectedFiles) {
  if (!fs.existsSync(rootDir)) return;

  for (const entry of fs.readdirSync(rootDir, { withFileTypes: true })) {
    const entryPath = path.join(rootDir, entry.name);
    if (entry.isDirectory()) {
      pruneTree(entryPath, expectedFiles);
      if (fs.readdirSync(entryPath).length === 0) {
        fs.rmdirSync(entryPath);
      }
      continue;
    }

    if (!expectedFiles.has(entryPath)) {
      fs.rmSync(entryPath, { force: true });
    }
  }
}

function syncMarkdownTree(srcDir, destDir, expectedFiles) {
  for (const entry of fs.readdirSync(srcDir, { withFileTypes: true })) {
    const srcPath = path.join(srcDir, entry.name);
    const destPath = path.join(destDir, entry.name);

    if (entry.isDirectory()) {
      ensureDir(destPath);
      syncMarkdownTree(srcPath, destPath, expectedFiles);
      continue;
    }

    if (!entry.name.endsWith(".md")) {
      continue;
    }

    expectedFiles.add(destPath);
    copyFileIfChanged(srcPath, destPath);
  }
}

function copyAllowedDocs() {
  const expectedFiles = new Set();
  for (const relPath of DOCS_ALLOWLIST) {
    const sourcePath = path.join(DOCS, relPath);
    const targetPath = path.join(TARGET_DOCS, relPath);
    if (!fs.existsSync(sourcePath)) continue;

    const stat = fs.statSync(sourcePath);
    if (stat.isDirectory()) {
      ensureDir(targetPath);
      syncMarkdownTree(sourcePath, targetPath, expectedFiles);
    } else if (relPath.endsWith(".md")) {
      expectedFiles.add(targetPath);
      copyFileIfChanged(sourcePath, targetPath);
    }
  }

  pruneTree(TARGET_DOCS, expectedFiles);
}

function main() {
  ensureDir(TARGET_DOCS);
  copyAllowedDocs();
  console.log(`Synced docs markdown into ${path.relative(ROOT, TARGET_DOCS)}`);
}

function sourceWatchPaths() {
  const paths = DOCS_ALLOWLIST
    .map((relPath) => path.join(DOCS, relPath))
    .filter((sourcePath) => fs.existsSync(sourcePath));

  return paths;
}

function watchDirectoryTree(rootDir, onChange, watchers) {
  watchers.push(fs.watch(rootDir, onChange));

  for (const entry of fs.readdirSync(rootDir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    watchDirectoryTree(path.join(rootDir, entry.name), onChange, watchers);
  }
}

function watchPath(sourcePath, onChange, watchers) {
  const stat = fs.statSync(sourcePath);
  if (stat.isDirectory()) {
    try {
      watchers.push(fs.watch(sourcePath, { recursive: true }, onChange));
      return;
    } catch {
      watchDirectoryTree(sourcePath, onChange, watchers);
      return;
    }
  }

  const basename = path.basename(sourcePath);
  watchers.push(
    fs.watch(path.dirname(sourcePath), (_eventType, filename) => {
      if (!filename || filename.toString() === basename) {
        onChange();
      }
    }),
  );
}

function watchSources() {
  const watchers = [];
  let timer = null;
  let running = false;
  let rerun = false;

  const runSynced = () => {
    if (running) {
      rerun = true;
      return;
    }

    running = true;
    try {
      main();
    } catch (error) {
      console.error(error);
    } finally {
      running = false;
      if (rerun) {
        rerun = false;
        runSynced();
      }
    }
  };

  const schedule = () => {
    clearTimeout(timer);
    timer = setTimeout(runSynced, WATCH_DEBOUNCE_MS);
  };

  for (const sourcePath of sourceWatchPaths()) {
    watchPath(sourcePath, schedule, watchers);
  }

  console.log("Watching docs and lib assets for website sync...");

  const close = () => {
    clearTimeout(timer);
    for (const watcher of watchers) {
      watcher.close();
    }
    process.exit(0);
  };

  process.on("SIGINT", close);
  process.on("SIGTERM", close);
}

const watchMode = process.argv.includes("--watch") || process.argv.includes("--watch-only");

if (!process.argv.includes("--watch-only")) {
  main();
}

if (watchMode) {
  watchSources();
}
