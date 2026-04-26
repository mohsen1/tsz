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

function loadBenchmarks() {
  const artifactsDir = path.join(ROOT, "artifacts");
  const artifactFiles = (() => {
    try {
      return fs.readdirSync(artifactsDir)
        .filter((file) => file.startsWith("bench-vs-tsgo-") && file.endsWith(".json"))
        .sort()
        .reverse()
        .map((file) => path.join(artifactsDir, file));
    } catch {
      return [];
    }
  })();

  for (const location of artifactFiles) {
    const data = readJsonIfExists(location);
    if (data?.results?.length) return data.results;
  }

  return [];
}

function format(n) {
  return Number(n).toLocaleString("en-US");
}

function toNumber(value) {
  return Number.isFinite(value) ? value : Number.NaN;
}

function formatMs(value) {
  return Number(value).toFixed(1);
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

function renderHighlightedBenchmark(results) {
  const row = results.find((r) => r.name === "large-ts-repo");
  if (!row) {
    return "";
  }

  const tszMs = toNumber(row.tsz_ms);
  const tsgoMs = toNumber(row.tsgo_ms);

  if (!Number.isFinite(tszMs) || !Number.isFinite(tsgoMs)) {
    return "";
  }

  const maxMs = Math.max(tszMs, tsgoMs);
  const widthMax = 420;
  const tszWidth = Math.max(2, (tszMs / maxMs) * widthMax);
  const tsgoWidth = Math.max(2, (tsgoMs / maxMs) * widthMax);
  const ratioLabel = formatSpeedupLabel(tszMs, tsgoMs);
  const statusLine = row.status ? ` (${row.status})` : "";

  return `<section class="benchmark-mean-card" id="main-benchmark-spotlight">
  <p class="bench-category-title">Featured benchmark: <a href="/benchmarks/#projects-large-ts-repo">large-ts-repo</a></p>
  <p class="bench-category-desc">Real-world production-style project benchmark (~${format(row.lines || 0)} lines, ${format(row.kb || 0)} KB).</p>
  ${statusLine ? `<p class="bench-bar-time">${statusLine}</p>` : ""}
  <div class="bench-bars">
    <div class="bench-bar-row">
      <span class="bench-bar-label">tsz</span>
      <div class="bench-bar tsz" style="width: ${tszWidth}px"></div>
      <span class="bench-bar-time">${formatMs(tszMs)}ms</span>
    </div>
    <div class="bench-bar-row">
      <span class="bench-bar-label">tsgo</span>
      <div class="bench-bar tsgo" style="width: ${tsgoWidth}px"></div>
      <span class="bench-bar-time">${formatMs(tsgoMs)}ms</span>
      ${ratioLabel ? `<span class="bench-winner">${ratioLabel}</span>` : ""}
    </div>
  </div>
</section>`;
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
  const widthMax = 420;
  const tszWidth = Math.max(2, (tszMean / maxMs) * widthMax);
  const tsgoWidth = Math.max(2, (tsgoMean / maxMs) * widthMax);
  const speedupLabel = formatSpeedupLabel(tszMean, tsgoMean);

  return `<section class="benchmark-mean-card">
  <p class="bench-category-desc">Arithmetic mean across ${format(valid.length)} <a href="/benchmarks/">benchmark cases</a>.</p>
  <div class="bench-bars">
    <div class="bench-bar-row">
      <span class="bench-bar-label">tsz</span>
      <div class="bench-bar tsz" style="width: ${tszWidth}px"></div>
      <span class="bench-bar-time">${tszMean.toFixed(1)}ms</span>
    </div>
    <div class="bench-bar-row">
      <span class="bench-bar-label">tsgo</span>
      <div class="bench-bar tsgo" style="width: ${tsgoWidth}px"></div>
      <span class="bench-bar-time">${tsgoMean.toFixed(1)}ms</span>
      ${speedupLabel ? `<span class="bench-winner">${speedupLabel}</span>` : ""}
    </div>
  </div>
</section>`;
}

const results = loadBenchmarks();
const highlight = renderHighlightedBenchmark(results);
export default highlight || renderMeanChart(results);
