import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { marked } from "marked";

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

function durationLabelFitsBar(label, widthPx) {
  const width = Number(widthPx);
  if (!Number.isFinite(width) || width <= 0) return false;

  // Bench labels use the monospace 0.8rem style plus 0.45rem horizontal padding
  // on each side. Estimate conservatively so labels move outside before clipping.
  const approximateTextWidth = String(label).length * 8;
  const horizontalPadding = 14.5;
  return width >= approximateTextWidth + horizontalPadding;
}

function renderBenchmarkBar(kind, widthPx, label) {
  const width = Number.isFinite(Number(widthPx)) ? Math.max(0, Number(widthPx)) : 0;
  const placementClass = durationLabelFitsBar(label, width) ? "" : " value-outside";
  return `<div class="bench-bar ${kind}${placementClass}" style="width: ${width.toFixed(2)}px">
          <span class="bench-bar-value">${label}</span>
        </div>`;
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

function hasTiming(value) {
  const time = Number(value);
  return Number.isFinite(time) && time > 0;
}

function fastestTiming(row) {
  const timings = [row?.tsz_ms, row?.tsgo_ms].map(Number).filter((time) => Number.isFinite(time) && time > 0);
  return timings.length ? Math.min(...timings) : Infinity;
}

function tszSpeedupScore(row) {
  const tsz = Number(row?.tsz_ms);
  const tsgo = Number(row?.tsgo_ms);
  if (!Number.isFinite(tsz) || !Number.isFinite(tsgo) || tsz <= 0 || tsgo <= 0) {
    return -Infinity;
  }
  return tsgo / tsz;
}

function compareByTszSpeedup(a, b) {
  const aScore = tszSpeedupScore(a);
  const bScore = tszSpeedupScore(b);
  if (aScore !== bScore) return bScore - aScore;

  const aFastest = fastestTiming(a);
  const bFastest = fastestTiming(b);
  if (aFastest !== bFastest) return aFastest - bFastest;

  return String(a?.name || "").localeCompare(String(b?.name || ""));
}

function hasSuccessfulTiming(row) {
  return !row?.status && row?.winner !== "error" && hasTiming(row?.tsz_ms) && hasTiming(row?.tsgo_ms);
}

function isFailedBenchmark(row) {
  if (!row || hasSuccessfulTiming(row)) return false;
  return Boolean(row.status) || row.winner === "error" || hasTiming(row.tsz_ms) || hasTiming(row.tsgo_ms);
}

function statusLabel(row) {
  return String(row?.status || "timing unavailable");
}

function firstPresent(...values) {
  for (const value of values) {
    if (value !== undefined && value !== null && value !== "") return value;
  }
  return null;
}

const TINY_BENCHMARK_MAX_LINES = 200;

const EXPECTED_PROJECT_BENCHMARKS = [
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
];

const COMPATIBILITY_CORPUS_ROWS = [
  {
    name: "kysely-project",
    label: "Kysely",
    owner: "Tracks 2, 3, 5, 6",
    family: "contextual generics, guards, indexed/property access",
  },
  {
    name: "zod-project",
    label: "Zod",
    owner: "Tracks 2, 3, 4, 7",
    family: "recursive conditionals, object guards, class/generic identity",
  },
  {
    name: "ts-toolbelt-project",
    label: "ts-toolbelt",
    owner: "Tracks 2, 3",
    family: "recursive type evaluation pressure",
  },
  {
    name: "type-fest-project",
    label: "type-fest",
    owner: "Tracks 2, 3, 5",
    family: "mapped/conditional/key-space utility surface",
  },
  {
    name: "ts-essentials-project",
    label: "ts-essentials",
    owner: "Tracks 2, 3, 5",
    family: "utility types plus recursive JSON shapes",
  },
  {
    name: "large-ts-repo",
    label: "large-ts-repo",
    owner: "Tracks 1, 7, 10",
    family: "residency/runtime/project graph stress",
  },
  {
    name: "nextjs",
    label: "Next.js full project",
    owner: "Tracks 1, 7, 9",
    family: "module graph plus generated app dependencies",
  },
];

function withExpectedProjectRows(results) {
  const rows = Array.isArray(results) ? results.slice() : [];
  const existingNames = new Set(rows.map((row) => row?.name).filter(Boolean));

  for (const name of EXPECTED_PROJECT_BENCHMARKS) {
    if (existingNames.has(name)) continue;
    rows.push({
      name,
      lines: 0,
      kb: 0,
      tsz_ms: null,
      tsgo_ms: null,
      tsz_lps: null,
      tsgo_lps: null,
      winner: "error",
      ratio: 0,
      status: "not recorded in latest benchmark artifact",
    });
  }

  return rows;
}

function compatibilityState(row) {
  if (hasSuccessfulTiming(row)) {
    return {
      className: "green",
      stateLabel: "Green",
      exitClass: "exit success",
      phase: "check",
      diagnosticDeltas: "none recorded",
    };
  }

  const status = String(row?.status || "").toLowerCase();
  if (!row || status.includes("not recorded") || status.includes("fixture") || status.includes("tsc fixture")) {
    return {
      className: "gray",
      stateLabel: "Gray",
      exitClass: status.includes("tsc fixture") ? "fixture invalid" : "missing or incomplete artifact",
      phase: status.includes("fixture") ? "fixture setup" : "artifact",
      diagnosticDeltas: "not available",
    };
  }

  if (status.includes("diagnostic mismatch")) {
    return {
      className: "yellow",
      stateLabel: "Yellow",
      exitClass: "diagnostic mismatch",
      phase: firstPresent(row?.compatibility?.phase, "check"),
      diagnosticDeltas: firstPresent(row?.compatibility?.diagnostic_deltas, "not captured by latest artifact"),
    };
  }

  return {
    className: "red",
    stateLabel: "Red",
    exitClass: status.includes("timeout") ? "timeout" : "nonzero exit",
    phase: firstPresent(row?.compatibility?.phase, "check"),
    diagnosticDeltas: firstPresent(row?.compatibility?.diagnostic_deltas, "not captured by latest artifact"),
  };
}

function compatibilityRowFor(definition, allResults) {
  const row = allResults.find((candidate) => candidate?.name === definition.name);
  return {
    ...definition,
    ...compatibilityState(row),
    row,
    lines: row?.lines || 0,
    status: row?.status || "not recorded in latest benchmark artifact",
    url: benchmarkUrl({ name: definition.name }),
  };
}

const PROJECT_README_PATHS = {
  "large-ts-repo": [".target-bench/external/large-ts-repo/README.md"],
  nextjs: [".target-bench/external/next.js/README.md"],
  "nextjs-fresh-app": [".target-bench/external/next-app-live/README.md"],
  "vite-vanilla-ts-app": [".target-bench/external/vite-vanilla-ts-live/README.md"],
  "rxjs-project": [".target-bench/external/rxjs/README.md"],
  "type-fest-project": [".target-bench/external/type-fest/readme.md", ".target-bench/external/type-fest/README.md"],
  "zod-project": [".target-bench/external/zod/README.md"],
  "kysely-project": [".target-bench/external/kysely/README.md"],
  "utility-types-project": [".target-bench/external/utility-types/README.md"],
  "ts-toolbelt-project": [".target-bench/external/ts-toolbelt/README.md"],
  "ts-essentials-project": [".target-bench/external/ts-essentials/README.md"],
};

const PROJECT_README_URLS = {
  "large-ts-repo": "https://raw.githubusercontent.com/mohsen1/large-ts-repo/e1b22bda18664a507ed0da19c155e0365d585b18/README.md",
  "rxjs-project": "https://raw.githubusercontent.com/ReactiveX/rxjs/e5351d02e225e275ac0e497c7b66eaa5f0c88791/README.md",
  "zod-project": "https://raw.githubusercontent.com/colinhacks/zod/93b0b6892cc0cfee8d0bec4e2e1242c7df771f95/README.md",
  "utility-types-project": "https://raw.githubusercontent.com/piotrwitek/utility-types/2ee1f6ecb241651ab22390fee7ee5349942efda2/README.md",
  "ts-toolbelt-project": "https://raw.githubusercontent.com/millsp/ts-toolbelt/b8a49285e3ed3a7d8bb8e0b433389eac46a5f140/README.md",
  "ts-essentials-project": "https://raw.githubusercontent.com/ts-essentials/ts-essentials/5abe8700b42068048bd3c368e0531b6defe56558/README.md",
};

const NEXTJS_FRESH_APP_README = `# Fresh Next.js app benchmark

This fixture is generated by \`scripts/bench/generate-next-app-fixture.mjs\`.

Each benchmark run recreates the app, installs current npm versions, and type-checks the generated Next.js project with:

\`\`\`sh
tsz --noEmit -p tsconfig.json
tsgo --noEmit -p tsconfig.json
\`\`\`

The app intentionally imports and uses common type-heavy dependencies:

- \`zod\`
- \`@tanstack/react-query\`
- \`react-hook-form\`
- \`type-fest\`
- \`ts-pattern\`
- \`superjson\`
- \`date-fns\`
- \`clsx\`
- \`zustand\`
- \`valibot\`

The generated source mixes App Router pages, server actions, schema inference, discriminated unions, form helpers, query typing, store typing, and JSON-safe utility types so the benchmark reflects a modern application rather than a tiny startup file.`;

const REMOTE_FIXTURE_REFS = {
  "utility-types": "2ee1f6ecb241651ab22390fee7ee5349942efda2",
  "ts-toolbelt": "b8a49285e3ed3a7d8bb8e0b433389eac46a5f140",
  "ts-essentials": "5abe8700b42068048bd3c368e0531b6defe56558",
};

const remoteSourceCache = new Map();

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
  if (name === "nextjs-fresh-app") return "Projects: fresh Next.js app";
  if (name === "vite-vanilla-ts-app") return "Projects: fresh Vite app";
  if (name === "rxjs-project") return "Projects: rxjs";
  if (name === "type-fest-project") return "Projects: type-fest";
  if (name === "zod-project") return "Projects: zod";
  if (name === "kysely-project") return "Projects: kysely";
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
    "Projects: fresh Next.js app": {
      title: "Fresh Next.js app",
    },
    "Projects: fresh Vite app": {
      title: "Fresh Vite app",
      repo: "https://github.com/vitejs/vite",
      repoLabel: "vitejs/vite",
    },
    "Projects: rxjs": {
      title: "RxJS",
      repo: "https://github.com/ReactiveX/rxjs",
      repoLabel: "ReactiveX/rxjs",
    },
    "Projects: type-fest": {
      title: "type-fest",
      repo: "https://github.com/sindresorhus/type-fest",
      repoLabel: "sindresorhus/type-fest",
    },
    "Projects: zod": {
      title: "Zod",
      repo: "https://github.com/colinhacks/zod",
      repoLabel: "colinhacks/zod",
    },
    "Projects: kysely": {
      title: "Kysely",
      repo: "https://github.com/kysely-org/kysely",
      repoLabel: "kysely-org/kysely",
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
      title: "utility-types files",
      description: "Real-world utility-types file-level benchmark set from pinned snapshot.",
      repo: "https://github.com/piotrwitek/utility-types",
      repoLabel: "piotrwitek/utility-types",
    },
    "Single file: ts-toolbelt": {
      title: "ts-toolbelt files",
      description: "Real-world ts-toolbelt file-level benchmark set with type-heavy examples.",
      repo: "https://github.com/millsp/ts-toolbelt",
      repoLabel: "millsp/ts-toolbelt",
    },
    "Single file: ts-essentials": {
      title: "ts-essentials files",
      description: "Real-world ts-essentials file-level benchmark set from pinned snapshot.",
      repo: "https://github.com/ts-essentials/ts-essentials",
      repoLabel: "ts-essentials/ts-essentials",
    },
    "Tiny File Benchmarks": {
      title: "Tiny files",
      description: "Small fixture files moved below the fold.",
    },
    "General Benchmarks": {
      title: "Compiler scenarios",
      description: "Focused compiler behavior on representative mixed workloads.",
    },
    "Synthetic Type Workloads": {
      title: "Generated type workloads",
      description: "Generated stress tests that isolate specific type-system patterns.",
    },
    "Solver Stress Tests": {
      title: "Solver stress",
      description: "Upper-bound tests for recursive, mapped, and conditional type complexity.",
    },
  }[category] || { description: "" };
}

