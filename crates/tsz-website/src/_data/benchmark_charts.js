import fs from "node:fs";
import path from "node:path";

const ROOT = path.resolve(import.meta.dirname, "..", "..", "..", "..");

function fmt(n) {
  return Number(n).toLocaleString("en-US");
}

function formatDurationMs(value, fractionDigits = 0) {
  const ms = Number(value);
  if (!Number.isFinite(ms)) return "";
  if (ms > 1000) {
    return `${(ms / 1000).toLocaleString("en-US", { maximumFractionDigits: 1 })}s`;
  }
  return `${ms.toFixed(fractionDigits)}ms`;
}

function formatSpeedupLabel(tszMs, tsgoMs) {
  const tsz = Number(tszMs);
  const tsgo = Number(tsgoMs);
  if (!Number.isFinite(tsz) || !Number.isFinite(tsgo) || tsz <= 0 || tsgo <= 0) return "";

  const factor = Math.max(tsz, tsgo) / Math.min(tsz, tsgo);
  if (factor < 1.05) return "equal";

  return tsz < tsgo
    ? `tsz ${factor.toFixed(1)}x faster`
    : `tsgo ${factor.toFixed(1)}x faster`;
}

const TINY_BENCHMARK_MAX_LINES = 200;

const PROJECT_FALLBACK_CONFIG = {
  "Projects: utility-types": {
    libraryCategory: "Single file: utility-types",
    fallbackName: "utility-types-project",
    libraryName: "utility-types",
  },
  "Projects: ts-toolbelt": {
    libraryCategory: "Single file: ts-toolbelt",
    fallbackName: "ts-toolbelt-project",
    libraryName: "ts-toolbelt",
  },
  "Projects: ts-essentials": {
    libraryCategory: "Single file: ts-essentials",
    fallbackName: "ts-essentials-project",
    libraryName: "ts-essentials",
  },
  "Projects: next.js": {
    libraryCategory: null,
    fallbackName: "nextjs",
    libraryName: "nextjs",
  },
};

const LIBRARY_CATEGORY_TO_PROJECT_CATEGORY = Object.entries(PROJECT_FALLBACK_CONFIG).reduce((map, [projectCategory, conf]) => {
  if (conf.libraryCategory) {
    map.set(conf.libraryCategory, projectCategory);
  }
  return map;
}, new Map());

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
    if (data?.results) return data;
  }

  const snapshot = readJsonIfExists(path.join(ROOT, "crates/tsz-website/bench-snapshot.json"));
  if (snapshot?.results) return snapshot;

  return null;
}

function isTinyBenchmark(lines) {
  const size = Number(lines);
  return Number.isFinite(size) && size < TINY_BENCHMARK_MAX_LINES;
}

function categoryFor(name, lines) {
  if (name === "large-ts-repo") return "Projects: large-ts-repo";
  if (name === "nextjs") return "Projects: next.js";
  if (name === "utility-types-project") return "Projects: utility-types";
  if (name === "ts-toolbelt-project") return "Projects: ts-toolbelt";
  if (name === "ts-essentials-project") return "Projects: ts-essentials";
  if (name.startsWith("utility-types/")) return "Single file: utility-types";
  if (name.startsWith("ts-toolbelt/")) return "Single file: ts-toolbelt";
  if (name.startsWith("ts-essentials/")) return "Single file: ts-essentials";
  if (isTinyBenchmark(lines)) return "Tiny File Benchmarks";
  if (/Recursive generic|Conditional dist|Mapped type/i.test(name)) return "Solver Stress Tests";
  if (/\d+\s+classes|\d+\s+generic functions|\d+\s+union members|DeepPartial|Shallow optional/i.test(name)) {
    return "Synthetic Type Workloads";
  }
  return "General Benchmarks";
}

function categorySlug(category) {
  return String(category)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/(^-|-$)/g, "");
}

function isProjectCategory(category) {
  return String(category).startsWith("Projects:");
}

function isExternalLibraryCategory(category) {
  return (
    category === "Single file: utility-types" ||
    category === "Single file: ts-toolbelt" ||
    category === "Single file: ts-essentials"
  );
}

