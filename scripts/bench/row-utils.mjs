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
