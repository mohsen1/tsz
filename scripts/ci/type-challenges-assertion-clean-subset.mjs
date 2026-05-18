#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const [
  candidateDir,
  candidateManifestPath,
  classificationPath,
  outputDir,
  subsetManifestPath,
] = process.argv.slice(2);

if (
  !candidateDir ||
  !candidateManifestPath ||
  !classificationPath ||
  !outputDir ||
  !subsetManifestPath
) {
  console.error(
    "usage: type-challenges-assertion-clean-subset.mjs <candidate-dir> <candidate-manifest.json> <classification.json> <output-dir> <subset-manifest.json>",
  );
  process.exit(2);
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function fail(message) {
  console.error(`error: ${message}`);
  process.exit(1);
}

function normalizeManifestPath(value, label) {
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

function normalizeManifestId(value, label) {
  if (typeof value !== "string" || value.trim() === "") {
    fail(`${label} must be a non-empty string`);
  }
  return value;
}

function validateEvidencePath(value, label) {
  if (typeof value !== "string" || value.trim() === "") {
    fail(`${label} must be a non-empty relative path`);
  }
  const normalized = value.split(/[\\/]+/).join("/").replace(/^\.\//, "");
  const segments = normalized.split("/");
  if (
    path.isAbsolute(value) ||
    /^[A-Za-z]:\//.test(normalized) ||
    normalized === "" ||
    normalized === "." ||
    segments.some((segment) => segment === "." || segment === "..")
  ) {
    fail(`${label} must be a relative source path: ${value}`);
  }
}

function validateSourceMetadata(source, label) {
  if (source?.repository && source?.ref) {
    return;
  }

  fail(
    [
      `candidate manifest sources.${label} is missing source metadata`,
      `${source?.repository || "<missing repository>"} @ ${source?.ref || "<missing ref>"}`,
    ].join("\n"),
  );
}

function validateCandidateManifestSources(manifest) {
  if (!manifest?.sources || typeof manifest.sources !== "object") {
    fail("candidate manifest is missing sources");
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
        "candidate manifest template and test-case sources come from different snapshots",
        `templates: ${templateSource.repository} @ ${templateSource.ref}`,
        `testCases: ${testCaseSource.repository} @ ${testCaseSource.ref}`,
      ].join("\n"),
    );
  }
}

function validateClassificationManifestSources(candidateManifest, classificationManifest) {
  if (!classificationManifest?.sources || typeof classificationManifest.sources !== "object") {
    fail("classification candidateManifest is missing sources");
  }

  for (const label of ["templates", "testCases", "solutions"]) {
    const candidateSource = candidateManifest.sources[label];
    const classificationSource = classificationManifest.sources[label];
    if (!classificationSource?.repository || !classificationSource?.ref) {
      fail(
        [
          `classification candidateManifest.sources.${label} is missing source metadata`,
          `${classificationSource?.repository || "<missing repository>"} @ ${
            classificationSource?.ref || "<missing ref>"
          }`,
        ].join("\n"),
      );
    }
    if (
      classificationSource.repository !== candidateSource.repository ||
      classificationSource.ref !== candidateSource.ref
    ) {
      fail(
        `classification candidateManifest.sources.${label} (${classificationSource.repository} @ ${classificationSource.ref}) does not match candidate manifest sources.${label} (${candidateSource.repository} @ ${candidateSource.ref})`,
      );
    }
  }
}

function validateSelectedEntryEvidence(entry, index) {
  validateEvidencePath(entry?.solution?.output, `selected entries[${index}].solution.output`);
  validateEvidencePath(entry?.solution?.source, `selected entries[${index}].solution.source`);
  validateEvidencePath(entry?.template?.output, `selected entries[${index}].template.output`);
  validateEvidencePath(entry?.template?.source, `selected entries[${index}].template.source`);
  validateEvidencePath(entry?.testCase?.output, `selected entries[${index}].testCase.output`);
  validateEvidencePath(entry?.testCase?.source, `selected entries[${index}].testCase.source`);

  const referencedDeclarations = entry?.assertion?.referencedSolutionDeclarations;
  if (!Array.isArray(referencedDeclarations)) {
    fail(`selected entries[${index}].assertion.referencedSolutionDeclarations must be an array`);
  }
  if (
    referencedDeclarations.some(
      (name) => typeof name !== "string" || name.trim() === "",
    )
  ) {
    fail(
      `selected entries[${index}].assertion.referencedSolutionDeclarations must contain only non-empty strings`,
    );
  }

  const hasReferencedDeclaration = entry?.assertion?.hasReferencedSolutionDeclaration;
  if (typeof hasReferencedDeclaration !== "boolean") {
    fail(`selected entries[${index}].assertion.hasReferencedSolutionDeclaration must be a boolean`);
  }
  if (hasReferencedDeclaration !== (referencedDeclarations.length > 0)) {
    fail(
      `selected entries[${index}].assertion declaration-reference metadata is inconsistent`,
    );
  }
}

function validateInputs(candidateManifest, classification) {
  if (candidateManifest?.fixture !== "type-challenges-assertion-candidates") {
    fail(`unexpected assertion candidate manifest fixture: ${candidateManifest?.fixture || "<missing>"}`);
  }
  if (!Array.isArray(candidateManifest.entries)) {
    fail("assertion candidate manifest entries must be an array");
  }
  if (candidateManifest.entries.length === 0) {
    fail("assertion candidate manifest entries must include at least one assertion candidate");
  }
  if (classification?.fixture !== "type-challenges-assertion-classification") {
    fail(`unexpected assertion classification fixture: ${classification?.fixture || "<missing>"}`);
  }
  if (!classification.compilers?.tsc) {
    fail("assertion classification must include a tsc compiler result");
  }

  const entries = candidateManifest.entries.map((entry, index) => ({
    ...entry,
    id: normalizeManifestId(entry?.id, `candidate manifest entries[${index}].id`),
    output: normalizeManifestPath(entry?.output, `candidate manifest entries[${index}].output`),
  }));
  const duplicateIds = duplicates(entries.map((entry) => entry.id));
  if (duplicateIds.length > 0) {
    reportFileSetError(
      "assertion candidate manifest reported duplicate candidate ids",
      duplicateIds,
    );
  }
  const duplicateOutputs = duplicates(entries.map((entry) => entry.output));
  if (duplicateOutputs.length > 0) {
    reportFileSetError(
      "assertion candidate manifest reported duplicate candidate outputs",
      duplicateOutputs,
    );
  }
  const generatedAssertions = candidateManifest.counts?.generatedAssertions;
  if (
    !Number.isInteger(generatedAssertions) ||
    generatedAssertions !== entries.length
  ) {
    fail(
      `candidate manifest counts.generatedAssertions (${generatedAssertions}) does not match entries length (${entries.length})`,
    );
  }

  if (classification.candidateManifest?.fixture !== candidateManifest.fixture) {
    fail(
      `classification candidate manifest fixture (${classification.candidateManifest?.fixture || "<missing>"}) does not match candidate manifest fixture (${candidateManifest.fixture})`,
    );
  }

  const classifiedGeneratedAssertions =
    classification.candidateManifest?.counts?.generatedAssertions;
  if (classifiedGeneratedAssertions !== generatedAssertions) {
    fail(
      `classification candidate manifest counts.generatedAssertions (${classifiedGeneratedAssertions}) does not match candidate manifest count (${generatedAssertions})`,
    );
  }

  return {
    candidateManifest: {
      ...candidateManifest,
      entries,
    },
    classification,
  };
}

function copyRequiredFile(from, to, label) {
  if (!fs.existsSync(from)) {
    console.error(`error: ${label} does not exist: ${from}`);
    process.exit(1);
  }
  fs.mkdirSync(path.dirname(to), { recursive: true });
  fs.copyFileSync(from, to);
}

const {
  candidateManifest,
  classification,
} = validateInputs(readJson(candidateManifestPath), readJson(classificationPath));
const tsc = classification.compilers?.tsc ?? {};
const tscCandidateDiagnostics = tsc.candidateDiagnostics ?? {};
const tscAcceptedFileList = Array.isArray(tscCandidateDiagnostics.filesWithoutDiagnostics)
  ? tscCandidateDiagnostics.filesWithoutDiagnostics
  : [];
const tscRejectedFileList = Array.isArray(tscCandidateDiagnostics.filesWithDiagnostics)
  ? tscCandidateDiagnostics.filesWithDiagnostics
  : [];
const tscAcceptedFiles = new Set(tscAcceptedFileList);
const tscRejectedFiles = new Set(tscRejectedFileList);
const originalEntries = candidateManifest.entries ?? [];
const originalOutputs = new Set(originalEntries.map((entry) => entry.output));

function reportFileSetError(summary, files) {
  console.error(
    [
      `error: ${summary}:`,
      ...files.map((file) => `  - ${file}`),
    ].join("\n"),
  );
  process.exit(1);
}

function duplicates(files) {
  const seen = new Set();
  return files
    .filter((file) => {
      if (seen.has(file)) return true;
      seen.add(file);
      return false;
    })
    .filter((file, index, all) => all.indexOf(file) === index)
    .sort();
}

function validateProvidedDiagnosticCount(value, expected, label) {
  if (value === null || value === undefined) {
    return;
  }
  if (!Number.isInteger(value)) {
    fail(`${label} must be an integer`);
  }
  if (value !== expected) {
    fail(`${label} (${value}) does not match ${expected}`);
  }
}

validateProvidedDiagnosticCount(
  tscCandidateDiagnostics.totalCandidates,
  originalEntries.length,
  "assertion classifier tsc candidateDiagnostics.totalCandidates",
);
validateProvidedDiagnosticCount(
  tscCandidateDiagnostics.candidatesWithoutDiagnostics,
  tscAcceptedFileList.length,
  "assertion classifier tsc candidateDiagnostics.candidatesWithoutDiagnostics",
);
validateProvidedDiagnosticCount(
  tscCandidateDiagnostics.candidatesWithDiagnostics,
  tscRejectedFileList.length,
  "assertion classifier tsc candidateDiagnostics.candidatesWithDiagnostics",
);

const duplicateAcceptedFiles = duplicates(tscAcceptedFileList);
if (duplicateAcceptedFiles.length > 0) {
  reportFileSetError(
    "assertion classifier reported duplicate tsc-clean candidate files",
    duplicateAcceptedFiles,
  );
}

const duplicateRejectedFiles = duplicates(tscRejectedFileList);
if (duplicateRejectedFiles.length > 0) {
  reportFileSetError(
    "assertion classifier reported duplicate tsc-diagnostic candidate files",
    duplicateRejectedFiles,
  );
}

const filesBothAcceptedAndRejected = [...tscAcceptedFiles]
  .filter((file) => tscRejectedFiles.has(file))
  .sort();
if (filesBothAcceptedAndRejected.length > 0) {
  reportFileSetError(
    "assertion classifier reported candidates as both tsc-clean and tsc-diagnostic",
    filesBothAcceptedAndRejected,
  );
}

const missingAcceptedFiles = [...tscAcceptedFiles]
  .filter((file) => !originalOutputs.has(file))
  .sort();
if (missingAcceptedFiles.length > 0) {
  reportFileSetError(
    "assertion classifier reported tsc-clean files missing from the candidate manifest",
    missingAcceptedFiles,
  );
}

const missingRejectedFiles = [...tscRejectedFiles]
  .filter((file) => !originalOutputs.has(file))
  .sort();
if (missingRejectedFiles.length > 0) {
  reportFileSetError(
    "assertion classifier reported tsc-diagnostic files missing from the candidate manifest",
    missingRejectedFiles,
  );
}

if (tsc.status === "pass" || tsc.status === "fail") {
  const unclassifiedFiles = [...originalOutputs]
    .filter((file) => !tscAcceptedFiles.has(file) && !tscRejectedFiles.has(file))
    .sort();
  if (unclassifiedFiles.length > 0) {
    reportFileSetError(
      "assertion classifier did not classify every candidate file with tsc diagnostics",
      unclassifiedFiles,
    );
  }
}

const selectedEntries = originalEntries.filter((entry) => tscAcceptedFiles.has(entry.output));
selectedEntries.forEach(validateSelectedEntryEvidence);
validateCandidateManifestSources(candidateManifest);
validateClassificationManifestSources(candidateManifest, classification.candidateManifest);
const selectedEntriesReferencingSolutionDeclaration = selectedEntries.filter(
  (entry) => entry.assertion?.hasReferencedSolutionDeclaration === true,
);
const selectedEntriesMissingSolutionDeclarationReference = selectedEntries.filter(
  (entry) => entry.assertion?.hasReferencedSolutionDeclaration !== true,
);

fs.rmSync(outputDir, { recursive: true, force: true });
fs.mkdirSync(path.join(outputDir, "assertions"), { recursive: true });
fs.mkdirSync(path.join(outputDir, "utils"), { recursive: true });

copyRequiredFile(
  path.join(candidateDir, "utils", "index.d.ts"),
  path.join(outputDir, "utils", "index.d.ts"),
  "Type Challenges assertion utils",
);

for (const entry of selectedEntries) {
  copyRequiredFile(
    path.join(candidateDir, entry.output),
    path.join(outputDir, entry.output),
    `Type Challenges accepted assertion ${entry.output}`,
  );
}

const manifest = {
  fixture: "type-challenges-assertions-tsc-clean",
  sources: candidateManifest.sources,
  sourceCandidateManifest: {
    fixture: candidateManifest.fixture,
    counts: candidateManifest.counts,
  },
  sourceClassification: {
    fixture: classification.fixture,
    tscStatus: tsc.status ?? null,
    tszStatus: classification.compilers?.tsz?.status ?? null,
    comparisonStatus: classification.comparison?.status ?? null,
  },
  selection: {
    acceptedBy: "tsc",
    rejectedByTsc: tscRejectedFileList,
    missingAcceptedManifestEntries: missingAcceptedFiles,
  },
  counts: {
    totalCandidates: originalEntries.length,
    tscAcceptedAssertions: selectedEntries.length,
    tscAcceptedAssertionsReferencingSolutionDeclaration:
      selectedEntriesReferencingSolutionDeclaration.length,
    tscAcceptedAssertionsMissingSolutionDeclarationReference:
      selectedEntriesMissingSolutionDeclarationReference.length,
    tscRejectedAssertions: Array.isArray(tscCandidateDiagnostics.filesWithDiagnostics)
      ? tscRejectedFileList.length
      : null,
    missingAcceptedManifestEntries: missingAcceptedFiles.length,
  },
  entries: selectedEntries,
};

fs.mkdirSync(path.dirname(subsetManifestPath), { recursive: true });
fs.writeFileSync(subsetManifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

fs.writeFileSync(
  path.join(outputDir, "tsconfig.tsz-guard.json"),
  `${JSON.stringify(
    {
      compilerOptions: {
        target: "es2017",
        lib: ["ESNext"],
        module: "commonjs",
        moduleResolution: "node",
        strict: true,
        noEmit: true,
        types: [],
        noImplicitReturns: true,
        noUnusedLocals: false,
        noUnusedParameters: false,
        esModuleInterop: true,
        skipLibCheck: true,
        ignoreDeprecations: "6.0",
        baseUrl: ".",
        paths: {
          "@type-challenges/utils": ["utils/index.d.ts"],
        },
      },
      include: ["assertions/**/*.ts", "utils/index.d.ts"],
    },
    null,
    2,
  )}\n`,
);

console.log(
  [
    `materialized ${manifest.counts.tscAcceptedAssertions} tsc-clean Type Challenges assertion candidates`,
    `from ${manifest.counts.totalCandidates} total candidates`,
    `manifest: ${path.relative(process.cwd(), subsetManifestPath).split(path.sep).join("/")}`,
  ].join("\n"),
);
