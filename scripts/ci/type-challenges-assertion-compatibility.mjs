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

function validateReport(report) {
  validateClassificationCompilerReport(report);
  if (!report.comparison || typeof report.comparison !== "object") {
    fail("assertion classification report is missing comparison");
  }
  if (!report.candidateManifest || typeof report.candidateManifest !== "object") {
    fail("assertion classification report is missing candidateManifest");
  }
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
  if (
    Number.isInteger(rejectedAssertions) &&
    Number.isInteger(totalCandidates) &&
    acceptedAssertions + rejectedAssertions !== totalCandidates
  ) {
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
}
if (cleanSubsetManifest && cleanSubsetClassification) {
  const acceptedAssertions = cleanSubsetManifest.counts.tscAcceptedAssertions;
  const generatedAssertions =
    cleanSubsetClassification.candidateManifest?.counts?.generatedAssertions;
  if (
    Number.isInteger(generatedAssertions) &&
    generatedAssertions !== acceptedAssertions
  ) {
    fail(
      `tsc-clean assertion classification generatedAssertions (${generatedAssertions}) does not match manifest tscAcceptedAssertions (${acceptedAssertions})`,
    );
  }
  for (const compiler of ["tsc", "tsz"]) {
    const totalCandidates =
      cleanSubsetClassification.compilers?.[compiler]?.candidateDiagnostics?.totalCandidates;
    if (
      Number.isInteger(totalCandidates) &&
      totalCandidates !== acceptedAssertions
    ) {
      fail(
        `tsc-clean assertion classification ${compiler} totalCandidates (${totalCandidates}) does not match manifest tscAcceptedAssertions (${acceptedAssertions})`,
      );
    }
  }
}
const tsc = report.compilers?.tsc || {};
const tsz = report.compilers?.tsz || {};
const comparison = report.comparison || {};
const counts = report.candidateManifest?.counts || {};
const tscCandidateDiagnostics = tsc.candidateDiagnostics || {};
const tszCandidateDiagnostics = tsz.candidateDiagnostics || {};
const candidateFileComparisonCounts = comparison.candidateFileComparison?.counts || {};
const tscFilesWithDiagnostics = Array.isArray(tscCandidateDiagnostics.filesWithDiagnostics)
  ? tscCandidateDiagnostics.filesWithDiagnostics
  : [];
const tszFilesWithDiagnostics = Array.isArray(tszCandidateDiagnostics.filesWithDiagnostics)
  ? tszCandidateDiagnostics.filesWithDiagnostics
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
      both_accepted: relCandidateFiles(comparison.candidateFileComparison.bothAccepted),
      both_rejected: relCandidateFiles(comparison.candidateFileComparison.bothRejected),
      tsc_accepted_tsz_rejected: relCandidateFiles(
        comparison.candidateFileComparison.tscAcceptedTszRejected,
      ),
      tsc_rejected_tsz_accepted: relCandidateFiles(
        comparison.candidateFileComparison.tscRejectedTszAccepted,
      ),
    }
  : null;
const candidateExamplesFor = (compiler, result) => {
  const candidates = result.candidateDiagnostics?.byCandidate;
  if (!Array.isArray(candidates)) {
    return [];
  }
  return candidates.slice(0, 5).map((entry) => ({
    compiler,
    file: entry.file ? relCandidateFile(entry.file) : null,
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
  }));
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
    paired_solutions: counts.pairedSolutions ?? null,
    generated_assertions: counts.generatedAssertions ?? null,
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
          generated_assertions: cleanSubsetManifest.counts?.tscAcceptedAssertions ?? null,
          rejected_from_full_corpus: cleanSubsetManifest.counts?.tscRejectedAssertions ?? null,
          tsc_status: cleanSubsetClassification?.compilers?.tsc?.status ?? null,
          tsz_status: cleanSubsetClassification?.compilers?.tsz?.status ?? null,
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
