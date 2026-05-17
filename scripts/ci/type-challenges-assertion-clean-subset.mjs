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

function validateInputs(candidateManifest, classification) {
  if (candidateManifest?.fixture !== "type-challenges-assertion-candidates") {
    fail(`unexpected assertion candidate manifest fixture: ${candidateManifest?.fixture || "<missing>"}`);
  }
  if (!Array.isArray(candidateManifest.entries)) {
    fail("assertion candidate manifest entries must be an array");
  }
  if (classification?.fixture !== "type-challenges-assertion-classification") {
    fail(`unexpected assertion classification fixture: ${classification?.fixture || "<missing>"}`);
  }
  if (!classification.compilers?.tsc) {
    fail("assertion classification must include a tsc compiler result");
  }

  const entries = candidateManifest.entries.map((entry, index) => ({
    ...entry,
    output: normalizeManifestPath(entry?.output, `candidate manifest entries[${index}].output`),
  }));
  const generatedAssertions = candidateManifest.counts?.generatedAssertions;
  if (
    generatedAssertions !== undefined &&
    (!Number.isInteger(generatedAssertions) || generatedAssertions !== entries.length)
  ) {
    fail(
      `candidate manifest counts.generatedAssertions (${generatedAssertions}) does not match entries length (${entries.length})`,
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