function libraryNameForCategory(category) {
  if (category.startsWith("Libraries: ")) {
    return category.slice("Libraries: ".length);
  }
  if (category.startsWith("Single file: ")) {
    return category.slice("Single file: ".length);
  }
  return "";
}

function hasProjectRowForLibrary(category, grouped) {
  const projectRowName = {
    "Single file: utility-types": "utility-types-project",
    "Single file: ts-toolbelt": "ts-toolbelt-project",
    "Single file: ts-essentials": "ts-essentials-project",
  }[category];
  if (!projectRowName) return false;
  const projectCategory = LIBRARY_CATEGORY_TO_PROJECT_CATEGORY.get(category);
  if (!projectCategory) {
    return grouped
      .get(category)
      ?.some((row) => row.name === projectRowName) ?? false;
  }
  return (grouped.get(projectCategory)?.length ?? 0) > 0;
}

function ensureProjectRows(grouped) {
  for (const [projectCategory, conf] of Object.entries(PROJECT_FALLBACK_CONFIG)) {
    const existing = grouped.get(projectCategory);
    if (existing?.length) continue;
    if (!conf.libraryCategory) continue;

    const libraryRows = grouped.get(conf.libraryCategory) || [];
    const aggregate = buildAggregateBenchmark(libraryRows, conf.libraryName);
    if (!aggregate) continue;

    grouped.set(projectCategory, [{
      ...aggregate,
      name: conf.fallbackName,
    }]);
  }
}

function categoryMeta(category) {
  return {
    "Projects: large-ts-repo": {
      description: "Large real-world workspace benchmark (6000+ files).",
      repo: "https://github.com/mohsen1/large-ts-repo",
      repoLabel: "mohsen1/large-ts-repo",
    },
    "Projects: next.js": {
      description: "Next.js project benchmark (nextjs fixture).",
      repo: "https://github.com/vercel/next.js",
      repoLabel: "vercel/next.js",
    },
    "Projects: utility-types": {
      description: "Full utility-types project benchmark.",
      repo: "https://github.com/piotrwitek/utility-types",
      repoLabel: "piotrwitek/utility-types",
    },
    "Projects: ts-toolbelt": {
      description: "Full ts-toolbelt project benchmark (when available).",
      repo: "https://github.com/millsp/ts-toolbelt",
      repoLabel: "millsp/ts-toolbelt",
    },
    "Projects: ts-essentials": {
      description: "Full ts-essentials project benchmark.",
      repo: "https://github.com/ts-essentials/ts-essentials",
      repoLabel: "ts-essentials/ts-essentials",
    },
    "Single file: utility-types": {
      description: "Real-world utility-types file-level benchmark set from pinned snapshot.",
      repo: "https://github.com/piotrwitek/utility-types",
      repoLabel: "piotrwitek/utility-types",
    },
    "Single file: ts-toolbelt": {
      description: "Real-world ts-toolbelt file-level benchmark set with type-heavy examples.",
      repo: "https://github.com/millsp/ts-toolbelt",
      repoLabel: "millsp/ts-toolbelt",
    },
    "Single file: ts-essentials": {
      description: "Real-world ts-essentials file-level benchmark set from pinned snapshot.",
      repo: "https://github.com/ts-essentials/ts-essentials",
      repoLabel: "ts-essentials/ts-essentials",
    },
    "Tiny File Benchmarks": {
      description: "Small fixture files moved below the fold.",
    },
    "General Benchmarks": {
      description: "Core compiler behavior on representative mixed workloads.",
    },
    "Synthetic Type Workloads": {
      description: "Generated stress tests that isolate specific type-system patterns.",
    },
    "Solver Stress Tests": {
      description: "Upper-bound tests for recursive, mapped, and conditional type complexity.",
    },
  }[category] || { description: "" };
}

