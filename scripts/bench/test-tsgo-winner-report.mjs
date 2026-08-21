#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { GREEN_COMPAT } from "./row-utils.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(ROOT, "scripts", "bench", "tsgo-winner-report.mjs");
const BENCH_WORKFLOW = path.join(ROOT, ".github", "workflows", "bench.yml");
const GH_PAGES_WORKFLOW = path.join(ROOT, ".github", "workflows", "gh-pages.yml");
const WEBSITE_ELEVENTY = path.join(ROOT, "crates", "tsz-website", ".eleventy.js");
const WEBSITE_BENCH_SNAPSHOT = path.join(ROOT, "crates", "tsz-website", "bench-snapshot.json");

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-tsgo-winner-report-"));
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function writeJson(file, value) {
  const serialized = Array.isArray(value?.results)
    ? {
        ...value,
        results: value.results.map((row) => row?.compatibility?.state === "green"
          ? { ...row, compatibility: greenCompatibility(row.compatibility) }
          : row),
      }
    : value;
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(serialized, null, 2)}\n`);
}

function greenCompatibility(overrides = {}) {
  const {
    state: _state,
    phase: _phase,
    last_successful_phase: _lastSuccessfulPhase,
    exit_class: _exitClass,
    diagnostic_status: _diagnosticStatus,
    ...evidence
  } = GREEN_COMPAT;
  const sourceFiles = overrides.files_reached ?? GREEN_COMPAT.source_files;
  return {
    ...evidence,
    source_files: sourceFiles,
    oracle_source_files: sourceFiles,
    files_reached: sourceFiles,
    ...overrides,
  };
}

const { createTsgoWinnerReport, renderMissingAttributionPlanMarkdown } = await import(
  pathToFileURL(SCRIPT)
);

{
  const report = createTsgoWinnerReport(
    JSON.parse(fs.readFileSync(WEBSITE_BENCH_SNAPSHOT, "utf8")),
    WEBSITE_BENCH_SNAPSHOT,
  );
  assert.equal(report.two_x_target.project_eligible_green_rows, 0);
  assert.equal(
    report.two_x_target.rows_below_target,
    7,
    "legacy website project rows without schema-v2 proof are not speed evidence",
  );
}

withTempDir((dir) => {
  const input = path.join(dir, "bench.json");
  const output = path.join(dir, "report.json");
  const attributionPlan = path.join(dir, "missing-attribution.md");
  writeJson(input, {
    benchmark_runner: "scripts/bench/bench-vs-tsgo.sh",
    quick_mode: true,
    filter: "project|single",
    measurement_profile: {
      mode: "release-pgo",
      tsz_binary_source: "bench-dist",
      rust_target_cpu: "x86-64-v3",
      generated_at: "2026-05-20T00:00:00.000Z",
      profile_guided_optimization: {
        requested: true,
        required: true,
        optimized: true,
        profile_fingerprint: "profile-abc123",
        training_fingerprint: "training-def456",
        training_input_count: 12,
        training_failure_count: 0,
      },
    },
    results: [
      {
        name: "type-fest-project",
        lines: 8044,
        kb: 216,
        project_files: 242,
        tsz_ms: null,
        tsgo_ms: null,
        winner: "error",
        factor: 0,
        status: "tsz slowdown (9.21x slower than tsgo; threshold 8x)",
        compatibility: {
          state: "red",
          exit_class: "slowdown",
          phase: "timing",
          last_successful_phase: null,
          diagnostic_status: "runtime slowdown",
          files_reached: 242,
          peak_memory_bytes: 734003200,
          semantic_owner_family: "mapped/conditional/key-space utility surface",
        },
      },
      {
        name: "ts-toolbelt-project",
        lines: 8044,
        kb: 216,
        project_files: 242,
        tsz_ms: 873.92,
        tsgo_ms: 106.15,
        winner: "tsgo",
        factor: 8.23,
        status: null,
        compatibility: {
          state: "green",
          exit_class: "exit success",
          phase: "check",
          last_successful_phase: "check",
          diagnostic_status: "none",
          files_reached: 242,
          peak_memory_bytes: 734003200,
          semantic_owner_family: "recursive type evaluation pressure",
        },
        attribution_artifact: {
          path: "artifacts/perf/ts-toolbelt-project-attribution.json",
          generated_at: "2026-05-20T00:05:00.000Z",
          mode: "attribution",
          dominant_subsystem: "solver:recursive-evaluation",
        },
      },
      {
        name: "vite-vanilla-ts-app",
        lines: 100,
        kb: 20,
        project_files: 12,
        tsz_ms: 165.15,
        tsgo_ms: 54.51,
        winner: "tsgo",
        factor: 3.03,
        status: null,
        compatibility: {
          state: "green",
          exit_class: "exit success",
          phase: "check",
          last_successful_phase: "check",
          diagnostic_status: "none",
          files_reached: 12,
          peak_memory_bytes: 209715200,
          semantic_owner_family: "generated Vite dependency graph",
        },
      },
      {
        name: "single-file-loss",
        lines: 50,
        kb: 2,
        tsz_ms: 20,
        tsgo_ms: 10,
        winner: "tsgo",
        factor: 2,
        status: null,
      },
      {
        name: "tsz-wins",
        tsz_ms: 5,
        tsgo_ms: 10,
        winner: "tsz",
        factor: 2,
      },
      {
        name: "red-project",
        tsz_ms: null,
        tsgo_ms: 10,
        winner: "error",
        factor: 0,
        status: "tsz error",
        compatibility: {
          exit_class: "nonzero exit",
          diagnostic_status: "compiler error",
          semantic_owner_family: "not counted",
        },
      },
      {
        name: "yellow-project",
        tsz_ms: 40,
        tsgo_ms: 20,
        winner: "tsgo",
        factor: 2,
        status: null,
        compatibility: {
          state: "yellow",
          exit_class: "exit success",
          phase: "check",
          last_successful_phase: "check",
          diagnostic_status: "diagnostic mismatch",
          semantic_owner_family: "not counted",
        },
      },
    ],
  });

  const result = spawnSync(process.execPath, [SCRIPT, input, output, attributionPlan], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.match(result.stdout, /missing attribution plan:/);

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.equal(report.source.quick_mode, true);
  assert.equal(report.totals.rows, 7);
  assert.equal(report.totals.duplicate_project_rows, 0);
  assert.equal(report.totals.green_tsgo_winners, 3);
  assert.equal(report.totals.project_green_tsgo_winners, 2);
  assert.equal(report.totals.green_tsgo_winners_with_closure, 2);
  assert.deepEqual(report.totals.missing_loss_closure_rows, ["single-file-loss"]);
  assert.equal(report.totals.green_tsgo_winners_with_attribution, 1);
  assert.deepEqual(report.totals.missing_attribution_rows, ["single-file-loss", "vite-vanilla-ts-app"]);
  assert.equal(report.totals.incomplete_compat_excluded, 0);
  assert.match(result.stdout, /2x target gaps with attribution commands: 2\/3/);
  assert.match(result.stdout, /project-row aggregate speedup: 0\.15x \(below 2x target; 2 row\(s\)\)/);
  assert.deepEqual(report.two_x_target, {
    tsz_speedup_target: 2,
    eligible_green_rows: 4,
    project_eligible_green_rows: 2,
    rows_meeting_target: 1,
    rows_below_target: 3,
    project_rows_below_target: 2,
    project_rows_aggregate: {
      eligible_green_rows: 2,
      measured_rows: 2,
      tsz_ms_total: 873.92 + 165.15,
      tsgo_ms_total: 106.15 + 54.51,
      tsz_speedup_vs_tsgo: (106.15 + 54.51) / (873.92 + 165.15),
      target_speedup: 2,
      target_met: false,
    },
    rows_with_attribution: 1,
    missing_attribution_rows: ["single-file-loss", "vite-vanilla-ts-app"],
    rows_with_attribution_command: 2,
    attribution_attempts: {},
    missing_attribution_plan: [
      {
        name: "vite-vanilla-ts-app",
        target_gap_factor: report.target_gaps[1].target_gap_factor,
        tsz_speedup_vs_tsgo: report.target_gaps[1].tsz_speedup_vs_tsgo,
        semantic_owner_family: "generated Vite dependency graph",
        owner: "Track 7/9 generated app lib/module identity",
        issue: 7378,
        url: "https://github.com/tsz-org/tsz/issues/7378",
        attribution_command: report.target_gaps[1].loss_closure.attribution_command,
        timing_command: report.target_gaps[1].loss_closure.command,
        attribution_warning: "attribution artifact missing",
        attribution_attempt_status: null,
        attribution_attempt_reason: null,
        attribution_attempt_exit_code: null,
        attribution_attempt_signal: null,
      },
      {
        name: "single-file-loss",
        target_gap_factor: report.target_gaps[2].target_gap_factor,
        tsz_speedup_vs_tsgo: report.target_gaps[2].tsz_speedup_vs_tsgo,
        semantic_owner_family: null,
        owner: null,
        issue: null,
        url: null,
        attribution_command: null,
        timing_command: null,
        attribution_warning: "attribution artifact missing",
        attribution_attempt_status: null,
        attribution_attempt_reason: null,
        attribution_attempt_exit_code: null,
        attribution_attempt_signal: null,
      },
    ],
    worst_gap: report.target_gaps[0],
  });
  assert.deepEqual(
    report.target_gaps.map((row) => row.name),
    ["ts-toolbelt-project", "vite-vanilla-ts-app", "single-file-loss"],
  );
  const planMarkdown = fs.readFileSync(attributionPlan, "utf8");
  assert.match(planMarkdown, /^# 2x Target Gap Attribution Plan/m);
  assert.match(planMarkdown, /Rows below 2x target \| 3/);
  assert.match(planMarkdown, /## 1\. vite-vanilla-ts-app/);
  assert.match(planMarkdown, /## 2\. single-file-loss/);
  assert.match(planMarkdown, /Attribution command:/);
  assert.match(planMarkdown, /vite-vanilla-ts-app\.perf\.json/);
  assert.match(planMarkdown, /Attribution command: n\/a/);
  assert.equal(report.target_gaps[0].semantic_owner_family, "recursive type evaluation pressure");
  assert.equal(report.target_gaps[0].tsz_speedup_vs_tsgo, 106.15 / 873.92);
  assert.equal(report.target_gaps[0].target_gap_factor, 2 / (106.15 / 873.92));
  assert.deepEqual(report.measurement_profile, {
    present: true,
    mode: "release-pgo",
    tsz_binary_source: "bench-dist",
    pgo_requested: true,
    pgo_required: true,
    pgo_optimized: true,
    profile_fingerprint: "profile-abc123",
    training_fingerprint: "training-def456",
    rust_target_cpu: "x86-64-v3",
    training_input_count: 12,
    training_failure_count: 0,
    warning: null,
  });
  assert.deepEqual(report.duplicate_rows, []);
  assert.equal(report.worst.name, "ts-toolbelt-project");
  assert.equal(report.worst.exit_class, "exit success");
  assert.equal(report.worst.files_reached, 242);
  assert.equal(report.worst.peak_memory_bytes, 734003200);
  assert.deepEqual(report.worst.loss_closure, {
    owner: "Track 1/2 recursive type evaluation",
    operation: "recursive conditional, mapped/indexed access, repeated instantiation and relation cache pressure",
    command: "scripts/safe-run.sh ./scripts/bench/perf-hotspots.sh --filter '^ts-toolbelt-project$' --json-file <artifact>.json",
    attribution_command:
      "TSZ_PERF_COUNTERS=1 TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=536870912 scripts/safe-run.sh cargo run -q -p tsz-cli --features perf-tools --bin tsz -- --extendedDiagnostics --perf-counters-json <artifact>.ts-toolbelt-project.perf.json --noEmit -p .target-bench/external/ts-toolbelt/tsconfig.flat.json",
    issue: 8356,
    url: "https://github.com/tsz-org/tsz/issues/8356",
  });
  assert.deepEqual(report.worst.attribution_status, {
    present: true,
    path: "artifacts/perf/ts-toolbelt-project-attribution.json",
    url: null,
    generated_at: "2026-05-20T00:05:00.000Z",
    mode: "attribution",
    dominant_subsystem: "solver:recursive-evaluation",
    warning: null,
  });
  assert.deepEqual(
    report.rows.map((row) => row.name),
    ["ts-toolbelt-project", "vite-vanilla-ts-app", "single-file-loss"],
  );
  assert.equal(report.rows[1].loss_closure.issue, 7378);
  assert.match(
    report.rows[1].loss_closure.attribution_command,
    /--perf-counters-json <artifact>\.vite-vanilla-ts-app\.perf\.json --noEmit -p .*vite-vanilla-ts-live\/tsconfig\.json/,
  );
  assert.deepEqual(report.rows[1].attribution_status, {
    present: false,
    path: null,
    url: null,
    generated_at: null,
    mode: null,
    dominant_subsystem: null,
    warning: "attribution artifact missing",
  });
  assert.equal(report.rows[2].loss_closure, null);
  assert.deepEqual(report.by_owner_family, [
    {
      family: "recursive type evaluation pressure",
      rows: 1,
      worst_factor: 8.23,
      worst_row: "ts-toolbelt-project",
    },
    {
      family: "generated Vite dependency graph",
      rows: 1,
      worst_factor: 3.03,
      worst_row: "vite-vanilla-ts-app",
    },
  ]);

  const importedReport = createTsgoWinnerReport(JSON.parse(fs.readFileSync(input, "utf8")), input);
  assert.equal(importedReport.totals.green_tsgo_winners, 3);
  assert.equal(importedReport.worst.name, "ts-toolbelt-project");
});

{
  const markdown = renderMissingAttributionPlanMarkdown({
    generated_at: "2026-06-06T00:00:00.000Z",
    source: { path: "bench-results.json" },
    two_x_target: {
      eligible_green_rows: 1,
      rows_below_target: 0,
      project_rows_below_target: 0,
      rows_with_attribution: 0,
      missing_attribution_plan: [],
    },
  });
  assert.match(markdown, /All current 2x target gap rows have attribution evidence\./);
}

withTempDir((dir) => {
  const input = path.join(dir, "bench.json");
  writeJson(input, {
    benchmark_runner: "scripts/bench/bench-vs-tsgo.sh",
    measurement_profile: {
      mode: "release-pgo",
      tsz_binary_source: "bench-dist",
      profile_guided_optimization: {
        requested: true,
        required: true,
        optimized: false,
      },
    },
    results: [],
  });

  const report = createTsgoWinnerReport(JSON.parse(fs.readFileSync(input, "utf8")), input);
  assert.deepEqual(report.measurement_profile, {
    present: true,
    mode: "release-pgo",
    tsz_binary_source: "bench-dist",
    pgo_requested: true,
    pgo_required: true,
    pgo_optimized: false,
    profile_fingerprint: null,
    training_fingerprint: null,
    rust_target_cpu: null,
    training_input_count: null,
    training_failure_count: null,
    warning: "release-pgo metadata missing pgo optimized flag, profile fingerprint, training fingerprint",
  });
});

withTempDir((dir) => {
  const input = path.join(dir, "bench.json");
  writeJson(input, {
    results: [
      {
        name: "BCT candidates=200",
        lines: 428,
        kb: 36,
        tsz_ms: 169.77,
        tsgo_ms: 156.16,
        winner: "tsgo",
        factor: 1.09,
      },
      {
        name: "200 classes",
        lines: 9203,
        kb: 162,
        tsz_ms: 145.09,
        tsgo_ms: 137.01,
        winner: "tsgo",
        factor: 1.06,
      },
      {
        name: "100 generic functions",
        lines: 2200,
        kb: 70,
        tsz_ms: 190,
        tsgo_ms: 160,
        winner: "tsgo",
        factor: 1.19,
      },
      {
        name: "200 generic functions",
        lines: 4200,
        kb: 120,
        tsz_ms: 396.48,
        tsgo_ms: 404.57,
        winner: "tsz",
        factor: 1.02,
      },
      {
        name: "CFA branches=100",
        lines: 900,
        kb: 28,
        tsz_ms: 180,
        tsgo_ms: 150,
        winner: "tsgo",
        factor: 1.2,
      },
      {
        name: "CFA branches=150",
        lines: 1200,
        kb: 38,
        tsz_ms: 220,
        tsgo_ms: 170,
        winner: "tsgo",
        factor: 1.29,
      },
      {
        name: "Template literal N=45",
        lines: 420,
        kb: 18,
        tsz_ms: 205,
        tsgo_ms: 200,
        winner: "tsgo",
        factor: 1.03,
      },
    ],
  });

  const report = createTsgoWinnerReport(JSON.parse(fs.readFileSync(input, "utf8")), input);
  const byName = new Map(report.rows.map((row) => [row.name, row]));
  assert.match(
    byName.get("BCT candidates=200").loss_closure.attribution_command,
    /TSZ_PERF_COUNTERS=1 .*<generated-bct-candidates-200>\.ts/,
  );
  assert.match(
    byName.get("200 classes").loss_closure.attribution_command,
    /TSZ_PERF_COUNTERS=1 .*<generated-200-classes>\.ts/,
  );
  assert.match(
    byName.get("100 generic functions").loss_closure.attribution_command,
    /TSZ_PERF_COUNTERS=1 .*<generated-100-generic-functions>\.ts/,
  );
  assert.match(
    report.target_gaps
      .find((row) => row.name === "200 generic functions")
      .loss_closure.attribution_command,
    /TSZ_PERF_COUNTERS=1 .*<generated-200-generic-functions>\.ts/,
  );
  assert.match(
    byName.get("CFA branches=100").loss_closure.attribution_command,
    /TSZ_PERF_COUNTERS=1 .*<generated-cfa-branches-100>\.ts/,
  );
  assert.match(
    byName.get("CFA branches=150").loss_closure.attribution_command,
    /TSZ_PERF_COUNTERS=1 .*<generated-cfa-branches-150>\.ts/,
  );
  assert.match(
    byName.get("Template literal N=45").loss_closure.attribution_command,
    /TSZ_PERF_COUNTERS=1 .*<generated-template-literal-45>\.ts/,
  );
  assert.deepEqual(report.totals.missing_attribution_rows, [
    "100 generic functions",
    "200 classes",
    "BCT candidates=200",
    "CFA branches=100",
    "CFA branches=150",
    "Template literal N=45",
  ]);
  assert.deepEqual(
    report.target_gaps.map((row) => row.name),
    [
      "CFA branches=150",
      "CFA branches=100",
      "100 generic functions",
      "BCT candidates=200",
      "200 classes",
      "Template literal N=45",
      "200 generic functions",
    ],
  );
  assert.equal(report.two_x_target.rows_below_target, 7);
  assert.equal(report.two_x_target.rows_with_attribution, 0);
  assert.equal(report.two_x_target.rows_with_attribution_command, 7);
  assert.deepEqual(report.two_x_target.missing_attribution_rows, [
    "100 generic functions",
    "200 classes",
    "200 generic functions",
    "BCT candidates=200",
    "CFA branches=100",
    "CFA branches=150",
    "Template literal N=45",
  ]);
});

withTempDir((dir) => {
  const input = path.join(dir, "bench.json");
  writeJson(input, {
    results: [
      {
        name: "utility-types-project",
        tsz_ms: 100,
        tsgo_ms: 90,
        winner: "tsgo",
        factor: 1.11,
        compatibility: {
          state: "green",
          exit_class: "exit success",
          phase: "check",
          last_successful_phase: "check",
          diagnostic_status: "none",
          semantic_owner_family: "baseline utility mapped/conditional surface",
        },
      },
      {
        name: "ts-essentials-project",
        tsz_ms: 100,
        tsgo_ms: 90,
        winner: "tsgo",
        factor: 1.11,
        compatibility: {
          state: "green",
          exit_class: "exit success",
          phase: "check",
          last_successful_phase: "check",
          diagnostic_status: "none",
          semantic_owner_family: "utility types plus recursive JSON shapes",
        },
      },
      {
        name: "nextjs-fresh-app",
        tsz_ms: 100,
        tsgo_ms: 90,
        winner: "tsgo",
        factor: 1.11,
        compatibility: {
          state: "green",
          exit_class: "exit success",
          phase: "check",
          last_successful_phase: "check",
          diagnostic_status: "none",
          semantic_owner_family: "generated app dependency graph",
        },
      },
      {
        name: "nextjs",
        tsz_ms: 100,
        tsgo_ms: 90,
        winner: "tsgo",
        factor: 1.11,
        compatibility: {
          state: "green",
          exit_class: "exit success",
          phase: "check",
          last_successful_phase: "check",
          diagnostic_status: "none",
          semantic_owner_family: "Next.js full project module graph",
        },
      },
      {
        name: "ts-essentials/xor.ts",
        tsz_ms: 100,
        tsgo_ms: 90,
        winner: "tsgo",
        factor: 1.11,
      },
      {
        name: "ts-essentials/paths.ts",
        tsz_ms: 100,
        tsgo_ms: 90,
        winner: "tsgo",
        factor: 1.11,
      },
      {
        name: "ts-essentials/deep-pick.ts",
        tsz_ms: 100,
        tsgo_ms: 90,
        winner: "tsgo",
        factor: 1.11,
      },
      {
        name: "ts-essentials/deep-readonly.ts",
        tsz_ms: 100,
        tsgo_ms: 90,
        winner: "tsgo",
        factor: 1.11,
      },
    ],
  });

  const report = createTsgoWinnerReport(JSON.parse(fs.readFileSync(input, "utf8")), input);
  const byName = new Map(report.rows.map((row) => [row.name, row]));
  assert.match(
    byName.get("utility-types-project").loss_closure.attribution_command,
    /--perf-counters-json <artifact>\.utility-types-project\.perf\.json --noEmit -p .*utility-types\/tsconfig\.flat\.json/,
  );
  assert.match(
    byName.get("ts-essentials-project").loss_closure.attribution_command,
    /--perf-counters-json <artifact>\.ts-essentials-project\.perf\.json --noEmit -p .*ts-essentials\/tsconfig\.flat\.json/,
  );
  assert.match(
    byName.get("nextjs-fresh-app").loss_closure.attribution_command,
    /--perf-counters-json <artifact>\.nextjs-fresh-app\.perf\.json --noEmit -p .*next-app-live\/tsconfig\.json/,
  );
  assert.match(
    byName.get("nextjs").loss_closure.attribution_command,
    /--perf-counters-json <artifact>\.nextjs\.perf\.json --noEmit -p .*nextjs\/packages\/next\/tsconfig\.tsz-bench\.json/,
  );
  assert.match(
    byName.get("ts-essentials/xor.ts").loss_closure.attribution_command,
    /--perf-counters-json <artifact>\.ts-essentials-xor\.perf\.json --noEmit --lib es2018 .*ts-essentials\/lib\/xor\/index\.ts/,
  );
  assert.match(
    byName.get("ts-essentials/paths.ts").loss_closure.attribution_command,
    /--perf-counters-json <artifact>\.ts-essentials-paths\.perf\.json --noEmit --lib es2018 .*ts-essentials\/lib\/paths\/index\.ts/,
  );
  assert.match(
    byName.get("ts-essentials/deep-pick.ts").loss_closure.attribution_command,
    /--perf-counters-json <artifact>\.ts-essentials-deep-pick\.perf\.json --noEmit --lib es2018 .*ts-essentials\/lib\/deep-pick\/index\.ts/,
  );
  assert.match(
    byName.get("ts-essentials/deep-readonly.ts").loss_closure.attribution_command,
    /--perf-counters-json <artifact>\.ts-essentials-deep-readonly\.perf\.json --noEmit --lib es2018 .*ts-essentials\/lib\/deep-readonly\/index\.ts/,
  );
  assert.equal(report.two_x_target.rows_with_attribution_command, 8);
});

// Duplicate known project rows make the green-tsgo-winner summary non-authoritative.
// Single-file duplicate names are not project rows and remain eligible.
withTempDir((dir) => {
  const input = path.join(dir, "bench.json");
  const output = path.join(dir, "report.json");
  const greenProjectCompat = {
    state: "green",
    exit_class: "exit success",
    phase: "check",
    last_successful_phase: "check",
    diagnostic_status: "none",
    semantic_owner_family: "project family",
  };
  writeJson(input, {
    results: [
      { name: "ts-toolbelt-project", winner: "tsgo", factor: 8, tsz_ms: 80, tsgo_ms: 10, compatibility: greenProjectCompat },
      { name: "ts-toolbelt-project", winner: "tsz", factor: 2, tsz_ms: 10, tsgo_ms: 20, compatibility: greenProjectCompat },
      { name: "ts-essentials-project", winner: "tsgo", factor: 4, tsz_ms: 40, tsgo_ms: 10, compatibility: greenProjectCompat },
      { name: "single-file-loss", winner: "tsgo", factor: 3, tsz_ms: 30, tsgo_ms: 10 },
      { name: "single-file-loss", winner: "tsgo", factor: 2, tsz_ms: 20, tsgo_ms: 10 },
    ],
  });

  const report = createTsgoWinnerReport(JSON.parse(fs.readFileSync(input, "utf8")), input);
  assert.equal(report.totals.rows, 5);
  assert.equal(report.totals.duplicate_project_rows, 1);
  assert.deepEqual(report.duplicate_rows, [{ name: "ts-toolbelt-project", label: "ts-toolbelt", count: 2 }]);
  assert.equal(report.totals.green_tsgo_winners, 3);
  assert.equal(report.totals.project_green_tsgo_winners, 1);
  assert.deepEqual(
    report.rows.map((row) => row.name),
    ["ts-essentials-project", "single-file-loss", "single-file-loss"],
  );
  assert.equal(report.worst.name, "ts-essentials-project");

  const result = spawnSync(process.execPath, [SCRIPT, input, output], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 1, "duplicate project rows should fail the winner audit");
  assert.match(result.stderr, /duplicate project rows: ts-toolbelt-project \(2\)/);
  const cliReport = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.equal(cliReport.totals.duplicate_project_rows, 1);
});

// Rows with missing required phase/exit metadata must not appear as speed wins.
// Each sub-case below verifies that a row with one specific missing field is
// excluded from the green winner list and counted in incomplete_compat_excluded.
withTempDir((dir) => {
  const baseCompatibility = {
    state: "green",
    exit_class: "exit success",
    phase: "check",
    last_successful_phase: "check",
    diagnostic_status: "none",
    semantic_owner_family: "test family",
  };

  function withoutField(field) {
    const { [field]: _dropped, ...rest } = baseCompatibility;
    return rest;
  }

  const input = path.join(dir, "bench.json");
  writeJson(input, {
    results: [
      { name: "complete-project", winner: "tsgo", factor: 5, status: null, tsz_ms: 100, tsgo_ms: 20, compatibility: baseCompatibility },
      { name: "missing-state", winner: "tsgo", factor: 4, status: null, tsz_ms: 100, tsgo_ms: 25, compatibility: withoutField("state") },
      { name: "missing-phase", winner: "tsgo", factor: 3, status: null, tsz_ms: 100, tsgo_ms: 33, compatibility: withoutField("phase") },
      { name: "missing-last-phase", winner: "tsgo", factor: 2, status: null, tsz_ms: 100, tsgo_ms: 50, compatibility: withoutField("last_successful_phase") },
      { name: "missing-exit-class", winner: "tsgo", factor: 2, status: null, tsz_ms: 100, tsgo_ms: 50, compatibility: withoutField("exit_class") },
      { name: "missing-diag-status", winner: "tsgo", factor: 2, status: null, tsz_ms: 100, tsgo_ms: 50, compatibility: withoutField("diagnostic_status") },
      { name: "artifact-missing", winner: "tsgo", factor: 2, status: null, tsz_ms: 100, tsgo_ms: 50, artifact_missing: true },
      // single-file rows without compatibility are always eligible — no metadata required
      { name: "single-file-win", winner: "tsgo", factor: 1.5, status: null, tsz_ms: 15, tsgo_ms: 10 },
    ],
  });

  const report = createTsgoWinnerReport(JSON.parse(fs.readFileSync(input, "utf8")), input);

  assert.equal(report.totals.rows, 8);
  // Only complete-project and single-file-win are green winners
  assert.equal(report.totals.green_tsgo_winners, 2);
  assert.equal(report.totals.project_green_tsgo_winners, 1);
  assert.equal(report.totals.green_tsgo_winners_with_closure, 0);
  assert.deepEqual(report.totals.missing_loss_closure_rows, ["complete-project", "single-file-win"]);
  assert.equal(report.totals.green_tsgo_winners_with_attribution, 0);
  assert.deepEqual(report.totals.missing_attribution_rows, ["complete-project", "single-file-win"]);
  assert.equal(report.two_x_target.eligible_green_rows, 2);
  assert.equal(report.two_x_target.rows_below_target, 2);
  assert.equal(report.two_x_target.rows_with_attribution, 0);
  assert.deepEqual(report.two_x_target.missing_attribution_rows, ["complete-project", "single-file-win"]);
  assert.deepEqual(
    report.target_gaps.map((row) => row.name),
    ["complete-project", "single-file-win"],
  );
  assert.deepEqual(report.measurement_profile, {
    present: false,
    mode: null,
    tsz_binary_source: null,
    pgo_requested: null,
    pgo_required: null,
    pgo_optimized: null,
    profile_fingerprint: null,
    training_fingerprint: null,
    rust_target_cpu: null,
    training_input_count: null,
    training_failure_count: null,
    warning: "measurement_profile missing",
  });
  // 6 rows excluded due to missing phase/exit metadata or artifact_missing
  assert.equal(report.totals.incomplete_compat_excluded, 6);
  assert.deepEqual(
    report.rows.map((r) => r.name),
    ["complete-project", "single-file-win"],
  );
});

withTempDir((dir) => {
  const input = path.join(dir, "bench.json");
  const output = path.join(dir, "report.json");
  writeJson(input, {
    results: [
      { name: "target-met", winner: "tsz", factor: 2.5, tsz_ms: 10, tsgo_ms: 25 },
      { name: "target-short", winner: "tsz", factor: 1.5, tsz_ms: 10, tsgo_ms: 15 },
      { name: "tsgo-win", winner: "tsgo", factor: 3, tsz_ms: 30, tsgo_ms: 10 },
    ],
  });

  const result = spawnSync(process.execPath, [SCRIPT, input, output], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.match(result.stdout, /2x target gaps: 2\/3/);
  assert.match(result.stdout, /startup-floor target gaps: 1\/2/);

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.equal(report.two_x_target.rows_meeting_target, 1);
  assert.equal(report.two_x_target.rows_below_target, 2);
  assert.equal(report.two_x_target.rows_with_attribution, 0);
  assert.deepEqual(report.two_x_target.missing_attribution_rows, ["target-short", "tsgo-win"]);
  assert.deepEqual(
    report.target_gaps.map((row) => [row.name, row.tsz_speedup_vs_tsgo]),
    [
      ["tsgo-win", 10 / 30],
      ["target-short", 15 / 10],
    ],
  );
  assert.equal(report.target_gaps.find((row) => row.name === "target-short").startup_floor_win, true);
  assert.equal(report.target_gaps.find((row) => row.name === "tsgo-win").startup_floor_win, false);
});

withTempDir((dir) => {
  const input = path.join(dir, "bench.json");
  const output = path.join(dir, "report.json");
  const perfPath = path.join(dir, "bench.perf.json");
  writeJson(input, {
    results: [
      {
        name: "ts-essentials-project",
        winner: "tsz",
        factor: 1.1,
        tsz_ms: 100,
        tsgo_ms: 110,
        compatibility: {
          state: "green",
          exit_class: "exit success",
          phase: "check",
          last_successful_phase: "check",
          diagnostic_status: "none",
          semantic_owner_family: "utility types plus recursive JSON shapes",
        },
      },
    ],
  });
  writeJson(perfPath, {
    mode: "attribution",
    delegate: { misses: 0 },
    checker: { with_parent_cache_constructed: 2 },
    slow_type_alias_check_timings: [
      {
        file: "ts-essentials/lib/xor/index.ts",
        name: "XOR",
        phase: "body_validation",
        elapsed_ms: 55.98,
      },
    ],
    slow_check_file_timings: [
      { file: "ts-essentials/lib/xor/index.ts", elapsed_ms: 150, diagnostics: 0 },
    ],
  });

  const result = spawnSync(process.execPath, [SCRIPT, input, output], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.match(result.stdout, /2x target gaps with attribution: 1\/1/);

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.equal(report.two_x_target.rows_below_target, 1);
  assert.equal(report.two_x_target.rows_with_attribution, 1);
  assert.equal(report.two_x_target.rows_with_attribution_command, 1);
  assert.deepEqual(report.two_x_target.missing_attribution_rows, []);
  assert.deepEqual(report.target_gaps[0].attribution_status, {
    present: true,
    path: path.relative(ROOT, perfPath).split(path.sep).join("/"),
    url: null,
    generated_at: report.target_gaps[0].attribution_status.generated_at,
    mode: "attribution",
    dominant_subsystem: "checker:semantic-check",
    dominant_hotspot: {
      kind: "type_alias_phase",
      name: "XOR",
      phase: "body_validation",
      elapsed_ms: 55.98,
      file: "ts-essentials/lib/xor/index.ts",
    },
    warning: null,
  });
  assert.match(report.target_gaps[0].attribution_status.generated_at, /^\d{4}-\d{2}-\d{2}T/);
});

withTempDir((dir) => {
  const input = path.join(dir, "bench.json");
  const output = path.join(dir, "report.json");
  const perfPath = path.join(dir, "bench.ts-essentials-project.perf.json");
  writeJson(input, {
    results: [
      {
        name: "ts-essentials-project",
        winner: "tsgo",
        factor: 1.2,
        tsz_ms: 120,
        tsgo_ms: 100,
        compatibility: {
          state: "green",
          exit_class: "exit success",
          phase: "check",
          last_successful_phase: "check",
          diagnostic_status: "none",
          semantic_owner_family: "utility types plus recursive JSON shapes",
        },
      },
      {
        name: "vite-vanilla-ts-app",
        winner: "tsgo",
        factor: 1.5,
        tsz_ms: 150,
        tsgo_ms: 100,
        compatibility: {
          state: "green",
          exit_class: "exit success",
          phase: "check",
          last_successful_phase: "check",
          diagnostic_status: "none",
          semantic_owner_family: "generated app dependency/config sanity",
        },
      },
    ],
  });
  writeJson(perfPath, {
    mode: "attribution",
    delegate: { misses: 0 },
    checker: { with_parent_cache_constructed: 0 },
    slow_check_file_timings: [
      { file: "ts-essentials/lib/xor/index.ts", elapsed_ms: 150, diagnostics: 0 },
    ],
  });

  const result = spawnSync(process.execPath, [SCRIPT, input, output], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.match(result.stdout, /2x target gaps with attribution: 1\/2/);

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.equal(report.two_x_target.rows_below_target, 2);
  assert.equal(report.two_x_target.rows_with_attribution, 1);
  assert.deepEqual(report.two_x_target.missing_attribution_rows, ["vite-vanilla-ts-app"]);
  assert.equal(report.two_x_target.rows_with_attribution_command, 2);
  assert.deepEqual(report.two_x_target.missing_attribution_plan.map((row) => row.name), [
    "vite-vanilla-ts-app",
  ]);
  assert.match(
    report.two_x_target.missing_attribution_plan[0].attribution_command,
    /nextjs-fresh-app|vite-vanilla-ts-app/,
  );
  const tsEssentials = report.target_gaps.find((row) => row.name === "ts-essentials-project");
  assert.deepEqual(tsEssentials.attribution_status, {
    present: true,
    path: path.relative(ROOT, perfPath).split(path.sep).join("/"),
    url: null,
    generated_at: tsEssentials.attribution_status.generated_at,
    mode: "attribution",
    dominant_subsystem: "checker:semantic-check",
    dominant_hotspot: {
      kind: "file",
      elapsed_ms: 150,
      file: "ts-essentials/lib/xor/index.ts",
    },
    warning: null,
  });
});

withTempDir((dir) => {
  const input = path.join(dir, "bench.json");
  const output = path.join(dir, "report.json");
  const perfPath = path.join(dir, "bench.ts-essentials-project.perf.json");
  writeJson(input, {
    results: [
      {
        name: "ts-essentials-project",
        winner: "tsgo",
        factor: 1.2,
        tsz_ms: 120,
        tsgo_ms: 100,
        compatibility: {
          state: "green",
          exit_class: "exit success",
          phase: "check",
          last_successful_phase: "check",
          diagnostic_status: "none",
          semantic_owner_family: "utility types plus recursive JSON shapes",
        },
      },
    ],
  });
  writeJson(perfPath, {
    schema_version: 9,
    enabled: true,
    mode: "attribution",
    wired: {
      delegate_cross_arena: true,
      checker_construction: true,
      interner_intern_calls: true,
    },
    delegate: { misses: 0 },
    checker: { with_parent_cache_constructed: 0 },
    interner: { intern_calls: 0 },
  });

  const result = spawnSync(process.execPath, [SCRIPT, input, output], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.match(result.stdout, /2x target gaps with attribution: 1\/1/);

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.equal(report.two_x_target.rows_below_target, 1);
  assert.equal(report.two_x_target.rows_with_attribution, 1);
  assert.deepEqual(report.two_x_target.missing_attribution_rows, []);
  assert.deepEqual(report.two_x_target.missing_attribution_plan, []);
  assert.equal(
    report.target_gaps[0].attribution_status.warning,
    "attribution dominant_subsystem missing",
  );
});

withTempDir((dir) => {
  const input = path.join(dir, "bench.json");
  const output = path.join(dir, "report.json");
  const perfPath = path.join(dir, "bench.perf.json");
  writeJson(input, {
    results: [
      {
        name: "ts-essentials-project",
        winner: "tsz",
        factor: 1.1,
        tsz_ms: 100,
        tsgo_ms: 110,
        compatibility: {
          state: "green",
          exit_class: "exit success",
          phase: "check",
          last_successful_phase: "check",
          diagnostic_status: "none",
        },
      },
    ],
  });
  writeJson(perfPath, {
    mode: "timing",
    delegate: { misses: 0 },
    checker: { with_parent_cache_constructed: 2 },
    slow_check_file_timings: [
      { file: "ts-essentials/lib/xor/index.ts", elapsed_ms: 150, diagnostics: 0 },
    ],
  });

  const result = spawnSync(process.execPath, [SCRIPT, input, output], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.match(result.stdout, /2x target gaps with attribution: 0\/1/);

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.equal(report.two_x_target.rows_below_target, 1);
  assert.equal(report.two_x_target.rows_with_attribution, 0);
  assert.deepEqual(report.two_x_target.missing_attribution_rows, ["ts-essentials-project"]);
  assert.deepEqual(report.target_gaps[0].attribution_status, {
    present: true,
    path: path.relative(ROOT, perfPath).split(path.sep).join("/"),
    url: null,
    generated_at: report.target_gaps[0].attribution_status.generated_at,
    mode: "timing",
    dominant_subsystem: null,
    warning: "sidecar perf snapshot mode is not attribution",
  });
});

withTempDir((dir) => {
  const input = path.join(dir, "bench.json");
  const output = path.join(dir, "report.json");
  const attributionPlan = path.join(dir, "missing-attribution.md");
  const manifest = path.join(dir, "bench-attribution-manifest.json");
  const perfPath = path.join(dir, "bench.ts-essentials-project.perf.json");
  writeJson(input, {
    results: [
      {
        name: "ts-essentials-project",
        winner: "tsgo",
        factor: 1.2,
        tsz_ms: 120,
        tsgo_ms: 100,
        compatibility: {
          state: "green",
          exit_class: "exit success",
          phase: "check",
          last_successful_phase: "check",
          diagnostic_status: "none",
          semantic_owner_family: "utility types plus recursive JSON shapes",
        },
      },
      {
        name: "vite-vanilla-ts-app",
        winner: "tsgo",
        factor: 1.5,
        tsz_ms: 150,
        tsgo_ms: 100,
        compatibility: {
          state: "green",
          exit_class: "exit success",
          phase: "check",
          last_successful_phase: "check",
          diagnostic_status: "none",
          semantic_owner_family: "generated app dependency/config sanity",
        },
      },
    ],
  });
  writeJson(perfPath, {
    mode: "attribution",
    checker: { with_parent_cache_constructed: 0 },
    delegate: { misses: 0 },
    slow_check_file_timings: [],
  });
  writeJson(manifest, {
    schema_version: 1,
    rows: [
      {
        name: "ts-essentials-project",
        status: "failed",
        exit_code: 1,
        signal: null,
        perf_path: perfPath,
      },
      {
        name: "vite-vanilla-ts-app",
        status: "skipped",
        reason: "unresolved placeholder <generated-vite>",
      },
    ],
  });

  const result = spawnSync(process.execPath, [SCRIPT, input, output, attributionPlan], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.match(result.stdout, /2x target gaps with attribution: 1\/2/);

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.deepEqual(report.two_x_target.attribution_attempts, {
    failed: 1,
    skipped: 1,
  });
  assert.equal(report.two_x_target.rows_with_attribution, 1);
  assert.deepEqual(report.two_x_target.missing_attribution_rows, ["vite-vanilla-ts-app"]);
  assert.deepEqual(report.two_x_target.missing_attribution_plan.map((row) => row.name), [
    "vite-vanilla-ts-app",
  ]);

  const tsEssentials = report.target_gaps.find((row) => row.name === "ts-essentials-project");
  assert.equal(tsEssentials.attribution_status.present, true);
  assert.equal(tsEssentials.attribution_status.mode, "attribution");
  assert.equal(tsEssentials.attribution_status.attempt_status, "failed");
  assert.equal(tsEssentials.attribution_status.attempt_exit_code, 1);
  assert.equal(tsEssentials.attribution_status.warning, "attribution command failed: exit 1");

  const vite = report.target_gaps.find((row) => row.name === "vite-vanilla-ts-app");
  assert.equal(vite.attribution_status.present, false);
  assert.equal(vite.attribution_status.attempt_status, "skipped");
  assert.equal(
    vite.attribution_status.warning,
    "attribution attempt skipped: unresolved placeholder <generated-vite>",
  );

  const planMarkdown = fs.readFileSync(attributionPlan, "utf8");
  assert.doesNotMatch(planMarkdown, /Attribution attempt: failed/);
  assert.match(planMarkdown, /Attribution attempt: skipped/);
});

const benchWorkflow = fs.readFileSync(BENCH_WORKFLOW, "utf8");
assert.match(
  benchWorkflow,
  /node scripts\/bench\/tsgo-winner-report\.mjs\s+\\\s*\n\s+"\$GITHUB_WORKSPACE\/bench-results\.json"\s+\\\s*\n\s+"\$GITHUB_WORKSPACE\/bench-results-tsgo-winners\.json"\s+\\\s*\n\s+"\$GITHUB_WORKSPACE\/bench-results-missing-attribution\.md"/,
  "bench workflow should generate the green tsgo winner report from merged results",
);
// The GCP scale-down in #15343 (`ci: stop GCP-backed tsz automation`)
// deliberately reduced this workflow to a single manual `workflow_dispatch`
// job with `contents: read`/`issues: read` permissions. The heavy post-report
// pipeline it used to guard — the standalone `run-attribution-plan.mjs`
// sidecar collection, the timestamped/`latest` `bench-runs/*` publish steps
// (which needed `contents: write`), and the severe-alert issue filing (which
// needed `issues: write`) — was removed with it. The winner report itself is
// still generated inline (attribution attempts run inside
// `tsgo-winner-report.mjs`), so this test now guards the slimmed contract:
// generate the report, evaluate readiness, upload the merged artifact, and
// dispatch the site redeploy.
assert.match(
  benchWorkflow,
  /node scripts\/bench\/check-artifact-readiness\.mjs[\s\S]+?"\$GITHUB_WORKSPACE\/bench-results\.json"[\s\S]+?> "\$GITHUB_WORKSPACE\/bench-results-readiness\.json"/,
  "bench workflow should evaluate public benchmark readiness from the merged results",
);
assert.match(
  benchWorkflow,
  /name: bench-results-merged\s*\n\s+path: \|\s*\n\s+bench-results\.json\s*\n\s+bench-results-tsgo-winners\.json\s*\n\s+bench-results-missing-attribution\.md\s*\n\s+bench-results-readiness\.json/,
  "merged benchmark artifact should upload the results, green tsgo winner report, missing-attribution plan, and readiness verdict",
);
assert.match(
  benchWorkflow,
  /actions\/workflows\/gh-pages\.yml\/dispatches/,
  "bench workflow should dispatch the GitHub Pages site redeploy after uploading results",
);

const ghPagesWorkflow = fs.readFileSync(GH_PAGES_WORKFLOW, "utf8");
assert.match(
  ghPagesWorkflow,
  /mv artifacts\/bench-results-tsgo-winners\.json artifacts\/bench-vs-tsgo-github-latest\.tsgo-winners\.json/,
  "GitHub Pages workflow should preserve the downloaded green tsgo winner report",
);
assert.match(
  ghPagesWorkflow,
  /rm -f artifacts\/bench-results\.json artifacts\/bench-results-tsgo-winners\.json/,
  "GitHub Pages workflow should drop stale winner reports when benchmark data is stale or empty",
);

const eleventyConfig = fs.readFileSync(WEBSITE_ELEVENTY, "utf8");
assert.match(
  eleventyConfig,
  /latestBenchmarkArtifact\?\.replace\(\s*\/\\\.json\$\/,\s*"\.tsgo-winners\.json",\s*\)/,
  "website should derive the green tsgo winner artifact path from the selected benchmark data",
);
assert.match(
  eleventyConfig,
  /"benchmark-data\/latest\.tsgo-winners\.json"/,
  "website should publish the green tsgo winner report beside benchmark-data/latest.json",
);
assert.match(
  eleventyConfig,
  /createTsgoWinnerReport\(benchmarkData, latestBenchmarkArtifact\)/,
  "website should synthesize the green tsgo winner report when the selected benchmark has no prebuilt report",
);
assert.match(
  eleventyConfig,
  /renderReadmePerfSvg\(benchmarkData\)/,
  "website should render the README performance SVG from the selected benchmark data",
);
assert.match(
  eleventyConfig,
  /renderReadmePerfPng\(benchmarkData\)/,
  "website should render the README performance PNG from the selected benchmark data",
);
assert.match(
  eleventyConfig,
  /renderReadmePerfPng\(benchmarkData, \{ theme: "dark" \}\)/,
  "website should render a dark-mode README performance PNG from the selected benchmark data",
);
assert.match(
  eleventyConfig,
  /"benchmark-data", "readme-perf\.svg"/,
  "website should publish the README performance SVG beside benchmark-data/latest.json",
);
assert.match(
  eleventyConfig,
  /"benchmark-data", "readme-perf\.png"/,
  "website should publish the README performance PNG beside benchmark-data/latest.json",
);
assert.match(
  eleventyConfig,
  /"benchmark-data", "readme-perf-light\.png"/,
  "website should publish the light-mode README performance PNG beside benchmark-data/latest.json",
);
assert.match(
  eleventyConfig,
  /"benchmark-data", "readme-perf-dark\.png"/,
  "website should publish the dark-mode README performance PNG beside benchmark-data/latest.json",
);

withTempDir((dir) => {
  const script = [
    "import assert from 'node:assert/strict';",
    "import fs from 'node:fs';",
    "import path from 'node:path';",
    "import { createTsgoWinnerReport } from '../../scripts/bench/tsgo-winner-report.mjs';",
    "import { PROJECT_ROW_DEFINITIONS } from '../../scripts/bench/project-rows.mjs';",
    "const artifactDir = '../../artifacts';",
    "const artifactPath = '../../artifacts/bench-vs-tsgo-github-latest.json';",
    "fs.mkdirSync(artifactDir, { recursive: true });",
    "const sourceSnapshot = JSON.parse(fs.readFileSync('bench-snapshot.json', 'utf8'));",
    "const appRows = PROJECT_ROW_DEFINITIONS.filter((row) => row.category === 'application').map((row, index) => ({",
    "  name: row.name,",
    "  tsz_ms: null,",
    "  tsgo_ms: null,",
    "  winner: 'error',",
    "  status: 'compile canary tracked in CI; not timed by vs-tsgo benchmarks',",
    "  compatibility: {",
    "    state: index === 0 ? 'green' : 'red',",
    "    phase: 'check',",
    "    last_successful_phase: index === 0 ? 'check' : null,",
    "    exit_class: index === 0 ? 'exit success' : 'timeout',",
    "    diagnostic_status: index === 0 ? 'none' : 'compiler timed out',",
    "  },",
    "}));",
    "fs.writeFileSync(artifactPath, JSON.stringify({",
    "  ...sourceSnapshot,",
    "  generated_at: '2099-01-01T00:00:00.000Z',",
    "  results: [...sourceSnapshot.results, ...appRows],",
    "}, null, 2));",
    "const { default: configure } = await import('./.eleventy.js');",
    "const callbacks = [];",
    "const passthrough = [];",
    "configure({",
    "  addPassthroughCopy(copy) { passthrough.push(copy); },",
    "  addWatchTarget() {},",
    "  setServerOptions() {},",
    "  on(event, callback) { if (event === 'eleventy.after') callbacks.push(callback); },",
    "});",
    "assert.ok(passthrough.some((copy) => copy[artifactPath] === 'benchmark-data/latest.json'));",
    "fs.mkdirSync(process.env.TSZ_TEST_DIST, { recursive: true });",
    "try {",
    "  for (const callback of callbacks) await callback({ dir: { output: process.env.TSZ_TEST_DIST } });",
    "  const reportPath = path.join(process.env.TSZ_TEST_DIST, 'benchmark-data', 'latest.tsgo-winners.json');",
    "  const report = JSON.parse(fs.readFileSync(reportPath, 'utf8'));",
    "  const snapshot = JSON.parse(fs.readFileSync(artifactPath, 'utf8'));",
    "  const expected = createTsgoWinnerReport(snapshot, artifactPath);",
    "  assert.equal(report.worst.name, expected.worst.name);",
    "  assert.equal(report.totals.green_tsgo_winners, expected.totals.green_tsgo_winners);",
    "  const svgPath = path.join(process.env.TSZ_TEST_DIST, 'benchmark-data', 'readme-perf.svg');",
    "  const svg = fs.readFileSync(svgPath, 'utf8');",
    "  assert.doesNotMatch(svg, />Latest benchmark snapshot</);",
    "  assert.doesNotMatch(svg, />successful micro rows</);",
    "  assert.match(svg, /#cf222e/);",
    "  const pngPath = path.join(process.env.TSZ_TEST_DIST, 'benchmark-data', 'readme-perf.png');",
    "  const png = fs.readFileSync(pngPath);",
    "  assert.equal(png.slice(0, 8).toString('hex'), '89504e470d0a1a0a');",
    "  const lightPngPath = path.join(process.env.TSZ_TEST_DIST, 'benchmark-data', 'readme-perf-light.png');",
    "  const darkPngPath = path.join(process.env.TSZ_TEST_DIST, 'benchmark-data', 'readme-perf-dark.png');",
    "  const lightPng = fs.readFileSync(lightPngPath);",
    "  const darkPng = fs.readFileSync(darkPngPath);",
    "  assert.equal(lightPng.slice(0, 8).toString('hex'), '89504e470d0a1a0a');",
    "  assert.equal(darkPng.slice(0, 8).toString('hex'), '89504e470d0a1a0a');",
    "  assert.notEqual(lightPng.toString('base64'), darkPng.toString('base64'));",
    "} finally {",
    "  fs.rmSync(artifactPath, { force: true });",
    "}",
    "",
  ].join("\n");

  const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
    cwd: path.join(ROOT, "crates", "tsz-website"),
    encoding: "utf8",
    env: {
      ...process.env,
      TSZ_TEST_DIST: path.join(dir, "dist"),
    },
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
});
