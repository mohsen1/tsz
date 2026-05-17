import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { marked } from "marked";
import {
  COMPILE_CANARY_PROJECT_ROWS,
  COMPATIBILITY_CORPUS_ROWS,
  REQUIRED_PROJECT_ROWS,
} from "../../../../scripts/bench/project-rows.mjs";

const ROOT = path.resolve(import.meta.dirname, "..", "..", "..", "..");

function fmt(n) {
  return Number(n).toLocaleString("en-US");
}

function formatUtcTimestamp(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  return date.toISOString().replace(/\.\d{3}Z$/, "Z");
}

function formatMemory(bytes) {
  const value = Number(bytes);
  if (!Number.isFinite(value) || value <= 0) return null;
  return `${(value / (1024 ** 3)).toFixed(1)} GiB RAM`;
}

function runnerEnvironmentSummary(data) {
  const parts = [];
  const generatedAt = formatUtcTimestamp(data?.generated_at);
  if (generatedAt) parts.push(`Generated ${generatedAt}`);

  const env = data?.runner_environment;
  if (!env || typeof env !== "object") {
    parts.push("runner hardware metadata unavailable for this artifact");
    return parts.join(" · ");
  }

  const platform = [env.platform, env.arch].filter(Boolean).join("/");
  if (platform) parts.push(platform);
  if (env.cpu_count) {
    const cpuModel = env.cpu_model ? ` ${env.cpu_model}` : "";
    parts.push(`${env.cpu_count} CPU${env.cpu_count === 1 ? "" : "s"}${cpuModel}`);
  }
  const memory = formatMemory(env.total_memory_bytes);
  if (memory) parts.push(memory);
  if (env.github_actions?.runner_os || env.github_actions?.runner_arch) {
    const runner = [
      env.github_actions.runner_os,
      env.github_actions.runner_arch,
    ].filter(Boolean).join("/");
    parts.push(`GitHub Actions ${runner}`);
  } else if (env.ci) {
    parts.push("CI runner");
  }
  if (env.cloud_build?.machine_type) {
    parts.push(`Cloud Build ${env.cloud_build.machine_type}`);
  }

  return parts.join(" · ");
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

const DIAGNOSTIC_SUBSYSTEM_RULES = [
  ["project-config", new Set(["TS18003", "TS5052", "TS5069", "TS5070", "TS5083", "TS5110", "TS6053", "TS2688"])],
  ["syntax-parser-jsdoc", new Set(["TS1005", "TS1109", "TS1128", "TS17004", "TS8010", "TS8023", "TS8032"])],
  ["module-symbol-resolution", new Set(["TS2304", "TS2305", "TS2306", "TS2307", "TS2451", "TS2503", "TS2580", "TS2583", "TS2664", "TS2665", "TS2666", "TS2694"])],
  ["relations-assignability", new Set(["TS2322", "TS2345", "TS2352", "TS2394", "TS2416", "TS2420", "TS2430", "TS2559", "TS2740", "TS2741", "TS2769"])],
  ["evaluation-inference-instantiation", new Set(["TS2313", "TS2314", "TS2315", "TS2344", "TS2558", "TS2589", "TS2590", "TS2615", "TS7022"])],
  ["keyspace-property-indexed", new Set(["TS2339", "TS2353", "TS2536", "TS2537", "TS2538", "TS2540", "TS4111", "TS7053"])],
  ["flow-narrowing", new Set(["TS2367", "TS2677", "TS2774", "TS18047", "TS18048"])],
  ["class-this-accessor", new Set(["TS2415", "TS2511", "TS2515", "TS2526", "TS2683", "TS4113", "TS4114"])],
  ["emit-dts-nameability", new Set(["TS4023", "TS4058", "TS4082", "TS4094", "TS9005", "TS9039"])],
];

function subsystemForDiagnosticCode(code) {
  for (const [subsystem, codes] of DIAGNOSTIC_SUBSYSTEM_RULES) {
    if (codes.has(code)) return subsystem;
  }
  return "unclassified diagnostic";
}

function diagnosticSubsystemsFromDeltas(deltas) {
  const groups = new Map();
  for (const line of deltas) {
    const codes = [...String(line || "").matchAll(/\bTS\d{4,5}\b/g)].map((match) => match[0]);
    const lineCodes = codes.length ? codes : ["uncoded"];
    for (const code of lineCodes) {
      const subsystem = code === "uncoded" ? "uncoded diagnostic" : subsystemForDiagnosticCode(code);
      if (!groups.has(subsystem)) {
        groups.set(subsystem, { subsystem, codes: [], count: 0, examples: [] });
      }
      const group = groups.get(subsystem);
      group.count += 1;
      if (code !== "uncoded" && !group.codes.includes(code) && group.codes.length < 8) {
        group.codes.push(code);
      }
      if (group.examples.length < 3) {
        group.examples.push(String(line || ""));
      }
    }
  }
  return [...groups.values()];
}

function normalizedDiagnosticSubsystems(compatibility) {
  const existing = Array.isArray(compatibility?.diagnostic_subsystems)
    ? compatibility.diagnostic_subsystems
    : [];
  if (existing.length) {
    return existing
      .map((group) => ({
        subsystem: String(group?.subsystem || "unclassified diagnostic"),
        codes: Array.isArray(group?.codes) ? group.codes.map(String).filter(Boolean).slice(0, 8) : [],
        count: Number.isFinite(Number(group?.count)) ? Number(group.count) : 0,
        examples: Array.isArray(group?.examples) ? group.examples.map(String).filter(Boolean).slice(0, 3) : [],
      }))
      .filter((group) => group.count > 0 || group.codes.length || group.examples.length)
      .slice(0, 8);
  }
  const deltas = Array.isArray(compatibility?.diagnostic_deltas)
    ? compatibility.diagnostic_deltas
    : compatibility?.diagnostic_deltas
      ? [compatibility.diagnostic_deltas]
      : [];
  return diagnosticSubsystemsFromDeltas(deltas).slice(0, 8);
}

function diagnosticCodesFromDeltas(deltas) {
  const codes = [];
  const seen = new Set();
  for (const line of deltas) {
    for (const match of String(line || "").matchAll(/\bTS\d{4,5}\b/g)) {
      const code = match[0];
      if (seen.has(code)) continue;
      seen.add(code);
      codes.push(code);
      if (codes.length >= 8) return codes;
    }
  }
  return codes;
}

function normalizedKnownBlockers(compatibility, diagnosticSubsystems) {
  const existing = Array.isArray(compatibility?.known_blockers) ? compatibility.known_blockers : [];
  if (existing.length) {
    return existing.map(String).filter(Boolean).slice(0, 8);
  }

  const blockers = [];
  const add = (blocker) => {
    if (blocker && !blockers.includes(blocker) && blockers.length < 8) blockers.push(blocker);
  };
  const exitClass = String(compatibility?.exit_class || "");
  const phase = String(compatibility?.phase || "");

  if (exitClass === "timeout") add("timeout during project check");
  if (exitClass === "oom") add("OOM or killed during project check");
  if (exitClass === "crash") add("compiler crash during project check");
  if (exitClass === "fixture invalid") add("reference fixture invalid");
  if (exitClass === "runner error") add("benchmark runner error");
  if (exitClass === "tsz unavailable") add("tsz unavailable in benchmark runner");
  if (phase && phase !== "check") add(`${phase} phase blocker`);

  for (const group of diagnosticSubsystems) {
    add(String(group?.subsystem || ""));
  }

  const deltas = Array.isArray(compatibility?.diagnostic_deltas)
    ? compatibility.diagnostic_deltas
    : compatibility?.diagnostic_deltas
      ? [compatibility.diagnostic_deltas]
      : [];
  if (!blockers.length && diagnosticCodesFromDeltas(deltas).length) {
    add("unclassified diagnostic mismatch");
  }

  return blockers;
}

function normalizedLastSuccessfulPhase(compatibility) {
  if (compatibility?.last_successful_phase !== undefined && compatibility.last_successful_phase !== "") {
    return compatibility.last_successful_phase;
  }
  if (compatibility?.exit_class === "exit success" && compatibility?.diagnostic_status === "none") return "check";
  return null;
}

const COMPATIBILITY_METADATA_FIELDS = [
  ["exit_class", "exit class"],
  ["phase", "phase"],
  ["last_successful_phase", "last successful phase"],
  ["diagnostic_status", "diagnostic status"],
  ["diagnostic_deltas", "diagnostic deltas"],
  ["diagnostic_subsystems", "diagnostic subsystems"],
  ["known_blockers", "known blockers"],
  ["exit_codes", "exit codes"],
  ["files_reached", "files reached"],
  ["peak_memory_bytes", "peak memory"],
  ["emit_status", "emit status"],
  ["dts_status", "dts status"],
];

function missingCompatibilityMetadata(row) {
  const compatibility = row?.compatibility;
  if (!compatibility || typeof compatibility !== "object") return ["compatibility artifact"];
  return COMPATIBILITY_METADATA_FIELDS
    .filter(([field]) => !Object.prototype.hasOwnProperty.call(compatibility, field))
    .map(([, label]) => label);
}

const TINY_BENCHMARK_MAX_LINES = 200;

function withExpectedProjectRows(results) {
  const rows = Array.isArray(results) ? results.slice() : [];
  const existingNames = new Set(rows.map((row) => row?.name).filter(Boolean));

  for (const name of REQUIRED_PROJECT_ROWS) {
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

  for (const name of COMPILE_CANARY_PROJECT_ROWS) {
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
      status: "compile canary tracked in CI; not timed by vs-tsgo benchmarks",
    });
  }

  return rows;
}

