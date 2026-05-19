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

// Null factors sort last (treated as the lowest possible value) so that rows
// with a real factor always appear before rows with an unknown factor.
function factorOrLowest(value) {
  return value ?? -Infinity;
}

function compareWinnersByFactorDesc(a, b) {
  const factorDelta = factorOrLowest(b.factor) - factorOrLowest(a.factor);
  if (factorDelta !== 0) return factorDelta;
  return String(a.name).localeCompare(String(b.name));
}

function compareFamiliesByWorstFactorDesc(a, b) {
  const factorDelta = factorOrLowest(b.worst_factor) - factorOrLowest(a.worst_factor);
  if (factorDelta !== 0) return factorDelta;
  return a.family.localeCompare(b.family);
}

export function createTsgoWinnerReport(input, inputPath) {
  const rows = Array.isArray(input.results) ? input.results : [];

  let incompleteCompatExcluded = 0;
  for (const row of rows) {
    if (isIncompleteCompat(row)) incompleteCompatExcluded += 1;
  }

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
    }))
    .sort(compareWinnersByFactorDesc);

  const projects = winners.filter((row) => row.semantic_owner_family);
  const byOwnerFamily = new Map();
  for (const row of projects) {
    const family = row.semantic_owner_family;
    let bucket = byOwnerFamily.get(family);
    if (!bucket) {
      bucket = { family, rows: 0, worst_factor: null, worst_row: null };
      byOwnerFamily.set(family, bucket);
    }
    bucket.rows += 1;
    if (factorOrLowest(row.factor) > factorOrLowest(bucket.worst_factor)) {
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
