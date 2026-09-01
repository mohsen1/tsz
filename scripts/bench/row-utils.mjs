import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  computeFixtureStubInventory,
  fixtureStubEvidenceFromInventory,
} from "./lib/fixture-stub-inventory.mjs";
import { PROJECT_ROWS_BY_NAME } from "./project-rows.mjs";

// Fields that must be present in a compatibility object before a row
// can be reported as a speed win. Missing any of these means the artifact
// is incomplete and the row must render as gray/incomplete, not a win.
export const REQUIRED_PHASE_EXIT_FIELDS = [
  "state",
  "phase",
  "last_successful_phase",
  "exit_class",
  "diagnostic_status",
];

export function hasCompletePhaseMetadata(compatibility) {
  return REQUIRED_PHASE_EXIT_FIELDS.every((field) => Object.hasOwn(compatibility, field));
}

const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
let FIXTURE_STUB_INVENTORY = null;
try {
  FIXTURE_STUB_INVENTORY = computeFixtureStubInventory(REPO_ROOT);
} catch {
  // Consumers fail closed below. Keeping the module importable lets single-file
  // benchmarks render even when project inventory evidence is unavailable.
}
const ZERO_STUB_EVIDENCE = fixtureStubEvidenceFromInventory({}, "zero-stub-row");

function exactOrdinaryExit(codes) {
  if (!Array.isArray(codes) || codes.length !== 1) return null;
  const value = Number(codes[0]);
  return Number.isInteger(value) && value >= 0 && value <= 4 ? value : null;
}

// Schema-v2 project evidence proves what TSZ actually admitted and compares it
// with pinned TypeScript 7. Counts alone are insufficient: ordered path-graph
// fingerprints, length-delimited diagnostic-record fingerprints (including
// multiplicity/continuation ownership), and ordinary exit parity must all agree.
export function hasExactFixtureStubEvidence(compatibility, projectName) {
  if (!FIXTURE_STUB_INVENTORY || typeof projectName !== "string" || !projectName) return false;
  let expected;
  try {
    expected = fixtureStubEvidenceFromInventory(FIXTURE_STUB_INVENTORY, projectName);
  } catch {
    return false;
  }
  return compatibility?.stub_inventory_schema === expected.stubInventorySchema
    && compatibility.stubbed_modules === expected.stubbedModules
    && compatibility.stubbed_any_members === expected.stubbedAnyMembers
    && compatibility.stub_inventory_fingerprint === expected.stubInventoryFingerprint
    && JSON.stringify(compatibility.stub_inventory_owners) === JSON.stringify(expected.stubInventoryOwners)
    && expected.stubbedModules === 0
    && expected.stubbedAnyMembers === 0
    && expected.stubInventoryOwners.length === 0;
}

export function hasExactProjectEvidence(compatibility, projectName) {
  if (!compatibility || compatibility.evidence_schema !== 3) return false;
  if (!/^[0-9a-f]{40}$/.test(compatibility.source_commit || "")
    || typeof compatibility.source_dirty !== "boolean"
    || compatibility.source_stable !== true
    || compatibility.compile_input_stable !== true) return false;
  for (const field of [
    "source_tree_fingerprint",
    "evidence_protocol_fingerprint",
    "tsz_binary_sha256",
    "build_manifest_sha256",
    "build_inputs_sha256",
    "build_manifest_binary_sha256",
    "compile_input_fingerprint",
    "oracle_fingerprint",
  ]) {
    if (!SHA256_PATTERN.test(compatibility[field] || "")) return false;
  }
  if (compatibility.build_manifest_binary_sha256 !== compatibility.tsz_binary_sha256) return false;
  if (compatibility.semantic_completion !== "complete") return false;
  if (!hasExactFixtureStubEvidence(compatibility, projectName)) return false;
  const positiveInteger = (value) => Number.isInteger(value) && value > 0;
  const nonnegativeInteger = (value) => Number.isInteger(value) && value >= 0;
  if (!positiveInteger(compatibility.root_files)
    || !positiveInteger(compatibility.source_files)
    || compatibility.root_files !== compatibility.oracle_root_files
    || compatibility.source_files !== compatibility.oracle_source_files
    || compatibility.files_reached !== compatibility.source_files) {
    return false;
  }
  for (const [actual, expected] of [
    [compatibility.root_file_fingerprint, compatibility.oracle_root_file_fingerprint],
    [compatibility.source_file_fingerprint, compatibility.oracle_source_file_fingerprint],
    [compatibility.diagnostic_fingerprint, compatibility.oracle_diagnostic_fingerprint],
  ]) {
    if (!SHA256_PATTERN.test(actual || "") || actual !== expected) return false;
  }
  if (!nonnegativeInteger(compatibility.diagnostic_records)
    || compatibility.diagnostic_records !== compatibility.oracle_diagnostic_records) {
    return false;
  }
  const tszExit = exactOrdinaryExit(compatibility.exit_codes?.tsz);
  const tscExit = exactOrdinaryExit(compatibility.exit_codes?.tsc);
  if (tszExit === null || tscExit === null || tszExit !== tscExit) return false;
  return compatibility.oracle_classification === "both-pass"
    || compatibility.oracle_classification === "both-fail-same";
}

