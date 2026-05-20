#!/usr/bin/env node
/**
 * Prints a compact coverage table showing which surfaces each project row
 * is present in, and reports drift between surfaces.
 *
 * Exit 0: all coverage checks pass.
 * Exit 1: at least one drift issue was found.
 * Exit 2: usage error.
 *
 * Usage:
 *   node scripts/bench/project-row-summary.mjs [--markdown]
 *
 * Without --markdown, outputs plain text.  With --markdown, outputs GFM.
 * When $GITHUB_STEP_SUMMARY is set, the markdown table is also appended to
 * that file automatically (regardless of --markdown) so the coverage is
 * visible in the PR checks UI without a separate step.
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import {
  COMPILE_CANARY_PROJECT_ROWS,
  COMPATIBILITY_CORPUS_ROWS,
  PROJECT_ROW_DEFINITIONS,
  REQUIRED_PROJECT_ROWS,
} from "./project-rows.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");

// Rows intentionally excluded from certain surfaces.
export const BENCH_RUNNER_EXCLUDED_ROWS = new Set([
  "type-challenges-solutions-project",
]);
export const COMPILE_GUARD_EXCLUDED_ROWS = new Set([
  "large-ts-repo",
  "nextjs",
]);

function extractCaseArmRows(scriptText) {
  return [...scriptText.matchAll(/^\s{4}([a-z0-9-]+(?:\|[a-z0-9-]+)*)\)\s*$/gm)]
    .flatMap((m) => m[1].split("|"));
}

export function extractBenchRunnerRows(scriptText) {
  const rows = [...scriptText.matchAll(/run_project_benchmark\s+"([^"]+)"/g)]
    .map((m) => m[1]);
  return [...new Set(rows)].sort();
}

export function extractCompileGuardRows(scriptText) {
  const literalRows = [...scriptText.matchAll(/check_project\s+"([^"]+)"/g)]
    .map((m) => m[1])
    .filter((r) => r !== "$name");
  return [...new Set([...literalRows, ...extractCaseArmRows(scriptText)])].sort();
}

function extractShellArrayRows(scriptText, arrayName) {
  const escapedName = arrayName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = scriptText.match(new RegExp(`${escapedName}=\\(\\n([\\s\\S]*?)\\n\\)`, "m"));
  if (!match) return [];
  return [...match[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
}

export function extractCompileGuardFallbackRows(scriptText) {
  return [...new Set([
    ...extractShellArrayRows(scriptText, "TSZ_COMPILE_GUARD_REQUIRED_ROWS"),
    ...extractShellArrayRows(scriptText, "TSZ_COMPILE_GUARD_CANARY_ROWS"),
  ])].sort();
}

export function extractFixtureSourceRows(scriptText) {
  return [...new Set(extractCaseArmRows(scriptText))].sort();
}

export function readSurfaceData() {
  const read = (rel) => fs.readFileSync(path.join(ROOT, rel), "utf8");
  const fixtureScript = read("scripts/bench/project-fixtures.sh");
  return {
    benchRunnerRows: extractBenchRunnerRows(read("scripts/bench/bench-vs-tsgo.sh")),
    compileGuardRows: extractCompileGuardRows(read("scripts/ci/project-compile-guard.sh")),
    compileGuardFallbackRows: extractCompileGuardFallbackRows(fixtureScript),
    fixtureSourceRows: extractFixtureSourceRows(fixtureScript),
    compatCorpusRows: COMPATIBILITY_CORPUS_ROWS.map((r) => r.name).sort(),
    requiredRows: [...REQUIRED_PROJECT_ROWS].sort(),
    canaryRows: [...COMPILE_CANARY_PROJECT_ROWS].sort(),
    rowDefinitions: PROJECT_ROW_DEFINITIONS,
  };
}

export function computeCoverage(surfaces) {
  const {
    benchRunnerRows,
    compileGuardRows,
    compileGuardFallbackRows = [],
    fixtureSourceRows,
    compatCorpusRows,
    requiredRows,
    canaryRows,
    rowDefinitions,
  } = surfaces;

  const benchRunnerSet = new Set(benchRunnerRows);
  const compileGuardSet = new Set(compileGuardRows);
  const compileGuardFallbackSet = new Set(compileGuardFallbackRows);
  const fixtureSourceSet = new Set(fixtureSourceRows);
  const compatCorpusSet = new Set(compatCorpusRows);
  const requiredSet = new Set(requiredRows);
  const allTracked = new Set([...requiredRows, ...canaryRows]);
  const definedNames = new Set(rowDefinitions.map((r) => r.name));

  const drift = [];

  for (const def of rowDefinitions) {
    const name = def.name;
    const isTracked = allTracked.has(name);
    const isRequired = requiredSet.has(name);
    const hasPinnedSource = def.repo !== undefined || def.ref !== undefined;

    if (isTracked && !BENCH_RUNNER_EXCLUDED_ROWS.has(name) && !benchRunnerSet.has(name)) {
      drift.push(`${name}: present in project-rows.mjs but missing from scripts/bench/bench-vs-tsgo.sh`);
    }
    if (isTracked && !COMPILE_GUARD_EXCLUDED_ROWS.has(name) && !compileGuardSet.has(name)) {
      drift.push(`${name}: present in project-rows.mjs but missing from scripts/ci/project-compile-guard.sh`);
    }
    if (isTracked && !COMPILE_GUARD_EXCLUDED_ROWS.has(name) && !compileGuardFallbackSet.has(name)) {
      drift.push(`${name}: present in project-rows.mjs but missing from project-fixtures.sh compile-guard fallback rows`);
    }
    if (hasPinnedSource && !fixtureSourceSet.has(name)) {
      drift.push(`${name}: has repo/ref pin in project-rows.mjs but missing from scripts/bench/project-fixtures.sh`);
    }
    if (isTracked && !compatCorpusSet.has(name)) {
      const setLabel = isRequired ? "required" : "canary";
      drift.push(`${name}: present in project-rows.mjs (${setLabel}) but missing from COMPATIBILITY_CORPUS_ROWS`);
    }
  }

  for (const name of benchRunnerSet) {
    if (!definedNames.has(name)) {
      drift.push(`${name}: present in scripts/bench/bench-vs-tsgo.sh but not defined in project-rows.mjs`);
    }
  }
  for (const name of compileGuardSet) {
    if (!definedNames.has(name)) {
      drift.push(`${name}: present in scripts/ci/project-compile-guard.sh but not defined in project-rows.mjs`);
    }
  }
  for (const name of compileGuardFallbackSet) {
    if (!definedNames.has(name)) {
      drift.push(`${name}: present in project-fixtures.sh compile-guard fallback rows but not defined in project-rows.mjs`);
    }
  }
  for (const name of fixtureSourceSet) {
    if (!definedNames.has(name)) {
      drift.push(`${name}: present in scripts/bench/project-fixtures.sh but not defined in project-rows.mjs`);
    }
  }
  for (const name of compatCorpusSet) {
    if (!definedNames.has(name)) {
      drift.push(`${name}: present in COMPATIBILITY_CORPUS_ROWS but not defined in project-rows.mjs`);
    }
  }

  const rows = rowDefinitions.map((def) => {
    const name = def.name;
    const hasPinnedSource = def.repo !== undefined || def.ref !== undefined;
    return {
      name,
      benchmarkSet: def.benchmark_set ?? "—",
      guardSet: def.guard_set ?? "—",
      category: def.category ?? "—",
      inBenchRunner: benchRunnerSet.has(name)
        ? "✓"
        : (BENCH_RUNNER_EXCLUDED_ROWS.has(name) ? "—" : "✗"),
      inCompileGuard: compileGuardSet.has(name)
        ? "✓"
        : (COMPILE_GUARD_EXCLUDED_ROWS.has(name) ? "—" : "✗"),
      inCompileGuardFallback: compileGuardFallbackSet.has(name)
        ? "✓"
        : (COMPILE_GUARD_EXCLUDED_ROWS.has(name) ? "—" : "✗"),
      inFixtureSource: fixtureSourceSet.has(name)
        ? "✓"
        : (hasPinnedSource ? "✗" : "—"),
      inCompatCorpus: compatCorpusSet.has(name) ? "✓" : "✗",
    };
  });

  return { rows, drift };
}

export function formatMarkdown(coverage) {
  const { rows, drift } = coverage;
  const lines = [];

  lines.push("## Project Row Coverage");
  lines.push("");
  lines.push("| Row | Bench Set | Guard Set | Category | Bench Runner | Compile Guard | Guard Fallback | Fixture Source | Compat Corpus |");
  lines.push("|-----|-----------|-----------|----------|:------------:|:-------------:|:--------------:|:--------------:|:-------------:|");

  for (const r of rows) {
    lines.push(
      `| \`${r.name}\` | ${r.benchmarkSet} | ${r.guardSet} | ${r.category} | ${r.inBenchRunner} | ${r.inCompileGuard} | ${r.inCompileGuardFallback} | ${r.inFixtureSource} | ${r.inCompatCorpus} |`,
    );
  }

  lines.push("");

  if (drift.length === 0) {
    lines.push(`> ✅ All ${rows.length} rows consistent across surfaces.`);
  } else {
    lines.push(`> ❌ **${drift.length} drift issue${drift.length === 1 ? "" : "s"} detected.**`);
    lines.push("");
    lines.push("### Drift Issues");
    lines.push("");
    for (const issue of drift) {
      lines.push(`- ${issue}`);
    }
  }

  return lines.join("\n");
}

export function formatPlainText(coverage) {
  const { rows, drift } = coverage;
  const lines = [];

  const COL = { name: 44, benchSet: 9, guardSet: 9, category: 9, runner: 6, compileGuard: 6, fallback: 8, fixture: 7 };
  const header = [
    "Row".padEnd(COL.name),
    "Bench".padEnd(COL.benchSet),
    "Guard".padEnd(COL.guardSet),
    "Category".padEnd(COL.category),
    "Runner".padEnd(COL.runner),
    "Guard".padEnd(COL.compileGuard),
    "Fallback".padEnd(COL.fallback),
    "Fixture".padEnd(COL.fixture),
    "Compat",
  ].join("  ");
  const separator = "-".repeat(header.length);

  lines.push("Project Row Coverage");
  lines.push(separator);
  lines.push(header);
  lines.push(separator);

  for (const r of rows) {
    lines.push(
      [
        r.name.padEnd(COL.name),
        r.benchmarkSet.padEnd(COL.benchSet),
        r.guardSet.padEnd(COL.guardSet),
        r.category.padEnd(COL.category),
        r.inBenchRunner.padEnd(COL.runner),
        r.inCompileGuard.padEnd(COL.compileGuard),
        r.inCompileGuardFallback.padEnd(COL.fallback),
        r.inFixtureSource.padEnd(COL.fixture),
        r.inCompatCorpus,
      ].join("  "),
    );
  }

  lines.push(separator);

  if (drift.length === 0) {
    lines.push(`All ${rows.length} rows consistent across surfaces.`);
  } else {
    lines.push(`${drift.length} drift issue${drift.length === 1 ? "" : "s"} detected:`);
    for (const issue of drift) {
      lines.push(`  - ${issue}`);
    }
  }

  return lines.join("\n");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = process.argv.slice(2);
  const markdown = args.includes("--markdown");
  const unknownArgs = args.filter((a) => a !== "--markdown");
  if (unknownArgs.length > 0) {
    process.stderr.write(`Unknown arguments: ${unknownArgs.join(", ")}\n`);
    process.stderr.write("Usage: project-row-summary.mjs [--markdown]\n");
    process.exit(2);
  }

  const surfaces = readSurfaceData();
  const coverage = computeCoverage(surfaces);
  const output = markdown ? formatMarkdown(coverage) : formatPlainText(coverage);
  process.stdout.write(`${output}\n`);

  // When running in GitHub Actions, also append the markdown table to the
  // step summary so it's visible in the PR checks UI without a second run.
  const stepSummary = process.env.GITHUB_STEP_SUMMARY;
  if (!markdown && stepSummary) {
    fs.appendFileSync(stepSummary, `${formatMarkdown(coverage)}\n`);
  }

  if (coverage.drift.length > 0) {
    process.exit(1);
  }
}
