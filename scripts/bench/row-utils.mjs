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