function compatibilityState(row) {
  const compatibility = row?.compatibility || {};
  const diagnosticStatus = String(compatibility.diagnostic_status || "").toLowerCase();
  if (hasSuccessfulTiming(row)) {
    if (diagnosticStatus && diagnosticStatus !== "none") {
      return {
        className: "yellow",
        stateLabel: "Yellow",
        exitClass: firstPresent(compatibility.exit_class, "diagnostic mismatch"),
        phase: firstPresent(compatibility.phase, "check"),
        diagnosticDeltas: firstPresent(compatibility.diagnostic_deltas, "not captured by latest artifact"),
      };
    }
    return {
      className: "green",
      stateLabel: "Green",
      exitClass: firstPresent(compatibility.exit_class, "exit success"),
      phase: firstPresent(compatibility.phase, "check"),
      diagnosticDeltas: firstPresent(compatibility.diagnostic_deltas, "none recorded"),
    };
  }

  const status = String(row?.status || "").toLowerCase();
  if (!row || status.includes("not recorded") || status.includes("fixture") || status.includes("tsc fixture")) {
    return {
      className: "gray",
      stateLabel: "Gray",
      exitClass: firstPresent(
        compatibility.exit_class,
        status.includes("tsc fixture") ? "fixture invalid" : "missing or incomplete artifact",
      ),
      phase: firstPresent(compatibility.phase, status.includes("fixture") ? "fixture setup" : "artifact"),
      diagnosticDeltas: firstPresent(compatibility.diagnostic_deltas, "not available"),
    };
  }

  if (status.includes("diagnostic mismatch") || diagnosticStatus.includes("diagnostic mismatch")) {
    return {
      className: "yellow",
      stateLabel: "Yellow",
      exitClass: firstPresent(compatibility.exit_class, "diagnostic mismatch"),
      phase: firstPresent(compatibility.phase, "check"),
      diagnosticDeltas: firstPresent(compatibility.diagnostic_deltas, "not captured by latest artifact"),
    };
  }

  return {
    className: "red",
    stateLabel: "Red",
    exitClass: firstPresent(compatibility.exit_class, status.includes("timeout") ? "timeout" : "nonzero exit"),
    phase: firstPresent(compatibility.phase, "check"),
    diagnosticDeltas: firstPresent(compatibility.diagnostic_deltas, "not captured by latest artifact"),
  };
}

