import fs from "node:fs";
import path from "node:path";

const WEBSITE = path.resolve(import.meta.dirname, "..", "..");
const ROOT = path.resolve(WEBSITE, "..", "..");

function readJsonIfExists(p) {
  try {
    return JSON.parse(fs.readFileSync(p, "utf8"));
  } catch {
    return null;
  }
}

function loadBenchmarks() {
  const locations = [
    path.join(WEBSITE, "data", "benchmarks.json"),
    ...(() => {
      const artifactsDir = path.join(ROOT, "artifacts");
      try {
        return fs.readdirSync(artifactsDir)
          .filter((file) => file.startsWith("bench-vs-tsgo-") && file.endsWith(".json"))
          .sort()
          .reverse()
          .map((file) => path.join(artifactsDir, file));
      } catch {
        return [];
      }
    })(),
  ];

  for (const location of locations) {
    const data = readJsonIfExists(location);
    if (data?.results?.length) return data.results;
  }

  return [];
}

function format(n) {
  return Number(n).toLocaleString("en-US");
}

function renderMeanChart(results) {
  if (!results.length) {
    return `<div class="bench-placeholder">
  <p>No benchmark data available.</p>
  <p>Run <code>./scripts/bench/bench-vs-tsgo.sh --json</code> to generate benchmarks.</p>
</div>`;
  }

  const valid = results.filter((r) => Number.isFinite(r.tsz_ms) && Number.isFinite(r.tsgo_ms));
  if (!valid.length) {
    return `<div class="bench-placeholder">No valid benchmark rows found.</div>`;
  }

  const tszMean = valid.reduce((sum, r) => sum + r.tsz_ms, 0) / valid.length;
  const tsgoMean = valid.reduce((sum, r) => sum + r.tsgo_ms, 0) / valid.length;
  const maxMs = Math.max(tszMean, tsgoMean);
  const widthMax = 420;
  const tszWidth = Math.max(2, (tszMean / maxMs) * widthMax);
  const tsgoWidth = Math.max(2, (tsgoMean / maxMs) * widthMax);
  const factor = tszMean > 0 ? tsgoMean / tszMean : 0;

  return `<section class="benchmark-mean-card">
  <p class="bench-category-desc">Arithmetic mean across ${format(valid.length)} <a href="/benchmarks/">benchmark cases</a>.</p>
  <div class="bench-bars">
    <div class="bench-bar-row">
      <span class="bench-bar-label">tsz</span>
      <div class="bench-bar tsz" style="width: ${tszWidth}px"></div>
      <span class="bench-bar-time">${tszMean.toFixed(1)}ms</span>
    </div>
    <div class="bench-bar-row">
      <span class="bench-bar-label">TSGO</span>
      <div class="bench-bar tsgo" style="width: ${tsgoWidth}px"></div>
      <span class="bench-bar-time">${tsgoMean.toFixed(1)}ms</span>
      <span class="bench-winner">tsz ${factor.toFixed(2)}x faster</span>
    </div>
  </div>
</section>`;
}

export default renderMeanChart(loadBenchmarks());
