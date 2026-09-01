import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { GREEN_COMPAT } from "../../../scripts/bench/row-utils.mjs";

const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "tsz-benchmark-data-"));
const artifact = path.join(tmpDir, "bench-vs-tsgo-test.json");
const failedOnlyArtifact = path.join(tmpDir, "bench-vs-tsgo-failed-only.json");

function exactProjectCompatibility(sourceFiles) {
  return {
    ...GREEN_COMPAT,
    source_files: sourceFiles,
    oracle_source_files: sourceFiles,
    files_reached: sourceFiles,
    exit_codes: { tsc: [0], tsz: [0], tsgo: [0] },
  };
}

const fixtureSource = `type Variant =
  | { kind: "a"; value: string }
  | { kind: "b"; value: number };

type PickValue<T> = T extends { value: infer V } ? V : never;
type Result = PickValue<Variant>;`;

await fs.writeFile(artifact, `${JSON.stringify({
  generated_at: "2026-05-16T00:00:00.000Z",
  source_commit: "0123456789abcdef0123456789abcdef01234567",
  workflow_name: "Bench",
  workflow_run_id: "1001",
  workflow_run_url: "https://github.com/tsz-org/tsz/actions/runs/1001",
  workflow_run_attempt: "1",
  run_status: "completed",
  benchmark_runner: "scripts/bench/bench-vs-tsgo.sh",
  validation: {
    hyperfine_exit_codes_required: true,
  },
  results: [
    {
      name: "conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable.ts",
      lines: 6,
      kb: 1,
      tsz_ms: 8,
      tsgo_ms: 12,
      winner: "tsz",
      source: {
        origin: "typescript",
        path: "TypeScript/tests/cases/compiler/conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable.ts",
        sha256: "test-sha",
        content: fixtureSource,
      },
    },
    {
      name: "Infer stress N=15",
      lines: 100,
      kb: 4,
      tsz_ms: 3,
      tsgo_ms: 4,
      winner: "tsz",
    },
    {
      name: "utility-types-project",
      lines: 1000,
      kb: 40,
      tsz_ms: 20,
      tsgo_ms: 30,
      winner: "tsz",
      compatibility: {
        ...exactProjectCompatibility(10),
        generated_at: "2026-05-16T00:00:00.000Z",
        source_commit: "local",
        workflow_name: "Bench",
        workflow_run_id: "1001",
        workflow_run_url: "https://github.com/tsz-org/tsz/actions/runs/1001",
        workflow_run_attempt: "1",
        run_status: "completed",
        state: "green",
        exit_class: "exit success",
        first_failure_class: null,
        owner_track: null,
        phase: "check",
        last_successful_phase: "check",
        diagnostic_status: "none",
        diagnostic_deltas: [],
        diagnostic_subsystems: [],
        known_blockers: [],
        reduced_repro_path: null,
        repro: {},
        exit_codes: { tsc: [0], tsz: [0], tsgo: [0] },
        files_reached: 10,
        files_reached_reason: null,
        peak_memory_bytes: 104857600,
        peak_memory_bytes_reason: null,
        fixture_sources: [
          {
            name: "utility-types",
            repository: "https://github.com/piotrwitek/utility-types.git",
            ref: "utility-ref",
          },
        ],
        emit_status: "not in scope (noEmit project check)",
        dts_status: "not in scope (noEmit project check)",
      },
    },
    {
      // Regression for #16196: a row killed at a SHORT timeout ceiling whose
      // ceiling wall time lands under 1.5x tsgo. It deliberately carries a
      // non-error `winner` and finite timings (the merge step's incidental
      // `winner: "error"`/null-timing stamp is absent here), so the ONLY thing
      // that can exclude it is the structural `didNotFinish` guard keyed on
      // `exit_class: "timeout"`. Without that guard it charts as a fabricated
      // "tsz 125.0x faster" win for a compiler run that never finished.
      // `ofetch-project` is a real perf-timed corpus row, so it exercises the
      // project chart path where #16196's `large-ts-repo` timeout actually lives.
      name: "ofetch-project",
      lines: 500,
      kb: 20,
      tsz_ms: 40,
      tsgo_ms: 5000,
      winner: "tsz",
      compatibility: {
        generated_at: "2026-05-16T00:00:00.000Z",
        source_commit: "local",
        workflow_name: "Bench",
        workflow_run_id: "1001",
        workflow_run_url: "https://github.com/tsz-org/tsz/actions/runs/1001",
        workflow_run_attempt: "1",
        run_status: "completed",
        state: "red",
        exit_class: "timeout",
        first_failure_class: "timeout",
        owner_track: null,
        phase: "check",
        last_successful_phase: null,
        diagnostic_status: "compiler timed out",
        diagnostic_deltas: [],
        diagnostic_subsystems: [],
        known_blockers: [],
        reduced_repro_path: null,
        repro: {},
        exit_codes: { tsc: [0], tsz: [124], tsgo: [0] },
        files_reached: 5,
        files_reached_reason: null,
        peak_memory_bytes: null,
        peak_memory_bytes_reason: null,
        fixture_sources: [],
        emit_status: "not in scope (noEmit project check)",
        dts_status: "not in scope (noEmit project check)",
      },
    },
    {
      name: "rxjs-project",
      lines: 12000,
      kb: 900,
      tsz_ms: 300,
      tsgo_ms: 100,
      winner: "tsgo",
      factor: 3,
      compatibility: {
        ...exactProjectCompatibility(12),
        generated_at: "2026-05-16T00:00:00.000Z",
        source_commit: "local",
        workflow_name: "Bench",
        workflow_run_id: "1001",
        workflow_run_url: "https://github.com/tsz-org/tsz/actions/runs/1001",
        workflow_run_attempt: "1",
        run_status: "completed",
        state: "green",
        exit_class: "exit success",
        first_failure_class: null,
        owner_track: null,
        phase: "check",
        last_successful_phase: "check",
        diagnostic_status: "none",
        diagnostic_deltas: [],
        diagnostic_subsystems: [],
        known_blockers: [],
        reduced_repro_path: null,
        repro: {},
        exit_codes: { tsc: [0], tsz: [0], tsgo: [0] },
        files_reached: 12,
        files_reached_reason: null,
        peak_memory_bytes: 104857600,
        peak_memory_bytes_reason: null,
        fixture_sources: [
          {
            name: "rxjs",
            repository: "https://github.com/ReactiveX/rxjs.git",
            ref: "rxjs-ref",
          },
        ],
        emit_status: "not in scope (noEmit project check)",
        dts_status: "not in scope (noEmit project check)",
      },
    },
    {
      name: "type-challenges-solutions-project",
      lines: 78,
      kb: 0,
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
      status: "compile canary tracked in CI; not timed by vs-tsgo benchmarks",
      compatibility: {
        generated_at: "2026-05-16T00:00:00.000Z",
        source_commit: "local",
        workflow_name: "Bench",
        workflow_run_id: "1001",
        workflow_run_url: "https://github.com/tsz-org/tsz/actions/runs/1001",
        workflow_run_attempt: "1",
        run_status: "completed",
        state: "green",
        exit_class: "exit success",
        first_failure_class: null,
        owner_track: null,
        phase: "check",
        last_successful_phase: "check",
        diagnostic_status: "none",
        diagnostic_deltas: [],
        diagnostic_subsystems: [],
        known_blockers: [],
        reduced_repro_path: "type-challenges-solutions/.tsz-compile/solutions",
        repro: {
          tsconfig_path: "type-challenges-solutions/.tsz-compile/tsconfig.tsz-guard.json",
          source_root: "type-challenges-solutions/.tsz-compile/solutions",
          first_failure_path: null,
          first_failure_line: null,
          first_failure_column: null,
          first_failure_code: null,
          reduced_repro_path: "type-challenges-solutions/.tsz-compile/solutions",
          command: "$TSZ_BIN --noEmit -p type-challenges-solutions/.tsz-compile/tsconfig.tsz-guard.json",
        },
        exit_codes: { tsc: [0], tsz: [0], tsgo: [] },
        files_reached: 78,
        files_reached_reason: null,
        peak_memory_bytes: null,
        peak_memory_bytes_reason: "not measured on platform",
        fixture_sources: [
          {
            name: "type-challenges-solutions",
            repository: "https://github.com/ghaiklor/type-challenges-solutions.git",
            ref: "91a6d2986650475f29eeb3bd18ebd025128aa07e",
          },
        ],
        emit_status: "not in scope (noEmit project check)",
        dts_status: "not in scope (noEmit project check)",
      },
    },
    {
      name: "umami-project",
      lines: 204,
      kb: 0,
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
      status: "compile canary tracked in CI; not timed by vs-tsgo benchmarks",
      compatibility: {
        generated_at: "2026-05-16T00:00:00.000Z",
        source_commit: "local",
        workflow_name: "Bench",
        workflow_run_id: "1001",
        workflow_run_url: "https://github.com/tsz-org/tsz/actions/runs/1001",
        workflow_run_attempt: "1",
        run_status: "completed",
        state: "green",
        exit_class: "exit success",
        first_failure_class: null,
        owner_track: null,
        phase: "check",
        last_successful_phase: "check",
        diagnostic_status: "none",
        diagnostic_deltas: [],
        diagnostic_subsystems: [],
        known_blockers: [],
        reduced_repro_path: null,
        repro: {
          tsconfig_path: "umami/tsconfig.json",
          source_root: "umami/src",
          first_failure_path: null,
          first_failure_line: null,
          first_failure_column: null,
          first_failure_code: null,
          reduced_repro_path: null,
          command: "$TSZ_BIN --noEmit -p umami/tsconfig.json",
        },
        exit_codes: { tsc: [0], tsz: [0], tsgo: [] },
        files_reached: 204,
        files_reached_reason: null,
        peak_memory_bytes: 209715200,
        peak_memory_bytes_reason: null,
        fixture_sources: [
          {
            name: "umami",
            repository: "https://github.com/umami-software/umami.git",
            ref: "9f3a52f7b62d875d8b6f2ef6b6e4fc621876d2be",
          },
        ],
        emit_status: "not in scope (noEmit project check)",
        dts_status: "not in scope (noEmit project check)",
      },
    },
  ],
}, null, 2)}\n`, "utf8");

