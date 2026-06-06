#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { measurementProfileStatus } from "./measurement-profile.mjs";
import { PROJECT_ROWS_BY_NAME } from "./project-rows.mjs";
import { isGreen, isIncompleteCompat } from "./row-utils.mjs";

const TARGET_TSZ_SPEEDUP = 2;

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function toPortablePath(file) {
  return file.split(path.sep).join("/");
}

function asNumber(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

const LOSS_CLOSURE_BY_ROW = new Map([
  [
    "utility-types-project",
    {
      owner: "Track 1/2/5 utility type key-space and mapped type evaluation",
      operation: "utility-type mapped/key-space workload with cross-file helper imports",
      command:
        "scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^utility-types-project$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=536870912 scripts/safe-run.sh cargo run -q -p tsz-cli --features perf-tools --bin tsz -- --extendedDiagnostics --perf-counters-json <artifact>.utility-types-project.perf.json --noEmit -p .target-bench/external/utility-types/tsconfig.flat.json",
      issue: 7378,
      url: "https://github.com/tsz-org/tsz/issues/7378",
    },
  ],
  [
    "ts-toolbelt-project",
    {
      owner: "Track 1/2 recursive type evaluation",
      operation:
        "recursive conditional, mapped/indexed access, repeated instantiation and relation cache pressure",
      command:
        "scripts/safe-run.sh ./scripts/bench/perf-hotspots.sh --filter '^ts-toolbelt-project$' --json-file <artifact>.json",
      issue: 8356,
      url: "https://github.com/tsz-org/tsz/issues/8356",
    },
  ],
  [
    "vite-vanilla-ts-app",
    {
      owner: "Track 7/9 generated app lib/module identity",
      operation: "generated app setup, lib/module identity, child-checker/project skeleton residency",
      command:
        "scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^vite-vanilla-ts-app$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=536870912 scripts/safe-run.sh cargo run -q -p tsz-cli --features perf-tools --bin tsz -- --extendedDiagnostics --perf-counters-json <artifact>.vite-vanilla-ts-app.perf.json --noEmit -p .target-bench/external/vite-vanilla-ts-live/tsconfig.json",
      issue: 7378,
      url: "https://github.com/tsz-org/tsz/issues/7378",
    },
  ],
  [
    "ts-essentials-project",
    {
      owner: "Track 1/2/5 utility type key-space and recursive shape evaluation",
      operation: "utility-type mapped/conditional/key-space workload with recursive JSON-like shapes",
      command:
        "scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^ts-essentials-project$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=536870912 scripts/safe-run.sh cargo run -q -p tsz-cli --features perf-tools --bin tsz -- --extendedDiagnostics --perf-counters-json <artifact>.ts-essentials-project.perf.json --noEmit -p .target-bench/external/ts-essentials/tsconfig.flat.json",
      issue: 7378,
      url: "https://github.com/tsz-org/tsz/issues/7378",
    },
  ],
  [
    "ts-essentials/xor.ts",
    {
      owner: "Track 1/2/5 utility type key-space and union exclusion evaluation",
      operation: "large XOR helper with repeated Exclude/keyof intersections and union normalization",
      command:
        "scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^ts-essentials/xor\\.ts$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=536870912 scripts/safe-run.sh cargo run -q -p tsz-cli --features perf-tools --bin tsz -- --extendedDiagnostics --perf-counters-json <artifact>.ts-essentials-xor.perf.json --noEmit --lib es2018 .target-bench/external/ts-essentials/lib/xor/index.ts",
      issue: 7378,
      url: "https://github.com/tsz-org/tsz/issues/7378",
    },
  ],
  [
    "ts-essentials/paths.ts",
    {
      owner: "Track 1/2/5 recursive path utility and cross-file helper evaluation",
      operation: "recursive path/key utility expansion with imported helper aliases",
      command:
        "scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^ts-essentials/paths\\.ts$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=536870912 scripts/safe-run.sh cargo run -q -p tsz-cli --features perf-tools --bin tsz -- --extendedDiagnostics --perf-counters-json <artifact>.ts-essentials-paths.perf.json --noEmit --lib es2018 .target-bench/external/ts-essentials/lib/paths/index.ts",
      issue: 7378,
      url: "https://github.com/tsz-org/tsz/issues/7378",
    },
  ],
  [
    "ts-essentials/deep-pick.ts",
    {
      owner: "Track 1/2/5 recursive key-path mapped type evaluation",
      operation: "deep-pick recursive mapped/key-path helper expansion",
      command:
        "scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^ts-essentials/deep-pick\\.ts$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=536870912 scripts/safe-run.sh cargo run -q -p tsz-cli --features perf-tools --bin tsz -- --extendedDiagnostics --perf-counters-json <artifact>.ts-essentials-deep-pick.perf.json --noEmit --lib es2018 .target-bench/external/ts-essentials/lib/deep-pick/index.ts",
      issue: 7378,
      url: "https://github.com/tsz-org/tsz/issues/7378",
    },
  ],
  [
    "ts-essentials/deep-readonly.ts",
    {
      owner: "Track 1/2/5 recursive mapped readonly evaluation",
      operation: "deep-readonly recursive mapped helper expansion over imported utility aliases",
      command:
        "scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^ts-essentials/deep-readonly\\.ts$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=536870912 scripts/safe-run.sh cargo run -q -p tsz-cli --features perf-tools --bin tsz -- --extendedDiagnostics --perf-counters-json <artifact>.ts-essentials-deep-readonly.perf.json --noEmit --lib es2018 .target-bench/external/ts-essentials/lib/deep-readonly/index.ts",
      issue: 7378,
      url: "https://github.com/tsz-org/tsz/issues/7378",
    },
  ],
  [
    "nextjs-fresh-app",
    {
      owner: "Track 7/9 generated app dependency graph",
      operation: "generated app dependency/config setup and module/lib graph pressure",
      command:
        "scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^nextjs-fresh-app$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=536870912 scripts/safe-run.sh cargo run -q -p tsz-cli --features perf-tools --bin tsz -- --extendedDiagnostics --perf-counters-json <artifact>.nextjs-fresh-app.perf.json --noEmit -p .target-bench/external/next-app-live/tsconfig.json",
      issue: 7378,
      url: "https://github.com/tsz-org/tsz/issues/7378",
    },
  ],
  [
    "nextjs",
    {
      owner: "Track 7/9 large app module graph and lib identity",
      operation: "Next.js package graph checking, module resolution, and lib/global symbol residency",
      command:
        "NEXTJS_BENCHMARK_ENABLED=1 scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^nextjs$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=536870912 scripts/safe-run.sh cargo run -q -p tsz-cli --features perf-tools --bin tsz -- --extendedDiagnostics --perf-counters-json <artifact>.nextjs.perf.json --noEmit -p .target-bench/external/nextjs/packages/next/tsconfig.tsz-bench.json",
      issue: 7378,
      url: "https://github.com/tsz-org/tsz/issues/7378",
    },
  ],
  [
    "BCT candidates=200",
    {
      owner: "Track 10 best-common-type scale guard",
      operation: "best-common-type fallback candidate subtype reduction",
      command:
        "scripts/safe-run.sh ./scripts/bench/perf-hotspots.sh --filter '^BCT candidates=200$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 .target/release/tsz --extendedDiagnostics --perf-counters-json <artifact>.perf.json --noEmit <generated-bct-candidates-200>.ts",
      issue: 8857,
      url: "https://github.com/tsz-org/tsz/issues/8857",
    },
  ],
  [
    "200 classes",
    {
      owner: "Track 10 class/symbol/member table scale guard",
      operation: "class declaration/member-table construction and checker/binder symbol lookup pressure",
      command:
        "scripts/safe-run.sh ./scripts/bench/perf-hotspots.sh --filter '^200 classes$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 .target/release/tsz --extendedDiagnostics --perf-counters-json <artifact>.perf.json --noEmit <generated-200-classes>.ts",
      issue: 8858,
      url: "https://github.com/tsz-org/tsz/issues/8858",
    },
  ],
  [
    "100 generic functions",
    {
      owner: "Track 10 generic function scaling guard",
      operation:
        "generic async function checking with recursive DeepPartial option types and Promise<Result<T>> return construction",
      command:
        "scripts/safe-run.sh ./scripts/bench/perf-hotspots.sh --filter '^100 generic functions$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 .target/release/tsz --extendedDiagnostics --perf-counters-json <artifact>.perf.json --noEmit <generated-100-generic-functions>.ts",
      issue: 12271,
      url: "https://github.com/tsz-org/tsz/issues/12271",
    },
  ],
  [
    "200 generic functions",
    {
      owner: "Track 10 generic function scaling guard",
      operation:
        "generic async function checking with recursive DeepPartial option types and Promise<Result<T>> return construction",
      command:
        "scripts/safe-run.sh ./scripts/bench/perf-hotspots.sh --filter '^200 generic functions$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 .target/release/tsz --extendedDiagnostics --perf-counters-json <artifact>.perf.json --noEmit <generated-200-generic-functions>.ts",
      issue: 12271,
      url: "https://github.com/tsz-org/tsz/issues/12271",
    },
  ],
  [
    "CFA branches=100",
    {
      owner: "Track 10 control-flow analysis scaling guard",
      operation: "control-flow narrowing across many generated branch joins",
      command:
        "scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^CFA branches=100$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 .target/release/tsz --extendedDiagnostics --perf-counters-json <artifact>.perf.json --noEmit <generated-cfa-branches-100>.ts",
      issue: 12271,
      url: "https://github.com/tsz-org/tsz/issues/12271",
    },
  ],
  [
    "CFA branches=150",
    {
      owner: "Track 10 control-flow analysis scaling guard",
      operation: "control-flow narrowing across many generated branch joins",
      command:
        "scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^CFA branches=150$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 .target/release/tsz --extendedDiagnostics --perf-counters-json <artifact>.perf.json --noEmit <generated-cfa-branches-150>.ts",
      issue: 12271,
      url: "https://github.com/tsz-org/tsz/issues/12271",
    },
  ],
  [
    "Template literal N=45",
    {
      owner: "Track 10 template literal expansion scaling guard",
      operation: "template literal Cartesian-product expansion and string manipulation helper evaluation",
      command:
        "scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^Template literal N=45$' --json-file <artifact>.json",
      attribution_command:
        "TSZ_PERF_COUNTERS=1 .target/release/tsz --extendedDiagnostics --perf-counters-json <artifact>.perf.json --noEmit <generated-template-literal-45>.ts",
      issue: 12271,
      url: "https://github.com/tsz-org/tsz/issues/12271",
    },
  ],
]);

function lossClosureForRow(row) {
  return LOSS_CLOSURE_BY_ROW.get(row.name) ?? null;
}

function tszSpeedupVsTsgo(row) {
  const tszMs = asNumber(row?.tsz_ms);
  const tsgoMs = asNumber(row?.tsgo_ms);
  if (tszMs != null && tsgoMs != null && tszMs > 0 && tsgoMs > 0) {
    return tsgoMs / tszMs;
  }

  const factor = asNumber(row?.factor ?? row?.ratio);
  if (factor == null || factor <= 0) return null;
  if (row?.winner === "tsz") return factor;
  if (row?.winner === "tsgo") return 1 / factor;
  return null;
}

function inferDominantSubsystemFromPerfSnapshot(snapshot) {
  const delegateMisses = asNumber(snapshot?.delegate?.misses) ?? 0;
  const parentCache = asNumber(snapshot?.checker?.with_parent_cache_constructed) ?? 0;
  if (delegateMisses > 0 || parentCache > 10) {
    return "checker:cross-arena-delegation";
  }

  if (Array.isArray(snapshot?.slow_check_file_timings) && snapshot.slow_check_file_timings.length > 0) {
    return "checker:semantic-check";
  }

  const internerCalls = asNumber(snapshot?.interner?.intern_calls) ?? 0;
  if (internerCalls > 0) {
    return "solver:type-interning";
  }

  return null;
}

function topSlowTiming(rows) {
  if (!Array.isArray(rows)) return null;

  let best = null;
  for (const row of rows) {
    const elapsedMs = asNumber(row?.elapsed_ms);
    if (elapsedMs == null) continue;
    if (!best || elapsedMs > best.elapsed_ms) {
      best = { row, elapsed_ms: elapsedMs };
    }
  }

  return best;
}

function inferDominantHotspotFromPerfSnapshot(snapshot) {
  const typeAlias = topSlowTiming(snapshot?.slow_type_alias_check_timings);
  if (typeAlias) {
    const row = typeAlias.row;
    return {
      kind: "type_alias_phase",
      name: typeof row?.name === "string" ? row.name : null,
      phase: typeof row?.phase === "string" ? row.phase : null,
      elapsed_ms: typeAlias.elapsed_ms,
      file: typeof row?.file === "string" ? toPortablePath(row.file) : null,
    };
  }

  const statement = topSlowTiming(snapshot?.slow_check_statement_timings);
  if (statement) {
    const row = statement.row;
    return {
      kind: "statement",
      syntax_kind: asNumber(row?.kind),
      elapsed_ms: statement.elapsed_ms,
      file: typeof row?.file === "string" ? toPortablePath(row.file) : null,
    };
  }

  const file = topSlowTiming(snapshot?.slow_check_file_timings);
  if (file) {
    const row = file.row;
    return {
      kind: "file",
      elapsed_ms: file.elapsed_ms,
      file: typeof row?.file === "string" ? toPortablePath(row.file) : null,
    };
  }

  return null;
}

function sidecarPerfPath(inputPath) {
  if (typeof inputPath !== "string" || !inputPath.endsWith(".json")) return null;
  return inputPath.replace(/\.json$/, ".perf.json");
}

function rowSidecarPerfPath(inputPath, rowName) {
  if (typeof inputPath !== "string" || !inputPath.endsWith(".json")) return null;
  if (typeof rowName !== "string" || rowName.length === 0) return null;
  const slug = rowName.replace(/[^A-Za-z0-9._-]+/g, "_");
  return inputPath.replace(/\.json$/, `.${slug}.perf.json`);
}

function sidecarAttributionForPath(perfPath) {
  if (!perfPath || !fs.existsSync(perfPath)) return null;

  let snapshot;
  try {
    snapshot = readJson(perfPath);
  } catch {
    return null;
  }

  const relativePath = toPortablePath(path.relative(process.cwd(), perfPath));
  const mode = snapshot.mode ?? null;
  const isAttributionMode = mode === "attribution";
  return {
    path: relativePath,
    generated_at: fs.statSync(perfPath).mtime.toISOString(),
    mode,
    dominant_subsystem: isAttributionMode
      ? inferDominantSubsystemFromPerfSnapshot(snapshot)
      : null,
    dominant_hotspot: isAttributionMode
      ? inferDominantHotspotFromPerfSnapshot(snapshot)
      : null,
    warning: isAttributionMode ? null : "sidecar perf snapshot mode is not attribution",
  };
}

function sidecarAttribution(rows, inputPath) {
  const attributions = new Map();
  if (rows.length === 1) {
    const attribution = sidecarAttributionForPath(sidecarPerfPath(inputPath));
    if (attribution) attributions.set(rows[0].name, attribution);
  }

  for (const row of rows) {
    if (typeof row?.name !== "string" || attributions.has(row.name)) continue;
    const attribution = sidecarAttributionForPath(rowSidecarPerfPath(inputPath, row.name));
    if (attribution) attributions.set(row.name, attribution);
  }

  return attributions;
}

function pickAttributionArtifact(row, fallbackArtifact = null) {
  return (
    row?.attribution_artifact ??
    row?.performance_attribution ??
    row?.attribution ??
    row?.compatibility?.attribution_artifact ??
    row?.compatibility?.performance_attribution ??
    row?.compatibility?.attribution ??
    fallbackArtifact ??
    null
  );
}

function attributionStatusForRow(row, fallbackArtifact = null) {
  const artifact = pickAttributionArtifact(row, fallbackArtifact);
  if (!artifact) {
    return {
      present: false,
      path: null,
      url: null,
      generated_at: null,
      mode: null,
      dominant_subsystem: null,
      warning: "attribution artifact missing",
    };
  }

  if (typeof artifact === "string") {
    return {
      present: true,
      path: artifact,
      url: null,
      generated_at: null,
      mode: null,
      dominant_subsystem: null,
      warning: "attribution dominant_subsystem missing",
    };
  }

  const pathValue = artifact.path ?? artifact.file ?? artifact.artifact ?? null;
  const urlValue = artifact.url ?? null;
  const dominantSubsystem = artifact.dominant_subsystem ?? artifact.dominantSubsystem ?? null;
  const dominantHotspot = artifact.dominant_hotspot ?? artifact.dominantHotspot ?? null;
  const warningValue = artifact.warning ?? null;
  const status = {
    present: true,
    path: pathValue,
    url: urlValue,
    generated_at: artifact.generated_at ?? artifact.generatedAt ?? null,
    mode: artifact.mode ?? null,
    dominant_subsystem: dominantSubsystem,
    warning: warningValue ?? (dominantSubsystem ? null : "attribution dominant_subsystem missing"),
  };
  if (dominantHotspot) {
    status.dominant_hotspot = dominantHotspot;
  }
  return status;
}

function hasCompleteAttribution(status) {
  return Boolean(status?.present && status?.dominant_subsystem && !status?.warning);
}

function missingAttributionPlanForRow(row) {
  const closure = row?.loss_closure ?? null;
  return {
    name: row.name,
    target_gap_factor: row.target_gap_factor,
    tsz_speedup_vs_tsgo: row.tsz_speedup_vs_tsgo,
    semantic_owner_family: row.semantic_owner_family ?? null,
    owner: closure?.owner ?? null,
    issue: closure?.issue ?? null,
    url: closure?.url ?? null,
    attribution_command: closure?.attribution_command ?? null,
    timing_command: closure?.command ?? null,
    attribution_warning: row.attribution_status?.warning ?? null,
  };
}

function markdownValue(value) {
  if (value == null || value === "") return "n/a";
  return String(value).replaceAll("|", "\\|");
}

function formatNumber(value, digits = 2) {
  const number = asNumber(value);
  return number == null ? "n/a" : number.toFixed(digits);
}

export function renderMissingAttributionPlanMarkdown(report) {
  const target = report?.two_x_target ?? {};
  const rows = Array.isArray(target.missing_attribution_plan)
    ? target.missing_attribution_plan
    : [];
  const lines = [
    "# 2x Target Gap Attribution Plan",
    "",
    `Generated: ${markdownValue(report?.generated_at)}`,
    `Source: ${markdownValue(report?.source?.path)}`,
    "",
    "| Metric | Value |",
    "| --- | ---: |",
    `| Eligible green rows | ${markdownValue(target.eligible_green_rows)} |`,
    `| Rows below 2x target | ${markdownValue(target.rows_below_target)} |`,
    `| Project rows below 2x target | ${markdownValue(target.project_rows_below_target)} |`,
    `| Rows with attribution | ${markdownValue(target.rows_with_attribution)} |`,
    `| Missing attribution rows | ${rows.length} |`,
    "",
  ];

  if (rows.length === 0) {
    lines.push("All current 2x target gap rows have attribution evidence.", "");
    return `${lines.join("\n")}\n`;
  }

  lines.push(
    "| Rank | Row | Speedup vs tsgo | Gap factor | Owner | Issue |",
    "| ---: | --- | ---: | ---: | --- | --- |",
  );
  rows.forEach((row, index) => {
    lines.push(
      `| ${index + 1} | \`${markdownValue(row.name)}\` | ${formatNumber(row.tsz_speedup_vs_tsgo)}x | ${formatNumber(row.target_gap_factor)}x | ${markdownValue(row.owner)} | ${row.url ? `[${markdownValue(row.issue)}](${row.url})` : markdownValue(row.issue)} |`,
    );
  });

  lines.push("");
  rows.forEach((row, index) => {
    lines.push(
      `## ${index + 1}. ${markdownValue(row.name)}`,
      "",
      `Owner: ${markdownValue(row.owner)}`,
      `Semantic family: ${markdownValue(row.semantic_owner_family)}`,
      `Attribution status: ${markdownValue(row.attribution_warning)}`,
      "",
    );
    if (row.attribution_command) {
      lines.push("Attribution command:", "", "```bash", row.attribution_command, "```", "");
    } else {
      lines.push("Attribution command: n/a", "");
    }
    if (row.timing_command) {
      lines.push("Timing command:", "", "```bash", row.timing_command, "```", "");
    }
  });

  return `${lines.join("\n")}\n`;
}

function targetGapFactor(speedup) {
  if (speedup == null || speedup <= 0) return null;
  return TARGET_TSZ_SPEEDUP / speedup;
}

function targetGapForSort(value) {
  return value ?? -Infinity;
}

// Null factors sort last (treated as the lowest possible value) so that rows
// with a real factor always appear before rows with an unknown factor.
function factorForSort(value) {
  return value ?? -Infinity;
}

function compareWinnersByFactorDesc(a, b) {
  const factorDelta = factorForSort(b.factor) - factorForSort(a.factor);
  if (factorDelta !== 0) return factorDelta;
  return String(a.name).localeCompare(String(b.name));
}

function compareFamiliesByWorstFactorDesc(a, b) {
  const factorDelta = factorForSort(b.worst_factor) - factorForSort(a.worst_factor);
  if (factorDelta !== 0) return factorDelta;
  return a.family.localeCompare(b.family);
}

function compareTargetGaps(a, b) {
  const gapDelta = targetGapForSort(b.target_gap_factor) - targetGapForSort(a.target_gap_factor);
  if (gapDelta !== 0) return gapDelta;
  return String(a.name).localeCompare(String(b.name));
}

function duplicateProjectRows(rows) {
  const counts = new Map();
  for (const row of rows) {
    const name = typeof row?.name === "string" ? row.name : null;
    if (!name || !Object.hasOwn(PROJECT_ROWS_BY_NAME, name)) continue;
    counts.set(name, (counts.get(name) ?? 0) + 1);
  }

  return [...counts]
    .filter(([, count]) => count > 1)
    .map(([name, count]) => ({
      name,
      label: PROJECT_ROWS_BY_NAME[name]?.label ?? name,
      count,
    }))
    .sort((a, b) => a.name.localeCompare(b.name));
}

export function createTsgoWinnerReport(input, inputPath) {
  const rows = Array.isArray(input.results) ? input.results : [];
  const rowSidecarAttribution = sidecarAttribution(rows, inputPath);
  const duplicateRows = duplicateProjectRows(rows);
  const duplicateNames = new Set(duplicateRows.map((row) => row.name));
  const incompleteCompatExcluded = rows.filter(isIncompleteCompat).length;
  const eligibleRows = rows
    .filter((row) => isGreen(row) && !duplicateNames.has(row?.name))
    .map((row) => {
      const speedup = tszSpeedupVsTsgo(row);
      const gapFactor = speedup == null ? null : targetGapFactor(speedup);
      return {
        name: row.name,
        winner: row.winner ?? null,
        factor: asNumber(row.factor),
        tsz_speedup_vs_tsgo: speedup,
        target_gap_factor: gapFactor,
        tsz_ms: asNumber(row.tsz_ms),
        tsgo_ms: asNumber(row.tsgo_ms),
        lines: asNumber(row.lines),
        kb: asNumber(row.kb),
        project_files: asNumber(row.project_files),
        files_reached: asNumber(row.compatibility?.files_reached ?? row.project_files),
        peak_memory_bytes: asNumber(row.compatibility?.peak_memory_bytes),
        exit_class: row.compatibility?.exit_class ?? null,
        semantic_owner_family: row.compatibility?.semantic_owner_family ?? null,
        loss_closure: lossClosureForRow(row),
        attribution_status: attributionStatusForRow(row, rowSidecarAttribution.get(row.name)),
      };
    });
  const targetGapRows = eligibleRows
    .filter((row) => row.tsz_speedup_vs_tsgo == null || row.tsz_speedup_vs_tsgo < TARGET_TSZ_SPEEDUP)
    .sort(compareTargetGaps);
  const missingTargetGapAttributionRows = targetGapRows
    .filter((row) => !hasCompleteAttribution(row.attribution_status))
    .map((row) => row.name)
    .sort();
  const missingTargetGapAttributionPlan = targetGapRows
    .filter((row) => !hasCompleteAttribution(row.attribution_status))
    .map(missingAttributionPlanForRow);
  const targetGapRowsWithAttributionCommand = missingTargetGapAttributionPlan
    .filter((row) => row.attribution_command).length;

  const winners = rows
    .filter((row) => row?.winner === "tsgo" && isGreen(row) && !duplicateNames.has(row?.name))
    .map((row) => ({
      name: row.name,
      factor: asNumber(row.factor),
      tsz_ms: asNumber(row.tsz_ms),
      tsgo_ms: asNumber(row.tsgo_ms),
      lines: asNumber(row.lines),
      kb: asNumber(row.kb),
      project_files: asNumber(row.project_files),
      files_reached: asNumber(row.compatibility?.files_reached ?? row.project_files),
      peak_memory_bytes: asNumber(row.compatibility?.peak_memory_bytes),
      exit_class: row.compatibility?.exit_class ?? null,
      semantic_owner_family: row.compatibility?.semantic_owner_family ?? null,
      loss_closure: lossClosureForRow(row),
      attribution_status: attributionStatusForRow(row, rowSidecarAttribution.get(row.name)),
    }))
    .sort(compareWinnersByFactorDesc);

  const projects = winners.filter((row) => row.semantic_owner_family);
  const missingLossClosureRows = winners
    .filter((row) => !row.loss_closure)
    .map((row) => row.name)
    .sort();
  const missingAttributionRows = winners
    .filter((row) => !hasCompleteAttribution(row.attribution_status))
    .map((row) => row.name)
    .sort();
  const byOwnerFamily = new Map();
  for (const row of projects) {
    const family = row.semantic_owner_family;
    let bucket = byOwnerFamily.get(family);
    if (!bucket) {
      bucket = { family, rows: 0, worst_factor: null, worst_row: null };
      byOwnerFamily.set(family, bucket);
    }
    bucket.rows += 1;
    if (factorForSort(row.factor) > factorForSort(bucket.worst_factor)) {
      bucket.worst_factor = row.factor;
      bucket.worst_row = row.name;
    }
  }

  return {
    generated_at: new Date().toISOString(),
    source: {
      path: inputPath,
      benchmark_runner: input.benchmark_runner ?? null,
      quick_mode: input.quick_mode ?? null,
      filter: input.filter ?? null,
    },
    totals: {
      rows: rows.length,
      duplicate_project_rows: duplicateRows.length,
      green_tsgo_winners: winners.length,
      project_green_tsgo_winners: projects.length,
      green_tsgo_winners_with_closure: winners.length - missingLossClosureRows.length,
      missing_loss_closure_rows: missingLossClosureRows,
      green_tsgo_winners_with_attribution: winners.length - missingAttributionRows.length,
      missing_attribution_rows: missingAttributionRows,
      incomplete_compat_excluded: incompleteCompatExcluded,
    },
    two_x_target: {
      tsz_speedup_target: TARGET_TSZ_SPEEDUP,
      eligible_green_rows: eligibleRows.length,
      project_eligible_green_rows: eligibleRows.filter((row) => row.semantic_owner_family).length,
      rows_meeting_target: eligibleRows.length - targetGapRows.length,
      rows_below_target: targetGapRows.length,
      project_rows_below_target: targetGapRows.filter((row) => row.semantic_owner_family).length,
      rows_with_attribution: targetGapRows.length - missingTargetGapAttributionRows.length,
      missing_attribution_rows: missingTargetGapAttributionRows,
      rows_with_attribution_command: targetGapRowsWithAttributionCommand,
      missing_attribution_plan: missingTargetGapAttributionPlan,
      worst_gap: targetGapRows[0] ?? null,
    },
    measurement_profile: measurementProfileStatus(input),
    duplicate_rows: duplicateRows,
    target_gaps: targetGapRows,
    worst: winners[0] ?? null,
    by_owner_family: [...byOwnerFamily.values()].sort(compareFamiliesByWorstFactorDesc),
    rows: winners,
  };
}

export function writeTsgoWinnerReport(inputPath, outputPath) {
  const report = createTsgoWinnerReport(readJson(inputPath), inputPath);
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
  return report;
}

export function writeMissingAttributionPlan(report, outputPath) {
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, renderMissingAttributionPlanMarkdown(report));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [inputPath, outputPath, attributionPlanPath] = process.argv.slice(2);

  if (!inputPath || !outputPath) {
    console.error("usage: tsgo-winner-report.mjs <bench-results.json> <output.json> [missing-attribution.md]");
    process.exit(2);
  }

  const report = writeTsgoWinnerReport(inputPath, outputPath);
  if (attributionPlanPath) {
    writeMissingAttributionPlan(report, attributionPlanPath);
  }
  const outputLines = [
    `green tsgo winners: ${report.totals.green_tsgo_winners}`,
    `project green tsgo winners: ${report.totals.project_green_tsgo_winners}`,
    `2x target gaps: ${report.two_x_target.rows_below_target}/${report.two_x_target.eligible_green_rows}`,
    `2x target gaps with attribution: ${report.two_x_target.rows_with_attribution}/${report.two_x_target.rows_below_target}`,
    `2x target gaps with attribution commands: ${report.two_x_target.rows_with_attribution_command}/${report.two_x_target.rows_below_target}`,
    `report: ${path.relative(process.cwd(), outputPath).split(path.sep).join("/")}`,
  ];
  if (attributionPlanPath) {
    outputLines.push(
      `missing attribution plan: ${path.relative(process.cwd(), attributionPlanPath).split(path.sep).join("/")}`,
    );
  }
  console.log(outputLines.join("\n"));

  if (report.totals.duplicate_project_rows > 0) {
    console.error(
      `duplicate project rows: ${report.duplicate_rows
        .map((row) => `${row.name} (${row.count})`)
        .join(", ")}`,
    );
    process.exit(1);
  }
}
