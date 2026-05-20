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

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function copyMarkdownTree(srcDir, destDir) {
  for (const entry of fs.readdirSync(srcDir, { withFileTypes: true })) {
    const srcPath = path.join(srcDir, entry.name);
    const destPath = path.join(destDir, entry.name);

    if (entry.isDirectory()) {
      ensureDir(destPath);
      copyMarkdownTree(srcPath, destPath);
      continue;
    }

    if (!entry.name.endsWith(".md")) {
      continue;
    }

    fs.copyFileSync(srcPath, destPath);
  }
}

function copyAllowedDocs() {
  for (const relPath of DOCS_ALLOWLIST) {
    const sourcePath = path.join(DOCS, relPath);
    const targetPath = path.join(TARGET_DOCS, relPath);
    if (!fs.existsSync(sourcePath)) continue;

    const stat = fs.statSync(sourcePath);
    if (stat.isDirectory()) {
      ensureDir(targetPath);
      copyMarkdownTree(sourcePath, targetPath);
    } else if (relPath.endsWith(".md")) {
      ensureDir(path.dirname(targetPath));
      fs.copyFileSync(sourcePath, targetPath);
    }
  }
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
  fs.writeFileSync(TARGET_ARCH_DATA, `export default ${JSON.stringify(archData, null, 2)};\n`);

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

  fs.writeFileSync(TARGET_ARCH_TEMPLATE, archTemplate);
}

function syncPlaygroundLibFiles() {
  fs.rmSync(TARGET_LIB, { recursive: true, force: true });
  ensureDir(TARGET_LIB);

  if (!fs.existsSync(LIB_ASSETS)) return;

  for (const entry of fs.readdirSync(LIB_ASSETS, { withFileTypes: true })) {
    if (!entry.isFile()) continue;
    if (!entry.name.endsWith(".d.ts")) continue;
    const sourcePath = path.join(LIB_ASSETS, entry.name);
    const destPath = path.join(TARGET_LIB, `lib.${entry.name}`);
    fs.copyFileSync(sourcePath, destPath);
  }
}

function main() {
  fs.rmSync(TARGET_DOCS, { recursive: true, force: true });
  fs.rmSync(TARGET_ARCH_LEGACY_DIR, { recursive: true, force: true });
  ensureDir(TARGET_DOCS);
  copyAllowedDocs();
  buildArchitecturePage();
  syncPlaygroundLibFiles();
  console.log(`Synced docs markdown into ${path.relative(ROOT, TARGET_DOCS)}`);
}

main();
