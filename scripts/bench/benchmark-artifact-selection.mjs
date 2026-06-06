import fs from "node:fs";

import {
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

export function selectLatestBenchmarkArtifact(files, options = {}) {
  const minimumProjectTimingPairs = Math.max(0, Number(options.minimumProjectTimingPairs ?? 0));
  const candidates = [];
  for (const [index, file] of files.entries()) {
    const data = readBenchmarkArtifact(file);
    if (!data) continue;
    const projectTimingPairs = successfulProjectTimingPairCount(data);
    if (projectTimingPairs < minimumProjectTimingPairs) continue;
    candidates.push({
      file,
      data,
      generatedAtMs: benchmarkGeneratedAtMs(data),
      projectTimingPairs,
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