function displayName(name) {
  if (name === "privacyFunctionParameterDeclFile.ts") {
    return "Privacy function parameter declaration file";
  }
  if (name === "rxjs-project") return "RxJS project";
  if (name === "type-fest-project") return "type-fest project";
  if (name === "zod-project") return "Zod project";
  if (name === "nextjs-fresh-app") return "Fresh Next.js app";
  if (name === "vite-vanilla-ts-app") return "Fresh Vite app";
  if (name === "kysely-project") return "Kysely project";

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

function countFromName(name, pattern) {
  const match = String(name || "").match(pattern);
  return match ? Number(match[1]) : null;
}

function generatedUnionSource(count) {
  const lines = [
    "// Union type stress test - discriminated unions with many members",
    "",
    "type StressEvent =",
  ];
  for (let i = 0; i < count; i += 1) {
    lines.push(`  | { type: "event${i}"; payload${i}: string; timestamp: number }${i === count - 1 ? ";" : ""}`);
  }
  lines.push("", "function handleEvent(event: StressEvent): string {", "  switch (event.type) {");
  for (let i = 0; i < count; i += 1) {
    lines.push(`    case "event${i}": return event.payload${i};`);
  }
  lines.push("    default: throw new Error(\"unreachable\");", "  }", "}", "");
  for (let i = 0; i < count; i += 10) {
    lines.push(`function isEvent${i}(e: StressEvent): e is Extract<StressEvent, { type: "event${i}" }> {`);
    lines.push(`  return e.type === "event${i}";`);
    lines.push("}", "");
  }
  return lines.join("\n").trimEnd();
}

function generatedClassesSource(count) {
  const lines = ["// Synthetic TypeScript benchmark file", ""];
  for (let i = 0; i < count; i += 1) {
    lines.push(`export interface Config${i} {`);
    lines.push("  readonly id: number;");
    lines.push("  name: string;");
    lines.push("  enabled: boolean;");
    lines.push("  options?: Record<string, unknown>;");
    lines.push("}", "");
    lines.push(`export class Service${i} implements Config${i} {`);
    lines.push(`  readonly id: number = ${i};`);
    lines.push("  name: string;");
    lines.push("  enabled: boolean = true;");
    lines.push("  private items: string[] = [];");
    lines.push("  constructor(name: string) { this.name = name; }");
    lines.push("  getId(): number { return this.id; }");
    lines.push("  getName(): string { return this.name; }");
    lines.push("  setName(value: string): void { this.name = value; }");
    lines.push("  isEnabled(): boolean { return this.enabled; }");
    lines.push("  addItem(item: string): void { this.items.push(item); }");
    lines.push("  getItems(): readonly string[] { return this.items; }");
    lines.push(`  static create(name: string): Service${i} { return new Service${i}(name); }`);
    lines.push("}", "");
  }
  return lines.join("\n").trimEnd();
}

function generatedGenericFunctionsSource(count) {
  const lines = [
    "// Complex TypeScript with generics, unions, and conditional types",
    '/// <reference lib="es2015.promise" />',
    "",
    "type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;",
    "",
    "interface Result<T, E = Error> {",
    "  ok: boolean;",
    "  value?: T;",
    "  error?: E;",
    "}",
    "",
  ];
  for (let i = 0; i < count; i += 1) {
    lines.push(`async function process${i}<T extends Record<string, unknown>>(`);
    lines.push("  input: T,");
    lines.push("  options?: DeepPartial<{ timeout: number; retries: number }>");
    lines.push("): Promise<Result<T>> {");
    lines.push("  const timeout = options?.timeout ?? 1000;");
    lines.push("  const retries = options?.retries ?? 3;");
    lines.push("  for (let attempt = 0; attempt < retries; attempt++) {");
    lines.push("    try {");
    lines.push("      const result = await Promise.resolve(input);");
    lines.push("      if (timeout < 0) throw new Error(\"timeout\");");
    lines.push("      return { ok: true, value: result };");
    lines.push("    } catch (e) {");
    lines.push("      if (attempt === retries - 1) return { ok: false, error: e as Error };");
    lines.push("    }");
    lines.push("  }");
    lines.push("  return { ok: false, error: new Error(\"exhausted\") };");
    lines.push("}", "");
  }
  return lines.join("\n").trimEnd();
}

function generatedOptionalChainSource(count, deep) {
  const scoreExpr = "(options?.timeout ?? 1000) + (options?.nested?.transport?.backoff?.base ?? 10) + (options?.nested?.transport?.backoff?.max ?? 100) + (options?.nested?.transport?.backoff?.jitter ?? 1) + (options?.nested?.flags?.safe ? 1 : 0) + (options?.nested?.flags?.fast ? 1 : 0) + (options?.retries ?? 3)";
  const lines = [
    deep
      ? "// DeepPartial + optional-chain hotspot benchmark."
      : "// Shallow optional-chain control benchmark.",
    "",
  ];
  if (deep) {
    lines.push("type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;");
    lines.push("type Normalize<T> = T extends object ? { [P in keyof T]: Normalize<T[P]> } : T;");
    lines.push("type DeepInput<T> = DeepPartial<Normalize<T>>;", "");
  }
  lines.push(`interface RetryOptions${deep ? "" : "Shallow"} {`);
  lines.push(`  timeout${deep ? "" : "?"}: number;`);
  lines.push(`  retries${deep ? "" : "?"}: number;`);
  lines.push(`  nested${deep ? "" : "?"}: {`);
  lines.push(`    transport${deep ? "" : "?"}: { backoff${deep ? "" : "?"}: { base${deep ? "" : "?"}: number; max${deep ? "" : "?"}: number; jitter${deep ? "" : "?"}: number; }; };`);
  lines.push(`    flags${deep ? "" : "?"}: { fast${deep ? "" : "?"}: boolean; safe${deep ? "" : "?"}: boolean; };`);
  lines.push("  };");
  lines.push("}", "");
  for (let i = 0; i < count; i += 1) {
    lines.push(`function ${deep ? "deepPartialHotspot" : "shallowOptionalControl"}${i}(`);
    lines.push(`  options?: ${deep ? "DeepInput<RetryOptions>" : "RetryOptionsShallow"}`);
    lines.push("): number {");
    lines.push("  let score = 0;");
    for (let j = 0; j < 34; j += 1) {
      lines.push(`  score += ${scoreExpr};`);
    }
    lines.push("  return score;");
    lines.push("}", "");
  }
  return lines.join("\n").trimEnd();
}

function generatedMappedTypeSource(count) {
  const lines = [
    "// Mapped type expansion stress test",
    "",
    "type MyOptional<T> = { [K in keyof T]?: T[K] };",
    "type MyRequired<T> = { [K in keyof T]-?: T[K] };",
    "type MyReadonly<T> = { readonly [K in keyof T]: T[K] };",
    "type MyMutable<T> = { -readonly [K in keyof T]: T[K] };",
    "type Getters<T> = { [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K] };",
    "type Setters<T> = { [K in keyof T as `set${Capitalize<string & K>}`]: (val: T[K]) => void };",
    "",
    "interface BigObject {",
  ];
  for (let i = 0; i < count; i += 1) lines.push(`  prop${i}: string;`);
  lines.push("}", "");
  lines.push("type Partial1 = MyOptional<BigObject>;");
  lines.push("type Readonly1 = MyReadonly<BigObject>;");
  lines.push("type Both = MyReadonly<MyOptional<BigObject>>;");
  lines.push("type BigGetters = Getters<BigObject>;");
  lines.push("type BigSetters = Setters<BigObject>;");
  lines.push("type DeepOptional<T> = T extends object ? { [K in keyof T]?: DeepOptional<T[K]> } : T;");
  lines.push("type DeepBigObject = DeepOptional<BigObject>;", "");
  lines.push("declare const partial: Partial1;");
  lines.push("declare const getters: BigGetters;");
  lines.push("declare const deep: DeepBigObject;");
  lines.push("const _prop0 = partial.prop0;");
  return lines.join("\n").trimEnd();
}

function generatedConditionalDistributionSource(count) {
  const lines = [
    "// Conditional type distribution stress test",
    "",
    "type ExtractString<T> = T extends string ? T : never;",
    "type ToArray<T> = T extends any ? T[] : never;",
    "type Flatten<T> = T extends (infer U)[] ? Flatten<U> : T;",
    "",
    "type BigUnion =",
  ];
  for (let i = 0; i < count; i += 1) lines.push(`  | "value${i}"${i === count - 1 ? ";" : ""}`);
  lines.push("", "type Distributed1 = ToArray<BigUnion>;");
  lines.push("type Distributed2 = ExtractString<BigUnion | number>;");
  lines.push("type ChainedConditional<T> = T extends string ? `prefix_${T}` : T extends number ? T : never;");
  lines.push("type Applied = ChainedConditional<BigUnion>;");
  lines.push("type NestedConditional<T> = T extends `value${infer N}` ? N extends `${infer D}${infer Rest}` ? D : never : never;");
  lines.push("type Extracted = NestedConditional<BigUnion>;");
  lines.push("declare const distributed: Distributed1;");
  lines.push("declare const applied: Applied;");
  lines.push("declare const extracted: Extracted;");
  return lines.join("\n").trimEnd();
}

function generatedTemplateLiteralSource(count) {
  const maxVariants = Math.min(count, 50);
  const lines = ["// Template literal type expansion stress test", ""];
  for (const name of ["Colors", "Sizes", "Variants"]) {
    lines.push(`type ${name} =`);
    const prefix = name === "Colors" ? "color" : name === "Sizes" ? "size" : "variant";
    for (let i = 0; i < maxVariants; i += 1) lines.push(`  | "${prefix}${i}"${i === maxVariants - 1 ? ";" : ""}`);
    lines.push("");
  }
  lines.push("type ProductSmall = `${Colors}-${Sizes}`;");
  lines.push("type ProductMedium = `${Colors}-${Sizes}-${Variants}`;");
  lines.push("type Prefixed = `prefix_${Colors}`;");
  lines.push("type Suffixed = `${Colors}_suffix`;");
  lines.push("type Wrapped = `[${Colors}]`;");
  lines.push("type NestedTemplate = `start_${`mid_${Colors}`}_end`;");
  lines.push("declare const product: ProductSmall;");
  lines.push("declare const prefixed: Prefixed;");
  return lines.join("\n").trimEnd();
}

function generatedRecursiveGenericSource(depth) {
  const lines = [
    "// Recursive generic type instantiation stress test",
    "",
    "type LinkedList<T> = { value: T; next: LinkedList<T> | null };",
    "type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;",
    "type DeepReadonly<T> = T extends object ? { readonly [P in keyof T]: DeepReadonly<T[P]> } : T;",
    "",
  ];
  for (let i = 0; i < depth; i += 1) lines.push(`type Wrap${i}<T> = { layer${i}: T };`);
  let chain = "string";
  for (let i = Math.min(depth, 40) - 1; i >= 0; i -= 1) chain = `Wrap${i}<${chain}>`;
  lines.push("", "type DeepWrapped = " + chain + ";");
  lines.push("declare const deep: DeepWrapped;");
  lines.push("declare function extract<T>(x: Wrap0<T>): T;");
  lines.push("const _test = extract(deep);");
  lines.push("declare const list: LinkedList<number>;");
  lines.push("declare function mapList<T, U>(l: LinkedList<T>, f: (x: T) => U): LinkedList<U>;");
  lines.push("const mapped = mapList(list, x => x.toString());");
  return lines.join("\n").trimEnd();
}

function generatedDeepSubtypeSource(depth) {
  const lines = [
    "// Deep subtype checking stress test",
    "",
    "interface TreeNode<T> { value: T; children: TreeNode<T>[]; }",
    "interface MutualA<T> { data: T; ref: MutualB<T>; }",
    "interface MutualB<T> { info: T; back: MutualA<T>; }",
    "type Json = string | number | boolean | null | Json[] | { [key: string]: Json };",
    "",
    "class Base0 { x0: string = \"\"; }",
  ];
  for (let i = 1; i < Math.min(depth, 50); i += 1) {
    lines.push(`class Base${i} extends Base${i - 1} { x${i}: string = ""; }`);
  }
  let deepFn = "string";
  for (let i = 0; i < Math.min(depth, 30); i += 1) deepFn = `(x: ${deepFn}) => void`;
  lines.push("", "type CovariantContainer<T> = { get(): T };");
  lines.push("type ContravariantContainer<T> = { set(x: T): void };");
  lines.push("type InvariantContainer<T> = { get(): T; set(x: T): void };");
  lines.push(`type DeepFunction = ${deepFn};`);
  lines.push("declare const tree1: TreeNode<string>;");
  lines.push("const _check: TreeNode<string | number> = tree1;");
  return lines.join("\n").trimEnd();
}

function generatedIntersectionSource(count) {
  const lines = ["// Intersection type stress test", ""];
  for (let i = 0; i < count; i += 1) {
    lines.push(`interface Part${i} {`);
    lines.push(`  prop${i}: string;`);
    lines.push("  shared: number;");
    lines.push(`  method${i}(): number;`);
    lines.push("}", "");
  }
  let intersection = "Part0";
  for (let i = 1; i < Math.min(count, 50); i += 1) intersection += ` & Part${i}`;
  lines.push(`type BigIntersection = ${intersection};`);
  lines.push("type OverloadIntersection = ((x: string) => string) & ((x: number) => number) & ((x: boolean) => boolean);");
  lines.push("declare const big: BigIntersection;");
  lines.push("const _prop0 = big.prop0;");
  lines.push("const _shared = big.shared;");
  lines.push(`const _propLast = big.prop${Math.min(count, 50) - 1};`);
  return lines.join("\n").trimEnd();
}

function generatedCfaSource(count) {
  const lines = [
    "// Control flow analysis stress test",
    "",
    "type Entity =",
  ];
  for (let i = 0; i < count; i += 1) {
    lines.push(`  | { kind: "type${i}"; data${i}: string; common: number }${i === count - 1 ? ";" : ""}`);
  }
  lines.push("", "function processEntity(e: Entity): string {", "  switch (e.kind) {");
  for (let i = 0; i < count; i += 1) lines.push(`    case "type${i}": return e.data${i};`);
  lines.push("    default: throw new Error(\"unreachable\");", "  }", "}", "");
  lines.push("function processWithIf(e: Entity): string {");
  for (let i = 0; i < count; i += 1) {
    lines.push(`  if (e.kind === "type${i}") return e.data${i};`);
  }
  lines.push("  return processEntity(e);", "}");
  return lines.join("\n").trimEnd();
}

function generatedBctSource(count) {
  const lines = ["// Best Common Type O(N^2) stress test", "", "class Base { base: string = \"\"; }"];
  for (let i = 0; i < count; i += 1) lines.push(`class Derived${i} extends Base { prop${i}: number = ${i}; }`);
  lines.push("", `const items = [${Array.from({ length: count }, (_, i) => `new Derived${i}()`).join(", ")}];`);
  lines.push("function pickOne(index: number) {");
  for (let i = 0; i < count; i += 1) lines.push(`  if (index === ${i}) return new Derived${i}();`);
  lines.push("  return new Base();", "}", "function identity<T>(x: T): T { return x; }");
  lines.push(`const mixed = [${Array.from({ length: count }, (_, i) => `identity(new Derived${i}())`).join(", ")}];`);
  lines.push("declare const flag: number;");
  lines.push(`const chosen = ${Array.from({ length: count }, (_, i) => `flag === ${i} ? new Derived${i}() :`).join(" ")} new Base();`);
  return lines.join("\n").trimEnd();
}

function generatedConstraintConflictSource(count) {
  const lines = ["// Constraint Conflict Detection O(N^2) stress test", ""];
  for (let i = 0; i < count; i += 1) lines.push(`interface Constraint${i} { key${i}: string; shared: number; }`);
  lines.push("");
  for (let i = 0; i < count; i += 1) lines.push(`declare function constrain${i}<T extends Constraint${i}>(x: T): T;`);
  lines.push("");
  for (let i = 0; i < count; i += 1) {
    const keys = Array.from({ length: i + 1 }, (_, j) => `key${j}: "val"`).join(", ");
    lines.push(`const obj${i} = { shared: ${i}, ${keys} };`);
  }
  lines.push("");
  for (let i = 0; i < count; i += 1) lines.push(`const res${i} = constrain${i}(obj${i});`);
  lines.push(`function multiConstrained<T extends ${Array.from({ length: count }, (_, i) => `Constraint${i}`).join(" & ")}>(x: T): T { return x; }`);
  lines.push(`const allConstraints = { shared: 0, ${Array.from({ length: count }, (_, i) => `key${i}: "val"`).join(", ")} };`);
  lines.push("const _result = multiConstrained(allConstraints);");
  return lines.join("\n").trimEnd();
}

function generatedMappedComplexSource(count) {
  const lines = [
    "// Mapped Type Complex Template Expansion O(N^2) stress test",
    "",
    "type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;",
    "type Stringify<T> = { [K in keyof T]: T[K] extends number ? string : T[K] extends boolean ? \"true\" | \"false\" : T[K] extends string ? T[K] : string };",
    "type Validate<T> = { [K in keyof T]: T[K] extends string | number ? { valid: true; value: T[K] } : { valid: false; value: never } };",
    "type Nullable<T> = { [K in keyof T]: T[K] | null | undefined };",
    "type Promisify<T> = { [K in keyof T]: Promise<T[K]> };",
    "type FormField<T> = T extends string ? { type: \"text\"; value: T } : T extends number ? { type: \"number\"; value: T } : T extends boolean ? { type: \"checkbox\"; value: T } : T extends (infer U)[] ? { type: \"list\"; items: FormField<U>[] } : T extends object ? { type: \"group\"; fields: FormFields<T> } : { type: \"unknown\"; value: T };",
    "type FormFields<T> = { [K in keyof T]: FormField<T[K]> };",
    "",
    "interface BigModel {",
  ];
  for (let i = 0; i < count; i += 1) {
    const type = ["string", "number", "boolean", "string[]", "{ nested: string; count: number }"][i % 5];
    lines.push(`  field${i}: ${type};`);
  }
  lines.push("}", "");
  lines.push("type BigForm = FormFields<BigModel>;");
  lines.push("type BigStringified = Stringify<BigModel>;");
  lines.push("type BigValidated = Validate<BigModel>;");
  lines.push("type Chained3 = FormFields<Nullable<BigModel>>;");
  lines.push("declare const form: BigForm;");
  lines.push(`const _fLast = form.field${count - 1};`);
  return lines.join("\n").trimEnd();
}

function generatedBenchmarkSource(name) {
  const unionCount = countFromName(name, /^(\d+)\s+union members$/i);
  if (unionCount) return generatedUnionSource(unionCount);

  const classCount = countFromName(name, /^(\d+)\s+classes$/i);
  if (classCount) return generatedClassesSource(classCount);

  const functionCount = countFromName(name, /^(\d+)\s+generic functions$/i);
  if (functionCount) return generatedGenericFunctionsSource(functionCount);

  const deepOptionalCount = countFromName(name, /^DeepPartial optional-chain N=(\d+)$/i);
  if (deepOptionalCount) return generatedOptionalChainSource(deepOptionalCount, true);

  const shallowOptionalCount = countFromName(name, /^Shallow optional-chain N=(\d+)$/i);
  if (shallowOptionalCount) return generatedOptionalChainSource(shallowOptionalCount, false);

  const mappedCount = countFromName(name, /^Mapped type keys=(\d+)$/i);
  if (mappedCount) return generatedMappedTypeSource(mappedCount);

  const conditionalCount = countFromName(name, /^Conditional dist N=(\d+)$/i);
  if (conditionalCount) return generatedConditionalDistributionSource(conditionalCount);

  const templateCount = countFromName(name, /^Template literal N=(\d+)$/i);
  if (templateCount) return generatedTemplateLiteralSource(templateCount);

  const recursiveDepth = countFromName(name, /^Recursive generic depth=(\d+)$/i);
  if (recursiveDepth) return generatedRecursiveGenericSource(recursiveDepth);

  const deepSubtypeDepth = countFromName(name, /^Deep subtype depth=(\d+)$/i);
  if (deepSubtypeDepth) return generatedDeepSubtypeSource(deepSubtypeDepth);

  const intersectionCount = countFromName(name, /^Intersection N=(\d+)$/i);
  if (intersectionCount) return generatedIntersectionSource(intersectionCount);

  const cfaCount = countFromName(name, /^CFA branches=(\d+)$/i);
  if (cfaCount) return generatedCfaSource(cfaCount);

  const bctCount = countFromName(name, /^BCT candidates=(\d+)$/i);
  if (bctCount) return generatedBctSource(bctCount);

  const constraintCount = countFromName(name, /^Constraint conflicts N=(\d+)$/i);
  if (constraintCount) return generatedConstraintConflictSource(constraintCount);

  const mappedComplexCount = countFromName(name, /^Mapped complex template keys=(\d+)$/i);
  if (mappedComplexCount) return generatedMappedComplexSource(mappedComplexCount);

  return null;
}

function snippetForBenchmark(row, category) {
  const name = String(row.name || "");
  const generatedSource = generatedBenchmarkSource(name);
  if (generatedSource) return generatedSource;

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

function externalFixtureUrl(name) {
  const fixtureName = String(name || "");
  if (fixtureName.startsWith("utility-types/")) {
    const rel = fixtureName.slice("utility-types/".length);
    return `https://raw.githubusercontent.com/piotrwitek/utility-types/${REMOTE_FIXTURE_REFS["utility-types"]}/src/${rel}`;
  }
  if (fixtureName.startsWith("ts-toolbelt/")) {
    const rel = fixtureName.slice("ts-toolbelt/".length);
    return `https://raw.githubusercontent.com/millsp/ts-toolbelt/${REMOTE_FIXTURE_REFS["ts-toolbelt"]}/sources/${rel}`;
  }
  if (fixtureName.startsWith("ts-essentials/")) {
    const rel = fixtureName.slice("ts-essentials/".length).replace(/\.ts$/, "/index.ts");
    return `https://raw.githubusercontent.com/ts-essentials/ts-essentials/${REMOTE_FIXTURE_REFS["ts-essentials"]}/lib/${rel}`;
  }
  return null;
}

function readRemoteText(url) {
  if (!url) return null;
  if (remoteSourceCache.has(url)) return remoteSourceCache.get(url);

  try {
    const text = execFileSync("curl", ["-fsSL", url], {
      encoding: "utf8",
      timeout: 10000,
      maxBuffer: 1024 * 1024,
      stdio: ["ignore", "pipe", "ignore"],
    }).trimEnd();
    remoteSourceCache.set(url, text);
    return text;
  } catch {
    remoteSourceCache.set(url, null);
    return null;
  }
}

function readExternalFixtureSource(name) {
  const sourcePath = externalFixturePath(name);
  if (sourcePath) {
    try {
      return fs.readFileSync(sourcePath, "utf8").trimEnd();
    } catch {
      // Deployed static builds may not have the prepared benchmark fixtures.
    }
  }

  return readRemoteText(externalFixtureUrl(name));
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

function readProjectReadme(row, category) {
  if (!isProjectCategory(category)) return null;

  if (row.readme) return truncateReadme(row.readme);

  const candidates = PROJECT_README_PATHS[row.name] || [];
  for (const candidate of candidates) {
    try {
      const text = fs.readFileSync(path.join(ROOT, candidate), "utf8").trim();
      if (!text) continue;
      return truncateReadme(text);
    } catch {
      // README is optional for local benchmark fixtures that have not been prepared.
    }
  }

  if (row.name === "nextjs-fresh-app") return NEXTJS_FRESH_APP_README;

  const remoteReadme = readRemoteText(PROJECT_README_URLS[row.name]);
  if (remoteReadme) return truncateReadme(remoteReadme);

  return null;
}

function truncateReadme(text) {
  const trimmed = String(text || "").trim();
  if (!trimmed) return null;
  return trimmed.length > 18000 ? `${trimmed.slice(0, 18000).trimEnd()}\n\n...` : trimmed;
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
  const readme = readProjectReadme(row, category);
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
    readme,
    readme_html: readme ? marked.parse(readme) : "",
    tsz_command: benchmarkCommand(row, category, "tsz"),
    tsgo_command: benchmarkCommand(row, category, "tsgo"),
    tsz_time: row.tsz_ms ? formatDurationMs(row.tsz_ms, 2) : "",
    tsgo_time: row.tsgo_ms ? formatDurationMs(row.tsgo_ms, 2) : "",
    tsz_width: maxMs > 0 && row.tsz_ms ? Math.max(1, (row.tsz_ms / maxMs) * 100).toFixed(2) : "1.00",
    tsgo_width: maxMs > 0 && row.tsgo_ms ? Math.max(1, (row.tsgo_ms / maxMs) * 100).toFixed(2) : "1.00",
    status_label: row.status ? statusLabel(row) : "",
    failed: isFailedBenchmark(row),
    is_aggregate: Boolean(options.isAggregate),
  };
  decorated.source_files_json = escapeAttributeJson(decorated.source_files);
  decorated.comparison = comparison(decorated);
  decorated.speedup_label = formatSpeedupLabel(decorated.tsz_ms, decorated.tsgo_ms);
  return decorated;
}

function buildGroupedBenchmarks(data) {
  const allResults = withExpectedProjectRows(data?.results);
  const results = allResults.filter(hasSuccessfulTiming);
  const grouped = new Map();

  for (const row of results) {
    const category = categoryFor(row.name || "", row.lines);
    const bucket = grouped.get(category) || [];
    bucket.push(row);
    grouped.set(category, bucket);
  }

  const successfulNames = new Set([
    ...results.map((row) => row.name),
    ...[...grouped.values()].flat().map((row) => row.name),
  ]);
  const failedResults = allResults.filter((row) => isFailedBenchmark(row) && !successfulNames.has(row.name));

  const order = [
    "Projects: large-ts-repo",
    "Projects: utility-types",
    "Projects: ts-toolbelt",
    "Projects: ts-essentials",
    "Projects: next.js",
    "Projects: fresh Next.js app",
    "Projects: fresh Vite app",
    "Projects: rxjs",
    "Projects: type-fest",
    "Projects: zod",
    "Projects: kysely",
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

function categoryBelongsToMode(category, mode) {
  if (mode === "projects") return isProjectCategory(category);
  if (mode === "micro") return !isProjectCategory(category) && category !== "Tiny File Benchmarks";
  return category !== "Tiny File Benchmarks";
}

function failedBelongsToMode(row, mode) {
  const category = categoryFor(row.name || "", row.lines);
  return categoryBelongsToMode(category, mode);
}

function generateCharts(data, mode = "projects") {
  if (!data?.results?.length) {
    return `<div class="bench-placeholder">No benchmark data is available for this local build.</div>`;
  }

  const { results, failedResults, grouped, categories } = buildGroupedBenchmarks(data);
  if (!results.length && !failedResults.length) return "";

  const barMaxWidth = 420;
  const entriesForCategory = (category) => {
    return (grouped.get(category) || []).slice();
  };
  const categoryTszSpeedupScore = (category) => Math.max(
    -Infinity,
    ...entriesForCategory(category).map(tszSpeedupScore),
  );
  const visibleCategories = categories
    .filter((category) => categoryBelongsToMode(category, mode))
    .sort((a, b) => {
      if (mode !== "projects") return 0;
      const aScore = categoryTszSpeedupScore(a);
      const bScore = categoryTszSpeedupScore(b);
      if (aScore !== bScore) return bScore - aScore;
      return categoryTitle(a).localeCompare(categoryTitle(b));
    });
  const visibleFailedResults = failedResults.filter((row) => failedBelongsToMode(row, mode));
  const chartMaxMs = Math.max(
    1,
    ...visibleCategories
      .flatMap((category) => entriesForCategory(category))
      .flatMap((row) => [Number(row.tsz_ms) || 0, Number(row.tsgo_ms) || 0]),
    ...visibleFailedResults.flatMap((row) => [Number(row.tsz_ms) || 0, Number(row.tsgo_ms) || 0]),
  );

  let html = "";
  for (const category of visibleCategories) {
    const entries = entriesForCategory(category);
    const slug = categorySlug(category);
    const meta = categoryMeta(category);
    const isProject = isProjectCategory(category);
    if (!entries.length) continue;

    entries.sort((a, b) => {
      if (isProject) {
        return compareByTszSpeedup(a, b);
      } else {
        const aLines = Number(a.lines) || 0;
        const bLines = Number(b.lines) || 0;
        if (bLines !== aLines) return bLines - aLines;
      }
      return (String(a.name || "") > String(b.name || "") ? 1 : -1);
    });
    const desc = isProject ? "" : categoryDescription(category);
    const repoLink = meta.repo
      ? ` <a class="bench-category-repo" href="${meta.repo}" target="_blank" rel="noopener noreferrer">${escapeHtml(meta.repoLabel || meta.repo)}</a>`
      : "";
    const title = categoryTitle(category);

    html += `<section class="bench-category${isProject ? " bench-project-category" : ""}">
  <h3 class="bench-category-title" id="${slug}">${escapeHtml(title)}${repoLink}</h3>
  ${desc ? `<p class="bench-category-desc">${escapeHtml(desc)}</p>` : ""}
  <div class="bench-chart">\n`;

    for (const r of entries) {
      const decorated = decorateRow(r, category, { isAggregate: r.is_aggregate });
      const tszWidth = (r.tsz_ms / chartMaxMs) * barMaxWidth;
      const tsgoWidth = (r.tsgo_ms / chartMaxMs) * barMaxWidth;
      const winnerLabel = formatSpeedupLabel(r.tsz_ms, r.tsgo_ms);
      const tszLabel = formatDurationMs(r.tsz_ms);
      const tsgoLabel = formatDurationMs(r.tsgo_ms);

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
        ${renderBenchmarkBar("tsz", tszWidth, tszLabel)}
      </div>
      <div class="bench-bar-row">
        <span class="bench-bar-label">tsgo</span>
        ${renderBenchmarkBar("tsgo", tsgoWidth, tsgoLabel)}
      </div>
      ${winnerLabel ? `<div class="bench-winner">${winnerLabel}</div>` : ""}
    </div>
    <a class="bench-detail-link" href="${decorated.url}">View details</a>
  </div>\n`;
    }

    html += `  </div>
 </section>\n`;
  }

  if (visibleFailedResults.length > 0) {
    const failedTitle = mode === "projects" ? "Projects without complete timing" : "Incomplete timings";
    const failedDescription = mode === "projects"
      ? "Project runs recorded by CI without a full tsz and tsgo timing pair."
      : "Rows recorded by CI without a full tsz and tsgo timing pair.";
    html += `<section class="bench-category bench-failures">
  <h3 class="bench-category-title" id="failures">${escapeHtml(failedTitle)}</h3>
  <p class="bench-category-desc">${escapeHtml(failedDescription)}</p>
  <div class="bench-chart">\n`;
    for (const r of visibleFailedResults) {
      const category = categoryFor(r.name || "", r.lines);
      const decorated = decorateRow(r, category);
      const tszWidth = hasTiming(r.tsz_ms) ? (r.tsz_ms / chartMaxMs) * barMaxWidth : 0;
      const tsgoWidth = hasTiming(r.tsgo_ms) ? (r.tsgo_ms / chartMaxMs) * barMaxWidth : 0;
      const metaParts = [decorated.kind, `${fmt(r.lines || 0)} lines`, `${fmt(r.kb || 0)} KB`];
      html += `  <div class="bench-row bench-row-error">
    <div class="bench-name"><a href="${decorated.url}">${escapeHtml(displayName(r.name))}</a></div>
    <div class="bench-meta">${escapeHtml(metaParts.join(" · "))}</div>
    <p class="bench-focus bench-failure-status">${escapeHtml(statusLabel(r))}</p>
    <div class="bench-bars">
      <div class="bench-bar-row">
        <span class="bench-bar-label">tsz</span>
        ${hasTiming(r.tsz_ms)
          ? renderBenchmarkBar("tsz", tszWidth, formatDurationMs(r.tsz_ms))
          : `<span class="bench-bar-status">failed</span>`}
      </div>
      <div class="bench-bar-row">
        <span class="bench-bar-label">tsgo</span>
        ${hasTiming(r.tsgo_ms)
          ? renderBenchmarkBar("tsgo", tsgoWidth, formatDurationMs(r.tsgo_ms))
          : `<span class="bench-bar-status">n/a</span>`}
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
  return generateCharts(loadBenchmarks(), "projects");
}

export function getBenchmarkMicroCharts() {
  return generateCharts(loadBenchmarks(), "micro");
}

export function getProjectCompatibilityDashboard() {
  const data = loadBenchmarks();
  const allResults = withExpectedProjectRows(data?.results);
  const rows = COMPATIBILITY_CORPUS_ROWS.map((definition) => compatibilityRowFor(definition, allResults));

  const counts = rows.reduce((acc, row) => {
    acc[row.className] = (acc[row.className] || 0) + 1;
    return acc;
  }, {});
  const summary = [
    `${counts.green || 0} green`,
    `${counts.yellow || 0} yellow`,
    `${counts.red || 0} red`,
    `${counts.gray || 0} gray`,
  ].join(" · ");

  const detailLabel = (row) => {
    if (row.className === "green") return "passes";
    if (row.exitClass === "missing or incomplete artifact") return "missing artifact";
    return row.exitClass;
  };

  return `<section class="compat-dashboard">
  <h2>Compatibility</h2>
  <div class="compat-summary">${escapeHtml(summary)}</div>
  <ul class="compat-list">
    ${rows.map((row) => `<li class="compat-item">
      <a href="${row.url}">${escapeHtml(row.label)}</a>
      <span class="compat-state ${row.className}">${escapeHtml(row.className)}</span>
      <span class="compat-detail">${escapeHtml(detailLabel(row))}</span>
    </li>`).join("\n")}
  </ul>
</section>`;
}
