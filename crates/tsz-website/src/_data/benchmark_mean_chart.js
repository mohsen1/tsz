import fs from "node:fs";
import path from "node:path";
import { selectLatestBenchmarkArtifact } from "../../../../scripts/bench/benchmark-artifact-selection.mjs";
import {
  PERF_TIMED_PROJECT_ROWS,
  REQUIRED_PROJECT_ROWS,
} from "../../../../scripts/bench/project-rows.mjs";
import { isSpeedRatioEligible } from "../../../../scripts/bench/row-utils.mjs";
import { fmt } from "./loc.js";

const ROOT = path.resolve(import.meta.dirname, "..", "..", "..", "..");

function sanitizeLegacyBenchmarkResults(data) {
  if (data?.validation?.hyperfine_exit_codes_required === true) {
    return data.results || [];
  }
  return (data?.results || []).filter((row) => row.name !== "large-ts-repo");
}

const PROJECT_BENCHMARK_NAMES = new Set([
  ...REQUIRED_PROJECT_ROWS,
  ...PERF_TIMED_PROJECT_ROWS,
]);

function isProjectBenchmark(row) {
  return PROJECT_BENCHMARK_NAMES.has(String(row?.name || ""));
}

// A micro benchmark is any timed case that is not a whole-project row. We do
// not exclude short fixtures: every successful non-project case counts toward
// the micro sum, so the homepage total matches the /benchmarks/micro/ page.
function isMicroBenchmark(row) {
  return !isProjectBenchmark(row);
}

function loadBenchmarks() {
  const artifactsDir = path.join(ROOT, "artifacts");
  const ciLatest = [
    "bench-vs-tsgo-github-latest.json",
  ].map((file) => path.join(artifactsDir, file));
  const artifactFiles = (() => {
    try {
      const localArtifacts = fs.readdirSync(artifactsDir)
        .filter((file) => file.startsWith("bench-vs-tsgo-") && file.endsWith(".json"))
        .filter((file) => !["bench-vs-tsgo-github-latest.json"].includes(file))
        .sort()
        .reverse()
        .map((file) => path.join(artifactsDir, file));
      return [...ciLatest, ...localArtifacts];
    } catch {
      return ciLatest;
    }
  })();

  // Always use the latest available data; do not gate on app-compat cleanliness
  // or a green-project minimum (benchmarks may fail individually). The renderer
  // selects which rows to show by the 1.5x speed rule.
  const selectedArtifact = selectLatestBenchmarkArtifact([
    ...artifactFiles,
    path.join(ROOT, "crates/tsz-website/bench-snapshot.json"),
  ], { minimumProjectTimingPairs: 0 });
  if (selectedArtifact) {
    return sanitizeLegacyBenchmarkResults(selectedArtifact.data);
  }

  return [];
}

function formatDurationMs(value) {
  const ms = Number(value);
  if (!Number.isFinite(ms)) return "";
  if (ms > 1000) {
    return `${Math.round(ms / 1000).toLocaleString("en-US")}s`;
  }
  return `${Math.round(ms).toLocaleString("en-US")}ms`;
}

function formatDurationPrecision(value) {
  return 0;
}

function formatRatio(value) {
  return Number(value).toFixed(2);
}

function formatSpeedupLabel(tszMs, tsgoMs) {
  if (!Number.isFinite(tszMs) || !Number.isFinite(tsgoMs) || tszMs <= 0) return "";

  if (tszMs < tsgoMs) {
    return `tsz ${formatRatio(tsgoMs / tszMs)}x faster`;
  }
  if (tsgoMs > 0) {
    return `tsgo ${formatRatio(tszMs / tsgoMs)}x faster`;
  }
  return "";
}

function aggregate(msValues) {
  return msValues.reduce((sum, value) => sum + value, 0);
}

function renderMeanChart(results) {
  if (!results.length) {
    return "";
  }

  // `isSpeedRatioEligible` (row-utils.mjs) keeps a killed/errored row's
  // ceiling/error sentinel out of the aggregate mean structurally (#16196).
  const valid = results.filter((r) => isMicroBenchmark(r) && isSpeedRatioEligible(r));
  if (!valid.length) {
    return "";
  }

  const tszTotal = aggregate(valid.map((r) => r.tsz_ms));
  const tsgoTotal = aggregate(valid.map((r) => r.tsgo_ms));
  const maxMs = Math.max(tszTotal, tsgoTotal);
  const tszWidth = Math.max(0.5, (tszTotal / maxMs) * 100);
  const tsgoWidth = Math.max(0.5, (tsgoTotal / maxMs) * 100);
  const speedupLabel = formatSpeedupLabel(tszTotal, tsgoTotal);

  return `<section class="benchmark-mean-card">
  <p class="bench-category-desc">Sum across ${fmt(valid.length)} successful <a href="/benchmarks/micro/">micro benchmark cases</a>.</p>
  <div class="bench-bars">
    <div class="bench-bar-row">
      <span class="bench-bar-label">tsz</span>
      <div class="bench-bar tsz" style="--bench-bar-width: ${tszWidth}%" data-target-width="${tszWidth}" data-target-ms="${tszTotal}" data-duration-precision="${formatDurationPrecision(tszTotal)}">
        <span class="bench-bar-value">${formatDurationMs(tszTotal)}</span>
      </div>
    </div>
    <div class="bench-bar-row">
      <span class="bench-bar-label">tsgo</span>
      <div class="bench-bar tsgo" style="--bench-bar-width: ${tsgoWidth}%" data-target-width="${tsgoWidth}" data-target-ms="${tsgoTotal}" data-duration-precision="${formatDurationPrecision(tsgoTotal)}">
        <span class="bench-bar-value">${formatDurationMs(tsgoTotal)}</span>
      </div>
    </div>
    ${speedupLabel ? `<div class="bench-winner">${speedupLabel}</div>` : ""}
  </div>
</section>`;
}

export default renderMeanChart(loadBenchmarks());
