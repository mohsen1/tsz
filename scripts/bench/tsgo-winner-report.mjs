#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { isGreen, isIncompleteCompat } from "./row-utils.mjs";

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
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
      issue: 8858,
      url: "https://github.com/mohsen1/tsz/issues/8858",
    },
  ],
]);

function lossClosureForRow(row) {
  return LOSS_CLOSURE_BY_ROW.get(row.name) ?? null;
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

export function createTsgoWinnerReport(input, inputPath) {
  const rows = Array.isArray(input.results) ? input.results : [];
  const incompleteCompatExcluded = rows.filter(isIncompleteCompat).length;

  const winners = rows
    .filter((row) => row?.winner === "tsgo" && isGreen(row))
    .map((row) => ({
      name: row.name,
      factor: asNumber(row.factor),
      tsz_ms: asNumber(row.tsz_ms),
      tsgo_ms: asNumber(row.tsgo_ms),
      lines: asNumber(row.lines),
      kb: asNumber(row.kb),
      project_files: asNumber(row.project_files),
      semantic_owner_family: row.compatibility?.semantic_owner_family ?? null,
      loss_closure: lossClosureForRow(row),
    }))
    .sort(compareWinnersByFactorDesc);

  const projects = winners.filter((row) => row.semantic_owner_family);
  const missingLossClosureRows = winners
    .filter((row) => !row.loss_closure)
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
      green_tsgo_winners: winners.length,
      project_green_tsgo_winners: projects.length,
      green_tsgo_winners_with_closure: winners.length - missingLossClosureRows.length,
      missing_loss_closure_rows: missingLossClosureRows,
      incomplete_compat_excluded: incompleteCompatExcluded,
    },
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
      `report: ${path.relative(process.cwd(), outputPath).split(path.sep).join("/")}`,
    ].join("\n"),
  );
}