// The declared row names (from any benchmark_set/guard_set/corpus name list)
// that have no result row in `rows`, preserving `declaredNames` order. This is
// the raw "declared minus measured" primitive shared by the corpus advisory, the
// required-coverage signal, and the readiness gate. It applies NO all-absent
// guard: a caller that must not flag a shard carrying none of the declared rows
// (a standalone/timing-only shard) applies that guard itself.
export function missingDeclaredRows(declaredNames, rows) {
  const measuredNames = new Set((rows ?? []).map((row) => row?.name).filter(Boolean));
  return declaredNames.filter((name) => !measuredNames.has(name));
}

// A row is green when it succeeded (no status error, not artifact_missing) and
// either has no compatibility object at all (single-file rows are always
// eligible) or has a complete green compatibility object.
export function isGreen(row) {
  if (row.status) return false;
  if (row.artifact_missing === true) return false;
  const compatibility = row.compatibility;
  if (!compatibility) return !Object.hasOwn(PROJECT_ROWS_BY_NAME, String(row?.name || ""));
  if (!hasCompletePhaseMetadata(compatibility)) return false;
  return (
    hasExactProjectEvidence(compatibility, row.name) &&
    compatibility.state === "green" &&
    compatibility.exit_class === "exit success" &&
    compatibility.diagnostic_status === "none"
  );
}

// A row has incomplete compatibility metadata when the run succeeded (no
// status error) but the compatibility artifact was missing or lacked one of
// the required phase/exit fields.
export function isIncompleteCompat(row) {
  if (row.status) return false;
  if (row.artifact_missing === true) return true;
  const compatibility = row.compatibility;
  if (!compatibility) return Object.hasOwn(PROJECT_ROWS_BY_NAME, String(row?.name || ""));
  return !hasCompletePhaseMetadata(compatibility);
}

function hasNonZeroExitCode(codes) {
  if (codes == null) return false;
  const list = Array.isArray(codes) ? codes : [codes];
  return list.some((code) => {
    const value = Number(code);
    return Number.isFinite(value) && value !== 0;
  });
}

// A row "did not finish" when a speed ratio between tsz and tsgo would be
// fabricated rather than measured: at least one compiler was killed at the
// timeout ceiling or exited non-zero, so its recorded wall time is a
// ceiling/error sentinel and any `tsz_ms`/`tsgo_ms` ratio derived from it is
// `ceiling / other_time`, containing no measurement of the losing side. Such a
// row must render as DNF and never contribute a per-row ratio or an aggregate
// datapoint (see #16196: a killed `large-ts-repo` row reported "42.99x faster"
// that was exactly `1500s / tsgo_time`, and three "narrowing" datapoints that
// tracked only tsgo's own runtime drift against the fixed ceiling).
//
// Keyed entirely on flags the row data already carries, so the exclusion is
// structural rather than incidental to the slowdown-failure heuristic: the
// merge step's explicit `winner: "error"` stub, the compatibility artifact's
// `exit_class` (`timeout`/`nonzero exit`), or a non-zero recorded exit code for
// either compiler.
export function didNotFinish(row) {
  if (!row) return false;
  if (row.winner === "error") return true;
  const compatibility = row.compatibility;
  if (!compatibility) return false;
  if (compatibility.exit_class === "timeout" || compatibility.exit_class === "nonzero exit") {
    return true;
  }
  const exitCodes = compatibility.exit_codes;
  if (exitCodes && (hasNonZeroExitCode(exitCodes.tsz) || hasNonZeroExitCode(exitCodes.tsgo))) {
    return true;
  }
  return false;
}

// True when `value` is a positive, finite wall-clock time in ms. Missing and
// sentinel timings (`null`/`undefined`/`""`/non-numeric, or a `<= 0` value)
// coerce to a non-positive or non-finite number and are rejected, so a row
// carrying a ceiling/error sentinel or an absent timing never reads as a real
// measurement. This is the single definition of "a usable timing"; the
// open-coded `hasTiming` / `Number.isFinite(x) && x > 0` timing checks that
// gated the speed-ratio surfaces route through it. (Helpers with a different
// contract stay put: `finiteNumber` returns `number | null` for width/format
// math, and the aggregate-mean label intentionally admits a zero denominator.)
export function isPositiveFiniteTiming(value) {
  const time = Number(value);
  return Number.isFinite(time) && time > 0;
}

