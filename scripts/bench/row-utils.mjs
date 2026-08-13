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
  if (!compatibility) return true;
  if (!hasCompletePhaseMetadata(compatibility)) return false;
  return (
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
  if (!compatibility) return false;
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