function buildAggregateBenchmark(rows, libraryName) {
  if (!rows.length) return null;

  const tszTotal = rows.reduce((sum, row) => sum + row.tsz_ms, 0);
  const tsgoTotal = rows.reduce((sum, row) => sum + row.tsgo_ms, 0);

  if (!Number.isFinite(tszTotal) || !Number.isFinite(tsgoTotal)) return null;

  const winner =
    tszTotal > 0 && tsgoTotal > 0
      ? tszTotal < tsgoTotal
        ? "tsz"
        : tsgoTotal < tszTotal
          ? "tsgo"
          : null
      : null;

  const factor =
    winner === "tsz"
      ? tsgoTotal / tszTotal
      : winner === "tsgo"
        ? tszTotal / tsgoTotal
        : null;

  return {
    name: `${libraryName} (all files)`,
    lines: rows.reduce((sum, row) => sum + row.lines, 0),
    kb: rows.reduce((sum, row) => sum + row.kb, 0),
    tsz_ms: tszTotal,
    tsgo_ms: tsgoTotal,
    tsz_lps: rows.reduce((sum, row) => sum + row.tsz_lps, 0),
    tsgo_lps: rows.reduce((sum, row) => sum + row.tsgo_lps, 0),
    winner,
    factor,
    status: null,
  };
}