// Canonical "successful timing pair" gate. A row is eligible to contribute a
// *measured* tsz/tsgo speed ratio — a per-row win or an aggregate datapoint —
// only when the run itself did not fail (`!status`), the row actually finished
// (`!didNotFinish`, which subsumes `winner === "error"` and the timeout /
// nonzero-exit ceiling sentinels — see it for the #16196 rationale), and BOTH
// compilers recorded a positive finite wall time.
//
// Every readiness gate, artifact-selection count, website chart, and README
// headline routes its eligibility question through this one predicate so that
// a did-not-finish row can never leak a fabricated `ceiling / other_time` ratio
// into any surface, and so "eligible for a speed ratio" cannot come to mean
// different things depending on which script asks (#16196, #17302).
export function isSpeedRatioEligible(row) {
  if (!row || row.status) return false;
  if (didNotFinish(row)) return false;
  const knownProject = Object.hasOwn(PROJECT_ROWS_BY_NAME, String(row.name || ""));
  if (knownProject && !row.compatibility) return false;
  if (row.compatibility && !hasExactProjectEvidence(row.compatibility, row.name)) return false;
  return isPositiveFiniteTiming(row.tsz_ms) && isPositiveFiniteTiming(row.tsgo_ms);
}

// The slowdown-failure threshold shared by the bench harness
// (`scripts/bench/project-fixtures.sh tsz_project_slowdown_failure_factor`),
// the website speed chart, and the README headline: a row where tsz is at or
// beyond `factor`x tsgo is a speed failure that must not render or count.
export const SLOWDOWN_FAILURE_FACTOR = 1.5;

// Canonical chart / headline gate: a row that is speed-ratio-eligible AND fast
// enough to chart (tsz strictly under `factor`x tsgo). Layered on the base gate
// so a did-not-finish row whose short-ceiling sentinel happens to land under
// the threshold still cannot leak into the chart or the headline average.
export function isSpeedChartEligible(row, factor = SLOWDOWN_FAILURE_FACTOR) {
  if (!isSpeedRatioEligible(row)) return false;
  return Number(row.tsz_ms) < factor * Number(row.tsgo_ms);
}

export const GREEN_COMPAT = {
  state: "green",
  phase: "check",
  last_successful_phase: "check",
  exit_class: "exit success",
  diagnostic_status: "none",
  evidence_schema: 3,
  evidence_status: "exact",
  evidence_failures: [],
  source_commit: "1".repeat(40),
  source_dirty: false,
  source_stable: true,
  source_tree_fingerprint: "2".repeat(64),
  evidence_protocol_fingerprint: "8".repeat(64),
  tsz_binary_sha256: "3".repeat(64),
  build_manifest_sha256: "4".repeat(64),
  build_inputs_sha256: "5".repeat(64),
  build_manifest_binary_sha256: "3".repeat(64),
  compile_input_fingerprint: "6".repeat(64),
  compile_input_stable: true,
  oracle_fingerprint: "7".repeat(64),
  semantic_completion: "complete",
  root_files: 1,
  source_files: 1,
  root_file_fingerprint: "a".repeat(64),
  source_file_fingerprint: "b".repeat(64),
  oracle_root_files: 1,
  oracle_source_files: 1,
  oracle_root_file_fingerprint: "a".repeat(64),
  oracle_source_file_fingerprint: "b".repeat(64),
  diagnostic_records: 0,
  diagnostic_fingerprint: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  oracle_diagnostic_records: 0,
  oracle_diagnostic_fingerprint: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  stub_inventory_schema: ZERO_STUB_EVIDENCE.stubInventorySchema,
  stubbed_modules: ZERO_STUB_EVIDENCE.stubbedModules,
  stubbed_any_members: ZERO_STUB_EVIDENCE.stubbedAnyMembers,
  stub_inventory_fingerprint: ZERO_STUB_EVIDENCE.stubInventoryFingerprint,
  stub_inventory_owners: ZERO_STUB_EVIDENCE.stubInventoryOwners,
  oracle_classification: "both-pass",
  files_reached: 1,
  exit_codes: { tsc: [0], tsz: [0] },
};

export const YELLOW_COMPAT = {
  state: "yellow",
  phase: "check",
  last_successful_phase: "check",
  exit_class: "exit success",
  diagnostic_status: "diagnostic mismatch",
};

export const RED_COMPAT = {
  state: "red",
  phase: "check",
  last_successful_phase: null,
  exit_class: "nonzero exit",
  diagnostic_status: "compiler error",
};
