#!/usr/bin/env node
// Machine-contract parser for project-compile evidence.
//
// TSZ's `--perf-counters-json` payload is authoritative. Directory walks and
// extended-diagnostics text describe a fixture, not what the compiler actually
// admitted to its program, so they must never substitute for these fields.

import fs from "node:fs";
import crypto from "node:crypto";
import path from "node:path";
import { pathToFileURL } from "node:url";

function nonnegativeInteger(value) {
  return Number.isInteger(value) && value >= 0;
}

const SEMANTIC_COMPLETIONS = new Set(["complete", "deferred", "cycle", "limit"]);

export function compilerStatsFrom(value) {
  if (value?.schema_version !== 2) {
    throw new Error("payload schema_version must be 2");
  }
  const stats = value?.stats;
  if (!stats || typeof stats !== "object" || Array.isArray(stats)) {
    throw new Error("payload has no stats object");
  }
  const semanticCompletion = stats.semantic_completion;
  if (!SEMANTIC_COMPLETIONS.has(semanticCompletion)) {
    throw new Error(
      'stats.semantic_completion must be exactly one of "complete", "deferred", "cycle", or "limit"',
    );
  }
  const rootFiles = stats.root_files;
  const sourceFiles = stats.source_files;
  if (!nonnegativeInteger(rootFiles)) {
    throw new Error("stats.root_files must be a nonnegative integer");
  }
  if (!nonnegativeInteger(sourceFiles)) {
    throw new Error("stats.source_files must be a nonnegative integer");
  }
  // `files` is retained only as a compatibility alias. Its presence can
  // strengthen the contract, but it can never replace either canonical field.
  if (Object.prototype.hasOwnProperty.call(stats, "files")) {
    if (!nonnegativeInteger(stats.files) || stats.files !== sourceFiles) {
      throw new Error("stats.files must equal stats.source_files when present");
    }
  }
  const rootFilePaths = stats.root_file_paths;
  const sourceFilePaths = stats.source_file_paths;
  for (const [name, paths, expected] of [
    ["root_file_paths", rootFilePaths, rootFiles],
    ["source_file_paths", sourceFilePaths, sourceFiles],
  ]) {
    if (!Array.isArray(paths) || !paths.every(
      (file) => typeof file === "string" && file.length > 0 && !file.includes("\0"),
    )) {
      throw new Error(`stats.${name} must be an array of non-empty strings`);
    }
    if (paths.length !== expected) {
      throw new Error(`stats.${name} length must equal its canonical count`);
    }
  }
  return {
    semanticCompletion,
    rootFiles,
    sourceFiles,
    rootFilePaths,
    sourceFilePaths,
  };
}

export function rootFilePathsFromShowConfig(value) {
  if (!value || !Array.isArray(value.files)) {
    throw new Error("showConfig payload has no files array");
  }
  if (!value.files.every(
    (file) => typeof file === "string" && file.length > 0 && !file.includes("\0"),
  )) {
    throw new Error("showConfig files must all be non-empty strings");
  }
  return value.files;
}

export function sourceFilePathsFromListFilesOnly(text, builtinLibDir = "") {
  const canonicalLibDir = builtinLibDir ? path.resolve(builtinLibDir) : "";
  const files = String(text)
    .split(/\r?\n/)
    .filter(Boolean)
    .filter((file) => {
      if (!canonicalLibDir) return true;
      const resolved = path.resolve(file);
      return !(
        path.dirname(resolved) === canonicalLibDir &&
        /^lib(?:\..+)?\.d\.ts$/i.test(path.basename(resolved))
      );
    });
  if (files.some((file) => file.includes("\0"))) {
    throw new Error("listFilesOnly paths may not contain NUL");
  }
  return files;
}

function asciiFoldPath(file) {
  return file.replace(/[A-Z]/g, (character) => character.toLowerCase());
}