function compatibilityRowFor(definition, allResults) {
  const row = allResults.find((candidate) => candidate?.name === definition.name);
  const artifactFamily = firstPresent(row?.compatibility?.semantic_owner_family, row?.compatibility?.owner_family);
  const compatibility = row?.compatibility || {};
  const diagnosticSubsystems = normalizedDiagnosticSubsystems(compatibility);
  const missingMetadata = missingCompatibilityMetadata(row);
  return {
    ...definition,
    family: artifactFamily || definition.family,
    ...compatibilityState(row),
    row,
    lines: row?.lines || 0,
    filesReached: compatibility.files_reached ?? null,
    lastSuccessfulPhase: normalizedLastSuccessfulPhase(compatibility),
    peakMemoryBytes: compatibility.peak_memory_bytes ?? null,
    emitStatus: compatibility.emit_status || "not in scope (noEmit project check)",
    dtsStatus: compatibility.dts_status || "not in scope (noEmit project check)",
    knownBlockers: normalizedKnownBlockers(compatibility, diagnosticSubsystems),
    exitCodes: compatibility.exit_codes && typeof compatibility.exit_codes === "object"
      ? {
          tsc: Array.isArray(compatibility.exit_codes.tsc) ? compatibility.exit_codes.tsc.slice(0, 8) : [],
          tsz: Array.isArray(compatibility.exit_codes.tsz) ? compatibility.exit_codes.tsz.slice(0, 8) : [],
          tsgo: Array.isArray(compatibility.exit_codes.tsgo) ? compatibility.exit_codes.tsgo.slice(0, 8) : [],
        }
      : { tsc: [], tsz: [], tsgo: [] },
    diagnosticCodes: Array.isArray(compatibility.diagnostic_codes) ? compatibility.diagnostic_codes.slice(0, 8) : [],
    diagnosticSubsystems,
    primarySubsystem: compatibility.primary_subsystem || diagnosticSubsystems[0]?.subsystem || null,
    assertionCandidates: compatibility.assertion_candidates && typeof compatibility.assertion_candidates === "object"
      ? compatibility.assertion_candidates
      : null,
    reductionCandidates: Array.isArray(compatibility.reduction_candidates)
      ? compatibility.reduction_candidates.slice(0, 5)
      : [],
    missingMetadata,
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
  "type-challenges-project": [".target/project-compile-guard/type-challenges/README.md"],
  "type-challenges-solutions-project": [".target/project-compile-guard/type-challenges-solutions/README.md"],
};

const PROJECT_README_URLS = {
  "large-ts-repo": "https://raw.githubusercontent.com/mohsen1/large-ts-repo/e1b22bda18664a507ed0da19c155e0365d585b18/README.md",
  "rxjs-project": "https://raw.githubusercontent.com/ReactiveX/rxjs/e5351d02e225e275ac0e497c7b66eaa5f0c88791/README.md",
  "zod-project": "https://raw.githubusercontent.com/colinhacks/zod/93b0b6892cc0cfee8d0bec4e2e1242c7df771f95/README.md",
  "utility-types-project": "https://raw.githubusercontent.com/piotrwitek/utility-types/2ee1f6ecb241651ab22390fee7ee5349942efda2/README.md",
  "ts-toolbelt-project": "https://raw.githubusercontent.com/millsp/ts-toolbelt/b8a49285e3ed3a7d8bb8e0b433389eac46a5f140/README.md",
  "ts-essentials-project": "https://raw.githubusercontent.com/ts-essentials/ts-essentials/5abe8700b42068048bd3c368e0531b6defe56558/README.md",
  "type-challenges-project": "https://raw.githubusercontent.com/type-challenges/type-challenges/0b0b0b18bcb7ac42dc22ce26ffb438231d4754b1/README.md",
  "type-challenges-solutions-project": "https://raw.githubusercontent.com/ghaiklor/type-challenges-solutions/91a6d2986650475f29eeb3bd18ebd025128aa07e/README.md",
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

const TYPESCRIPT_VERSIONS_PATH = path.join(ROOT, "scripts/conformance/typescript-versions.json");

function currentTypeScriptRef() {
  const versions = readJsonIfExists(TYPESCRIPT_VERSIONS_PATH);
  return versions?.current || "050880ce59e30b356b686bd3144efe24f875ebc8";
}

const TYPESCRIPT_FIXTURE_DIRS = [
  "tests/cases/compiler",
  "tests/cases/conformance",
];

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
  const overrideArtifact = process.env.TSZ_WEBSITE_BENCHMARK_ARTIFACT;
  if (overrideArtifact) {
    const data = readJsonIfExists(overrideArtifact);
    if (data?.results) return sanitizeLegacyBenchmarkData(data);
  }

  const artifactsDir = path.join(ROOT, "artifacts");
  const ciLatest = [
    "bench-vs-tsgo-github-latest.json",
    "bench-vs-tsgo-gcs-latest.json",
    "bench-results.json",
  ].map((file) => path.join(artifactsDir, file));
  const artifactFiles = (() => {
    try {
      const localArtifacts = fs.readdirSync(artifactsDir)
        .filter((file) => file.startsWith("bench-vs-tsgo-") && file.endsWith(".json"))
        .filter((file) => !["bench-vs-tsgo-github-latest.json", "bench-vs-tsgo-gcs-latest.json", "bench-results.json"].includes(file))
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
  if (name === "large-ts-repo" || name === "nextjs") return "Projects: large repositories";
  if (name === "nextjs-fresh-app" || name === "vite-vanilla-ts-app") return "Projects: generated apps";
  if (
    name === "rxjs-project" ||
    name === "type-fest-project" ||
    name === "utility-types-project" ||
    name === "ts-essentials-project" ||
    name === "ts-toolbelt-project" ||
    name === "zod-project" ||
    name === "kysely-project" ||
    name === "type-challenges-project" ||
    name === "type-challenges-solutions-project" ||
    name === "type-challenges-assertion-candidates" ||
    name === "type-challenges-assertions-tsc-clean"
  ) {
    return "Projects: external libraries";
  }
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
    "Projects: large repositories": {
      title: "Large repositories",
      description: "Full repository type-checks that stress project graph setup, residency, and cross-file analysis.",
    },
    "Projects: generated apps": {
      title: "Generated apps",
      description: "Generated application fixtures with modern framework dependencies and generated configs.",
    },
    "Projects: external libraries": {
      title: "External libraries",
      description: "Pinned real-world libraries and type-heavy repositories checked as project-mode fixtures.",
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
  if (name === "type-challenges-project") return "type-challenges project";
  if (name === "type-challenges-solutions-project") return "type-challenges solutions project";
  if (name === "type-challenges-assertion-candidates") return "type-challenges assertion candidates";
  if (name === "type-challenges-assertions-tsc-clean") return "type-challenges tsc-clean assertions";

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

function isTypeScriptFixtureName(name) {
  return String(name || "").endsWith(".ts") && !String(name || "").includes("/");
}

function displayBaseName(name) {
  return displayName(name)
    .replace(/\s+Speed Reasonable$/i, "")
    .replace(/\s+Not Too Large$/i, "")
    .trim();
}

function benchmarkTitle(row, category) {
  const name = String(row?.name || "");
  if (isProjectCategory(category)) return displayName(name);
  if (isExternalLibraryCategory(category)) return `${libraryNameForCategory(category)} file: ${displayBaseName(name)}`;
  if (isTypeScriptFixtureName(name)) return displayBaseName(name);
  return displayName(name);
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
  if (name === "conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable.ts") {
    return "Official TypeScript compiler fixture that stresses conditional type discrimination across a large union without falling off a performance cliff.";
  }
  if (name === "manyConstExports.ts") {
    return "Official TypeScript compiler fixture that stresses binder/export-table setup for many constant exports.";
  }
  if (name === "binderBinaryExpressionStress.ts" || name === "binderBinaryExpressionStressJs.ts") {
    return "Official TypeScript compiler fixture that stresses binder traversal over a very large binary-expression tree.";
  }
  if (name === "binaryArithmeticControlFlowGraphNotTooLarge.ts") {
    return "Official TypeScript compiler fixture that keeps arithmetic control-flow graph construction bounded.";
  }
  if (name === "enumLiteralsSubtypeReduction.ts") {
    return "Official TypeScript compiler fixture that exercises enum literal subtype reduction and related assignability checks.";
  }
  if (name === "controlFlowArrays.ts") {
    return "Official TypeScript compiler fixture for array-sensitive control-flow analysis.";
  }
  if (/privacy/i.test(name)) {
    return "Official TypeScript compiler fixture for declaration privacy checks on public APIs.";
  }
  if (name === "typedArrays.ts") {
    return "Generated fixture that type-checks typed-array constructor and from() overload surfaces.";
  }
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

function generatedTypedArraysSource() {
  return `// Typed array benchmark fixture used by bench-vs-tsgo.sh.
// Keep this strict/explicit so all compilers can parse and type-check it.

function createTypedArrayInstancesFromLength(length: number) {
    const typedArrays = [];
    typedArrays[0] = new Int8Array(length);
    typedArrays[1] = new Uint8Array(length);
    typedArrays[2] = new Int16Array(length);
    typedArrays[3] = new Uint16Array(length);
    typedArrays[4] = new Int32Array(length);
    typedArrays[5] = new Uint32Array(length);
    typedArrays[6] = new Float32Array(length);
    typedArrays[7] = new Float64Array(length);
    typedArrays[8] = new Uint8ClampedArray(length);
    return typedArrays;
}

function createTypedArrayInstancesFromArrayLike(obj: ArrayLike<number>) {
    const typedArrays = [];
    typedArrays[0] = new Int8Array(obj);
    typedArrays[1] = new Uint8Array(obj);
    typedArrays[2] = new Int16Array(obj);
    typedArrays[3] = new Uint16Array(obj);
    typedArrays[4] = new Int32Array(obj);
    typedArrays[5] = new Uint32Array(obj);
    typedArrays[6] = new Float32Array(obj);
    typedArrays[7] = new Float64Array(obj);
    typedArrays[8] = new Uint8ClampedArray(obj);
    return typedArrays;
}

function createTypedArraysFromMapFn(
    obj: ArrayLike<number>,
    mapFn: (n: number, v: number) => number
) {
    const typedArrays = [];
    typedArrays[0] = Int8Array.from(obj, mapFn);
    typedArrays[1] = Uint8Array.from(obj, mapFn);
    typedArrays[2] = Int16Array.from(obj, mapFn);
    typedArrays[3] = Uint16Array.from(obj, mapFn);
    typedArrays[4] = Int32Array.from(obj, mapFn);
    typedArrays[5] = Uint32Array.from(obj, mapFn);
    typedArrays[6] = Float32Array.from(obj, mapFn);
    typedArrays[7] = Float64Array.from(obj, mapFn);
    typedArrays[8] = Uint8ClampedArray.from(obj, mapFn);
    return typedArrays;
}

const values: number[] = [1, 2, 3, 4];
const mapped = createTypedArraysFromMapFn(values, (n, i) => n + i);
const fromLength = createTypedArrayInstancesFromLength(128);
const fromArrayLike = createTypedArrayInstancesFromArrayLike(values);
const sampleCount = mapped.length + fromLength.length + fromArrayLike.length;`;
}

function generatedInferStressSource(count) {
  const maxFunctions = Math.min(count, 30);
  const lines = [
    "// Infer keyword stress test",
    "// Tests inference variable resolution in conditional types",
    "",
    "type UnwrapPromise<T> = T extends Promise<infer U> ? U : T;",
    "type UnwrapArray<T> = T extends (infer U)[] ? U : T;",
    "type MyParameters<T> = T extends (...args: infer P) => any ? P : never;",
    "type MyReturnType<T> = T extends (...args: any[]) => infer R ? R : never;",
    "",
    "type FirstAndRest<T> = T extends [infer First, ...infer Rest] ? { first: First; rest: Rest } : never;",
    "",
    "type DeepUnwrap<T> =",
    "    T extends Promise<infer U> ? DeepUnwrap<U> :",
    "    T extends (infer V)[] ? DeepUnwrap<V>[] :",
    "    T;",
    "",
    "type ExtractPrefix<T> = T extends `${infer P}_${string}` ? P : never;",
    "type ExtractIfString<T> = T extends infer U extends string ? U : never;",
    "",
  ];

  for (let i = 0; i < maxFunctions; i += 1) {
    lines.push(`declare function func${i}(`);
    for (let j = 0; j <= i; j += 1) {
      lines.push(`    arg${j}: string${j === i ? "" : ","}`);
    }
    lines.push("): number;", "", `type Params${i} = MyParameters<typeof func${i}>;`, `type Return${i} = MyReturnType<typeof func${i}>;`, "");
  }

  lines.push(
    "type ComplexInfer<T> = T extends {",
    "    data: infer D;",
    "    nested: { value: infer V }[]",
    "} ? { data: D; values: V[] } : never;",
    "",
    "interface TestData {",
    "    data: string;",
    "    nested: { value: number }[];",
    "}",
    "",
    "type Inferred = ComplexInfer<TestData>;",
    "",
    `declare const params: Params${maxFunctions - 1};`,
    "declare const inferred: Inferred;",
  );

  return lines.join("\n").trimEnd();
}

function generatedBenchmarkSource(name) {
  if (String(name || "") === "typedArrays.ts") return generatedTypedArraysSource();

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

  const inferStressCount = countFromName(name, /^Infer stress N=(\d+)$/i);
  if (inferStressCount) return generatedInferStressSource(inferStressCount);

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

  const candidates = TYPESCRIPT_FIXTURE_DIRS.map((dir) => path.join(ROOT, "TypeScript", dir, fixtureName));

  for (const candidate of candidates) {
    try {
      return fs.readFileSync(candidate, "utf8").trimEnd();
    } catch {
      // Keep looking in the next known TypeScript fixture location.
    }
  }

  const ref = currentTypeScriptRef();
  for (const dir of TYPESCRIPT_FIXTURE_DIRS) {
    const remote = readRemoteText(`https://raw.githubusercontent.com/microsoft/TypeScript/${ref}/${dir}/${fixtureName}`);
    if (remote) return remote;
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
  const artifactSource = typeof row?.source?.content === "string" && row.source.content
    ? row.source.content.trimEnd()
    : null;
  if (artifactSource) {
    return [{
      name: row.source.path || fixtureName,
      language: "typescript",
      source: artifactSource,
    }];
  }

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
    display_name: benchmarkTitle(row, category),
    slug: benchmarkSlug(row.name),
    url: benchmarkUrl(row),
    kind: benchmarkKind(category),
    focus,
    detail_focus: focus,
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
    "Projects: external libraries",
    "Projects: generated apps",
    "Projects: large repositories",
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
    const failedTitle = mode === "projects" ? "Compile canaries and incomplete project timings" : "Incomplete timings";
    const failedDescription = mode === "projects"
      ? "Rows that are tracked for compile readiness but are not part of the timed vs-tsgo chart yet."
      : "Rows recorded by CI without a full tsz and tsgo timing pair.";
    html += `<section class="bench-category bench-failures">
  <h3 class="bench-category-title" id="failures">${escapeHtml(failedTitle)}</h3>
  <p class="bench-category-desc">${escapeHtml(failedDescription)}</p>
  <ul class="bench-failure-list">\n`;
    for (const r of visibleFailedResults) {
      const category = categoryFor(r.name || "", r.lines);
      const decorated = decorateRow(r, category);
      html += `  <li>
    <a href="${decorated.url}">${escapeHtml(displayName(r.name))}</a>
    <span>${escapeHtml(statusLabel(r))}</span>
  </li>\n`;
    }
    html += `  </ul>
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

export function getBenchmarkEnvironmentSummary() {
  const summary = runnerEnvironmentSummary(loadBenchmarks());
  if (!summary) return "";
  return `<p class="bench-runner-meta">${escapeHtml(summary)}</p>`;
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

  const diagnosticDeltas = (row) => {
    const deltas = Array.isArray(row.diagnosticDeltas)
      ? row.diagnosticDeltas
      : row.diagnosticDeltas
        ? [row.diagnosticDeltas]
        : [];
    return deltas.filter(Boolean).slice(0, 20);
  };

  const measurementParts = (row) => {
    const parts = [];
    if (row.filesReached !== null && row.filesReached !== undefined && Number.isFinite(Number(row.filesReached))) {
      parts.push(`${fmt(row.filesReached)} files`);
    }
    if (Number.isFinite(Number(row.peakMemoryBytes)) && Number(row.peakMemoryBytes) > 0) {
      parts.push(`${(Number(row.peakMemoryBytes) / (1024 * 1024)).toLocaleString("en-US", { maximumFractionDigits: 0 })} MiB peak`);
    }
    return parts;
  };

  const assertionCandidateParts = (row) => {
    const candidates = row.assertionCandidates;
    if (!candidates || typeof candidates !== "object") return [];

    const parts = [];
    const addCount = (label, value) => {
      if (Number.isFinite(Number(value))) {
        parts.push(`${label}: ${fmt(Number(value))}`);
      }
    };
    addCount("paired solutions", candidates.paired_solutions);
    addCount("assertions generated", candidates.generated_assertions);
    addCount(
      "assertions referencing solutions",
      candidates.assertions_referencing_solution_declaration,
    );
    addCount(
      "assertions missing solution references",
      candidates.assertions_missing_solution_declaration_reference,
    );
    addCount("tsc clean", candidates.tsc_diagnostic_free);
    addCount("tsz clean", candidates.tsz_diagnostic_free);
    const sources = candidates.sources && typeof candidates.sources === "object"
      ? candidates.sources
      : {};
    const addRef = (label, source) => {
      if (source?.ref) {
        parts.push(`${label} ref: ${source.ref}`);
      }
    };
    addRef("templates", sources.templates);
    addRef("test cases", sources.testCases);
    addRef("solutions", sources.solutions);

    const cleanSubset = candidates.tsc_clean_subset && typeof candidates.tsc_clean_subset === "object"
      ? candidates.tsc_clean_subset
      : null;
    if (cleanSubset) {
      addCount("tsc-clean subset", cleanSubset.generated_assertions);
      addCount(
        "tsc-clean references solutions",
        cleanSubset.assertions_referencing_solution_declaration,
      );
      addCount(
        "tsc-clean missing solution references",
        cleanSubset.assertions_missing_solution_declaration_reference,
      );
      addCount("tsc-clean rejected", cleanSubset.rejected_from_full_corpus);
      if (cleanSubset.tsc_status) {
        parts.push(`tsc-clean tsc: ${cleanSubset.tsc_status}`);
      }
      if (cleanSubset.tsz_status) {
        parts.push(`tsc-clean tsz: ${cleanSubset.tsz_status}`);
      }
    }

    const counts = candidates.file_comparison?.counts;
    addCount("both accepted", candidates.both_accepted ?? counts?.bothAccepted);
    addCount("both rejected", candidates.both_rejected ?? counts?.bothRejected);
    addCount(
      "tsc accepted/tsz rejected",
      candidates.tsc_accepted_tsz_rejected ?? counts?.tscAcceptedTszRejected,
    );
    addCount(
      "tsc rejected/tsz accepted",
      candidates.tsc_rejected_tsz_accepted ?? counts?.tscRejectedTszAccepted,
    );
    return parts;
  };

  const exitCodeParts = (row) => {
    const codes = row.exitCodes || {};
    return ["tsc", "tsz", "tsgo"]
      .map((compiler) => {
        const values = Array.isArray(codes[compiler]) ? codes[compiler].filter((value) => Number.isInteger(Number(value))) : [];
        return values.length ? `${compiler} exit ${values.join("|")}` : "";
      })
      .filter(Boolean);
  };

  const renderRowDetails = (row) => {
    const deltas = diagnosticDeltas(row);
    const diagnosticCodes = Array.isArray(row.diagnosticCodes) ? row.diagnosticCodes.filter(Boolean).slice(0, 8) : [];
    const diagnosticSubsystems = Array.isArray(row.diagnosticSubsystems)
      ? row.diagnosticSubsystems.filter((group) => group?.subsystem).slice(0, 8)
      : [];
    const reductionCandidates = Array.isArray(row.reductionCandidates)
      ? row.reductionCandidates.filter(Boolean).slice(0, 5)
      : [];
    const knownBlockers = Array.isArray(row.knownBlockers)
      ? row.knownBlockers.filter(Boolean).slice(0, 8)
      : [];
    const diagnosticCandidateExamples = Array.isArray(row.assertionCandidates?.diagnostic_candidate_examples)
      ? row.assertionCandidates.diagnostic_candidate_examples.filter(Boolean).slice(0, 5)
      : [];
    const parts = [
      `phase: ${row.phase || "unknown"}`,
      row.lastSuccessfulPhase ? `last successful: ${row.lastSuccessfulPhase}` : "",
      row.missingMetadata?.length
        ? `artifact missing: ${
            row.missingMetadata.slice(0, 4).join(", ")
          }${row.missingMetadata.length > 4 ? "..." : ""}`
        : "artifact: complete",
      `owner: ${row.family || "not classified"}`,
      row.primarySubsystem ? `subsystem: ${row.primarySubsystem}` : "",
      row.emitStatus ? `emit: ${row.emitStatus}` : "",
      row.dtsStatus ? `dts: ${row.dtsStatus}` : "",
      ...measurementParts(row),
      ...assertionCandidateParts(row),
      ...exitCodeParts(row),
    ].filter(Boolean);
    const blockerHtml = row.className === "green" || !knownBlockers.length
      ? ""
      : `<div class="compat-blockers">
          ${knownBlockers.map((blocker) => `<span>${escapeHtml(blocker)}</span>`).join("")}
        </div>`;
    const queueHtml = row.className === "green" || (!diagnosticCodes.length && !reductionCandidates.length)
      ? ""
      : `<div class="compat-queue">
          <span>${escapeHtml(`queue: ${diagnosticCodes.length ? diagnosticCodes.join(", ") : "unclassified diagnostic"}`)}</span>
          ${reductionCandidates.map((candidate) => `<code>${escapeHtml(candidate)}</code>`).join("")}
        </div>`;
    const candidateExampleHtml = row.className === "green" || !diagnosticCandidateExamples.length
      ? ""
      : `<div class="compat-queue">
          ${diagnosticCandidateExamples.map((example) => {
            const codes = Array.isArray(example.codes) && example.codes.length
              ? ` ${example.codes.slice(0, 3).join(",")}`
              : "";
            const file = example.file || example.candidate_id || "unknown candidate";
            return `<code>${escapeHtml(`${example.compiler || "compiler"}:${codes} ${file}`)}</code>`;
          }).join("")}
        </div>`;
    const subsystemHtml = row.className === "green" || !diagnosticSubsystems.length
      ? ""
      : `<div class="compat-subsystems">
          ${diagnosticSubsystems.map((group) => {
            const codes = Array.isArray(group.codes) && group.codes.length ? ` (${group.codes.join(", ")})` : "";
            const count = Number.isFinite(Number(group.count)) && Number(group.count) > 1 ? ` x${Number(group.count)}` : "";
            return `<span>${escapeHtml(`${group.subsystem}${codes}${count}`)}</span>`;
          }).join("")}
        </div>`;
    const deltaHtml = row.className === "green"
      ? ""
      : `<div class="compat-deltas">${deltas.length
          ? deltas.map((delta) => `<code>${escapeHtml(delta)}</code>`).join("")
          : `<span>${escapeHtml("diagnostic delta not captured")}</span>`}
        </div>`;
    return `<div class="compat-meta">${parts.map((part) => `<span>${escapeHtml(part)}</span>`).join("")}</div>${blockerHtml}${subsystemHtml}${queueHtml}${candidateExampleHtml}${deltaHtml}`;
  };

  return `<section class="compat-dashboard">
  <h2>Compatibility</h2>
  <div class="compat-summary">${escapeHtml(summary)}</div>
  <ul class="compat-list">
    ${rows.map((row) => `<li class="compat-item">
      <div class="compat-row-main">
        <a href="${row.url}">${escapeHtml(row.label)}</a>
        <span class="compat-state ${row.className}">${escapeHtml(row.className)}</span>
        <span class="compat-detail">${escapeHtml(detailLabel(row))}</span>
      </div>
      ${renderRowDetails(row)}
    </li>`).join("\n")}
  </ul>
</section>`;
}