await fs.writeFile(failedOnlyArtifact, `${JSON.stringify({
  generated_at: "2026-05-16T00:00:00.000Z",
  source_commit: "local",
  workflow_name: "Bench",
  workflow_run_id: "1002",
  workflow_run_url: "https://github.com/tsz-org/tsz/actions/runs/1002",
  workflow_run_attempt: "2",
  run_status: "cancelled",
  latest_completed_benchmark_run_id: "1003",
  latest_completed_benchmark_generated_at: "2026-05-17T00:00:00.000Z",
  benchmark_runner: "scripts/bench/bench-vs-tsgo.sh",
  validation: {
    hyperfine_exit_codes_required: true,
  },
  results: [
    {
      name: "rxjs-project",
      lines: 12000,
      kb: 900,
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
      status: "diagnostic mismatch",
      compatibility: {
        state: "yellow",
        exit_class: "diagnostic mismatch",
        first_failure_class: "relations-assignability",
        owner_track: "Track 4 relation diagnostics/compatibility",
        phase: "check",
        last_successful_phase: "parse",
        diagnostic_status: "diagnostic mismatch",
        diagnostic_deltas: ["TS2322 example"],
        diagnostic_subsystems: [{ subsystem: "relations-assignability", count: 1, codes: ["TS2322"] }],
        known_blockers: ["relations-assignability"],
        reduced_repro_path: "src/operators/map.ts",
        repro: {
          tsconfig_path: "tsconfig.json",
          source_root: "src",
          first_failure_path: "src/operators/map.ts",
          first_failure_line: 42,
          first_failure_column: 7,
          first_failure_code: "TS2322",
          reduced_repro_path: "src/operators/map.ts",
          command: "$TSZ_BIN --noEmit -p tsconfig.json",
        },
        exit_codes: {
          tsc: [0],
          tsz: [1],
          tsgo: [0],
        },
        files_reached: 12,
        files_reached_reason: null,
        peak_memory_bytes: 104857600,
        peak_memory_bytes_reason: null,
        fixture_sources: [
          {
            name: "rxjs",
            repository: "https://github.com/ReactiveX/rxjs.git",
            ref: "rxjs-ref",
          },
        ],
        emit_status: "not in scope (noEmit project check)",
        dts_status: "not in scope (noEmit project check)",
      },
    },
    {
      name: "utility-types-project",
      lines: 1000,
      kb: 80,
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
      status: "compatibility metadata malformed",
      compatibility: {
        state: "green",
        exit_class: "exit success",
        first_failure_class: null,
        owner_track: null,
        phase: "check",
        last_successful_phase: "check",
        diagnostic_status: "none",
        diagnostic_deltas: [],
        diagnostic_subsystems: [],
        known_blockers: [],
        reduced_repro_path: null,
        repro: {},
        exit_codes: { tsc: [0], tsz: [0], tsgo: [0] },
        files_reached: 10,
        peak_memory_bytes: null,
        peak_memory_bytes_reason: "not measured on platform",
        fixture_sources: [],
        emit_status: "not in scope (noEmit project check)",
        dts_status: "not in scope (noEmit project check)",
      },
    },
    {
      name: "type-fest-project",
      lines: 1000,
      kb: 80,
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
      status: "compatibility metadata malformed",
      compatibility: {
        state: "green",
        exit_class: "exit success",
        first_failure_class: null,
        owner_track: null,
        phase: "check",
        last_successful_phase: "check",
        diagnostic_status: "none",
        diagnostic_deltas: [],
        diagnostic_subsystems: [],
        known_blockers: [],
        reduced_repro_path: null,
        repro: {},
        exit_codes: { tsc: [0], tsz: [0], tsgo: [0] },
        files_reached: 10,
        peak_memory_bytes: null,
        fixture_sources: [{}],
        emit_status: "not in scope (noEmit project check)",
        dts_status: "not in scope (noEmit project check)",
      },
    },
    {
      name: "zod-project",
      lines: 1000,
      kb: 80,
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
      status: "compatibility metadata malformed",
      compatibility: {
        state: "green",
        exit_class: "exit success",
        first_failure_class: null,
        owner_track: null,
        phase: "check",
        last_successful_phase: "check",
        diagnostic_status: "none",
        diagnostic_deltas: [],
        diagnostic_subsystems: [],
        known_blockers: [],
        reduced_repro_path: null,
        repro: {},
        exit_codes: { tsc: [0], tsz: [0], tsgo: [0] },
        files_reached: 10,
        peak_memory_bytes: null,
        fixture_sources: [
          {
            name: "zod",
            repository: "https://github.com/colinhacks/zod.git",
            ref: "",
          },
        ],
        emit_status: "not in scope (noEmit project check)",
        dts_status: "not in scope (noEmit project check)",
      },
    },
    {
      name: "large-ts-repo",
      lines: 1000000,
      kb: 80000,
      tsz_ms: 1000,
      tsgo_ms: 10,
      winner: "tsgo",
      factor: 100,
      status: null,
      compatibility: {
        state: "gray",
        exit_class: "oracle unavailable",
        first_failure_class: "tsc oracle unavailable",
        owner_track: "Track 1 tsc oracle evidence",
        phase: "oracle",
        last_successful_phase: null,
        diagnostic_status: "tsc oracle unavailable",
        diagnostic_deltas: ["tsc oracle was not collected for this project row"],
        diagnostic_subsystems: [],
        known_blockers: ["tsc oracle unavailable"],
        reduced_repro_path: null,
        repro: {},
        exit_codes: { tsc: [], tsz: [0], tsgo: [0] },
        files_reached: 6061,
        files_reached_reason: null,
        peak_memory_bytes: null,
        peak_memory_bytes_reason: "not measured on platform",
        fixture_sources: [
          {
            name: "large-ts-repo",
            repository: "https://github.com/mohsen1/large-ts-repo.git",
            ref: "large-ref",
          },
        ],
        emit_status: "not in scope (noEmit project check)",
        dts_status: "not in scope (noEmit project check)",
      },
    },
  ],
}, null, 2)}\n`, "utf8");

