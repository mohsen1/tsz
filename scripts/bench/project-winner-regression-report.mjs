#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import {
  PROJECT_ROWS_BY_NAME,
} from "./project-rows.mjs";
import { isGreen } from "./row-utils.mjs";

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function asNumber(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function sourceInfo(artifact, file) {
  return {
    path: file,
    source_commit: artifact?.source_commit ?? null,
    generated_at: artifact?.generated_at ?? null,
    workflow_run_url: artifact?.workflow_run_url ?? null,
  };
}

function indexedProjectRows(artifact) {
  const rows = new Map();
  const duplicateCounts = new Map();

  for (const row of Array.isArray(artifact?.results) ? artifact.results : []) {
    const name = row?.name;
    if (typeof name !== "string") continue;
    if (rows.has(name)) {
      duplicateCounts.set(name, (duplicateCounts.get(name) ?? 1) + 1);
    } else {
      rows.set(name, row);
    }
  }

  const duplicates = [...duplicateCounts]
    .filter(([name]) => Object.hasOwn(PROJECT_ROWS_BY_NAME, name))
    .map(([name, count]) => ({
      name,
      label: PROJECT_ROWS_BY_NAME[name]?.label ?? name,
      count,
    }))
    .sort((a, b) => a.name.localeCompare(b.name));

  return { rows, duplicates };
}

function rowTiming(row) {
  return {
    winner: row?.winner ?? null,
    factor: asNumber(row?.factor ?? row?.ratio),
    tsz_ms: asNumber(row?.tsz_ms),
    tsgo_ms: asNumber(row?.tsgo_ms),
  };
}

function isGreenProjectRow(row) {
  return Boolean(row?.compatibility) && isGreen(row);
}

function compareRegressions(previous, current, previousPath, currentPath) {
  const previousIndex = indexedProjectRows(previous);
  const currentIndex = indexedProjectRows(current);
  const previousRows = previousIndex.rows;
  const currentRows = currentIndex.rows;
  const duplicateRows = [
    ...previousIndex.duplicates.map((row) => ({ ...row, artifact: "previous" })),
    ...currentIndex.duplicates.map((row) => ({ ...row, artifact: "current" })),
  ];
  const duplicatedKeys = new Set(duplicateRows.map((row) => `${row.artifact}:${row.name}`));
  const regressions = [];
  let compared = 0;

  for (const [name, def] of Object.entries(PROJECT_ROWS_BY_NAME)) {
    if (duplicatedKeys.has(`previous:${name}`) || duplicatedKeys.has(`current:${name}`)) continue;
    const before = previousRows.get(name);
    const after = currentRows.get(name);
    if (!before || !after) continue;
    if (!isGreenProjectRow(before) || !isGreenProjectRow(after)) continue;
    compared += 1;
    if (before.winner !== "tsz" || after.winner !== "tsgo") continue;
    regressions.push({
      name,
      label: def.label ?? name,
      owner: def.owner ?? null,
      family: def.family ?? null,
      previous: rowTiming(before),
      current: rowTiming(after),
    });
  }

  regressions.sort((a, b) => {
    const currentDelta = (b.current.factor ?? -Infinity) - (a.current.factor ?? -Infinity);
    if (currentDelta !== 0) return currentDelta;
    return a.name.localeCompare(b.name);
  });

  return {
    schema_version: 1,
    generated_at: new Date().toISOString(),
    previous: sourceInfo(previous, previousPath),
    current: sourceInfo(current, currentPath),
    totals: {
      known_project_rows: Object.keys(PROJECT_ROWS_BY_NAME).length,
      green_project_rows_compared: compared,
      tsz_to_tsgo_regressions: regressions.length,
      duplicate_project_rows: duplicateRows.length,
    },
    duplicate_rows: duplicateRows,
    rows: regressions,
  };
}

function fmtMs(value) {
  return value == null ? "-" : `${value.toFixed(2)}ms`;
}

function fmtFactor(value) {
  return value == null ? "-" : `${value.toFixed(2)}x`;
}

function markdownReport(report) {
  const lines = [
    "# Project Winner Regression Report",
    "",
    `Previous: ${report.previous.source_commit ?? "unknown"} (${report.previous.generated_at ?? "unknown"})`,
    `Current: ${report.current.source_commit ?? "unknown"} (${report.current.generated_at ?? "unknown"})`,
    "",
    "| Metric | Value |",
    "| --- | ---: |",
    `| Green project rows compared | ${report.totals.green_project_rows_compared} |`,
    `| tsz-to-tsgo regressions | ${report.totals.tsz_to_tsgo_regressions} |`,
    `| Duplicate project rows | ${report.totals.duplicate_project_rows} |`,
    "",
  ];

  if (report.duplicate_rows.length > 0) {
    lines.push("## Duplicate Project Rows", "");
    lines.push("| Artifact | Row | Count |");
    lines.push("| --- | --- | ---: |");
    for (const row of report.duplicate_rows) {
      lines.push(`| ${row.artifact} | \`${row.label}\` | ${row.count} |`);
    }
    lines.push("");
  }

  if (report.rows.length === 0) {
    lines.push("No green project rows moved from `tsz` winner to `tsgo` winner.");
    return `${lines.join("\n")}\n`;
  }

  lines.push("| Row | Owner | Previous | Current |");
  lines.push("| --- | --- | --- | --- |");
  for (const row of report.rows) {
    lines.push(
      `| \`${row.label}\` | ${row.owner ?? "-"} | ` +
        `tsz ${fmtFactor(row.previous.factor)} (${fmtMs(row.previous.tsz_ms)} vs ${fmtMs(row.previous.tsgo_ms)}) | ` +
        `tsgo ${fmtFactor(row.current.factor)} (${fmtMs(row.current.tsz_ms)} vs ${fmtMs(row.current.tsgo_ms)}) |`,
    );
  }
  return `${lines.join("\n")}\n`;
}

export function createProjectWinnerRegressionReport(previous, current, previousPath, currentPath) {
  return compareRegressions(previous, current, previousPath, currentPath);
}

export function writeProjectWinnerRegressionReport(previousPath, currentPath, outputPath, { jsonOnly = false } = {}) {
  const report = createProjectWinnerRegressionReport(
    readJson(previousPath),
    readJson(currentPath),
    previousPath,
    currentPath,
  );

  if (outputPath) {
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
  }

  if (jsonOnly) {
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  } else {
    process.stdout.write(markdownReport(report));
  }

  return report;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = process.argv.slice(2);
  const jsonOnly = args.includes("--json");
  const positional = args.filter((arg) => !arg.startsWith("-"));
  const [previousPath, currentPath, outputPath] = positional;

  if (!previousPath || !currentPath) {
    console.error("usage: project-winner-regression-report.mjs [--json] <previous.json> <current.json> [output.json]");
    process.exit(2);
  }

  try {
    const report = writeProjectWinnerRegressionReport(previousPath, currentPath, outputPath, { jsonOnly });
    process.exit(report.totals.tsz_to_tsgo_regressions > 0 || report.totals.duplicate_project_rows > 0 ? 1 : 0);
  } catch (err) {
    console.error(err instanceof Error ? err.message : String(err));
    process.exit(2);
  }
}
