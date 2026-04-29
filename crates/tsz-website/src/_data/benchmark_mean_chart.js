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

function loadBenchmarks() {
  const artifactsDir = path.join(ROOT, "artifacts");
  const ciLatest = path.join(artifactsDir, "bench-vs-tsgo-gcs-latest.json");
  const artifactFiles = (() => {
    try {
      const localArtifacts = fs.readdirSync(artifactsDir)
        .filter((file) => file.startsWith("bench-vs-tsgo-") && file.endsWith(".json"))
        .filter((file) => file !== "bench-vs-tsgo-gcs-latest.json")
        .sort()
        .reverse()
        .map((file) => path.join(artifactsDir, file));
      return [ciLatest, ...localArtifacts];
    } catch {
      return [ciLatest];
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

function renderMeanChart(results) {
  if (!results.length) {
    return "";
  }

  const valid = results.filter((r) => Number.isFinite(r.tsz_ms) && r.tsz_ms > 0 && Number.isFinite(r.tsgo_ms) && r.tsgo_ms > 0);
  if (!valid.length) {
    return "";
  }

  const tszMean = valid.reduce((sum, r) => sum + r.tsz_ms, 0) / valid.length;
  const tsgoMean = valid.reduce((sum, r) => sum + r.tsgo_ms, 0) / valid.length;
  const maxMs = Math.max(tszMean, tsgoMean);
  const tszWidth = Math.max(0.5, (tszMean / maxMs) * 100);
  const tsgoWidth = Math.max(0.5, (tsgoMean / maxMs) * 100);
  const speedupLabel = formatSpeedupLabel(tszMean, tsgoMean);

  return `<section class="benchmark-mean-card">
  <p class="bench-category-desc">Arithmetic mean across ${format(valid.length)} <a href="/benchmarks/">benchmark cases</a>.</p>
  <div class="bench-bars">
    <div class="bench-bar-row">
      <span class="bench-bar-label">tsz</span>
      <div class="bench-bar tsz" style="--bench-bar-width: ${tszWidth}%" data-target-width="${tszWidth}" data-target-ms="${tszMean}" data-duration-precision="${formatDurationPrecision(tszMean)}">
        <span class="bench-bar-value">${formatDurationMs(tszMean)}</span>
      </div>
    </div>
    <div class="bench-bar-row">
      <span class="bench-bar-label">tsgo</span>
      <div class="bench-bar tsgo" style="--bench-bar-width: ${tsgoWidth}%" data-target-width="${tsgoWidth}" data-target-ms="${tsgoMean}" data-duration-precision="${formatDurationPrecision(tsgoMean)}">
        <span class="bench-bar-value">${formatDurationMs(tsgoMean)}</span>
      </div>
    </div>
    ${speedupLabel ? `<div class="bench-winner">${speedupLabel}</div>` : ""}
  </div>
</section>`;
}

export default renderMeanChart(loadBenchmarks());
