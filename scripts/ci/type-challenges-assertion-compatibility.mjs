#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const [, , classificationPath, candidateDir, outFile, fixtureRoot = ""] = process.argv;

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

function validateReport(report) {
  if (report?.fixture !== "type-challenges-assertion-classification") {
    fail(`unexpected assertion classification fixture: ${report?.fixture || "<missing>"}`);
  }
  if (!report.compilers || typeof report.compilers !== "object") {
    fail("assertion classification report is missing compilers");
  }
  if (!report.compilers.tsc || !report.compilers.tsz) {
    fail("assertion classification report must include both tsc and tsz compiler results");
  }
  if (!report.comparison || typeof report.comparison !== "object") {
    fail("assertion classification report is missing comparison");
  }
  if (!report.candidateManifest || typeof report.candidateManifest !== "object") {
    fail("assertion classification report is missing candidateManifest");
  }
}

validateReport(report);

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
  },
};

fs.appendFileSync(outFile, `${JSON.stringify(row)}\n`, "utf8");
