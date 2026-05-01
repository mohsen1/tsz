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

function escapeAttributeJson(value) {
  return escapeHtml(JSON.stringify(value));
}

function readJsonIfExists(p) {
  try {
    return JSON.parse(fs.readFileSync(p, "utf8"));
  } catch {
    return null;
  }
}

function sanitizeLegacyBenchmarkData(data) {
  if (data?.validation?.hyperfine_exit_codes_required === true) {
    return data;
  }
  if (!data?.results?.length) {
    return data;
  }
  return {
    ...data,
    results: data.results.filter((row) => row.name !== "large-ts-repo"),
  };
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
    if (data?.results) return sanitizeLegacyBenchmarkData(data);
  }

  const snapshot = readJsonIfExists(path.join(ROOT, "crates/tsz-website/bench-snapshot.json"));
  if (snapshot?.results) return sanitizeLegacyBenchmarkData(snapshot);

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
      title: "large-ts-repo",
      repo: "https://github.com/mohsen1/large-ts-repo",
      repoLabel: "mohsen1/large-ts-repo",
    },
    "Projects: next.js": {
      title: "next.js",
      repo: "https://github.com/vercel/next.js",
      repoLabel: "vercel/next.js",
    },
    "Projects: utility-types": {
      title: "utility-types",
      repo: "https://github.com/piotrwitek/utility-types",
      repoLabel: "piotrwitek/utility-types",
    },
    "Projects: ts-toolbelt": {
      title: "ts-toolbelt",
      repo: "https://github.com/millsp/ts-toolbelt",
      repoLabel: "millsp/ts-toolbelt",
    },
    "Projects: ts-essentials": {
      title: "ts-essentials",
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
  if (name === "privacyFunctionParameterDeclFile.ts") {
    return "Privacy function parameter declaration file";
  }

  const cleaned = String(name || "")
    .replace(/^utility-types\//, "")
    .replace(/^ts-toolbelt\//, "")
    .replace(/^ts-essentials\//, "")
    .replace(/^utility-types-project$/, "utility-types project")
    .replace(/^ts-toolbelt-project$/, "ts-toolbelt project")
    .replace(/^ts-essentials-project$/, "ts-essentials project")
    .replace(/^large-ts-repo$/, "large-ts-repo project")
    .replace(/^nextjs$/, "next.js full project")
    .replace(/\.ts$/, "")
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/_/g, " ")
    .replace(/-/g, " ");
  return cleaned.charAt(0).toUpperCase() + cleaned.slice(1);
}

function benchmarkSlug(name) {
  return String(name || "benchmark")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/(^-|-$)/g, "") || "benchmark";
}

function benchmarkUrl(row) {
  return `/benchmarks/${benchmarkSlug(row.name)}/`;
}

function benchmarkKind(category) {
  if (isProjectCategory(category)) return "project";
  if (isExternalLibraryCategory(category)) return "library file";
  if (category === "Tiny File Benchmarks") return "startup";
  if (category === "Solver Stress Tests") return "solver stress";
  if (category === "Synthetic Type Workloads") return "synthetic";
  return "benchmark";
}

function benchmarkFocus(row, category) {
  const name = String(row.name || "");
  if (isProjectCategory(category)) {
    return "Full project type-check throughput, including module graph setup and cross-file type analysis.";
  }
  if (name.includes("Recursive generic")) {
    return "Recursive generic instantiation and cache behavior under deep type expansion.";
  }
  if (name.includes("Conditional dist")) {
    return "Distributive conditional types over broad unions.";
  }
  if (name.includes("Mapped type") || /DeepPartial|Shallow optional/i.test(name)) {
    return "Mapped-type and property traversal behavior in the solver.";
  }
  if (name.includes("union members")) {
    return "Union construction, reduction, and assignability checks.";
  }
  if (name.includes("classes")) {
    return "Class declaration binding plus constructor/member shape checking.";
  }
  if (name.includes("generic functions")) {
    return "Generic signature checking and type-parameter environment setup.";
  }
  if (isExternalLibraryCategory(category)) {
    return `Single-file type-check from ${libraryNameForCategory(category)} with real-world helper types.`;
  }
  if (/privacy/i.test(name)) {
    return "Declaration emit privacy checks for public APIs that reference private parameter types.";
  }
  if (/binder/i.test(name)) {
    return "Binder and symbol-table setup for syntax-heavy TypeScript input.";
  }
  if (/controlflow|cfa/i.test(name)) {
    return "Control-flow graph construction and narrowing analysis.";
  }
  if (/enum/i.test(name)) {
    return "Enum literal subtype reduction and related assignability checks.";
  }
  return `No-emit type-check timing for ${displayName(name).toLowerCase()}.`;
}

function snippetForBenchmark(row, category) {
  const name = String(row.name || "");

  if (name.includes("Recursive generic")) {
    return `type Recurse<T, N extends number> =
  N extends 0 ? T : Recurse<{ value: T }, N>;

type Result = Recurse<string, 40>;`;
  }
  if (name.includes("Conditional dist")) {
    return `type Dist<T> = T extends unknown
  ? { value: T; optional?: T }
  : never;

type Result = Dist<"a" | "b" | "c">;`;
  }
  if (name.includes("Mapped type") || /DeepPartial/i.test(name)) {
    return `type DeepPartial<T> = {
  [K in keyof T]?: T[K] extends object
    ? DeepPartial<T[K]>
    : T[K];
};`;
  }
  if (/Shallow optional/i.test(name)) {
    return `type Optional<T> = {
  [K in keyof T]?: T[K];
};`;
  }
  if (name.includes("union members")) {
    return `type Variant =
  | { kind: "a"; value: string }
  | { kind: "b"; value: number }
  | { kind: "c"; value: boolean };`;
  }
  if (name.includes("classes")) {
    return `class Example {
  constructor(public id: string) {}
  read(): string { return this.id; }
}`;
  }
  if (name.includes("generic functions")) {
    return `function map<T, U>(
  value: T,
  fn: (value: T) => U,
): U {
  return fn(value);
}`;
  }
  if (isProjectCategory(category)) {
    return `# Project benchmark
tsz --noEmit -p tsconfig.json
tsgo --noEmit -p tsconfig.json`;
  }
  if (isExternalLibraryCategory(category)) {
    return `import type { DeepPartial } from "./helpers";

type Fixture<T> = DeepPartial<T> & {
  readonly id: string;
};`;
  }
  return `type Fixture<T> = {
  [K in keyof T]: T[K] extends string
    ? K
    : never;
};`;
}

function readFixtureSource(name) {
  const fixtureName = String(name || "");
  if (!fixtureName.endsWith(".ts") || fixtureName.includes("/")) return null;

  const candidates = [
    path.join(ROOT, "TypeScript/tests/cases/compiler", fixtureName),
    path.join(ROOT, "TypeScript/tests/cases/conformance", fixtureName),
  ];

  for (const candidate of candidates) {
    try {
      return fs.readFileSync(candidate, "utf8").trimEnd();
    } catch {
      // Keep looking in the next known TypeScript fixture location.
    }
  }

  return null;
}

function externalFixturePath(name) {
  const fixtureName = String(name || "");
  if (fixtureName.startsWith("utility-types/")) {
    return path.join(ROOT, ".target-bench/external/utility-types/src", fixtureName.slice("utility-types/".length));
  }
  if (fixtureName.startsWith("ts-toolbelt/")) {
    return path.join(ROOT, ".target-bench/external/ts-toolbelt/sources", fixtureName.slice("ts-toolbelt/".length));
  }
  if (fixtureName.startsWith("ts-essentials/")) {
    const rel = fixtureName.slice("ts-essentials/".length).replace(/\.ts$/, "/index.ts");
    return path.join(ROOT, ".target-bench/external/ts-essentials/lib", rel);
  }
  return null;
}

function readExternalFixtureSource(name) {
  const sourcePath = externalFixturePath(name);
  if (!sourcePath) return null;
  try {
    return fs.readFileSync(sourcePath, "utf8").trimEnd();
  } catch {
    return null;
  }
}

function sourceFilesForBenchmark(row, category) {
  if (isProjectCategory(category)) return [];

  const name = String(row.name || "fixture.ts");
  const fixtureName = name.endsWith(".ts") ? name : `${name}.ts`;
  const externalSource = isExternalLibraryCategory(category)
    ? readExternalFixtureSource(fixtureName)
    : null;
  const snippet = externalSource || readFixtureSource(fixtureName) || snippetForBenchmark(row, category);

  if (isExternalLibraryCategory(category)) {
    if (!externalSource) return [];
    return [{
      name: fixtureName,
      language: "typescript",
      source: externalSource,
    }];
  }

  return [
    {
      name: fixtureName,
      language: "typescript",
      source: snippet,
    },
  ];
}

function benchmarkCommand(row, category, compiler) {
  if (isProjectCategory(category)) {
    return `${compiler} --noEmit -p tsconfig.json`;
  }
  const name = String(row.name || "fixture.ts");
  return `${compiler} --noEmit ${name.endsWith(".ts") ? name : `${name}.ts`}`;
}

function comparison(row) {
  const tsz = Number(row.tsz_ms);
  const tsgo = Number(row.tsgo_ms);
  if (!Number.isFinite(tsz) || !Number.isFinite(tsgo) || tsz <= 0 || tsgo <= 0) {
    return {
      available: false,
      winner: row.status ? "unavailable" : "unknown",
      factor: null,
      deltaMs: null,
      percent: null,
    };
  }
  const winner = tsz < tsgo ? "tsz" : tsgo < tsz ? "tsgo" : "tie";
  const factor = Math.max(tsz, tsgo) / Math.min(tsz, tsgo);
  return {
    available: true,
    winner,
    factor,
    deltaMs: Math.abs(tsz - tsgo),
    percent: ((tsz - tsgo) / tsgo) * 100,
  };
}

function decorateRow(row, category, options = {}) {
  const maxMs = Math.max(Number(row.tsz_ms) || 0, Number(row.tsgo_ms) || 0);
  const sourceFiles = sourceFilesForBenchmark(row, category);
  const focus = benchmarkFocus(row, category);
  const decorated = {
    ...row,
    category,
    category_slug: categorySlug(category),
    display_name: displayName(row.name || ""),
    slug: benchmarkSlug(row.name),
    url: benchmarkUrl(row),
    kind: benchmarkKind(category),
    focus,
    detail_focus: isExternalLibraryCategory(category) ? "" : focus,
    snippet: sourceFiles[0]?.source || snippetForBenchmark(row, category),
    source_files: sourceFiles,
    tsz_command: benchmarkCommand(row, category, "tsz"),
    tsgo_command: benchmarkCommand(row, category, "tsgo"),
    tsz_time: row.tsz_ms ? formatDurationMs(row.tsz_ms, 2) : "",
    tsgo_time: row.tsgo_ms ? formatDurationMs(row.tsgo_ms, 2) : "",
    tsz_width: maxMs > 0 && row.tsz_ms ? Math.max(1, (row.tsz_ms / maxMs) * 100).toFixed(2) : "1.00",
    tsgo_width: maxMs > 0 && row.tsgo_ms ? Math.max(1, (row.tsgo_ms / maxMs) * 100).toFixed(2) : "1.00",
    is_aggregate: Boolean(options.isAggregate),
  };
  decorated.source_files_json = escapeAttributeJson(decorated.source_files);
  decorated.comparison = comparison(decorated);
  decorated.speedup_label = formatSpeedupLabel(decorated.tsz_ms, decorated.tsgo_ms);
  return decorated;
}

function buildGroupedBenchmarks(data) {
  const allResults = data?.results || [];
  const results = allResults.filter((r) => r.tsz_ms != null && r.tsz_ms > 0 && r.tsgo_ms != null && r.tsgo_ms > 0);
  const failedResults = allResults.filter((r) => !(r.tsz_ms != null && r.tsz_ms > 0) && r.tsgo_ms != null && r.tsgo_ms > 0);
  const grouped = new Map();

  for (const row of results) {
    const category = categoryFor(row.name || "", row.lines);
    const bucket = grouped.get(category) || [];
    bucket.push(row);
    grouped.set(category, bucket);
  }

  ensureProjectRows(grouped);

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

  return { allResults, results, failedResults, grouped, categories };
}

export function getBenchmarkPages() {
  const data = loadBenchmarks();
  if (!data?.results?.length) return [];

  const { grouped, categories, failedResults } = buildGroupedBenchmarks(data);
  const pages = [];
  const seen = new Set();

  for (const category of categories) {
    const entries = (grouped.get(category) || []).slice();
    if (isExternalLibraryCategory(category)) {
      const libraryName = libraryNameForCategory(category);
      const aggregate = buildAggregateBenchmark(entries, libraryName);
      if (aggregate && !hasProjectRowForLibrary(category, grouped)) {
        entries.push({ ...aggregate, is_aggregate: true });
      }
    }

    entries.sort((a, b) => {
      const aLines = Number(a.lines) || 0;
      const bLines = Number(b.lines) || 0;
      if (bLines !== aLines) return bLines - aLines;
      return String(a.name || "").localeCompare(String(b.name || ""));
    });

    for (const row of entries) {
      if (seen.has(row.name)) continue;
      seen.add(row.name);
      pages.push(decorateRow(row, category, { isAggregate: row.is_aggregate }));
    }
  }

  for (const row of failedResults) {
    if (seen.has(row.name)) continue;
    seen.add(row.name);
    const category = categoryFor(row.name || "", row.lines);
    pages.push(decorateRow(row, category));
  }

  return pages;
}

function categoryDescription(category) {
  return categoryMeta(category).description || "";
}

function categoryTitle(category) {
  return categoryMeta(category).title || category;
}

function generateCharts(data) {
  if (!data?.results?.length) {
    return `<div class="bench-placeholder">No benchmark data is available for this local build.</div>`;
  }

  const { results, failedResults, grouped, categories } = buildGroupedBenchmarks(data);
  if (!results.length && !failedResults.length) return "";

  const barMaxWidth = 420;

  let html = "";
  for (const category of categories) {
    const isTinyCategory = category === "Tiny File Benchmarks";
    const entries = (grouped.get(category) || []).slice();
    const slug = categorySlug(category);
    const meta = categoryMeta(category);

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
    const isProject = isProjectCategory(category);
    const desc = isProject ? "" : categoryDescription(category);
    const repoLink = meta.repo
      ? ` <a class="bench-category-repo" href="${meta.repo}" target="_blank" rel="noopener noreferrer">${escapeHtml(meta.repoLabel || meta.repo)}</a>`
      : "";
    const title = categoryTitle(category);

    if (isTinyCategory) {
      html += `<section class="bench-category bench-tiny-category">
  <details id="${slug}" class="bench-category-details">
    <summary class="bench-category-title">${escapeHtml(title)}${repoLink}</summary>
    ${desc ? `<p class="bench-category-desc">${escapeHtml(desc)}</p>` : ""}
    <div class="bench-chart">\n`;
    } else {
      html += `<section class="bench-category${isProject ? " bench-project-category" : ""}">
  <h3 class="bench-category-title" id="${slug}">${escapeHtml(title)}${repoLink}</h3>
  ${desc ? `<p class="bench-category-desc">${escapeHtml(desc)}</p>` : ""}
  <div class="bench-chart">\n`;
    }

    for (const r of entries) {
      const decorated = decorateRow(r, category, { isAggregate: r.is_aggregate });
      const tszWidth = Math.max(2, (r.tsz_ms / maxMs) * barMaxWidth);
      const tsgoWidth = Math.max(2, (r.tsgo_ms / maxMs) * barMaxWidth);
      const winnerLabel = formatSpeedupLabel(r.tsz_ms, r.tsgo_ms);

      const metaParts = isProject
        ? [`${fmt(r.lines || 0)} lines`, `${fmt(r.kb || 0)} KB`]
        : [decorated.kind, `${fmt(r.lines || 0)} lines`, `${fmt(r.kb || 0)} KB`];

      html += `  <div class="bench-row">
    <div class="bench-name"><a href="${decorated.url}">${escapeHtml(decorated.display_name)}</a></div>
    <div class="bench-meta">${escapeHtml(metaParts.join(" · "))}</div>
    <p class="bench-focus">${escapeHtml(decorated.focus)}</p>
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
    <a class="bench-detail-link" href="${decorated.url}">View details</a>
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
      const decorated = decorateRow(r, categoryFor(r.name || "", r.lines));
      const tsgoWidth = maxFailMs > 0 ? Math.max(2, (r.tsgo_ms / maxFailMs) * barMaxWidth) : 2;
      html += `  <div class="bench-row">
    <div class="bench-name"><a href="${decorated.url}">${escapeHtml(displayName(r.name))}</a></div>
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
    <a class="bench-detail-link" href="${decorated.url}">View details</a>
  </div>\n`;
    }
    html += `  </div>
 </section>\n`;
  }

  return html;
}

export function getBenchmarkCharts() {
  return generateCharts(loadBenchmarks());
}
