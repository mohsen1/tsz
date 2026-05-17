#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const [candidateDir, candidateManifestPath, outputPath] = process.argv.slice(2);

if (!candidateDir || !candidateManifestPath || !outputPath) {
  console.error(
    "usage: type-challenges-assertion-classifier.mjs <candidate-dir> <candidate-manifest.json> <output.json>",
  );
  process.exit(2);
}

const candidateRoot = path.resolve(candidateDir);

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function executableOrNull(file) {
  if (!file) {
    return null;
  }
  try {
    fs.accessSync(file, fs.constants.X_OK);
    return file;
  } catch {
    return null;
  }
}

function discoverTscBin() {
  if (Object.hasOwn(process.env, "TYPE_CHALLENGES_ASSERTION_TSC_BIN")) {
    return executableOrNull(process.env.TYPE_CHALLENGES_ASSERTION_TSC_BIN);
  }
  return (
    executableOrNull(path.join("scripts", "node_modules", ".bin", "tsc")) ??
    executableOrNull(path.join("node_modules", ".bin", "tsc"))
  );
}

function diagnosticLines(output) {
  return output
    .split(/\r?\n/)
    .filter((line) => /\berror TS\d+:/.test(line));
}

function normalizePath(file) {
  return file.split(/[\\/]+/).join("/");
}

function parseDiagnostic(line) {
  const match = /^(.*?)(?:\((\d+),(\d+)\))?: error (TS\d+): (.*)$/.exec(line);
  if (!match) {
    return {
      raw: line,
      file: null,
      line: null,
      column: null,
      code: null,
      message: line,
    };
  }

  return {
    raw: line,
    file: match[1] ? normalizePath(match[1]) : null,
    line: match[2] ? Number(match[2]) : null,
    column: match[3] ? Number(match[3]) : null,
    code: match[4],
    message: match[5],
  };
}

function increment(map, key, amount = 1) {
  map.set(key, (map.get(key) ?? 0) + amount);
}

