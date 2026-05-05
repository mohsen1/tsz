#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);

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

function isTypeScriptFile(fileName) {
  return /\.(d\.)?[cm]?tsx?$/.test(fileName);
}

function isLocalProjectFile(fileName) {
  const normalized = fileName.split(path.sep).join("/");
  return !normalized.includes("/node_modules/") && !normalized.includes("/.next/");
}

function countLines(text) {
  if (text.length === 0) return 0;
  const newlineCount = text.match(/\n/g)?.length || 0;
  return text.endsWith("\n") ? newlineCount : newlineCount + 1;
}

const tsconfig = process.argv[2] ? path.resolve(process.argv[2]) : "";
if (!tsconfig) {
  console.error("usage: project-file-stats.mjs <tsconfig>");
  process.exit(2);
}

const ts = loadTypeScript();
const config = ts.readConfigFile(tsconfig, ts.sys.readFile);
if (config.error) {
  console.error(ts.flattenDiagnosticMessageText(config.error.messageText, "\n"));
  process.exit(1);
}

const parsed = ts.parseJsonConfigFileContent(
  config.config,
  ts.sys,
  path.dirname(tsconfig),
  {},
  tsconfig,
);

const files = [...new Set(parsed.fileNames)]
  .filter(isTypeScriptFile)
  .filter(isLocalProjectFile)
  .sort();

let lines = 0;
let bytes = 0;
let countedFiles = 0;
for (const file of files) {
  try {
    const text = fs.readFileSync(file, "utf8");
    lines += countLines(text);
    bytes += Buffer.byteLength(text);
    countedFiles += 1;
  } catch {
    // Ignore files that disappeared between config parsing and counting.
  }
}

console.log(`${lines} ${bytes} ${countedFiles}`);
