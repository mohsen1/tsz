import fs from "node:fs";
import path from "node:path";

const ROOT = path.resolve(import.meta.dirname, "..", "..", "..", "..");

function readJsonIfExists(p) {
  try {
    return JSON.parse(fs.readFileSync(p, "utf8"));
  } catch {
    return null;
  }
}

function sanitizeLegacyBenchmarkResults(data) {
  if (data?.validation?.hyperfine_exit_codes_required === true) {
    return data.results || [];
  }
  return (data?.results || []).filter((row) => row.name !== "large-ts-repo");
}

function hasSuccessfulTiming(row) {
  return (
    !row?.status &&
    row?.winner !== "error" &&
    Number.isFinite(row?.tsz_ms) &&
    row.tsz_ms > 0 &&
    Number.isFinite(row?.tsgo_ms) &&
    row.tsgo_ms > 0
  );
}

const TINY_BENCHMARK_MAX_LINES = 200;
const PROJECT_BENCHMARK_NAMES = new Set([
  "large-ts-repo",
  "utility-types-project",
  "ts-toolbelt-project",
  "ts-essentials-project",
  "nextjs",
  "nextjs-fresh-app",
  "vite-vanilla-ts-app",
  "rxjs-project",
  "type-fest-project",
  "zod-project",
  "kysely-project",
]);
const SINGLE_FILE_BENCHMARK_PREFIXES = [
  "utility-types/",
  "ts-toolbelt/",
  "ts-essentials/",
];

function isTinyBenchmark(row) {
  const size = Number(row?.lines);
  return Number.isFinite(size) && size < TINY_BENCHMARK_MAX_LINES;
}

function isProjectBenchmark(row) {
  return PROJECT_BENCHMARK_NAMES.has(String(row?.name || ""));
}

function isSingleFileBenchmark(row) {
  const name = String(row?.name || "");
  return SINGLE_FILE_BENCHMARK_PREFIXES.some((prefix) => name.startsWith(prefix));
}

function isMicroBenchmark(row) {
  return !isProjectBenchmark(row) && (!isTinyBenchmark(row) || isSingleFileBenchmark(row));
}

function loadBenchmarks() {
  const artifactsDir = path.join(ROOT, "artifacts");
  const ciLatest = [
    "bench-vs-tsgo-github-latest.json",
    "bench-vs-tsgo-gcs-latest.json",
  ].map((file) => path.join(artifactsDir, file));
  const artifactFiles = (() => {
    try {
      const localArtifacts = fs.readdirSync(artifactsDir)
        .filter((file) => file.startsWith("bench-vs-tsgo-") && file.endsWith(".json"))
        .filter((file) => !["bench-vs-tsgo-github-latest.json", "bench-vs-tsgo-gcs-latest.json"].includes(file))
        .sort()
        .reverse()
        .map((file) => path.join(artifactsDir, file));
      return [...ciLatest, ...localArtifacts];
    } catch {
      return ciLatest;
    }
  })();

  for (const location of artifactFiles) {
    const data = readJsonIfExists(location);
    if (data?.results?.length) return sanitizeLegacyBenchmarkResults(data);
  }

  const snapshot = readJsonIfExists(path.join(ROOT, "crates/tsz-website/bench-snapshot.json"));
  if (snapshot?.results?.length) return sanitizeLegacyBenchmarkResults(snapshot);

  return [];
}

function format(n) {
  return Number(n).toLocaleString("en-US");
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

function geometricMean(values) {
  if (!values.length) return Number.NaN;
  return Math.exp(values.reduce((sum, value) => sum + Math.log(value), 0) / values.length);
}

function formatPerCaseSpeedupLabel(rows) {
  const ratio = geometricMean(rows.map((row) => row.tsgo_ms / row.tsz_ms));
  if (!Number.isFinite(ratio) || ratio <= 0) return "";

  if (ratio >= 1) {
    return `Per-case geomean across the same rows: tsz ${formatRatio(ratio)}x faster.`;
  }
  return `Per-case geomean across the same rows: tsgo ${formatRatio(1 / ratio)}x faster.`;
}

function renderMeanChart(results) {
  if (!results.length) {
    return "";
  }

  const valid = results.filter((r) => isMicroBenchmark(r) && hasSuccessfulTiming(r));
  if (!valid.length) {
    return "";
  }

  const tszTotal = aggregate(valid.map((r) => r.tsz_ms));
  const tsgoTotal = aggregate(valid.map((r) => r.tsgo_ms));
  const maxMs = Math.max(tszTotal, tsgoTotal);
  const tszWidth = Math.max(0.5, (tszTotal / maxMs) * 100);
  const tsgoWidth = Math.max(0.5, (tsgoTotal / maxMs) * 100);
  const speedupLabel = formatSpeedupLabel(tszTotal, tsgoTotal);
  const perCaseSpeedupLabel = formatPerCaseSpeedupLabel(valid);

  return `<section class="benchmark-mean-card">
  <p class="bench-category-desc">Sum across ${format(valid.length)} successful <a href="/benchmarks/micro/">micro benchmark cases</a>.</p>
  ${perCaseSpeedupLabel ? `<p class="bench-category-desc">${perCaseSpeedupLabel}</p>` : ""}
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
