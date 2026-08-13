import fs from "node:fs";

import {
  PERF_TIMED_PROJECT_ROWS,
  PROJECT_ROW_DEFINITIONS,
  PROJECT_ROWS_BY_NAME,
} from "./project-rows.mjs";
import {
  isGreen,
  isSpeedRatioEligible,
} from "./row-utils.mjs";

export function readBenchmarkArtifact(file) {
  try {
    const data = JSON.parse(fs.readFileSync(file, "utf8"));
    return Array.isArray(data?.results) && data.results.length > 0 ? data : null;
  } catch {
    return null;
  }
}

export function benchmarkGeneratedAtMs(data) {
  const timestamp = Date.parse(data?.generated_at ?? "");
  return Number.isFinite(timestamp) ? timestamp : Number.NEGATIVE_INFINITY;
}

const APPLICATION_PROJECT_ROW_NAMES = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.category === "application")
  .map((row) => row.name);
const PERF_TIMED_PROJECT_ROW_NAMES = new Set(PERF_TIMED_PROJECT_ROWS);

export function successfulProjectTimingPairCount(data) {
  return (Array.isArray(data?.results) ? data.results : []).filter((row) => (
    Object.hasOwn(PROJECT_ROWS_BY_NAME, String(row?.name || "")) &&
    // Shared bench gate (`row-utils.mjs`): measured timing pair, run did not
    // fail, and the row actually finished — the did-not-finish guard this
    // count previously lacked (#17302) — on top of the green-compat check.
    isSpeedRatioEligible(row) &&
    isGreen(row)
  )).length;
}

export function hasSuccessfulProjectTimingPairs(data, minimum = 1) {
  return successfulProjectTimingPairCount(data) >= minimum;
}

function hasGreenCompatibilityEvidence(row) {
  const compatibility = row?.compatibility;
  if (!compatibility || typeof compatibility !== "object") return false;
  const state = String(compatibility.state || "").toLowerCase();
  const exitClass = String(compatibility.exit_class || "").toLowerCase();
  const diagnosticStatus = String(compatibility.diagnostic_status || "").toLowerCase();
  return state === "green"
    && exitClass === "exit success"
    && (!diagnosticStatus || diagnosticStatus === "none");
}

export function greenProjectTimingPairGapCount(data) {
  return (Array.isArray(data?.results) ? data.results : []).filter((row) => (
    PERF_TIMED_PROJECT_ROW_NAMES.has(String(row?.name || "")) &&
    hasGreenCompatibilityEvidence(row) &&
    // `isSpeedRatioEligible` (row-utils.mjs) adds the did-not-finish guard this
    // gap count previously lacked (#17302), so a green-compat row killed at the
    // ceiling is correctly reported as a missing timing pair.
    !isSpeedRatioEligible(row)
  )).length;
}

export function hasGreenProjectTimingPairs(data) {
  return greenProjectTimingPairGapCount(data) === 0;
}

export function applicationCompatibilityRowCount(data) {
  const rowsByName = new Map(
    (Array.isArray(data?.results) ? data.results : [])
      .filter((row) => row?.name)
      .map((row) => [row.name, row]),
  );
  return APPLICATION_PROJECT_ROW_NAMES.filter((name) => {
    const compatibility = rowsByName.get(name)?.compatibility;
    return compatibility && typeof compatibility === "object" && Object.keys(compatibility).length > 0;
  }).length;
}

export function hasApplicationCompatibilityRows(data) {
  return applicationCompatibilityRowCount(data) === APPLICATION_PROJECT_ROW_NAMES.length;
}

export function selectLatestBenchmarkArtifact(files, options = {}) {
  const minimumProjectTimingPairs = Math.max(0, Number(options.minimumProjectTimingPairs ?? 0));
  const requireApplicationCompat = options.requireApplicationCompat === true;
  const requireGreenProjectTimingPairs = options.requireGreenProjectTimingPairs === true;
  const candidates = [];
  for (const [index, file] of files.entries()) {
    const data = readBenchmarkArtifact(file);
    if (!data) continue;
    const projectTimingPairs = successfulProjectTimingPairCount(data);
    if (projectTimingPairs < minimumProjectTimingPairs) continue;
    const applicationCompatibilityRows = applicationCompatibilityRowCount(data);
    if (requireApplicationCompat && applicationCompatibilityRows < APPLICATION_PROJECT_ROW_NAMES.length) continue;
    const greenProjectTimingPairGaps = greenProjectTimingPairGapCount(data);
    if (requireGreenProjectTimingPairs && greenProjectTimingPairGaps > 0) continue;
    candidates.push({
      file,
      data,
      generatedAtMs: benchmarkGeneratedAtMs(data),
      projectTimingPairs,
      applicationCompatibilityRows,
      greenProjectTimingPairGaps,
      index,
    });
  }

  candidates.sort((a, b) => {
    if (a.generatedAtMs !== b.generatedAtMs) {
      return b.generatedAtMs - a.generatedAtMs;
    }
    return a.index - b.index;
  });

  const selected = candidates[0];
  return selected ? { file: selected.file, data: selected.data } : null;
}