function displayName(name) {
  return name
    .replace(/^utility-types\//, "")
    .replace(/^ts-toolbelt\//, "")
    .replace(/^ts-essentials\//, "")
    .replace(/^utility-types-project$/, "utility-types project")
    .replace(/^ts-toolbelt-project$/, "ts-toolbelt project")
    .replace(/^ts-essentials-project$/, "ts-essentials project")
    .replace(/^large-ts-repo$/, "large-ts-repo project")
    .replace(/^nextjs$/, "next.js full project")
    .replace(/_/g, " ")
    .replace(/-/g, " ");
}

function categoryDescription(category) {
  return categoryMeta(category).description || "";
}

function generateCharts(data) {
  if (!data?.results?.length) {
    return "";
  }

  const allResults = data.results;
  const results = allResults.filter((r) => r.tsz_ms != null && r.tsz_ms > 0 && r.tsgo_ms != null && r.tsgo_ms > 0);
  const failedResults = allResults.filter((r) => !(r.tsz_ms != null && r.tsz_ms > 0) && r.tsgo_ms != null && r.tsgo_ms > 0);
  if (!results.length && !failedResults.length) return "";
  const grouped = new Map();
  for (const row of results) {
    const category = categoryFor(row.name || "", row.lines);
    const bucket = grouped.get(category) || [];
    bucket.push(row);
    grouped.set(category, bucket);
  }

  ensureProjectRows(grouped);

  const barMaxWidth = 420;
  const order = [
    "Projects: large-ts-repo",
    "Projects: utility-types",
    "Projects: ts-toolbelt",
    "Projects: ts-essentials",
    "Projects: next.js",
    "Single file: utility-types",
    "Single file: ts-toolbelt",
    "Single file: ts-essentials",
    "General Benchmarks",
    "Synthetic Type Workloads",
    "Solver Stress Tests",
    "Tiny File Benchmarks",
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
    const isTinyCategory = category === "Tiny File Benchmarks";
    const entries = (grouped.get(category) || []).slice();
    const slug = categorySlug(category);
    const meta = categoryMeta(category);
    const shouldHideSingleProjectNames = isProjectCategory(category) && entries.length === 1;

    if (isExternalLibraryCategory(category)) {
      const libraryName = libraryNameForCategory(category);
      const aggregate = buildAggregateBenchmark(entries, libraryName);
      if (aggregate && !hasProjectRowForLibrary(category, grouped)) {
        entries.push(aggregate);
      }
    }

    entries.sort((a, b) => {
      const aLines = Number(a.lines) || 0;
      const bLines = Number(b.lines) || 0;
      if (bLines !== aLines) return bLines - aLines;
      return (String(a.name || "") > String(b.name || "") ? 1 : -1);
    });
    const maxMs = Math.max(...entries.map((r) => Math.max(r.tsz_ms, r.tsgo_ms)));
    const desc = categoryDescription(category);
    const repoLink = meta.repo
      ? ` <a class="bench-category-repo" href="${meta.repo}" target="_blank" rel="noopener noreferrer">${escapeHtml(meta.repoLabel || meta.repo)}</a>`
      : "";

    if (isTinyCategory) {
      html += `<section class="bench-category bench-tiny-category">
  <details id="${slug}" class="bench-category-details">
    <summary class="bench-category-title">${escapeHtml(category)}${repoLink}</summary>
    ${desc ? `<p class="bench-category-desc">${escapeHtml(desc)}</p>` : ""}
    <div class="bench-chart">\n`;
    } else {
      html += `<section class="bench-category">
  <h3 class="bench-category-title" id="${slug}">${escapeHtml(category)}${repoLink}</h3>
  ${desc ? `<p class="bench-category-desc">${escapeHtml(desc)}</p>` : ""}
  <div class="bench-chart">\n`;
    }

    for (const r of entries) {
      const tszWidth = Math.max(2, (r.tsz_ms / maxMs) * barMaxWidth);
      const tsgoWidth = Math.max(2, (r.tsgo_ms / maxMs) * barMaxWidth);
      const winnerLabel = formatSpeedupLabel(r.tsz_ms, r.tsgo_ms);

      html += `  <div class="bench-row">
${shouldHideSingleProjectNames ? "" : `    <div class="bench-name">${escapeHtml(displayName(r.name))}</div>\n`}
    <div class="bench-meta">${fmt(r.lines || 0)} lines, ${fmt(r.kb || 0)} KB</div>
    <div class="bench-bars">
      <div class="bench-bar-row">
  <span class="bench-bar-label">tsz</span>
        <div class="bench-bar tsz" style="width: ${tszWidth}px">
          <span class="bench-bar-value">${formatDurationMs(r.tsz_ms)}</span>
        </div>
      </div>
      <div class="bench-bar-row">
        <span class="bench-bar-label">tsgo</span>
        <div class="bench-bar tsgo" style="width: ${tsgoWidth}px">
          <span class="bench-bar-value">${formatDurationMs(r.tsgo_ms)}</span>
        </div>
      </div>
      ${winnerLabel ? `<div class="bench-winner">${winnerLabel}</div>` : ""}
    </div>
  </div>\n`;
    }

    if (isTinyCategory) {
      html += `  </div>
  </details>
 </section>\n`;
    } else {
      html += `  </div>
 </section>\n`;
    }
  }

  if (failedResults.length > 0) {
    html += `<section class="bench-category bench-failures">
  <h3 class="bench-category-title" id="failures">Failures</h3>
  <p class="bench-category-desc">These benchmarks could not be completed by tsz. tsgo time shown for reference.</p>
  <div class="bench-chart">\n`;
    const maxFailMs = Math.max(...failedResults.map((r) => r.tsgo_ms || 0));
    for (const r of failedResults) {
      const tsgoWidth = maxFailMs > 0 ? Math.max(2, (r.tsgo_ms / maxFailMs) * barMaxWidth) : 2;
      html += `  <div class="bench-row">
    <div class="bench-name">${escapeHtml(displayName(r.name))}</div>
    <div class="bench-meta">${fmt(r.lines || 0)} lines, ${fmt(r.kb || 0)} KB</div>
    <div class="bench-bars">
      <div class="bench-bar-row">
        <span class="bench-bar-label">tsz</span>
        <div class="bench-bar tsz bench-bar-failed" style="width: 2px"></div>
        <span class="bench-bar-time bench-failed-label">tsz failed</span>
      </div>
      <div class="bench-bar-row">
        <span class="bench-bar-label">tsgo</span>
        <div class="bench-bar tsgo" style="width: ${tsgoWidth}px">
          <span class="bench-bar-value">${formatDurationMs(r.tsgo_ms)}</span>
        </div>
      </div>
    </div>
  </div>\n`;
    }
    html += `  </div>
 </section>\n`;
  }

  return html;
}

export default generateCharts(loadBenchmarks());
