#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { PROJECT_ROWS_BY_NAME } from "./project-rows.mjs";
import { isGreen } from "./row-utils.mjs";

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function asNumber(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function ratio(candidate, baseline) {
  if (candidate == null || baseline == null || baseline === 0) return null;
  return candidate / baseline;
}

function sourceInfo(artifact, file) {
  return {
    path: file,
    source_commit: artifact?.source_commit ?? null,
    generated_at: artifact?.generated_at ?? null,
    workflow_run_url: artifact?.workflow_run_url ?? null,
  };
}

function runnerSignature(environment) {
  return {
    platform: environment?.platform ?? null,
    arch: environment?.arch ?? null,
    release: environment?.release ?? null,
    cpu_count: environment?.cpu_count ?? null,
    cpu_model: environment?.cpu_model ?? null,
    total_memory_bytes: environment?.total_memory_bytes ?? null,
    github_runner_os: environment?.github_actions?.runner_os ?? null,
    github_runner_arch: environment?.github_actions?.runner_arch ?? null,
    cloud_build_machine_type: environment?.cloud_build?.machine_type ?? null,
  };
}

function rowsByName(artifact) {
  const rows = new Map();
  for (const row of artifact?.results ?? []) {
    if (row?.name) rows.set(row.name, row);
  }
  return rows;
}

function rowFamily(name, row) {
  if (name === "large-ts-repo") return "large-ts-repo";
  if (PROJECT_ROWS_BY_NAME[name]) return "project";
  if (row?.compatibility) return "project";
  if (/\.tsx?$/.test(name) || /\//.test(name)) return "compiler-file";
  if (/Recursive generic|Conditional dist|Mapped type|Template literal|Deep subtype|Intersection|Infer stress|CFA branches|BCT|Constraint conflicts/i.test(name)) {
    return "solver-stress";
  }
  if (/classes|generic functions|optional-chain|union members/i.test(name)) return "synthetic";
  return "other";
}

function rowTiming(row) {
  return {
    winner: row?.winner ?? null,
    factor: asNumber(row?.factor ?? row?.ratio),
    tsz_ms: asNumber(row?.tsz_ms),
    tsgo_ms: asNumber(row?.tsgo_ms),
    status: row?.status ?? null,
    compatibility_state: row?.compatibility?.state ?? null,
  };
}

function compareRow(name, baselineRow, candidateRow) {
  const baseline = rowTiming(baselineRow);
  const candidate = rowTiming(candidateRow);
  return {
    name,
    family: rowFamily(name, candidateRow),
    baseline,
    candidate,
    ratios: {
      tsz_ms: ratio(candidate.tsz_ms, baseline.tsz_ms),
      tsgo_ms: ratio(candidate.tsgo_ms, baseline.tsgo_ms),
    },
  };
}

function summarizeFamilies(rows) {
  const byFamily = new Map();
  for (const row of rows) {
    const summary = byFamily.get(row.family) ?? {
      family: row.family,
      rows: 0,
      median_tsz_ratio: null,
      median_tsgo_ratio: null,
    };
    summary.rows += 1;
    byFamily.set(row.family, summary);
  }

  for (const summary of byFamily.values()) {
    const familyRows = rows.filter((row) => row.family === summary.family);
    summary.median_tsz_ratio = median(familyRows.map((row) => row.ratios.tsz_ms));
    summary.median_tsgo_ratio = median(familyRows.map((row) => row.ratios.tsgo_ms));
  }

  return [...byFamily.values()].sort((a, b) => a.family.localeCompare(b.family));
}

function median(values) {
  const sorted = values.filter((value) => value != null).sort((a, b) => a - b);
  if (sorted.length === 0) return null;
  const mid = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 1) return sorted[mid];
  return (sorted[mid - 1] + sorted[mid]) / 2;
}

function collectWarnings(baseline, candidate, rows) {
  const warnings = [];
  if (!baseline?.source_commit || !candidate?.source_commit) {
    warnings.push("source_commit missing from one or both artifacts");
  } else if (baseline.source_commit !== candidate.source_commit) {
    warnings.push(`source_commit mismatch: ${baseline.source_commit} != ${candidate.source_commit}`);
  }
  if (!baseline?.runner_environment) warnings.push("baseline runner_environment missing");
  if (!candidate?.runner_environment) warnings.push("candidate runner_environment missing");
  if (rows.length === 0) warnings.push("no shared green rows with timing data");
  return warnings;
}

export function createRunnerCalibrationReport(baseline, candidate, baselinePath, candidatePath) {
  const baselineRows = rowsByName(baseline);
  const candidateRows = rowsByName(candidate);
  const rows = [];
  let sharedRows = 0;
  let excludedNonGreen = 0;
  let excludedMissingTiming = 0;

  for (const [name, baselineRow] of baselineRows) {
    const candidateRow = candidateRows.get(name);
    if (!candidateRow) continue;
    sharedRows += 1;
    if (!isGreen(baselineRow) || !isGreen(candidateRow)) {
      excludedNonGreen += 1;
      continue;
    }
    const row = compareRow(name, baselineRow, candidateRow);
    if (row.ratios.tsz_ms == null && row.ratios.tsgo_ms == null) {
      excludedMissingTiming += 1;
      continue;
    }
    rows.push(row);
  }

  rows.sort((a, b) => {
    const family = a.family.localeCompare(b.family);
    if (family !== 0) return family;
    return a.name.localeCompare(b.name);
  });

  const report = {
    schema_version: 1,
    generated_at: new Date().toISOString(),
    baseline: {
      ...sourceInfo(baseline, baselinePath),
      runner_environment: baseline?.runner_environment ?? null,
      runner_signature: runnerSignature(baseline?.runner_environment),
    },
    candidate: {
      ...sourceInfo(candidate, candidatePath),
      runner_environment: candidate?.runner_environment ?? null,
      runner_signature: runnerSignature(candidate?.runner_environment),
    },
    totals: {
      baseline_rows: baselineRows.size,
      candidate_rows: candidateRows.size,
      shared_rows: sharedRows,
      green_rows_compared: rows.length,
      excluded_non_green_rows: excludedNonGreen,
      excluded_missing_timing_rows: excludedMissingTiming,
    },
    family_summary: summarizeFamilies(rows),
    rows,
  };
  report.warnings = collectWarnings(baseline, candidate, rows);
  report.calibration_ready = report.warnings.length === 0;
  return report;
}

function fmtRatio(value) {
  return value == null ? "-" : `${value.toFixed(3)}x`;
}

function fmtMs(value) {
  return value == null ? "-" : `${value.toFixed(2)}ms`;
}

export function markdownReport(report) {
  const lines = [
    "# Benchmark Runner Calibration Report",
    "",
    `Baseline: ${report.baseline.source_commit ?? "unknown"} (${report.baseline.generated_at ?? "unknown"})`,
    `Candidate: ${report.candidate.source_commit ?? "unknown"} (${report.candidate.generated_at ?? "unknown"})`,
    "",
    "| Metric | Value |",
    "| --- | ---: |",
    `| Shared rows | ${report.totals.shared_rows} |`,
    `| Green rows compared | ${report.totals.green_rows_compared} |`,
    `| Excluded non-green rows | ${report.totals.excluded_non_green_rows} |`,
    `| Excluded missing timing rows | ${report.totals.excluded_missing_timing_rows} |`,
    "",
  ];

  if (report.warnings.length > 0) {
    lines.push("## Warnings", "");
    for (const warning of report.warnings) lines.push(`- ${warning}`);
    lines.push("");
  }

  lines.push("## Family Summary", "");
  if (report.family_summary.length === 0) {
    lines.push("No comparable green timing rows.");
  } else {
    lines.push("| Family | Rows | Median tsz ratio | Median tsgo ratio |");
    lines.push("| --- | ---: | ---: | ---: |");
    for (const family of report.family_summary) {
      lines.push(
        `| ${family.family} | ${family.rows} | ${fmtRatio(family.median_tsz_ratio)} | ${fmtRatio(family.median_tsgo_ratio)} |`,
      );
    }
  }
  lines.push("");

  if (report.rows.length > 0) {
    lines.push("## Rows", "");
    lines.push("| Row | Family | tsz baseline -> candidate | tsgo baseline -> candidate |");
    lines.push("| --- | --- | ---: | ---: |");
    for (const row of report.rows) {
      lines.push(
        `| \`${row.name}\` | ${row.family} | ${fmtMs(row.baseline.tsz_ms)} -> ${fmtMs(row.candidate.tsz_ms)} (${fmtRatio(row.ratios.tsz_ms)}) | ` +
          `${fmtMs(row.baseline.tsgo_ms)} -> ${fmtMs(row.candidate.tsgo_ms)} (${fmtRatio(row.ratios.tsgo_ms)}) |`,
      );
    }
    lines.push("");
  }

  return `${lines.join("\n")}\n`;
}

export function writeRunnerCalibrationReport(baselinePath, candidatePath, outputPath, { jsonOnly = false } = {}) {
  const report = createRunnerCalibrationReport(
    readJson(baselinePath),
    readJson(candidatePath),
    baselinePath,
    candidatePath,
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
  const [baselinePath, candidatePath, outputPath] = positional;

  if (!baselinePath || !candidatePath) {
    console.error("usage: calibrate-runner-series.mjs [--json] <baseline.json> <candidate.json> [output.json]");
    process.exit(2);
  }

  try {
    const report = writeRunnerCalibrationReport(baselinePath, candidatePath, outputPath, { jsonOnly });
    process.exit(report.calibration_ready ? 0 : 1);
  } catch (err) {
    console.error(err instanceof Error ? err.message : String(err));
    process.exit(2);
  }
}
