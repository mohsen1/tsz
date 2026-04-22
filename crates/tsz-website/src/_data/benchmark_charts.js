import fs from "node:fs";
import path from "node:path";

const WEBSITE = path.resolve(import.meta.dirname, "..", "..");
const ROOT = path.resolve(WEBSITE, "..", "..");

function fmt(n) {
  return Number(n).toLocaleString("en-US");
}

function escapeHtml(str) {
  return String(str)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

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
    if (data?.results) return data;
  }
  return null;
}

function categoryFor(name) {
  if (name.startsWith("utility-types/")) return "External Libraries: utility-types";
  if (name.startsWith("ts-toolbelt/")) return "External Libraries: ts-toolbelt";
  if (name.startsWith("ts-essentials/")) return "External Libraries: ts-essentials";
  if (name.startsWith("nextjs")) return "External Projects: next.js";
  if (/Recursive generic|Conditional dist|Mapped type/i.test(name)) return "Solver Stress Tests";
  if (/\d+\s+classes|\d+\s+generic functions|\d+\s+union members|DeepPartial|Shallow optional/i.test(name)) {
    return "Synthetic Type Workloads";
  }
  return "General Benchmarks";
}

function displayName(name) {
  return name
    .replace(/^utility-types\//, "")
    .replace(/^ts-toolbelt\//, "")
    .replace(/^ts-essentials\//, "")
    .replace(/^nextjs$/, "next.js full project")
    .replace(/_/g, " ")
    .replace(/-/g, " ");
}

function categoryDescription(category) {
  const map = {
    "General Benchmarks": "Core compiler behavior on representative mixed workloads.",
    "Synthetic Type Workloads": "Generated stress tests that isolate specific type-system patterns.",
    "Solver Stress Tests": "Upper-bound tests for recursive, mapped, and conditional type complexity.",
    "External Libraries: utility-types": "Real-world utility-types sources from the pinned upstream snapshot.",
    "External Libraries: ts-toolbelt": "Real-world ts-toolbelt files with heavy type-level programming patterns.",
    "External Libraries: ts-essentials": "Real-world ts-essentials files from the pinned upstream snapshot.",
    "External Projects: next.js": "Large project benchmark using next.js fixture (when enabled).",
  };
  return map[category] || "";
}

function generateCharts(data) {
  if (!data?.results?.length) {
    return `<div class="bench-placeholder">
  <p>No benchmark data available.</p>
  <p>Run <code>./scripts/bench/bench-vs-tsgo.sh --json</code> to generate benchmarks.</p>
</div>`;
  }

  const results = data.results.filter((r) => r.tsz_ms != null && r.tsgo_ms != null);
  if (!results.length) return `<div class="bench-placeholder">No valid benchmark results found.</div>`;

  const grouped = new Map();
  for (const row of results) {
    const category = categoryFor(row.name || "");
    const bucket = grouped.get(category) || [];
    bucket.push(row);
    grouped.set(category, bucket);
  }

  const barMaxWidth = 420;
  const order = [
    "General Benchmarks",
    "Synthetic Type Workloads",
    "Solver Stress Tests",
    "External Libraries: utility-types",
    "External Libraries: ts-toolbelt",
    "External Libraries: ts-essentials",
    "External Projects: next.js",
  ];
  const categories = [...grouped.keys()].sort((a, b) => {
    const ia = order.indexOf(a);
    const ib = order.indexOf(b);
    if (ia === -1 && ib === -1) return a.localeCompare(b);
    if (ia === -1) return 1;
    if (ib === -1) return -1;
    return ia - ib;
  });

  let html = "";
  for (const category of categories) {
    const entries = grouped.get(category) || [];
    const maxMs = Math.max(...entries.map((r) => Math.max(r.tsz_ms, r.tsgo_ms)));
    const desc = categoryDescription(category);

    html += `<section class="bench-category">
  <h3 class="bench-category-title">${escapeHtml(category)}</h3>
  ${desc ? `<p class="bench-category-desc">${escapeHtml(desc)}</p>` : ""}
  <div class="bench-chart">\n`;

    for (const r of entries) {
      const tszWidth = Math.max(2, (r.tsz_ms / maxMs) * barMaxWidth);
      const tsgoWidth = Math.max(2, (r.tsgo_ms / maxMs) * barMaxWidth);
      const winnerLabel =
        r.winner === "tsz"
          ? `tsz ${r.factor?.toFixed(1)}x faster`
          : r.winner === "tsgo"
            ? `tsgo ${r.factor?.toFixed(1)}x faster`
            : "";

      html += `  <div class="bench-row">
    <div class="bench-name">${escapeHtml(displayName(r.name))}</div>
    <div class="bench-meta">${fmt(r.lines || 0)} lines, ${fmt(r.kb || 0)} KB</div>
    <div class="bench-bars">
      <div class="bench-bar-row">
        <span class="bench-bar-label">tsz</span>
        <div class="bench-bar tsz" style="width: ${tszWidth}px"></div>
        <span class="bench-bar-time">${r.tsz_ms.toFixed(0)}ms</span>
      </div>
      <div class="bench-bar-row">
        <span class="bench-bar-label">tsgo</span>
        <div class="bench-bar tsgo" style="width: ${tsgoWidth}px"></div>
        <span class="bench-bar-time">${r.tsgo_ms.toFixed(0)}ms</span>
        ${winnerLabel ? `<span class="bench-winner">${winnerLabel}</span>` : ""}
      </div>
    </div>
  </div>\n`;
    }

    html += `  </div>
 </section>\n`;
  }

  html += `<section class="bench-category bench-notes">
  <h3 class="bench-category-title">How to Read These Charts</h3>
  <p class="bench-category-desc">
    Each category is normalized independently for readability: bar lengths are scaled to the
    slowest benchmark <em>within that category</em>.
  </p>
 </section>`;

  return html;
}

export default generateCharts(loadBenchmarks());
