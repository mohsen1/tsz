#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
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
    "ts-toolbelt-project",
    {
      owner: "Track 1/2 recursive type evaluation",
      operation:
        "recursive conditional, mapped/indexed access, repeated instantiation and relation cache pressure",
      command:
        "scripts/safe-run.sh ./scripts/bench/perf-hotspots.sh --filter '^ts-toolbelt-project$' --json-file <artifact>.json",
      issue: 8356,
      url: "https://github.com/mohsen1/tsz/issues/8356",
    },
  ],
  [
    "vite-vanilla-ts-app",
    {
      owner: "Track 7/9 generated app lib/module identity",
      operation: "generated app setup, lib/module identity, child-checker/project skeleton residency",
      command:
        "scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^vite-vanilla-ts-app$' --json-file <artifact>.json",
      issue: 7378,
      url: "https://github.com/mohsen1/tsz/issues/7378",
    },
  ],
  [
    "ts-essentials-project",
    {
      owner: "Track 1/2/5 utility type key-space and recursive shape evaluation",
      operation: "utility-type mapped/conditional/key-space workload with recursive JSON-like shapes",
      command:
        "scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^ts-essentials-project$' --json-file <artifact>.json",
      issue: 7378,
      url: "https://github.com/mohsen1/tsz/issues/7378",
    },
  ],
  [
    "nextjs-fresh-app",
    {
      owner: "Track 7/9 generated app dependency graph",
      operation: "generated app dependency/config setup and module/lib graph pressure",
      command:
        "scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --quick --filter '^nextjs-fresh-app$' --json-file <artifact>.json",
      issue: 7378,
      url: "https://github.com/mohsen1/tsz/issues/7378",
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
      url: "https://github.com/mohsen1/tsz/issues/8857",
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
      url: "https://github.com/mohsen1/tsz/issues/8858",
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

function measurementProfileStatus(input) {
  const profile = input?.measurement_profile;
  if (!profile || typeof profile !== "object") {
    return {
      present: false,
      mode: null,
      warning: "measurement_profile missing",
    };
  }

  const mode = typeof profile.mode === "string" && profile.mode ? profile.mode : null;
  return {
    present: true,
    mode,
    warning: mode ? null : "measurement_profile.mode missing",
  };
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

function sidecarPerfPath(inputPath) {
  if (typeof inputPath !== "string" || !inputPath.endsWith(".json")) return null;
  return inputPath.replace(/\.json$/, ".perf.json");
}

function singleRowSidecarAttribution(rows, inputPath) {
  if (rows.length !== 1) return new Map();

  const perfPath = sidecarPerfPath(inputPath);
  if (!perfPath || !fs.existsSync(perfPath)) return new Map();

  let snapshot;
  try {
    snapshot = readJson(perfPath);
  } catch {
    return new Map();
  }

  const row = rows[0];
  const relativePath = toPortablePath(path.relative(process.cwd(), perfPath));
  return new Map([
    [
      row.name,
      {
        path: relativePath,
        generated_at: fs.statSync(perfPath).mtime.toISOString(),
        mode: snapshot.mode ?? null,
        dominant_subsystem: inferDominantSubsystemFromPerfSnapshot(snapshot),
      },
    ],
  ]);
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
  return {
    present: true,
    path: pathValue,
    url: urlValue,
    generated_at: artifact.generated_at ?? artifact.generatedAt ?? null,
    mode: artifact.mode ?? null,
    dominant_subsystem: dominantSubsystem,
    warning: dominantSubsystem ? null : "attribution dominant_subsystem missing",
  };
}

function hasCompleteAttribution(status) {
  return Boolean(status?.present && status?.dominant_subsystem);
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
  const sidecarAttribution = singleRowSidecarAttribution(rows, inputPath);
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
        attribution_status: attributionStatusForRow(row, sidecarAttribution.get(row.name)),
      };
    });
  const targetGapRows = eligibleRows
    .filter((row) => row.tsz_speedup_vs_tsgo == null || row.tsz_speedup_vs_tsgo < TARGET_TSZ_SPEEDUP)
    .sort(compareTargetGaps);
  const missingTargetGapAttributionRows = targetGapRows
    .filter((row) => !hasCompleteAttribution(row.attribution_status))
    .map((row) => row.name)
    .sort();

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
      attribution_status: attributionStatusForRow(row, sidecarAttribution.get(row.name)),
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

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [inputPath, outputPath] = process.argv.slice(2);

  if (!inputPath || !outputPath) {
    console.error("usage: tsgo-winner-report.mjs <bench-results.json> <output.json>");
    process.exit(2);
  }

  const report = writeTsgoWinnerReport(inputPath, outputPath);
  console.log(
    [
      `green tsgo winners: ${report.totals.green_tsgo_winners}`,
      `project green tsgo winners: ${report.totals.project_green_tsgo_winners}`,
      `2x target gaps: ${report.two_x_target.rows_below_target}/${report.two_x_target.eligible_green_rows}`,
      `2x target gaps with attribution: ${report.two_x_target.rows_with_attribution}/${report.two_x_target.rows_below_target}`,
      `report: ${path.relative(process.cwd(), outputPath).split(path.sep).join("/")}`,
    ].join("\n"),
  );

  if (report.totals.duplicate_project_rows > 0) {
    console.error(
      `duplicate project rows: ${report.duplicate_rows
        .map((row) => `${row.name} (${row.count})`)
        .join(", ")}`,
    );
    process.exit(1);
  }
}
