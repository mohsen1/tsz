#!/usr/bin/env node
import assert from "node:assert/strict";
import {
  BENCH_RUNNER_EXCLUDED_ROWS,
  COMPILE_GUARD_EXCLUDED_ROWS,
  computeCoverage,
  extractBenchRunnerRows,
  extractCompileGuardRows,
  extractFixtureSourceRows,
  formatMarkdown,
  formatPlainText,
} from "./project-row-summary.mjs";
import {
  COMPILE_CANARY_PROJECT_ROWS,
  COMPATIBILITY_CORPUS_ROWS,
  PROJECT_ROW_DEFINITIONS,
  REQUIRED_PROJECT_ROWS,
} from "./project-rows.mjs";

// Baseline surface data built from the live project-rows.mjs exports.
function baseSurfaces() {
  // allTracked = benchmark_set:required rows + guard_set:canary rows (same as test-project-rows.mjs).
  const allTracked = new Set([...REQUIRED_PROJECT_ROWS, ...COMPILE_CANARY_PROJECT_ROWS]);
  return {
    benchRunnerRows: [...allTracked]
      .filter((name) => !BENCH_RUNNER_EXCLUDED_ROWS.has(name))
      .sort(),
    compileGuardRows: [...allTracked]
      .filter((name) => !COMPILE_GUARD_EXCLUDED_ROWS.has(name))
      .sort(),
    fixtureSourceRows: PROJECT_ROW_DEFINITIONS
      .filter((r) => r.repo !== undefined || r.ref !== undefined)
      .map((r) => r.name)
      .sort(),
    compatCorpusRows: COMPATIBILITY_CORPUS_ROWS.map((r) => r.name).sort(),
    requiredRows: [...REQUIRED_PROJECT_ROWS].sort(),
    canaryRows: [...COMPILE_CANARY_PROJECT_ROWS].sort(),
    rowDefinitions: PROJECT_ROW_DEFINITIONS,
  };
}

// Clean state: no drift
{
  const coverage = computeCoverage(baseSurfaces());
  assert.equal(coverage.drift.length, 0, `Expected no drift, got: ${coverage.drift.join("; ")}`);
  assert.equal(coverage.rows.length, PROJECT_ROW_DEFINITIONS.length);
  // Every row in the table has the four surface fields set to ✓, ✗, or —.
  const validSymbols = new Set(["✓", "✗", "—"]);
  for (const row of coverage.rows) {
    assert.ok(validSymbols.has(row.inBenchRunner), `${row.name} inBenchRunner unexpected: ${row.inBenchRunner}`);
    assert.ok(validSymbols.has(row.inCompileGuard), `${row.name} inCompileGuard unexpected: ${row.inCompileGuard}`);
    assert.ok(validSymbols.has(row.inFixtureSource), `${row.name} inFixtureSource unexpected: ${row.inFixtureSource}`);
    assert.ok(validSymbols.has(row.inCompatCorpus), `${row.name} inCompatCorpus unexpected: ${row.inCompatCorpus}`);
  }
}

// Row added to project-rows.mjs (required) but missing from bench runner.
{
  const surfaces = baseSurfaces();
  surfaces.rowDefinitions = [
    ...PROJECT_ROW_DEFINITIONS,
    { name: "new-required-row", benchmark_set: "required", guard_set: "required", category: "external" },
  ];
  surfaces.requiredRows = [...surfaces.requiredRows, "new-required-row"].sort();
  const coverage = computeCoverage(surfaces);
  assert.ok(
    coverage.drift.some((d) => d.includes("new-required-row") && d.includes("bench-vs-tsgo.sh")),
    `Expected bench runner drift for new-required-row, got: ${coverage.drift.join("; ")}`,
  );
  assert.ok(
    coverage.drift.some((d) => d.includes("new-required-row") && d.includes("project-compile-guard.sh")),
    `Expected compile guard drift for new-required-row, got: ${coverage.drift.join("; ")}`,
  );
  assert.ok(
    coverage.drift.some((d) => d.includes("new-required-row") && d.includes("COMPATIBILITY_CORPUS_ROWS")),
    `Expected compat corpus drift for new-required-row, got: ${coverage.drift.join("; ")}`,
  );
}

// Row added to project-rows.mjs (canary, different name) but missing from bench runner.
{
  const surfaces = baseSurfaces();
  surfaces.rowDefinitions = [
    ...PROJECT_ROW_DEFINITIONS,
    { name: "new-canary-row", benchmark_set: "canary", guard_set: "canary", category: "external" },
  ];
  surfaces.canaryRows = [...surfaces.canaryRows, "new-canary-row"].sort();
  const coverage = computeCoverage(surfaces);
  assert.ok(
    coverage.drift.some((d) => d.includes("new-canary-row") && d.includes("bench-vs-tsgo.sh")),
    `Expected bench runner drift for new-canary-row`,
  );
}

// Row in bench runner that is not defined in project-rows.mjs.
{
  const surfaces = baseSurfaces();
  surfaces.benchRunnerRows = [...surfaces.benchRunnerRows, "phantom-row"].sort();
  const coverage = computeCoverage(surfaces);
  assert.ok(
    coverage.drift.some((d) => d.includes("phantom-row") && d.includes("bench-vs-tsgo.sh") && d.includes("not defined")),
    `Expected bench runner orphan drift for phantom-row, got: ${coverage.drift.join("; ")}`,
  );
}

