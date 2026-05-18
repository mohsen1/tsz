#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const [
  ,
  ,
  classificationPath,
  candidateDir,
  outFile,
  fixtureRoot = "",
  cleanSubsetManifestPath = "",
  cleanSubsetClassificationPath = "",
  cleanSubsetDir = cleanSubsetManifestPath ? path.dirname(cleanSubsetManifestPath) : "",
] = process.argv;

if (!classificationPath || !candidateDir || !outFile) {
  console.error(
    "usage: type-challenges-assertion-compatibility.mjs <classification.json> <candidate-dir> <out.jsonl> [fixture-root]",
  );
  process.exit(2);
}

if (!fs.existsSync(classificationPath)) {
  process.exit(0);
}

const report = JSON.parse(fs.readFileSync(classificationPath, "utf8"));

function fail(message) {
  console.error(`error: ${message}`);
  process.exit(1);
}

function validateClassificationCompilerReport(report) {
  if (report?.fixture !== "type-challenges-assertion-classification") {
    fail(`unexpected assertion classification fixture: ${report?.fixture || "<missing>"}`);
  }
  if (!report.compilers || typeof report.compilers !== "object") {
    fail("assertion classification report is missing compilers");
  }
  if (!report.compilers.tsc || !report.compilers.tsz) {
    fail("assertion classification report must include both tsc and tsz compiler results");
  }
}

function validateSourceMetadata(source, label) {
  if (source?.repository && source?.ref) return;

  fail(
    [
      `assertion classification candidateManifest.sources.${label} is missing source metadata`,
      `${source?.repository || "<missing repository>"} @ ${source?.ref || "<missing ref>"}`,
    ].join("\n"),
  );
}

function validateCandidateManifestSources(manifest) {
  if (!manifest?.sources || typeof manifest.sources !== "object") {
    fail("assertion classification candidateManifest is missing sources");
  }

  for (const label of ["templates", "testCases", "solutions"]) {
    validateSourceMetadata(manifest.sources[label], label);
  }
}

function validateCleanManifestSources(manifest) {
  if (!manifest?.sources || typeof manifest.sources !== "object") {
    fail("tsc-clean assertion manifest is missing sources");
  }

  for (const label of ["templates", "testCases", "solutions"]) {
    const source = manifest.sources[label];
    if (!source?.repository || !source?.ref) {
      fail(
        [
          `tsc-clean assertion manifest.sources.${label} is missing source metadata`,
          `${source?.repository || "<missing repository>"} @ ${source?.ref || "<missing ref>"}`,
        ].join("\n"),
      );
    }
  }
}

function validateCleanClassificationSources(cleanManifest, classificationManifest) {
  for (const label of ["templates", "testCases", "solutions"]) {
    const manifestSource = cleanManifest.sources[label];
    const classificationSource = classificationManifest.sources[label];
    if (
      classificationSource.repository !== manifestSource.repository ||
      classificationSource.ref !== manifestSource.ref
    ) {
      fail(
        `tsc-clean assertion classification candidateManifest.sources.${label} (${classificationSource.repository} @ ${classificationSource.ref}) does not match manifest sources.${label} (${manifestSource.repository} @ ${manifestSource.ref})`,
      );
    }
  }
}

