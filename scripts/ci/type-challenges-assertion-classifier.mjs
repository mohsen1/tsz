#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import {
  normalizePath,
  semanticFamiliesForFile,
} from "./type-challenges-semantic-families.mjs";

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

function fail(message) {
  console.error(`error: ${message}`);
  process.exit(1);
}

function executableOrNull(file) {
  if (!file) {
    return null;
  }
  try {
    fs.accessSync(file, fs.constants.X_OK);
    return path.resolve(file);
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

function requiredRelativeManifestPath(value, label) {
  if (typeof value !== "string" || value.trim() === "") {
    fail(`manifest ${label} must be a non-empty relative path`);
  }
  const normalized = normalizePath(value).replace(/^\.\//, "");
  const segments = normalized.split("/");
  if (
    path.isAbsolute(value) ||
    normalized.startsWith("/") ||
    /^[A-Za-z]:(?:\/|$)/.test(normalized) ||
    normalized === "" ||
    normalized === "." ||
    segments.includes("") ||
    segments.includes(".") ||
    segments.includes("..")
  ) {
    fail(`manifest ${label} must be a relative path inside the candidate directory: ${value}`);
  }
  return normalized;
}

function requiredManifestString(value, label) {
  if (typeof value !== "string" || value.trim() === "") {
    fail(`manifest ${label} must be a non-empty string`);
  }
  return value;
}

function requiredCount(value, label) {
  if (!Number.isInteger(value) || value < 0) {
    fail(`manifest counts.${label} must be a non-negative integer`);
  }
  return value;
}

function optionalCount(value, label) {
  if (value === undefined) {
    return null;
  }
  return requiredCount(value, label);
}

function validateSourceMetadata(source, label) {
  if (
    typeof source?.repository === "string" &&
    source.repository.trim() !== "" &&
    typeof source?.ref === "string" &&
    source.ref.trim() !== ""
  ) {
    return;
  }
  fail(
    [
      `manifest sources.${label} is missing source metadata`,
      `${source?.repository || "<missing repository>"} @ ${source?.ref || "<missing ref>"}`,
    ].join("\n"),
  );
}

function validateManifestSources(manifest) {
  if (!manifest?.sources || typeof manifest.sources !== "object") {
    fail("manifest is missing sources");
  }
  for (const label of ["templates", "testCases", "solutions"]) {
    validateSourceMetadata(manifest.sources[label], label);
  }

  const templateSource = manifest.sources.templates;
  const testCaseSource = manifest.sources.testCases;
  if (
    templateSource.repository !== testCaseSource.repository ||
    templateSource.ref !== testCaseSource.ref
  ) {
    fail(
      [
        "manifest template and test-case sources come from different snapshots",
        `templates: ${templateSource.repository} @ ${templateSource.ref}`,
        `testCases: ${testCaseSource.repository} @ ${testCaseSource.ref}`,
      ].join("\n"),
    );
  }
}

function validateCandidateManifest(manifest) {
  const acceptedFixtures = new Set([
    "type-challenges-assertion-candidates",
    "type-challenges-assertions-tsc-clean",
  ]);
  if (!acceptedFixtures.has(manifest?.fixture)) {
    fail(`unexpected assertion candidate manifest fixture: ${manifest?.fixture || "<missing>"}`);
  }
  if (!Array.isArray(manifest.entries)) {
    fail("manifest entries must be an array");
  }
  if (manifest.entries.length === 0) {
    fail("manifest entries must include at least one assertion candidate");
  }

  const counts = manifest.counts ?? {};
  const pairedSolutions =
    manifest.fixture === "type-challenges-assertion-candidates"
      ? requiredCount(counts.pairedSolutions, "pairedSolutions")
      : optionalCount(counts.pairedSolutions, "pairedSolutions");
  const generatedAssertions =
    manifest.fixture === "type-challenges-assertion-candidates"
      ? requiredCount(counts.generatedAssertions, "generatedAssertions")
      : optionalCount(counts.generatedAssertions, "generatedAssertions");
  const tscAcceptedAssertions =
    manifest.fixture === "type-challenges-assertions-tsc-clean"
      ? requiredCount(counts.tscAcceptedAssertions, "tscAcceptedAssertions")
      : optionalCount(counts.tscAcceptedAssertions, "tscAcceptedAssertions");
  const referenced =
    manifest.fixture === "type-challenges-assertion-candidates"
      ? requiredCount(
          counts.assertionsReferencingSolutionDeclaration,
          "assertionsReferencingSolutionDeclaration",
        )
      : optionalCount(
          counts.assertionsReferencingSolutionDeclaration,
          "assertionsReferencingSolutionDeclaration",
        );
  const missing =
    manifest.fixture === "type-challenges-assertion-candidates"
      ? requiredCount(
          counts.assertionsMissingSolutionDeclarationReference,
          "assertionsMissingSolutionDeclarationReference",
        )
      : optionalCount(
          counts.assertionsMissingSolutionDeclarationReference,
          "assertionsMissingSolutionDeclarationReference",
        );
  const tscAcceptedReferenced =
    manifest.fixture === "type-challenges-assertions-tsc-clean"
      ? requiredCount(
          counts.tscAcceptedAssertionsReferencingSolutionDeclaration,
          "tscAcceptedAssertionsReferencingSolutionDeclaration",
        )
      : optionalCount(
          counts.tscAcceptedAssertionsReferencingSolutionDeclaration,
          "tscAcceptedAssertionsReferencingSolutionDeclaration",
        );
  const tscAcceptedMissing =
    manifest.fixture === "type-challenges-assertions-tsc-clean"
      ? requiredCount(
          counts.tscAcceptedAssertionsMissingSolutionDeclarationReference,
          "tscAcceptedAssertionsMissingSolutionDeclarationReference",
        )
      : optionalCount(
          counts.tscAcceptedAssertionsMissingSolutionDeclarationReference,
          "tscAcceptedAssertionsMissingSolutionDeclarationReference",
        );

  if (pairedSolutions !== null && pairedSolutions !== manifest.entries.length) {
    fail(
      `manifest counts.pairedSolutions (${pairedSolutions}) does not match entries length (${manifest.entries.length})`,
    );
  }
  if (generatedAssertions !== null && generatedAssertions !== manifest.entries.length) {
    fail(
      `manifest counts.generatedAssertions (${generatedAssertions}) does not match entries length (${manifest.entries.length})`,
    );
  }
  if (tscAcceptedAssertions !== null && tscAcceptedAssertions !== manifest.entries.length) {
    fail(
      `manifest counts.tscAcceptedAssertions (${tscAcceptedAssertions}) does not match entries length (${manifest.entries.length})`,
    );
  }
  if (referenced !== null && missing !== null && referenced + missing !== manifest.entries.length) {
    fail(
      "manifest declaration-reference counts do not account for every assertion candidate",
    );
  }
  if (
    tscAcceptedReferenced !== null &&
    tscAcceptedMissing !== null &&
    tscAcceptedReferenced + tscAcceptedMissing !== manifest.entries.length
  ) {
    fail(
      "manifest tsc-accepted declaration-reference counts do not account for every assertion candidate",
    );
  }

  const seenOutputs = new Set();
  const seenIds = new Set();
  const entries = manifest.entries.map((entry, index) => {
    const id = requiredManifestString(entry?.id, `entries[${index}].id`);
    if (seenIds.has(id)) {
      fail(`duplicate assertion candidate id in manifest: ${id}`);
    }
    seenIds.add(id);

    const output = requiredRelativeManifestPath(entry?.output, `entries[${index}].output`);
    if (!output.startsWith("assertions/")) {
      fail(`manifest entries[${index}].output must be under assertions/: ${output}`);
    }
    if (seenOutputs.has(output)) {
      fail(`duplicate assertion candidate output in manifest: ${output}`);
    }
    seenOutputs.add(output);

    const outputPath = path.resolve(candidateRoot, output);
    if (
      outputPath === candidateRoot ||
      !outputPath.startsWith(`${candidateRoot}${path.sep}`) ||
      !fs.existsSync(outputPath)
    ) {
      fail(`manifest assertion candidate does not exist inside candidate directory: ${output}`);
    }

    return {
      ...entry,
      id,
      output,
    };
  });
  validateManifestSources(manifest);

  return {
    ...manifest,
    entries,
  };
}

function normalizeDiagnosticFile(file) {
  const normalized = normalizePath(file).replace(/^\.\//, "");
  const resolved = path.resolve(candidateRoot, normalized);
  if (
    resolved === candidateRoot ||
    resolved.startsWith(`${candidateRoot}${path.sep}`)
  ) {
    return path.relative(candidateRoot, resolved).split(path.sep).join("/");
  }
  return normalized;
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
    file: match[1] ? normalizeDiagnosticFile(match[1]) : null,
    line: match[2] ? Number(match[2]) : null,
    column: match[3] ? Number(match[3]) : null,
    code: match[4],
    message: match[5],
  };
}

function normalizeDiagnosticLine(line) {
  return formatDiagnostic(parseDiagnostic(line));
}

function formatDiagnostic(diagnostic) {
  if (!diagnostic.file || !diagnostic.code) {
    return diagnostic.raw;
  }

  const location =
    diagnostic.line === null || diagnostic.column === null
      ? ""
      : `(${diagnostic.line},${diagnostic.column})`;
  return `${diagnostic.file}${location}: error ${diagnostic.code}: ${diagnostic.message}`;
}

function increment(map, key, amount = 1) {
  map.set(key, (map.get(key) ?? 0) + amount);
}

function sortedCounts(map) {
  return [...map.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .map(([key, count]) => ({ key, count }));
}

function familiesForDiagnosticFile(file, sourceCache) {
  return semanticFamiliesForFile(file, candidateRoot, sourceCache);
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
  const byCandidate = new Map();
  const sourceCache = new Map();

  for (const diagnostic of parsed) {
    increment(byCode, diagnostic.code ?? "unknown");
    if (diagnostic.file) {
      increment(byFile, diagnostic.file);
      if (!byCandidate.has(diagnostic.file)) {
        byCandidate.set(diagnostic.file, {
          file: diagnostic.file,
          errorCount: 0,
          codes: new Map(),
          firstErrors: [],
          semanticFamilies: new Set(),
        });
      }
      const candidate = byCandidate.get(diagnostic.file);
      candidate.errorCount += 1;
      increment(candidate.codes, diagnostic.code ?? "unknown");
      if (candidate.firstErrors.length < 5) {
        candidate.firstErrors.push({
          line: diagnostic.line,
          column: diagnostic.column,
          code: diagnostic.code,
          message: diagnostic.message,
          text: formatDiagnostic(diagnostic),
        });
      }
    }
    for (const family of familiesForDiagnosticFile(diagnostic.file, sourceCache)) {
      increment(bySemanticFamily, family);
      if (!semanticFamilyFiles.has(family)) {
        semanticFamilyFiles.set(family, new Set());
      }
      if (diagnostic.file) {
        semanticFamilyFiles.get(family).add(diagnostic.file);
        byCandidate.get(diagnostic.file)?.semanticFamilies.add(family);
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
    byCandidate: [...byCandidate.values()]
      .sort((a, b) => b.errorCount - a.errorCount || a.file.localeCompare(b.file))
      .map((entry) => ({
        file: entry.file,
        errorCount: entry.errorCount,
        codes: sortedCounts(entry.codes),
        semanticFamilies: [...entry.semanticFamilies].sort(),
        firstErrors: entry.firstErrors,
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
        byCandidate: [],
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
      firstErrors: errors.slice(0, 20).map(normalizeDiagnosticLine),
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

function withCandidateDiagnostics(result, manifest) {
  const entries = manifest.entries ?? [];
  const outputs = entries
    .map((entry) => entry.output)
    .filter(Boolean);
  const entriesByOutput = new Map(entries.map((entry) => [entry.output, entry]));
  const diagnosticFiles = new Set(
    (result.diagnostics?.byFile ?? []).map((entry) => entry.key),
  );

  if (!result.available || result.diagnostics?.errorCount === null) {
    return {
      ...result,
      candidateDiagnostics: {
        totalCandidates: outputs.length,
        candidatesWithDiagnostics: null,
        candidatesWithoutDiagnostics: null,
        filesWithDiagnostics: [],
        filesWithoutDiagnostics: [],
        byCandidate: [],
      },
    };
  }

  const filesWithDiagnostics = outputs.filter((output) => diagnosticFiles.has(output));
  const filesWithoutDiagnostics = outputs.filter((output) => !diagnosticFiles.has(output));
  const diagnosticsByCandidate = new Map(
    (result.diagnostics?.byCandidate ?? []).map((entry) => [entry.file, entry]),
  );
  return {
    ...result,
    candidateDiagnostics: {
      totalCandidates: outputs.length,
      candidatesWithDiagnostics: filesWithDiagnostics.length,
      candidatesWithoutDiagnostics: filesWithoutDiagnostics.length,
      filesWithDiagnostics,
      filesWithoutDiagnostics,
      byCandidate: filesWithDiagnostics
        .map((file) => {
          const diagnostic = diagnosticsByCandidate.get(file);
          if (!diagnostic) {
            return null;
          }
          const entry = entriesByOutput.get(file);
          return {
            ...diagnostic,
            candidate: entry
              ? {
                  id: entry.id ?? null,
                  solution: entry.solution
                    ? {
                        output: entry.solution.output ?? null,
                        source: entry.solution.source ?? null,
                        declarations: entry.solution.declarations ?? [],
                      }
                    : null,
                  testCase: entry.testCase
                    ? {
                        output: entry.testCase.output ?? null,
                        source: entry.testCase.source ?? null,
                      }
                    : null,
                  assertion: entry.assertion
                    ? {
                        hasReferencedSolutionDeclaration:
                          entry.assertion.hasReferencedSolutionDeclaration ?? null,
                        referencedSolutionDeclarations:
                          entry.assertion.referencedSolutionDeclarations ?? [],
                      }
                    : null,
                }
              : null,
          };
        })
        .filter(Boolean),
    },
  };
}

function countsByKey(counts, keyField = "key", countField = "count") {
  return new Map((counts ?? []).map((entry) => [entry[keyField], entry[countField]]));
}

function deltaCounts(left, right, keyField = "key", countField = "count") {
  const leftCounts = countsByKey(left, keyField, countField);
  const rightCounts = countsByKey(right, keyField, countField);
  const keys = new Set([...leftCounts.keys(), ...rightCounts.keys()]);

  return [...keys]
    .map((key) => ({
      key,
      tsc: leftCounts.get(key) ?? 0,
      tsz: rightCounts.get(key) ?? 0,
      delta: (rightCounts.get(key) ?? 0) - (leftCounts.get(key) ?? 0),
    }))
    .filter((entry) => entry.delta !== 0)
    .sort((a, b) => Math.abs(b.delta) - Math.abs(a.delta) || a.key.localeCompare(b.key));
}

function intersectSorted(left, right) {
  const rightSet = new Set(right);
  return left.filter((file) => rightSet.has(file)).sort();
}

function compareCandidateFiles(tsc, tsz) {
  const tscDiagnostics = tsc.candidateDiagnostics;
  const tszDiagnostics = tsz.candidateDiagnostics;
  if (
    !tscDiagnostics ||
    !tszDiagnostics ||
    tscDiagnostics.candidatesWithDiagnostics === null ||
    tszDiagnostics.candidatesWithDiagnostics === null
  ) {
    return null;
  }

  const bothAccepted = intersectSorted(
    tscDiagnostics.filesWithoutDiagnostics,
    tszDiagnostics.filesWithoutDiagnostics,
  );
  const bothRejected = intersectSorted(
    tscDiagnostics.filesWithDiagnostics,
    tszDiagnostics.filesWithDiagnostics,
  );
  const tscAcceptedTszRejected = intersectSorted(
    tscDiagnostics.filesWithoutDiagnostics,
    tszDiagnostics.filesWithDiagnostics,
  );
  const tscRejectedTszAccepted = intersectSorted(
    tscDiagnostics.filesWithDiagnostics,
    tszDiagnostics.filesWithoutDiagnostics,
  );

  return {
    totalCandidates: tscDiagnostics.totalCandidates,
    counts: {
      bothAccepted: bothAccepted.length,
      bothRejected: bothRejected.length,
      tscAcceptedTszRejected: tscAcceptedTszRejected.length,
      tscRejectedTszAccepted: tscRejectedTszAccepted.length,
    },
    bothAccepted,
    bothRejected,
    tscAcceptedTszRejected,
    tscRejectedTszAccepted,
  };
}

function compareCompilers(tsc, tsz) {
  if (!tsc.available || !tsz.available) {
    return {
      status: "unavailable",
      tscStatus: tsc.status,
      tszStatus: tsz.status,
      errorCountDelta: null,
      diagnosticFreeCandidateDelta: null,
      candidateFileComparison: null,
      byCodeDelta: [],
      bySemanticFamilyDelta: [],
    };
  }

  const tscErrorCount = tsc.diagnostics.errorCount;
  const tszErrorCount = tsz.diagnostics.errorCount;
  let status = "both-nonpassing";
  if (tsc.status === "pass" && tsz.status === "pass") {
    status = "both-pass";
  } else if (tsc.status === "pass") {
    status = "tsz-rejects-tsc-accepted";
  } else if (tsz.status === "pass") {
    status = "tsz-accepts-tsc-rejected";
  }

  return {
    status,
    tscStatus: tsc.status,
    tszStatus: tsz.status,
    errorCountDelta:
      tscErrorCount === null || tszErrorCount === null ? null : tszErrorCount - tscErrorCount,
    diagnosticFreeCandidateDelta:
      tsc.candidateDiagnostics?.candidatesWithoutDiagnostics === null ||
      tsz.candidateDiagnostics?.candidatesWithoutDiagnostics === null
        ? null
        : tsz.candidateDiagnostics.candidatesWithoutDiagnostics -
          tsc.candidateDiagnostics.candidatesWithoutDiagnostics,
    candidateFileComparison: compareCandidateFiles(tsc, tsz),
    byCodeDelta: deltaCounts(tsc.diagnostics.byCode, tsz.diagnostics.byCode),
    bySemanticFamilyDelta: deltaCounts(
      tsc.diagnostics.bySemanticFamily,
      tsz.diagnostics.bySemanticFamily,
      "family",
      "errorCount",
    ),
  };
}

const manifest = validateCandidateManifest(readJson(candidateManifestPath));
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
const tscResult = withCandidateDiagnostics(
  runCompiler("tsc", tscBin, tsconfig, timeoutMs),
  manifest,
);
const tszResult = withCandidateDiagnostics(
  runCompiler("tsz", tszBin, tsconfig, timeoutMs),
  manifest,
);

const report = {
  fixture: "type-challenges-assertion-classification",
  candidateManifest: {
    fixture: manifest.fixture,
    sources: manifest.sources ?? null,
    counts: manifest.counts,
    semanticFamilies: summarizeCandidateSemanticFamilies(manifest),
  },
  tsconfig: path.relative(candidateDir, tsconfig).split(path.sep).join("/"),
  timeoutMs,
  compilers: {
    tsc: tscResult,
    tsz: tszResult,
  },
  comparison: compareCompilers(tscResult, tszResult),
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