export function normalizeProjectPath(file, baseDir, projectRoot) {
  const normalizedInput = file.replaceAll("\\", "/");
  const normalizedBase = path.resolve(baseDir || ".");
  const normalizedRoot = path.resolve(projectRoot || normalizedBase);
  const absolute = path.isAbsolute(normalizedInput)
    ? path.normalize(normalizedInput)
    : path.resolve(normalizedBase, normalizedInput);
  const caseSensitive = process.platform !== "win32" && process.platform !== "darwin";
  const comparisonRoot = caseSensitive ? normalizedRoot : asciiFoldPath(normalizedRoot);
  const comparisonAbsolute = caseSensitive ? absolute : asciiFoldPath(absolute);
  return path.relative(comparisonRoot, comparisonAbsolute).split(path.sep).join("/") || ".";
}

export function normalizedPathMultiset(files, baseDir, projectRoot) {
  return files.map((file) => normalizeProjectPath(file, baseDir, projectRoot)).sort();
}

export function pathMultisetFingerprint(files, baseDir, projectRoot) {
  const normalized = normalizedPathMultiset(files, baseDir, projectRoot);
  return crypto.createHash("sha256").update(JSON.stringify(normalized)).digest("hex");
}

export function pathGraphFingerprint(files, baseDir, projectRoot) {
  const normalized = files.map((file) => normalizeProjectPath(file, baseDir, projectRoot));
  // Order and duplicate multiplicity are compiler evidence. Hashing the exact
  // normalized sequence is stricter than set equality and therefore also
  // proves the normalized multiset.
  return crypto.createHash("sha256").update(JSON.stringify(normalized)).digest("hex");
}

export function rootFileCountFromShowConfig(value) {
  return rootFilePathsFromShowConfig(value).length;
}

export function sourceFileCountFromListFilesOnly(text, builtinLibDir = "") {
  return sourceFilePathsFromListFilesOnly(text, builtinLibDir).length;
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function main() {
  const command = process.argv[2] || "";
  const file = process.argv[3] || "";
  if (
    !file ||
    !["compiler-stats", "show-config-roots", "list-files-graph", "list-files-count"].includes(command)
  ) {
    console.error(
      "usage: project-compile-stats.mjs <compiler-stats|show-config-roots|list-files-graph|list-files-count> <file> [base] [project-root]",
    );
    process.exit(2);
  }

  if (command === "list-files-count" || command === "list-files-graph") {
    try {
      const paths = sourceFilePathsFromListFilesOnly(
        fs.readFileSync(file, "utf8"),
        process.argv[4] || "",
      );
      if (command === "list-files-count") {
        process.stdout.write(`${paths.length}\n`);
      } else {
        const projectRoot = process.argv[5] || ".";
        process.stdout.write(
          `${paths.length}\t${pathGraphFingerprint(paths, projectRoot, projectRoot)}\n`,
        );
      }
    } catch (error) {
      const kind = error?.code === "ENOENT" ? "missing" : "malformed";
      console.error(`${kind}: ${error instanceof Error ? error.message : String(error)}`);
      process.exit(kind === "missing" ? 3 : 4);
    }
    return;
  }

  let value;
  try {
    value = readJson(file);
  } catch (error) {
    const kind = error?.code === "ENOENT" ? "missing" : "malformed";
    console.error(`${kind}: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(kind === "missing" ? 3 : 4);
  }

  try {
    if (command === "compiler-stats") {
      const stats = compilerStatsFrom(value);
      const baseDir = process.argv[4] || ".";
      const projectRoot = process.argv[5] || baseDir;
      process.stdout.write(
        `${stats.rootFiles}\t${stats.sourceFiles}\t${pathGraphFingerprint(stats.rootFilePaths, baseDir, projectRoot)}\t${pathGraphFingerprint(stats.sourceFilePaths, baseDir, projectRoot)}\t${stats.semanticCompletion}\n`,
      );
    } else {
      const paths = rootFilePathsFromShowConfig(value);
      const baseDir = process.argv[4] || ".";
      const projectRoot = process.argv[5] || baseDir;
      process.stdout.write(
        `${paths.length}\t${pathGraphFingerprint(paths, baseDir, projectRoot)}\n`,
      );
    }
  } catch (error) {
    console.error(`malformed: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(4);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