// Row in compile guard that is not defined in project-rows.mjs.
{
  const surfaces = baseSurfaces();
  surfaces.compileGuardRows = [...surfaces.compileGuardRows, "ghost-row"].sort();
  const coverage = computeCoverage(surfaces);
  assert.ok(
    coverage.drift.some((d) => d.includes("ghost-row") && d.includes("project-compile-guard.sh") && d.includes("not defined")),
    `Expected compile guard orphan drift for ghost-row, got: ${coverage.drift.join("; ")}`,
  );
}

// Row has pinned repo/ref but missing from fixture source.
{
  const surfaces = baseSurfaces();
  surfaces.rowDefinitions = [
    ...PROJECT_ROW_DEFINITIONS,
    {
      name: "pinned-no-fixture",
      benchmark_set: "required",
      guard_set: "required",
      category: "external",
      repo: "https://github.com/example/repo.git",
      ref: "abc123",
    },
  ];
  surfaces.requiredRows = [...surfaces.requiredRows, "pinned-no-fixture"].sort();
  const coverage = computeCoverage(surfaces);
  assert.ok(
    coverage.drift.some((d) => d.includes("pinned-no-fixture") && d.includes("project-fixtures.sh")),
    `Expected fixture source drift for pinned-no-fixture, got: ${coverage.drift.join("; ")}`,
  );
}

// BENCH_RUNNER_EXCLUDED_ROWS rows are not flagged as missing from bench runner.
{
  const surfaces = baseSurfaces();
  // Remove the excluded row from bench runner to simulate it being absent.
  const excluded = [...BENCH_RUNNER_EXCLUDED_ROWS][0];
  surfaces.benchRunnerRows = surfaces.benchRunnerRows.filter((r) => r !== excluded);
  const coverage = computeCoverage(surfaces);
  assert.ok(
    !coverage.drift.some((d) => d.includes(excluded) && d.includes("bench-vs-tsgo.sh")),
    `BENCH_RUNNER_EXCLUDED_ROWS row ${excluded} should not trigger bench runner drift`,
  );
}

// COMPILE_GUARD_EXCLUDED_ROWS rows are not flagged as missing from compile guard.
{
  const surfaces = baseSurfaces();
  const excluded = [...COMPILE_GUARD_EXCLUDED_ROWS][0];
  surfaces.compileGuardRows = surfaces.compileGuardRows.filter((r) => r !== excluded);
  const coverage = computeCoverage(surfaces);
  assert.ok(
    !coverage.drift.some((d) => d.includes(excluded) && d.includes("project-compile-guard.sh")),
    `COMPILE_GUARD_EXCLUDED_ROWS row ${excluded} should not trigger compile guard drift`,
  );
}

// Markdown format includes the table header and summary line.
{
  const coverage = computeCoverage(baseSurfaces());
  const md = formatMarkdown(coverage);
  const cleanSummary = `All ${PROJECT_ROW_DEFINITIONS.length} rows consistent`;
  assert.ok(md.includes("## Project Row Coverage"), "markdown missing heading");
  assert.ok(md.includes("| Row |"), "markdown missing table header");
  assert.ok(md.includes(cleanSummary), "markdown missing clean summary");
  assert.ok(md.includes("✅"), "markdown missing green check");
}

// Markdown format includes drift section when drift is present.
{
  const surfaces = baseSurfaces();
  surfaces.benchRunnerRows = surfaces.benchRunnerRows.filter((r) => r !== REQUIRED_PROJECT_ROWS[0]);
  const coverage = computeCoverage(surfaces);
  const md = formatMarkdown(coverage);
  assert.ok(md.includes("❌"), "markdown missing red cross for drift");
  assert.ok(md.includes("### Drift Issues"), "markdown missing drift section header");
  assert.ok(md.includes(REQUIRED_PROJECT_ROWS[0]), "markdown missing drifted row name");
}

// Plain text format includes the table and clean summary.
{
  const coverage = computeCoverage(baseSurfaces());
  const text = formatPlainText(coverage);
  const cleanSummary = `All ${PROJECT_ROW_DEFINITIONS.length} rows consistent`;
  assert.ok(text.includes("Project Row Coverage"), "plain text missing heading");
  assert.ok(text.includes(cleanSummary), "plain text missing clean summary");
}

// Extractor: bench runner rows from shell snippet.
{
  const snippet = `
run_project_benchmark "alpha" "$tsconfig" "$src"
run_project_benchmark "beta" "$tsconfig" "$src"
run_project_benchmark "alpha" "$tsconfig" "$src"
`;
  const rows = extractBenchRunnerRows(snippet);
  assert.deepEqual(rows, ["alpha", "beta"]);
}

// Extractor: compile guard rows from shell snippet — both literal and case-arm forms.
{
  const snippet = `
check_project "alpha" "$tsconfig" "$src"
check_project "beta" "$tsconfig" "$src"
    gamma|delta)
check_project "$name" "$tsconfig" "$src"
`;
  const rows = extractCompileGuardRows(snippet);
  assert.ok(rows.includes("alpha"), "missing alpha");
  assert.ok(rows.includes("beta"), "missing beta");
  assert.ok(rows.includes("gamma"), "missing gamma from case arm");
  assert.ok(rows.includes("delta"), "missing delta from case arm");
  assert.ok(!rows.includes("$name"), "$name must be filtered out");
}

// Extractor: fixture source rows from project-fixtures.sh case-arm format.
{
  const snippet = `
    alpha|beta)
      setup_alpha
    ;;
    gamma)
      setup_gamma
    ;;
`;
  const rows = extractFixtureSourceRows(snippet);
  assert.deepEqual(rows, ["alpha", "beta", "gamma"]);
}

console.log("test-project-row-summary: all tests passed");