function validateCandidateOutputPath(value, label) {
  if (typeof value !== "string" || value.trim() === "") {
    fail(`${label} must be a non-empty relative path`);
  }
  const normalized = value.split(/[\\/]+/).join("/").replace(/^\.\//, "");
  if (
    path.isAbsolute(value) ||
    normalized === "" ||
    normalized === "." ||
    normalized.split("/").includes("..")
  ) {
    fail(`${label} must stay inside the assertion candidate directory: ${value}`);
  }
  if (!normalized.startsWith("assertions/")) {
    fail(`${label} must be under assertions/: ${normalized}`);
  }

  return normalized;
}

function duplicatedValues(values) {
  const seen = new Set();
  const duplicates = new Set();
  for (const value of values) {
    if (seen.has(value)) {
      duplicates.add(value);
    }
    seen.add(value);
  }
  return [...duplicates].sort();
}

function validateReport(report) {
  validateClassificationCompilerReport(report);
  if (!report.comparison || typeof report.comparison !== "object") {
    fail("assertion classification report is missing comparison");
  }
  if (
    typeof report.comparison.status !== "string" ||
    report.comparison.status.trim() === ""
  ) {
    fail("assertion classification comparison.status must be a non-empty string");
  }
  if (!report.candidateManifest || typeof report.candidateManifest !== "object") {
    fail("assertion classification report is missing candidateManifest");
  }
  validateCandidateManifestSources(report.candidateManifest);
}

validateReport(report);

const hasCleanSubsetManifest = cleanSubsetManifestPath && fs.existsSync(cleanSubsetManifestPath);
const hasCleanSubsetClassification =
  cleanSubsetClassificationPath && fs.existsSync(cleanSubsetClassificationPath);
const cleanSubsetManifest = hasCleanSubsetManifest
  ? JSON.parse(fs.readFileSync(cleanSubsetManifestPath, "utf8"))
  : null;
const cleanSubsetClassification = hasCleanSubsetClassification
  ? JSON.parse(fs.readFileSync(cleanSubsetClassificationPath, "utf8"))
  : null;

if (!cleanSubsetManifest && cleanSubsetClassification) {
  fail("tsc-clean assertion classification was provided without a manifest");
}

if (
  cleanSubsetManifest &&
  cleanSubsetManifest.fixture !== "type-challenges-assertions-tsc-clean"
) {
  fail(
    `unexpected tsc-clean assertion manifest fixture: ${
      cleanSubsetManifest.fixture || "<missing>"
    }`,
  );
}
if (
  cleanSubsetManifest &&
  (!cleanSubsetManifest.counts || typeof cleanSubsetManifest.counts !== "object")
) {
  fail("tsc-clean assertion manifest is missing counts");
}
if (cleanSubsetManifest) {
  const cleanSubsetEntries = cleanSubsetManifest.entries;
  if (!Array.isArray(cleanSubsetEntries)) {
    fail("tsc-clean assertion manifest entries must be an array");
  }
  const cleanSubsetCounts = cleanSubsetManifest.counts;
  const acceptedAssertions = cleanSubsetCounts.tscAcceptedAssertions;
  const acceptedReferencingSolutionDeclaration =
    cleanSubsetCounts.tscAcceptedAssertionsReferencingSolutionDeclaration;
  const acceptedMissingSolutionDeclarationReference =
    cleanSubsetCounts.tscAcceptedAssertionsMissingSolutionDeclarationReference;
  const rejectedAssertions = cleanSubsetCounts.tscRejectedAssertions;
  const totalCandidates = cleanSubsetCounts.totalCandidates;
  if (
    !Number.isInteger(acceptedAssertions) ||
    acceptedAssertions !== cleanSubsetEntries.length
  ) {
    fail(
      `tsc-clean assertion manifest counts.tscAcceptedAssertions (${acceptedAssertions}) does not match entries length (${cleanSubsetEntries.length})`,
    );
  }
  if (!Number.isInteger(acceptedReferencingSolutionDeclaration)) {
    fail(
      "tsc-clean assertion manifest counts.tscAcceptedAssertionsReferencingSolutionDeclaration must be an integer",
    );
  }
  if (!Number.isInteger(acceptedMissingSolutionDeclarationReference)) {
    fail(
      "tsc-clean assertion manifest counts.tscAcceptedAssertionsMissingSolutionDeclarationReference must be an integer",
    );
  }
  if (!Number.isInteger(rejectedAssertions)) {
    fail(
      "tsc-clean assertion manifest counts.tscRejectedAssertions must be an integer",
    );
  }
  if (!Number.isInteger(totalCandidates)) {
    fail("tsc-clean assertion manifest counts.totalCandidates must be an integer");
  }
  if (
    acceptedReferencingSolutionDeclaration + acceptedMissingSolutionDeclarationReference !==
      acceptedAssertions
  ) {
    fail(
      `tsc-clean assertion manifest declaration-reference counts (${acceptedReferencingSolutionDeclaration} + ${acceptedMissingSolutionDeclarationReference}) do not match tscAcceptedAssertions (${acceptedAssertions})`,
    );
  }
  if (acceptedAssertions + rejectedAssertions !== totalCandidates) {
    fail(
      `tsc-clean assertion manifest accepted/rejected counts (${acceptedAssertions} + ${rejectedAssertions}) do not match totalCandidates (${totalCandidates})`,
    );
  }
  if (acceptedAssertions > 0 && !cleanSubsetClassification) {
    fail(
      `tsc-clean assertion manifest has ${acceptedAssertions} accepted assertions but classification is missing`,
    );
  }
}
if (cleanSubsetClassification) {
  validateClassificationCompilerReport(cleanSubsetClassification);
  if (
    !cleanSubsetClassification.candidateManifest ||
    typeof cleanSubsetClassification.candidateManifest !== "object"
  ) {
    fail("tsc-clean assertion classification is missing candidateManifest");
  }
  if (
    cleanSubsetClassification.candidateManifest.fixture !==
    "type-challenges-assertions-tsc-clean"
  ) {
    fail(
      `unexpected tsc-clean assertion classification candidate manifest fixture: ${
        cleanSubsetClassification.candidateManifest.fixture || "<missing>"
      }`,
    );
  }
  validateCandidateManifestSources(cleanSubsetClassification.candidateManifest);
  if (
    !cleanSubsetClassification.comparison ||
    typeof cleanSubsetClassification.comparison !== "object"
  ) {
    fail("tsc-clean assertion classification report is missing comparison");
  }
  const cleanComparisonStatus = cleanSubsetClassification.comparison.status;
  if (
    typeof cleanComparisonStatus !== "string" ||
    cleanComparisonStatus.trim() === ""
  ) {
    fail(
      "tsc-clean assertion classification comparison.status must be a non-empty string",
    );
  }
}
if (cleanSubsetManifest && cleanSubsetClassification) {
  const acceptedAssertions = cleanSubsetManifest.counts.tscAcceptedAssertions;
  const cleanCounts = cleanSubsetManifest.counts;
  const classificationCounts = cleanSubsetClassification.candidateManifest?.counts || {};
  validateCleanManifestSources(cleanSubsetManifest);
  validateCleanClassificationSources(
    cleanSubsetManifest,
    cleanSubsetClassification.candidateManifest,
  );
  const generatedAssertions =
    classificationCounts.tscAcceptedAssertions;
  if (
    !Number.isInteger(generatedAssertions) ||
    generatedAssertions !== acceptedAssertions
  ) {
    fail(
      `tsc-clean assertion classification tscAcceptedAssertions (${generatedAssertions}) does not match manifest tscAcceptedAssertions (${acceptedAssertions})`,
    );
  }
  for (const field of [
    "totalCandidates",
    "tscAcceptedAssertionsReferencingSolutionDeclaration",
    "tscAcceptedAssertionsMissingSolutionDeclarationReference",
    "tscRejectedAssertions",
  ]) {
    if (classificationCounts[field] !== cleanCounts[field]) {
      fail(
        `tsc-clean assertion classification counts.${field} (${classificationCounts[field]}) does not match manifest counts.${field} (${cleanCounts[field]})`,
      );
    }
  }
  for (const compiler of ["tsc", "tsz"]) {
    const status = cleanSubsetClassification.compilers?.[compiler]?.status;
    if (typeof status !== "string" || status.trim() === "") {
      fail(
        `tsc-clean assertion classification ${compiler} status must be a non-empty string`,
      );
    }
    const totalCandidates =
      cleanSubsetClassification.compilers?.[compiler]?.candidateDiagnostics?.totalCandidates;
    if (!Number.isInteger(totalCandidates)) {
      fail(
        `tsc-clean assertion classification ${compiler} candidateDiagnostics.totalCandidates must be an integer`,
      );
    }
    if (totalCandidates !== acceptedAssertions) {
      fail(
        `tsc-clean assertion classification ${compiler} totalCandidates (${totalCandidates}) does not match manifest tscAcceptedAssertions (${acceptedAssertions})`,
      );
    }
    const candidatesWithoutDiagnostics =
      cleanSubsetClassification.compilers?.[compiler]?.candidateDiagnostics
        ?.candidatesWithoutDiagnostics;
    if (!Number.isInteger(candidatesWithoutDiagnostics)) {
      fail(
        `tsc-clean assertion classification ${compiler} candidateDiagnostics.candidatesWithoutDiagnostics must be an integer`,
      );
    }
    if (candidatesWithoutDiagnostics < 0 || candidatesWithoutDiagnostics > totalCandidates) {
      fail(
        `tsc-clean assertion classification ${compiler} candidatesWithoutDiagnostics (${candidatesWithoutDiagnostics}) must be between 0 and totalCandidates (${totalCandidates})`,
      );
    }
  }
}
const tsc = report.compilers?.tsc || {};
const tsz = report.compilers?.tsz || {};
const comparison = report.comparison || {};
const counts = report.candidateManifest?.counts || {};
for (const [compiler, result] of [
  ["tsc", tsc],
  ["tsz", tsz],
]) {
  if (typeof result.status !== "string" || result.status.trim() === "") {
    fail(`assertion classification ${compiler} status must be a non-empty string`);
  }
}
if (!Number.isInteger(counts.pairedSolutions)) {
  fail("assertion classification candidateManifest.counts.pairedSolutions must be an integer");
}
if (!Number.isInteger(counts.generatedAssertions)) {
  fail("assertion classification candidateManifest.counts.generatedAssertions must be an integer");
}
if (counts.pairedSolutions !== counts.generatedAssertions) {
  fail(
    `assertion classification pairedSolutions (${counts.pairedSolutions}) does not match generatedAssertions (${counts.generatedAssertions})`,
  );
}
if (counts.generatedAssertions === 0) {
  fail("assertion classification generatedAssertions must be greater than zero");
}
const assertionsReferencingSolutionDeclaration =
  counts.assertionsReferencingSolutionDeclaration;
const assertionsMissingSolutionDeclarationReference =
  counts.assertionsMissingSolutionDeclarationReference;
if (!Number.isInteger(assertionsReferencingSolutionDeclaration)) {
  fail(
    "assertion classification candidateManifest.counts.assertionsReferencingSolutionDeclaration must be an integer",
  );
}
if (!Number.isInteger(assertionsMissingSolutionDeclarationReference)) {
  fail(
    "assertion classification candidateManifest.counts.assertionsMissingSolutionDeclarationReference must be an integer",
  );
}
if (
  assertionsReferencingSolutionDeclaration +
    assertionsMissingSolutionDeclarationReference !==
  counts.generatedAssertions
) {
  fail(
    `assertion classification declaration-reference counts (${assertionsReferencingSolutionDeclaration} + ${assertionsMissingSolutionDeclarationReference}) do not match generatedAssertions (${counts.generatedAssertions})`,
  );
}
const tscCandidateDiagnostics = tsc.candidateDiagnostics || {};
const tszCandidateDiagnostics = tsz.candidateDiagnostics || {};
const normalizedCandidateDiagnosticFiles = {};
const normalizedCandidateExampleFiles = {};
for (const [compiler, result] of [
  ["tsc", tsc],
  ["tsz", tsz],
]) {
  const diagnostics = result.diagnostics;
  if (diagnostics === null || diagnostics === undefined) {
    continue;
  }
  if (typeof diagnostics !== "object" || Array.isArray(diagnostics)) {
    fail(`assertion classification ${compiler}.diagnostics must be an object`);
  }
  if (diagnostics.firstErrors !== null && diagnostics.firstErrors !== undefined) {
    if (!Array.isArray(diagnostics.firstErrors)) {
      fail(`assertion classification ${compiler}.diagnostics.firstErrors must be an array`);
    }
    diagnostics.firstErrors.forEach((line, index) => {
      if (typeof line !== "string" || line.trim() === "") {
        fail(
          `assertion classification ${compiler}.diagnostics.firstErrors[${index}] must be a non-empty string`,
        );
      }
    });
  }
  if (diagnostics.byCode !== null && diagnostics.byCode !== undefined) {
    if (!Array.isArray(diagnostics.byCode)) {
      fail(`assertion classification ${compiler}.diagnostics.byCode must be an array`);
    }
    diagnostics.byCode.forEach((entry, index) => {
      if (typeof entry?.key !== "string" || entry.key.trim() === "") {
        fail(
          `assertion classification ${compiler}.diagnostics.byCode[${index}].key must be a non-empty string`,
        );
      }
      if (!Number.isInteger(entry.count) || entry.count < 0) {
        fail(
          `assertion classification ${compiler}.diagnostics.byCode[${index}].count must be a non-negative integer`,
        );
      }
    });
  }
}
for (const [compiler, diagnostics] of [
  ["tsc", tscCandidateDiagnostics],
  ["tsz", tszCandidateDiagnostics],
]) {
  if (
    Number.isInteger(diagnostics.totalCandidates) &&
    diagnostics.totalCandidates !== counts.generatedAssertions
  ) {
    fail(
      `assertion classification ${compiler} totalCandidates (${diagnostics.totalCandidates}) does not match generatedAssertions (${counts.generatedAssertions})`,
    );
  }
  if (Number.isInteger(diagnostics.totalCandidates)) {
    for (const field of ["candidatesWithDiagnostics", "candidatesWithoutDiagnostics"]) {
      const value = diagnostics[field];
      if (value === null || value === undefined) {
        continue;
      }
      if (!Number.isInteger(value)) {
        fail(
          `assertion classification ${compiler} candidateDiagnostics.${field} must be an integer`,
        );
      }
      if (value < 0 || value > diagnostics.totalCandidates) {
        fail(
          `assertion classification ${compiler} candidateDiagnostics.${field} (${value}) must be between 0 and totalCandidates (${diagnostics.totalCandidates})`,
        );
      }
    }
    if (
      Number.isInteger(diagnostics.candidatesWithDiagnostics) &&
      Number.isInteger(diagnostics.candidatesWithoutDiagnostics) &&
      diagnostics.candidatesWithDiagnostics + diagnostics.candidatesWithoutDiagnostics !==
        diagnostics.totalCandidates
    ) {
      fail(
        `assertion classification ${compiler} candidate diagnostic counts (${diagnostics.candidatesWithDiagnostics} + ${diagnostics.candidatesWithoutDiagnostics}) do not match totalCandidates (${diagnostics.totalCandidates})`,
      );
    }
  }
  const diagnosticFiles = {};
  for (const [field, countField] of [
    ["filesWithDiagnostics", "candidatesWithDiagnostics"],
    ["filesWithoutDiagnostics", "candidatesWithoutDiagnostics"],
  ]) {
    const files = diagnostics[field];
    if (files === null || files === undefined) {
      diagnosticFiles[field] = [];
      if (
        field === "filesWithDiagnostics" &&
        Number.isInteger(diagnostics[countField]) &&
        diagnostics[countField] > 0
      ) {
        fail(
          `assertion classification ${compiler} candidateDiagnostics.${field} must be an array when ${countField} is nonzero`,
        );
      }
      continue;
    }
    if (!Array.isArray(files)) {
      fail(`assertion classification ${compiler} candidateDiagnostics.${field} must be an array`);
    }
    const normalizedFiles = files.map((file, index) =>
      validateCandidateOutputPath(
        file,
        `assertion classification ${compiler} candidateDiagnostics.${field}[${index}]`,
      ),
    );
    const duplicateFiles = duplicatedValues(normalizedFiles);
    if (duplicateFiles.length > 0) {
      fail(
        `assertion classification ${compiler} candidateDiagnostics.${field} contains duplicate files: ${duplicateFiles.join(", ")}`,
      );
    }
    if (
      Number.isInteger(diagnostics[countField]) &&
      normalizedFiles.length !== diagnostics[countField]
    ) {
      fail(
        `assertion classification ${compiler} candidateDiagnostics.${field} length (${normalizedFiles.length}) does not match ${countField} (${diagnostics[countField]})`,
      );
    }
    diagnosticFiles[field] = normalizedFiles;
  }
  const overlappingDiagnosticFiles = diagnosticFiles.filesWithDiagnostics
    .filter((file) => diagnosticFiles.filesWithoutDiagnostics.includes(file))
    .sort();
  if (overlappingDiagnosticFiles.length > 0) {
    fail(
      `assertion classification ${compiler} candidateDiagnostics files overlap between diagnostic and diagnostic-free lists: ${overlappingDiagnosticFiles.join(", ")}`,
    );
  }
  normalizedCandidateDiagnosticFiles[compiler] = diagnosticFiles;
  const byCandidate = diagnostics.byCandidate;
  if (byCandidate !== null && byCandidate !== undefined) {
    if (!Array.isArray(byCandidate)) {
      fail(
        `assertion classification ${compiler} candidateDiagnostics.byCandidate must be an array`,
      );
    }
    const exampleFiles = [];
    byCandidate.forEach((entry, index) => {
      if (entry?.file !== null && entry?.file !== undefined) {
        exampleFiles[index] = validateCandidateOutputPath(
          entry.file,
          `assertion classification ${compiler} candidateDiagnostics.byCandidate[${index}].file`,
        );
      }
      if (entry?.candidate?.id !== null && entry?.candidate?.id !== undefined) {
        if (typeof entry.candidate.id !== "string" || entry.candidate.id.trim() === "") {
          fail(
            `assertion classification ${compiler} candidateDiagnostics.byCandidate[${index}].candidate.id must be a non-empty string`,
          );
        }
      }
      if (entry?.errorCount !== null && entry?.errorCount !== undefined) {
        if (!Number.isInteger(entry.errorCount) || entry.errorCount < 0) {
          fail(
            `assertion classification ${compiler} candidateDiagnostics.byCandidate[${index}].errorCount must be a non-negative integer`,
          );
        }
      }
      if (entry?.codes !== null && entry?.codes !== undefined) {
        if (!Array.isArray(entry.codes)) {
          fail(
            `assertion classification ${compiler} candidateDiagnostics.byCandidate[${index}].codes must be an array`,
          );
        }
        entry.codes.forEach((code, codeIndex) => {
          if (typeof code?.key !== "string" || code.key.trim() === "") {
            fail(
              `assertion classification ${compiler} candidateDiagnostics.byCandidate[${index}].codes[${codeIndex}].key must be a non-empty string`,
            );
          }
          if (!Number.isInteger(code.count) || code.count < 0) {
            fail(
              `assertion classification ${compiler} candidateDiagnostics.byCandidate[${index}].codes[${codeIndex}].count must be a non-negative integer`,
            );
          }
        });
      }
      if (entry?.semanticFamilies !== null && entry?.semanticFamilies !== undefined) {
        if (!Array.isArray(entry.semanticFamilies)) {
          fail(
            `assertion classification ${compiler} candidateDiagnostics.byCandidate[${index}].semanticFamilies must be an array`,
          );
        }
        entry.semanticFamilies.forEach((family, familyIndex) => {
          if (typeof family !== "string" || family.trim() === "") {
            fail(
              `assertion classification ${compiler} candidateDiagnostics.byCandidate[${index}].semanticFamilies[${familyIndex}] must be a non-empty string`,
            );
          }
        });
      }
      if (entry?.firstErrors !== null && entry?.firstErrors !== undefined) {
        if (!Array.isArray(entry.firstErrors)) {
          fail(
            `assertion classification ${compiler} candidateDiagnostics.byCandidate[${index}].firstErrors must be an array`,
          );
        }
        entry.firstErrors.forEach((error, errorIndex) => {
          if (error?.line !== null && error?.line !== undefined && !Number.isInteger(error.line)) {
            fail(
              `assertion classification ${compiler} candidateDiagnostics.byCandidate[${index}].firstErrors[${errorIndex}].line must be an integer`,
            );
          }
          if (
            error?.column !== null &&
            error?.column !== undefined &&
            !Number.isInteger(error.column)
          ) {
            fail(
              `assertion classification ${compiler} candidateDiagnostics.byCandidate[${index}].firstErrors[${errorIndex}].column must be an integer`,
            );
          }
          if (error?.code !== null && error?.code !== undefined && typeof error.code !== "string") {
            fail(
              `assertion classification ${compiler} candidateDiagnostics.byCandidate[${index}].firstErrors[${errorIndex}].code must be a string`,
            );
          }
          if (
            error?.message !== null &&
            error?.message !== undefined &&
            typeof error.message !== "string"
          ) {
            fail(
              `assertion classification ${compiler} candidateDiagnostics.byCandidate[${index}].firstErrors[${errorIndex}].message must be a string`,
            );
          }
        });
      }
    });
    normalizedCandidateExampleFiles[compiler] = exampleFiles;
  }
}
if (
  Number.isInteger(tscCandidateDiagnostics.candidatesWithoutDiagnostics) &&
  Number.isInteger(tszCandidateDiagnostics.candidatesWithoutDiagnostics)
) {
  const expectedDiagnosticFreeDelta =
    tszCandidateDiagnostics.candidatesWithoutDiagnostics -
    tscCandidateDiagnostics.candidatesWithoutDiagnostics;
  if (comparison.diagnosticFreeCandidateDelta !== expectedDiagnosticFreeDelta) {
    fail(
      `assertion classification diagnosticFreeCandidateDelta (${comparison.diagnosticFreeCandidateDelta}) does not match tsz/tsc diagnostic-free delta (${expectedDiagnosticFreeDelta})`,
    );
  }
}
if (comparison.bySemanticFamilyDelta !== null && comparison.bySemanticFamilyDelta !== undefined) {
  if (!Array.isArray(comparison.bySemanticFamilyDelta)) {
    fail("assertion classification comparison.bySemanticFamilyDelta must be an array");
  }
  comparison.bySemanticFamilyDelta.forEach((entry, index) => {
    if (typeof entry?.key !== "string" || entry.key.trim() === "") {
      fail(
        `assertion classification comparison.bySemanticFamilyDelta[${index}].key must be a non-empty string`,
      );
    }
    if (!Number.isInteger(entry.delta)) {
      fail(
        `assertion classification comparison.bySemanticFamilyDelta[${index}].delta must be an integer`,
      );
    }
  });
}
const candidateFileComparisonCounts = comparison.candidateFileComparison?.counts || {};
const normalizedCandidateFileComparison = {};
if (comparison.candidateFileComparison) {
  const candidateFileComparisonTotal = comparison.candidateFileComparison.totalCandidates;
  const bucketEntries = [];
  if (!Number.isInteger(candidateFileComparisonTotal)) {
    fail(
      "assertion classification candidateFileComparison.totalCandidates must be an integer",
    );
  }
  if (candidateFileComparisonTotal !== counts.generatedAssertions) {
    fail(
      `assertion classification candidateFileComparison.totalCandidates (${candidateFileComparisonTotal}) does not match generatedAssertions (${counts.generatedAssertions})`,
    );
  }
  let bucketTotal = 0;
  for (const field of [
    "bothAccepted",
    "bothRejected",
    "tscAcceptedTszRejected",
    "tscRejectedTszAccepted",
  ]) {
    const count = candidateFileComparisonCounts[field];
    if (!Number.isInteger(count)) {
      fail(
        `assertion classification candidateFileComparison.counts.${field} must be an integer`,
      );
    }
    const files = comparison.candidateFileComparison[field];
    if (!Array.isArray(files)) {
      fail(
        `assertion classification candidateFileComparison.${field} must be an array`,
      );
    }
    if (files.length !== count) {
      fail(
        `assertion classification candidateFileComparison.${field} length (${files.length}) does not match counts.${field} (${count})`,
      );
    }
    const normalizedFiles = files.map((file, index) => {
      const normalized = validateCandidateOutputPath(
        file,
        `assertion classification candidateFileComparison.${field}[${index}]`,
      );
      bucketEntries.push({ field, file: normalized });
      return normalized;
    });
    normalizedCandidateFileComparison[field] = normalizedFiles;
    const duplicateFiles = duplicatedValues(normalizedFiles);
    if (duplicateFiles.length > 0) {
      fail(
        `assertion classification candidateFileComparison.${field} contains duplicate files: ${duplicateFiles.join(", ")}`,
      );
    }
    bucketTotal += count;
  }
  const duplicateBucketFiles = duplicatedValues(bucketEntries.map((entry) => entry.file));
  if (duplicateBucketFiles.length > 0) {
    const details = duplicateBucketFiles.map((file) => {
      const fields = bucketEntries
        .filter((entry) => entry.file === file)
        .map((entry) => entry.field)
        .join(", ");
      return `${file} (${fields})`;
    });
    fail(
      `assertion classification candidateFileComparison buckets overlap: ${details.join("; ")}`,
    );
  }
  if (bucketTotal !== candidateFileComparisonTotal) {
    fail(
      `assertion classification candidateFileComparison bucket counts (${bucketTotal}) do not match totalCandidates (${candidateFileComparisonTotal})`,
    );
  }
}
const tscFilesWithDiagnostics = Array.isArray(tscCandidateDiagnostics.filesWithDiagnostics)
  ? normalizedCandidateDiagnosticFiles.tsc.filesWithDiagnostics
  : [];
const tszFilesWithDiagnostics = Array.isArray(tszCandidateDiagnostics.filesWithDiagnostics)
  ? normalizedCandidateDiagnosticFiles.tsz.filesWithDiagnostics
  : [];

const rel = (value) => {
  if (!value) return null;
  if (!fixtureRoot || !path.isAbsolute(value)) return value;
  const relative = path.relative(fixtureRoot, value);
  return relative && !relative.startsWith("..") && !path.isAbsolute(relative)
    ? relative.split(path.sep).join("/")
    : value;
};

const exitCode = (result) => Number.isInteger(result.exitCode) ? [result.exitCode] : [];
const normalizeDiagnosticLine = (line) => {
  let text = String(line || "");
  if (fixtureRoot) {
    const normalizedRoot = fixtureRoot.split(path.sep).join("/");
    text = text.split(normalizedRoot).join(".");
  }
  return text;
};
const relCandidateFile = (file) => rel(path.join(candidateDir, file));
const relCandidateFiles = (files) => Array.isArray(files) ? files.map(relCandidateFile) : [];
const candidateFileComparison = comparison.candidateFileComparison
  ? {
      total_candidates: comparison.candidateFileComparison.totalCandidates ?? null,
      counts: comparison.candidateFileComparison.counts ?? {},
      both_accepted: relCandidateFiles(normalizedCandidateFileComparison.bothAccepted),
      both_rejected: relCandidateFiles(normalizedCandidateFileComparison.bothRejected),
      tsc_accepted_tsz_rejected: relCandidateFiles(
        normalizedCandidateFileComparison.tscAcceptedTszRejected,
      ),
      tsc_rejected_tsz_accepted: relCandidateFiles(
        normalizedCandidateFileComparison.tscRejectedTszAccepted,
      ),
    }
  : null;
const candidateExamplesFor = (compiler, result) => {
  const candidates = result.candidateDiagnostics?.byCandidate;
  if (!Array.isArray(candidates)) {
    return [];
  }
  return candidates.slice(0, 5).map((entry, index) => {
    const normalizedFile = normalizedCandidateExampleFiles[compiler]?.[index];
    return {
      compiler,
      file: normalizedFile ? relCandidateFile(normalizedFile) : null,
      candidate_id: entry.candidate?.id ?? null,
      error_count: entry.errorCount ?? null,
      codes: (entry.codes || []).map((code) => code.key).filter(Boolean).slice(0, 5),
      semantic_families: entry.semanticFamilies || [],
      first_error: entry.firstErrors?.[0]
        ? {
            line: entry.firstErrors[0].line ?? null,
            column: entry.firstErrors[0].column ?? null,
            code: entry.firstErrors[0].code ?? null,
            message: entry.firstErrors[0].message ?? null,
          }
        : null,
    };
  });
};
const diagnosticCandidateExamples = [
  ...candidateExamplesFor("tsc", tsc),
  ...candidateExamplesFor("tsz", tsz),
].slice(0, 10);
const diagnosticDeltas = [
  ...(tsc.diagnostics?.firstErrors || []).map((line) => `tsc: ${line}`),
  ...(tsz.diagnostics?.firstErrors || []).map((line) => `tsz: ${line}`),
].map(normalizeDiagnosticLine).slice(0, 20);
const diagnosticCodes = [
  ...(tsc.diagnostics?.byCode || []),
  ...(tsz.diagnostics?.byCode || []),
]
  .map((entry) => entry.key)
  .filter((code, index, codes) => code && codes.indexOf(code) === index)
  .slice(0, 8);
const diagnosticSubsystems = (comparison.bySemanticFamilyDelta || [])
  .map((entry) => ({
    subsystem: `type-challenges ${entry.key}`,
    codes: [],
    count: Math.abs(Number(entry.delta) || 0),
    examples: [],
  }))
  .filter((entry) => entry.count > 0)
  .slice(0, 8);

let state = "yellow";
let exitClass = "diagnostic mismatch";
let diagnosticStatus = comparison.status || "assertion comparison differs";
let firstFailureClass = comparison.status || "assertion comparison differs";
let knownBlockers = ["assertion comparison differs"];

if (tsc.status === "pass" && tsz.status === "pass") {
  state = "green";
  exitClass = "exit success";
  diagnosticStatus = "none";
  firstFailureClass = null;
  knownBlockers = [];
} else if (tsc.status === "fail") {
  state = "gray";
  exitClass = "fixture invalid";
  diagnosticStatus = "tsc assertion corpus failed";
  firstFailureClass = "assertion corpus not tsc-clean";
  knownBlockers = ["assertion corpus not tsc-clean"];
} else if (tsc.status === "unavailable" || tsc.status === "error" || tsc.status === "timeout") {
  state = "gray";
  exitClass = "fixture invalid";
  diagnosticStatus = `tsc ${tsc.status}`;
  firstFailureClass = "tsc oracle unavailable";
  knownBlockers = ["tsc oracle unavailable"];
} else if (tsc.status === "pass" && tsz.status !== "pass") {
  state = "red";
  exitClass = tsz.status === "timeout" ? "timeout" : "nonzero exit";
  diagnosticStatus = "tsz rejects tsc-accepted assertion candidates";
  firstFailureClass = "tsz rejects tsc-accepted assertion candidates";
  knownBlockers = ["tsz rejects tsc-accepted assertion candidates"];
}

const firstDiagnosticFile =
  tscFilesWithDiagnostics[0] || tszFilesWithDiagnostics[0] || null;
const tsconfigPath = rel(path.join(candidateDir, "tsconfig.tsz-guard.json"));
const sourceRoot = rel(path.join(candidateDir, "assertions"));
const row = {
  name: "type-challenges-assertion-candidates",
  state,
  exit_class: exitClass,
  first_failure_class: firstFailureClass,
  owner_track: state === "green" ? null : "Project Direction #7731 Type Challenges assertion gate",
  phase: "assertion-classification",
  last_successful_phase: state === "green" ? "assertion-classification" : null,
  diagnostic_status: diagnosticStatus,
  diagnostic_deltas: diagnosticDeltas,
  diagnostic_subsystems: diagnosticSubsystems,
  primary_subsystem: diagnosticSubsystems[0]?.subsystem || null,
  diagnostic_codes: diagnosticCodes,
  emit_status: "not in scope (noEmit assertion check)",
  dts_status: "not in scope (noEmit assertion check)",
  known_blockers: knownBlockers,
  reduced_repro_path: firstDiagnosticFile ? rel(path.join(candidateDir, firstDiagnosticFile)) : sourceRoot,
  repro: {
    tsconfig_path: tsconfigPath,
    source_root: sourceRoot,
    first_failure_path: firstDiagnosticFile ? rel(path.join(candidateDir, firstDiagnosticFile)) : null,
    first_failure_line: null,
    first_failure_column: null,
    first_failure_code: diagnosticCodes[0] || null,
    reduced_repro_path: firstDiagnosticFile ? rel(path.join(candidateDir, firstDiagnosticFile)) : sourceRoot,
    command: tsconfigPath ? `$TSZ_BIN --noEmit -p ${tsconfigPath}` : null,
  },
  exit_codes: {
    tsc: exitCode(tsc),
    tsz: exitCode(tsz),
    tsgo: [],
  },
  files_reached: Number(
    counts.generatedAssertions ||
      tscCandidateDiagnostics.totalCandidates ||
      tszCandidateDiagnostics.totalCandidates ||
      0,
  ),
  peak_memory_bytes: null,
  assertion_candidates: {
    sources: report.candidateManifest?.sources ?? null,
    paired_solutions: counts.pairedSolutions ?? null,
    generated_assertions: counts.generatedAssertions ?? null,
    assertions_referencing_solution_declaration:
      counts.assertionsReferencingSolutionDeclaration ?? null,
    assertions_missing_solution_declaration_reference:
      counts.assertionsMissingSolutionDeclarationReference ?? null,
    tsc_diagnostic_free: tscCandidateDiagnostics.candidatesWithoutDiagnostics ?? null,
    tsc_with_diagnostics: tscCandidateDiagnostics.candidatesWithDiagnostics ?? null,
    tsz_diagnostic_free: tszCandidateDiagnostics.candidatesWithoutDiagnostics ?? null,
    diagnostic_free_candidate_delta: comparison.diagnosticFreeCandidateDelta ?? null,
    both_accepted: candidateFileComparisonCounts.bothAccepted ?? null,
    both_rejected: candidateFileComparisonCounts.bothRejected ?? null,
    tsc_accepted_tsz_rejected:
      candidateFileComparisonCounts.tscAcceptedTszRejected ?? null,
    tsc_rejected_tsz_accepted:
      candidateFileComparisonCounts.tscRejectedTszAccepted ?? null,
    tsc_clean_subset: cleanSubsetManifest
      ? {
          manifest_path: rel(cleanSubsetManifestPath),
          classification_path: cleanSubsetClassification
            ? rel(cleanSubsetClassificationPath)
            : null,
          tsconfig_path: cleanSubsetDir
            ? rel(path.join(cleanSubsetDir, "tsconfig.tsz-guard.json"))
            : null,
          total_candidates: cleanSubsetManifest.counts?.totalCandidates ?? null,
          generated_assertions: cleanSubsetManifest.counts?.tscAcceptedAssertions ?? null,
          assertions_referencing_solution_declaration:
            cleanSubsetManifest.counts?.tscAcceptedAssertionsReferencingSolutionDeclaration ?? null,
          assertions_missing_solution_declaration_reference:
            cleanSubsetManifest.counts?.tscAcceptedAssertionsMissingSolutionDeclarationReference ?? null,
          rejected_from_full_corpus: cleanSubsetManifest.counts?.tscRejectedAssertions ?? null,
          tsc_status: cleanSubsetClassification?.compilers?.tsc?.status ?? null,
          tsz_status: cleanSubsetClassification?.compilers?.tsz?.status ?? null,
          comparison_status: cleanSubsetClassification?.comparison?.status ?? null,
          tsc_diagnostic_free:
            cleanSubsetClassification?.compilers?.tsc?.candidateDiagnostics?.candidatesWithoutDiagnostics ?? null,
          tsz_diagnostic_free:
            cleanSubsetClassification?.compilers?.tsz?.candidateDiagnostics?.candidatesWithoutDiagnostics ?? null,
        }
      : null,
    file_comparison: candidateFileComparison,
    diagnostic_candidate_examples: diagnosticCandidateExamples,
  },
};

fs.appendFileSync(outFile, `${JSON.stringify(row)}\n`, "utf8");