process.env.TSZ_WEBSITE_BENCHMARK_ARTIFACT = artifact;

try {
  const {
    getBenchmarkCharts,
    getBenchmarkEnvironmentSummary,
    getBenchmarkPages,
    getProjectCompatibilityDashboard,
  } = await import("../src/_data/benchmark_data.js");
  const envSummary = getBenchmarkEnvironmentSummary();
  // sha links to its GitHub commit; the generated timestamp is a <relative-time>.
  assert.match(
    envSummary,
    /sha <a href="https:\/\/github\.com\/tsz-org\/tsz\/commit\/0123456789ab[0-9a-f]*"[^>]*><code>0123456789ab<\/code><\/a>/,
  );
  assert.match(envSummary, /Generated <relative-time datetime="[^"]+"[^>]*>/);
  const pages = getBenchmarkPages();
  const fixturePage = pages.find((page) => page.name === "conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable.ts");
  assert.ok(fixturePage, "expected TypeScript fixture benchmark page");
  assert.equal(
    fixturePage.display_name,
    "Conditional Type Discriminating Large Union Regular Type Fetching",
  );
  assert.match(fixturePage.detail_focus, /large union/i);
  assert.equal(fixturePage.source_files.length, 1);
  assert.equal(fixturePage.source_files[0].name, "TypeScript/tests/cases/compiler/conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable.ts");
  assert.equal(fixturePage.source_files[0].source, fixtureSource);
  assert.equal(fixturePage.snippet, fixtureSource);

  const inferPage = pages.find((page) => page.name === "Infer stress N=15");
  assert.ok(inferPage, "expected generated infer benchmark page");
  assert.match(inferPage.source_files[0].source, /type ComplexInfer<T>/);
  assert.match(inferPage.detail_focus, /infer/i);

  const typeChallengesSolutionsPage = pages.find((page) => page.name === "type-challenges-solutions-project");
  assert.ok(typeChallengesSolutionsPage, "expected compile-canary type-challenges solutions page");
  assert.equal(typeChallengesSolutionsPage.failed, true);
  assert.match(typeChallengesSolutionsPage.status_label, /compile canary/i);

  const umamiPage = pages.find((page) => page.name === "umami-project");
  assert.ok(umamiPage, "expected compile-canary Umami application page");
  assert.equal(umamiPage.category, "Projects: applications");
  assert.equal(umamiPage.kind, "project");
  assert.equal(umamiPage.failed, true);
  assert.match(umamiPage.status_label, /compile canary/i);

  const valibotPage = pages.find((page) => page.name === "valibot-project");
  assert.ok(valibotPage, "expected compile-canary Valibot project page");
  assert.equal(valibotPage.category, "Projects: external libraries");
  assert.equal(valibotPage.kind, "project");
  assert.equal(valibotPage.source_files.length, 0);
  assert.match(valibotPage.detail_focus, /schema-validation library/i);

  const charts = getBenchmarkCharts();
  assert.match(charts, /External libraries/);
  assert.match(charts, /Utility types project/);
  // rxjs is a tsgo 3.0x win (tsz 3x slower), so it is NOT a timed chart bar; it
  // is surfaced in the "not charted" list with a slow label instead.
  assert.match(charts, /RxJS project/);
  assert.match(charts, /tsz 3\.0x slower than tsgo/);
  assert.doesNotMatch(charts, /tsgo 3\.0x faster/);
  assert.match(charts, /Not charted: canaries, incomplete, or tsz slower than tsgo/);
  assert.match(charts, /type-challenges solutions project/);
  // #16196: the ofetch timeout row carries finite timings and a non-error
  // winner, so only the structural `didNotFinish` guard keeps its ceiling/tsgo
  // ratio out of the chart. Its fabricated "125.0x faster" must never render...
  assert.doesNotMatch(charts, /125\.0x faster/);
  // ...and the row must still be surfaced (as DNF), not silently vanish.
  assert.match(charts, /Ofetch project/);
  assert.match(charts, /did not finish/);

  const compatibilityDashboard = getProjectCompatibilityDashboard();
  assert.match(compatibilityDashboard, /class="compat-table"/);
  assert.match(compatibilityDashboard, /data-compat-sort="exit"/);
  assert.match(compatibilityDashboard, /data-compat-sort="files"/);
  assert.match(compatibilityDashboard, /data-compat-sort="peak"/);
  assert.match(compatibilityDashboard, /leftRaw === "" \|\| !Number\.isFinite\(leftNumber\)/);
  assert.match(compatibilityDashboard, /utility-types[\s\S]*exit success/);
  assert.match(compatibilityDashboard, /utility-types[\s\S]*10 files/);
  assert.match(compatibilityDashboard, /utility-types[\s\S]*100 MiB peak/);
  // Ask 5 of #16310: the row's own source size (lines), distinct from the
  // "files reached" compile-progress column, is surfaced so a 1-file green
  // and a 1,000-line green do not read the same.
  assert.match(compatibilityDashboard, /data-compat-sort="size"/);
  assert.match(compatibilityDashboard, /utility-types[\s\S]*1,000 lines/);
  assert.match(compatibilityDashboard, /RxJS[\s\S]*12,000 lines/);
  // Ask 4 of #16311: a row measured against the no-install fixture model's
  // hand-written `declare module` shims loses coverage at its dependency
  // boundaries, so it is labeled distinctly from a row with real deps.
  assert.match(compatibilityDashboard, /data-compat-sort="stubs"/);
  assert.match(compatibilityDashboard, />MSW<[\s\S]*stubbed module[\s\S]*any member/);
  assert.match(compatibilityDashboard, /utility-types[\s\S]*no ambient stubs/);
  assert.match(compatibilityDashboard, /type-challenges solutions[\s\S]*compat-state green/);
  assert.match(compatibilityDashboard, /umami[\s\S]*compat-state green[\s\S]*204 files[\s\S]*200 MiB peak/);
  assert.doesNotMatch(compatibilityDashboard, /type-challenges assertions/);
  // Unmeasured (gray) rows are rendered explicitly as "Not measured" (#16310) so
  // shrinking coverage is visible instead of silently dropped, and the dashboard
  // publishes a coverage summary of measured-vs-defined rows.
  assert.match(compatibilityDashboard, /Not measured/);
  assert.match(compatibilityDashboard, /compat-state gray/);
  assert.match(compatibilityDashboard, /defined corpus rows measured/);

  process.env.TSZ_WEBSITE_BENCHMARK_ARTIFACT = failedOnlyArtifact;
  const failedOnlyCharts = getBenchmarkCharts();
  assert.doesNotMatch(failedOnlyCharts, /No benchmark data/i);
  assert.doesNotMatch(failedOnlyCharts, /No successful project benchmark timing pairs/);
  // large-ts-repo is a tsgo 100x win (tsz 100x slower): not a chart bar, listed
  // in the "not charted" section with a slow label.
  assert.match(failedOnlyCharts, /Large ts repo project/);
  assert.match(failedOnlyCharts, /tsz 100\.0x slower than tsgo/);
  assert.doesNotMatch(failedOnlyCharts, /tsgo 100\.0x faster/);
  assert.match(failedOnlyCharts, /Not charted: canaries, incomplete, or tsz slower than tsgo/);
  assert.match(failedOnlyCharts, /RxJS project/);
  const failedOnlyCompatibility = getProjectCompatibilityDashboard();
  assert.match(failedOnlyCompatibility, /data-compat-sort="project"/);
  assert.match(failedOnlyCompatibility, /data-compat-sort="state"/);
  assert.match(failedOnlyCompatibility, /data-compat-sort="exit"/);
  assert.match(failedOnlyCompatibility, /data-compat-sort="phase"/);
  assert.match(failedOnlyCompatibility, /data-compat-sort="files"/);
  assert.match(failedOnlyCompatibility, /data-compat-sort="peak"/);
  assert.match(failedOnlyCompatibility, /RxJS[\s\S]*compat-state yellow[\s\S]*diagnostic mismatch[\s\S]*12 files[\s\S]*100 MiB peak/);
  // Gray "oracle unavailable" / unmeasured rows (e.g. large-ts-repo) are now
  // rendered explicitly as "Not measured" rather than excluded (#16310).
  assert.match(failedOnlyCompatibility, /large-ts-repo/);
  assert.match(failedOnlyCompatibility, /Not measured/);
  assert.match(failedOnlyCompatibility, /compat-state gray/);
  assert.match(failedOnlyCompatibility, /utility-types[\s\S]*compat-state red[\s\S]*exit success[\s\S]*10 files[\s\S]*—/);
  // Size is a project-source-size fact independent of measurement state, so it
  // still renders for a gray/"Not measured" row instead of collapsing to "—".
  assert.match(failedOnlyCompatibility, /large-ts-repo[\s\S]*1,000,000 lines/);
  // A defined corpus row with no artifact record at all (e.g. "mitt", never
  // measured by either fixture) falls back to "—" rather than "0 lines".
  assert.match(failedOnlyCompatibility, /mitt[\s\S]*Not measured[\s\S]*—<\/td>/);
  assert.doesNotMatch(failedOnlyCompatibility, /mitt[\s\S]{0,200}0 lines/);

  const slugs = new Map();
  for (const page of pages) {
    assert.ok(page.detail_focus, `expected detail subtitle for ${page.name}`);
    assert.ok(!slugs.has(page.slug), `slug collision for ${page.slug}`);
    slugs.set(page.slug, page.name);
  }
} finally {
  delete process.env.TSZ_WEBSITE_BENCHMARK_ARTIFACT;
  await fs.rm(tmpDir, { recursive: true, force: true });
}
