#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const WEBSITE = path.resolve(import.meta.dirname, "..");
const ROOT = path.resolve(WEBSITE, "..", "..");
const DOCS = path.join(ROOT, "docs");
const SRC = path.join(WEBSITE, "src");
const TARGET_DOCS = path.join(SRC, "docs");
const TARGET_ARCH_TEMPLATE = path.join(SRC, "architecture.njk");
const TARGET_ARCH_DATA = path.join(SRC, "_data", "architecture_page.js");
const TARGET_ARCH_LEGACY_DIR = path.join(SRC, "architecture");
const LIB_ASSETS = path.join(ROOT, "crates", "tsz-core", "src", "lib-assets");
const TARGET_LIB = path.join(SRC, "lib");
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

function writeFileIfChanged(destPath, content) {
  if (fs.existsSync(destPath) && fs.readFileSync(destPath, "utf8") === content) {
    return;
  }

  ensureDir(path.dirname(destPath));
  fs.writeFileSync(destPath, content);
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

function buildArchitecturePage() {
  const source = path.join(DOCS, "architecture.html");
  if (!fs.existsSync(source)) return;

  const archHtml = fs.readFileSync(source, "utf8");
  const styleMatch = archHtml.match(/<style[\s\S]*?<\/style>/i);
  const bodyMatch = archHtml.match(/<body[^>]*>([\s\S]*?)<\/body>/i);

  const head = styleMatch?.[0] ?? "";
  let body = bodyMatch?.[1] ?? "";
  const scripts = [...body.matchAll(/<script[\s\S]*?<\/script>/gi)].map((m) => m[0]).join("\n");

  body = body.replace(/<script[\s\S]*?<\/script>/gi, "");
  body = body.replace(/<footer[\s\S]*?<\/footer>/i, "");
  body = body.replace(/<main class="content">/, '<div class="content">');
  body = body.replace(/<\/main>\s*<\/div>\s*$/, "</div>\n</div>");

  const archData = {
    head,
    body: body.trim(),
    scripts,
  };

  ensureDir(path.dirname(TARGET_ARCH_DATA));
  writeFileIfChanged(TARGET_ARCH_DATA, `export default ${JSON.stringify(archData, null, 2)};\n`);

  const archTemplate = `---
title: Deep Dive
layout: layouts/base.njk
page_class: architecture
permalink: /architecture/index.html
eleventyComputed:
  extra_head: "{{ architecture_page.head | safe }}"
  extra_scripts: "{{ architecture_page.scripts | safe }}"
---
{{ architecture_page.body | safe }}
`;

  writeFileIfChanged(TARGET_ARCH_TEMPLATE, archTemplate);
}

function syncPlaygroundLibFiles() {
  const expectedFiles = new Set();
  ensureDir(TARGET_LIB);

  if (!fs.existsSync(LIB_ASSETS)) {
    pruneTree(TARGET_LIB, expectedFiles);
    return;
  }

  for (const entry of fs.readdirSync(LIB_ASSETS, { withFileTypes: true })) {
    if (!entry.isFile()) continue;
    if (!entry.name.endsWith(".d.ts")) continue;
    const sourcePath = path.join(LIB_ASSETS, entry.name);
    const destPath = path.join(TARGET_LIB, `lib.${entry.name}`);
    expectedFiles.add(destPath);
    copyFileIfChanged(sourcePath, destPath);
  }

  pruneTree(TARGET_LIB, expectedFiles);
}

function main() {
  fs.rmSync(TARGET_ARCH_LEGACY_DIR, { recursive: true, force: true });
  ensureDir(TARGET_DOCS);
  copyAllowedDocs();
  buildArchitecturePage();
  syncPlaygroundLibFiles();
  console.log(`Synced docs markdown into ${path.relative(ROOT, TARGET_DOCS)}`);
}

function sourceWatchPaths() {
  const paths = DOCS_ALLOWLIST
    .map((relPath) => path.join(DOCS, relPath))
    .filter((sourcePath) => fs.existsSync(sourcePath));

  const architectureHtml = path.join(DOCS, "architecture.html");
  if (fs.existsSync(architectureHtml)) {
    paths.push(architectureHtml);
  }

  if (fs.existsSync(LIB_ASSETS)) {
    paths.push(LIB_ASSETS);
  }

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
