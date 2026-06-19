import fs from "node:fs";

import {
  PROJECT_ROW_DEFINITIONS,
  PROJECT_ROWS_BY_NAME,
} from "./project-rows.mjs";
import {
  isGreen,
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

function hasTiming(value) {
  const time = Number(value);
  return Number.isFinite(time) && time > 0;
}

const APPLICATION_PROJECT_ROW_NAMES = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.category === "application")
  .map((row) => row.name);

export function successfulProjectTimingPairCount(data) {
  return (Array.isArray(data?.results) ? data.results : []).filter((row) => (
    Object.hasOwn(PROJECT_ROWS_BY_NAME, String(row?.name || "")) &&
    hasTiming(row?.tsz_ms) &&
    hasTiming(row?.tsgo_ms) &&
    row?.winner !== "error" &&
    isGreen(row)
  )).length;
}

export function hasSuccessfulProjectTimingPairs(data, minimum = 1) {
  return successfulProjectTimingPairCount(data) >= minimum;
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
  const candidates = [];
  for (const [index, file] of files.entries()) {
    const data = readBenchmarkArtifact(file);
    if (!data) continue;
    const projectTimingPairs = successfulProjectTimingPairCount(data);
    if (projectTimingPairs < minimumProjectTimingPairs) continue;
    const applicationCompatibilityRows = applicationCompatibilityRowCount(data);
    if (requireApplicationCompat && applicationCompatibilityRows < APPLICATION_PROJECT_ROW_NAMES.length) continue;
    candidates.push({
      file,
      data,
      generatedAtMs: benchmarkGeneratedAtMs(data),
      projectTimingPairs,
      applicationCompatibilityRows,
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