function sortedCounts(map) {
  return [...map.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .map(([key, count]) => ({ key, count }));
}

const SEMANTIC_FAMILY_RULES = [
  {
    id: "template literal inference",
    test: (text) =>
      /`[^`]*\$\{/.test(text) || /\binfer\s+\w+\s+extends\s+string\b/.test(text),
  },
  {
    id: "mapped/key-remapped types",
    test: (text) =>
      /\[\s*(?:readonly\s+)?[A-Za-z_$][\w$]*\s+in\s+/.test(text) ||
      /\bas\s+keyof\b/.test(text),
  },
  {
    id: "indexed access",
    test: (text) =>
      /\bkeyof\b/.test(text) || /[A-Za-z_$][\w$]*(?:<[^>\n]+>)?\s*\[[^\]\n]+\]/.test(text),
  },
  {
    id: "tuple recursion",
    test: (text) =>
      /\[\s*(?:\.\.\.|infer\b)/.test(text) ||
      /\binfer\s+\w+\s*,/.test(text) ||
      /\.\.\.\s*[A-Za-z_$][\w$]*/.test(text),
  },
  {
    id: "recursive conditionals",
    test: (text) =>
      /\bextends\b/.test(text) &&
      (/\binfer\b/.test(text) || /\b[A-Za-z_$][\w$]*<[^>]+>/.test(text)),
  },
  {
    id: "distributive conditionals",
    test: (text) => /\b[A-Za-z_$][\w$]*\s+extends\s+/.test(text) && /\?/.test(text),
  },
  {
    id: "inference cache/session behavior",
    test: (text) => /\binfer\b/.test(text) || /<[^>]*\bextends\b/.test(text),
  },
];

function familiesForDiagnosticFile(file, sourceCache) {
  if (!file) {
    return ["unknown"];
  }

  const normalized = normalizePath(file).replace(/^\.\//, "");
  const candidatePath = path.resolve(candidateRoot, normalized);
  if (
    candidatePath !== candidateRoot &&
    !candidatePath.startsWith(`${candidateRoot}${path.sep}`)
  ) {
    return ["unknown"];
  }
  if (!fs.existsSync(candidatePath)) {
    return ["unknown"];
  }

  let source = sourceCache.get(candidatePath);
  if (source === undefined) {
    source = fs.readFileSync(candidatePath, "utf8");
    sourceCache.set(candidatePath, source);
  }

  const families = SEMANTIC_FAMILY_RULES.filter((rule) => rule.test(source)).map(
    (rule) => rule.id,
  );
  return families.length > 0 ? families : ["unclassified"];
}

function summarizeCandidateSemanticFamilies(manifest) {
  const bySemanticFamily = new Map();
  const semanticFamilyFiles = new Map();
  const sourceCache = new Map();

  for (const entry of manifest.entries ?? []) {
    const output = entry.output;
    if (!output) {
      continue;
    }
    for (const family of familiesForDiagnosticFile(output, sourceCache)) {
      increment(bySemanticFamily, family);
      if (!semanticFamilyFiles.has(family)) {
        semanticFamilyFiles.set(family, new Set());
      }
      semanticFamilyFiles.get(family).add(output);
    }
  }

  return sortedCounts(bySemanticFamily).map((entry) => ({
    family: entry.key,
    candidateCount: entry.count,
    files: [...(semanticFamilyFiles.get(entry.key) ?? [])].sort(),
  }));
}

function summarizeDiagnostics(errors) {
  const parsed = errors.map(parseDiagnostic);
  const byCode = new Map();
  const byFile = new Map();
  const bySemanticFamily = new Map();
  const semanticFamilyFiles = new Map();
  const sourceCache = new Map();

  for (const diagnostic of parsed) {
    increment(byCode, diagnostic.code ?? "unknown");
    if (diagnostic.file) {
      increment(byFile, diagnostic.file);
    }
    for (const family of familiesForDiagnosticFile(diagnostic.file, sourceCache)) {
      increment(bySemanticFamily, family);
      if (!semanticFamilyFiles.has(family)) {
        semanticFamilyFiles.set(family, new Set());
      }
      if (diagnostic.file) {
        semanticFamilyFiles.get(family).add(diagnostic.file);
      }
    }
  }

  return {
    byCode: sortedCounts(byCode),
    byFile: sortedCounts(byFile),
    bySemanticFamily: sortedCounts(bySemanticFamily).map((entry) => ({
      family: entry.key,
      errorCount: entry.count,
      files: [...(semanticFamilyFiles.get(entry.key) ?? [])].sort(),
    })),
  };
}

function commandFor(bin, tsconfig) {
  return [bin, "--noEmit", "-p", tsconfig, "--pretty", "false"];
}

function runCompiler(label, bin, tsconfig, timeoutMs) {
  if (!bin) {
    return {
      status: "unavailable",
      available: false,
      command: null,
      exitCode: null,
      signal: null,
      diagnostics: {
        errorCount: null,
        firstErrors: [],
        byCode: [],
        byFile: [],
        bySemanticFamily: [],
      },
    };
  }

  const args = ["--noEmit", "-p", tsconfig, "--pretty", "false"];
  const result = spawnSync(bin, args, {
    cwd: candidateDir,
    encoding: "utf8",
    timeout: timeoutMs,
  });
  const stdout = result.stdout ?? "";
  const stderr = result.stderr ?? "";
  const output = `${stdout}${stderr}`;
  const errors = diagnosticLines(output);
  const summary = summarizeDiagnostics(errors);
  const timedOut = result.error?.code === "ETIMEDOUT";
  const status =
    timedOut ? "timeout" : result.error ? "error" : result.status === 0 ? "pass" : "fail";

  return {
    status,
    available: true,
    command: commandFor(bin, tsconfig),
    exitCode: result.status,
    signal: result.signal,
    diagnostics: {
      errorCount: errors.length,
      firstErrors: errors.slice(0, 20),
      ...summary,
    },
    error: result.error
      ? {
          code: result.error.code,
          message: result.error.message,
        }
      : null,
  };
}

const manifest = readJson(candidateManifestPath);
const tsconfig = path.join(candidateDir, "tsconfig.tsz-guard.json");
if (!fs.existsSync(tsconfig)) {
  console.error(`error: assertion candidate tsconfig does not exist: ${tsconfig}`);
  process.exit(1);
}

const timeoutMs = Number(
  process.env.TYPE_CHALLENGES_ASSERTION_CLASSIFIER_TIMEOUT_MS ?? 30000,
);
if (!Number.isInteger(timeoutMs) || timeoutMs <= 0) {
  console.error("error: TYPE_CHALLENGES_ASSERTION_CLASSIFIER_TIMEOUT_MS must be a positive integer");
  process.exit(1);
}

const tszBin = executableOrNull(process.env.TSZ_BIN);
const tscBin = discoverTscBin();

const report = {
  fixture: "type-challenges-assertion-classification",
  candidateManifest: {
    fixture: manifest.fixture,
    counts: manifest.counts,
    semanticFamilies: summarizeCandidateSemanticFamilies(manifest),
  },
  tsconfig: path.relative(candidateDir, tsconfig).split(path.sep).join("/"),
  timeoutMs,
  compilers: {
    tsc: runCompiler("tsc", tscBin, tsconfig, timeoutMs),
    tsz: runCompiler("tsz", tszBin, tsconfig, timeoutMs),
  },
};

fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);

console.log(
  [
    "classified Type Challenges assertion candidates",
    `tsc: ${report.compilers.tsc.status}`,
    `tsz: ${report.compilers.tsz.status}`,
    `report: ${path.relative(process.cwd(), outputPath).split(path.sep).join("/")}`,
  ].join("\n"),
);
