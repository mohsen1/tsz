#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function asNumber(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function isGreen(row) {
  if (row.status) {
    return false;
  }
  const compatibility = row.compatibility;
  if (!compatibility) {
    return true;
  }
  return (
    compatibility.exit_class === "exit success" &&
    compatibility.diagnostic_status === "none"
  );
}

function ownerFamily(row) {
  return row.compatibility?.semantic_owner_family ?? null;
}

export function createTsgoWinnerReport(input, inputPath) {
  const rows = Array.isArray(input.results) ? input.results : [];
  const winners = rows
    .filter((row) => row?.winner === "tsgo" && isGreen(row))
    .map((row) => {
      const factor = asNumber(row.factor);
      const tszMs = asNumber(row.tsz_ms);
      const tsgoMs = asNumber(row.tsgo_ms);
      return {
        name: row.name,
        factor,
        tsz_ms: tszMs,
        tsgo_ms: tsgoMs,
        lines: asNumber(row.lines),
        kb: asNumber(row.kb),
        project_files: asNumber(row.project_files),
        semantic_owner_family: ownerFamily(row),
      };
    })
    .sort((a, b) => {
      const factorDelta = (b.factor ?? -Infinity) - (a.factor ?? -Infinity);
      if (factorDelta !== 0) {
        return factorDelta;
      }
      return String(a.name).localeCompare(String(b.name));
    });

  const projects = winners.filter((row) => row.semantic_owner_family);
  const byOwnerFamily = new Map();
  for (const row of projects) {
    const family = row.semantic_owner_family;
    if (!byOwnerFamily.has(family)) {
      byOwnerFamily.set(family, { family, rows: 0, worst_factor: null, worst_row: null });
    }
    const bucket = byOwnerFamily.get(family);
    bucket.rows += 1;
    if ((row.factor ?? -Infinity) > (bucket.worst_factor ?? -Infinity)) {
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
    },
    worst: winners[0] ?? null,
    by_owner_family: [...byOwnerFamily.values()].sort((a, b) => {
      const factorDelta = (b.worst_factor ?? -Infinity) - (a.worst_factor ?? -Infinity);
      if (factorDelta !== 0) {
        return factorDelta;
      }
      return a.family.localeCompare(b.family);
    }),
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
