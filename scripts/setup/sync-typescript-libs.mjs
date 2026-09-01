#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { resolveTypeScriptLibDir } from "./resolve-typescript-lib-dir.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "../..");
const CORE_FULL_DIR = path.join(ROOT, "crates/tsz-core/data/lib");
const CORE_STRIPPED_DIR = path.join(ROOT, "crates/tsz-core/data/lib-stripped");
const WEBSITE_DIR = path.join(ROOT, "crates/tsz-website/src/lib");
const VERSIONS_FILE = path.join(ROOT, "scripts/conformance/typescript-versions.json");

function parseArgs(argv) {
  let mode = "check";
  let packageJson = path.join(ROOT, "scripts/node_modules/typescript/package.json");
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--check") {
      mode = "check";
    } else if (arg === "--write") {
      mode = "write";
    } else if (arg === "--package-json") {
      packageJson = path.resolve(argv[index + 1] ?? "");
      index += 1;
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }
  return { mode, packageJson };
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function sourceToCoreName(sourceName) {
  if (sourceName === "lib.d.ts") return "es5.full.d.ts";
  if (!sourceName.startsWith("lib.")) {
    throw new Error(`Unexpected TypeScript lib filename: ${sourceName}`);
  }
  return sourceName.slice("lib.".length);
}

function sourceToWebsiteName(sourceName) {
  return sourceName === "lib.d.ts" ? "lib.es5.full.d.ts" : sourceName;
}

function stripComments(source) {
  const withoutBlocks = source.replace(/[ \t]*\/\*[\s\S]*?\*\/[ \t]*/g, "");
  const lines = withoutBlocks.replaceAll("\r\n", "\n").split("\n");
  const kept = lines.filter((line) => {
    const trimmed = line.trimStart();
    if (trimmed.length === 0) return false;
    return !trimmed.startsWith("//") || trimmed.startsWith("/// <reference");
  });
  return `${kept.join("\n")}\n`;
}

function parseReferences(source) {
  const references = [];
  const pattern = /\/\/\/\s*<reference\s+lib=(["'])(.*?)\1\s*\/>/g;
  for (const match of source.matchAll(pattern)) {
    references.push(match[2].trim().toLowerCase());
  }
  return references;
}

function desiredEntries(libDir) {
  const sourceNames = fs.readdirSync(libDir)
    .filter((name) => name === "lib.d.ts" || /^lib\..+\.d\.ts$/.test(name))
    .sort();
  return sourceNames.map((sourceName) => {
    const full = fs.readFileSync(path.join(libDir, sourceName));
    const source = full.toString("utf8");
    return {
      sourceName,
      coreName: sourceToCoreName(sourceName),
      websiteName: sourceToWebsiteName(sourceName),
      full,
      stripped: stripComments(source),
      references: parseReferences(source),
    };
  }).sort((left, right) => left.coreName.localeCompare(right.coreName));
}

function syncDtsTree(directory, desired, mode, differences) {
  const desiredNames = new Set(desired.keys());
  const existing = fs.existsSync(directory)
    ? fs.readdirSync(directory).filter((name) => name.endsWith(".d.ts"))
    : [];
  for (const name of existing) {
    if (desiredNames.has(name)) continue;
    const file = path.join(directory, name);
    if (mode === "write") fs.rmSync(file);
    else differences.push(`extra ${path.relative(ROOT, file)}`);
  }
  for (const [name, content] of desired) {
    const file = path.join(directory, name);
    const bytes = Buffer.isBuffer(content) ? content : Buffer.from(content);
    const matches = fs.existsSync(file) && fs.readFileSync(file).equals(bytes);
    if (matches) continue;
    if (mode === "write") {
      fs.mkdirSync(directory, { recursive: true });
      fs.writeFileSync(file, bytes);
    } else {
      differences.push(`stale ${path.relative(ROOT, file)}`);
    }
  }
}

function syncFile(file, content, mode, differences) {
  const bytes = Buffer.isBuffer(content) ? content : Buffer.from(content);
  const matches = fs.existsSync(file) && fs.readFileSync(file).equals(bytes);
  if (matches) return;
  if (mode === "write") {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, bytes);
  } else {
    differences.push(`stale ${path.relative(ROOT, file)}`);
  }
}

function main() {
  const { mode, packageJson } = parseArgs(process.argv.slice(2));
  const versions = readJson(VERSIONS_FILE);
  const mapping = versions.mappings?.[versions.current];
  if (!mapping?.npm) throw new Error(`No npm mapping for corpus ${versions.current}`);

  const wrapper = readJson(packageJson);
  if (wrapper.version !== mapping.npm) {
    throw new Error(`TypeScript package ${wrapper.version} does not match pin ${mapping.npm}`);
  }
  const libDir = resolveTypeScriptLibDir(packageJson);
  const platformPackage = readJson(path.join(libDir, "..", "package.json"));
  if (platformPackage.version !== mapping.npm) {
    throw new Error(`Platform package ${platformPackage.version} does not match pin ${mapping.npm}`);
  }

  const entries = desiredEntries(libDir);
  if (mapping.lib_count && entries.length !== mapping.lib_count) {
    throw new Error(`Expected ${mapping.lib_count} declaration files, found ${entries.length}`);
  }

  const manifest = {
    version: mapping.npm,
    source: `${platformPackage.name}@${platformPackage.version}`,
    generatedAt: `${mapping.date}T00:00:00.000Z`,
    libs: Object.fromEntries(entries.map((entry) => [
      entry.coreName.slice(0, -".d.ts".length),
      {
        fileName: entry.coreName,
        canonicalFileName: entry.sourceName,
        references: entry.references,
        size: entry.full.length,
      },
    ])),
  };
  const libVersion = {
    npm_version: mapping.npm,
    native_repository: mapping.native_repository,
    native_tag: mapping.native_tag,
    native_sha: mapping.native_sha,
    corpus_sha: versions.current,
    source_package: `${platformPackage.name}@${platformPackage.version}`,
    generated_at: `${mapping.date}T00:00:00.000Z`,
    lib_count: entries.length,
  };

  const full = new Map(entries.map((entry) => [entry.coreName, entry.full]));
  const stripped = new Map(entries.map((entry) => [entry.coreName, entry.stripped]));
  const website = new Map(entries.map((entry) => [entry.websiteName, entry.full]));
  const differences = [];
  syncDtsTree(CORE_FULL_DIR, full, mode, differences);
  syncDtsTree(CORE_STRIPPED_DIR, stripped, mode, differences);
  syncDtsTree(WEBSITE_DIR, website, mode, differences);
  syncFile(
    path.join(CORE_FULL_DIR, "lib_manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
    mode,
    differences,
  );
  syncFile(
    path.join(CORE_FULL_DIR, "lib_version.json"),
    `${JSON.stringify(libVersion, null, 2)}\n`,
    mode,
    differences,
  );
  if (differences.length > 0) {
    throw new Error(`TypeScript lib assets are out of date:\n${differences.join("\n")}`);
  }
  console.log(
    `${mode === "write" ? "Wrote" : "Verified"} ${entries.length} TypeScript ${mapping.npm} lib assets from ${libDir}`,
  );
}

try {
  main();
} catch (error) {
  console.error(`sync-typescript-libs: ${error.message}`);
  process.exitCode = 1;
}
