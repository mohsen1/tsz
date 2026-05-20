import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "tsz-benchmark-data-"));
const artifact = path.join(tmpDir, "bench-vs-tsgo-test.json");
const failedOnlyArtifact = path.join(tmpDir, "bench-vs-tsgo-failed-only.json");

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
  workflow_run_url: "https://github.com/mohsen1/tsz/actions/runs/1001",
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
        generated_at: "2026-05-16T00:00:00.000Z",
        source_commit: "local",
        workflow_name: "Bench",
        workflow_run_id: "1001",
        workflow_run_url: "https://github.com/mohsen1/tsz/actions/runs/1001",
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
        workflow_run_url: "https://github.com/mohsen1/tsz/actions/runs/1001",
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
  ],
}, null, 2)}\n`, "utf8");

await fs.writeFile(failedOnlyArtifact, `${JSON.stringify({
  generated_at: "2026-05-16T00:00:00.000Z",
  source_commit: "local",
  workflow_name: "Bench",
  workflow_run_id: "1002",
  workflow_run_url: "https://github.com/mohsen1/tsz/actions/runs/1002",
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
  assert.match(getBenchmarkEnvironmentSummary(), /sha 0123456789ab/);
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

  const charts = getBenchmarkCharts();
  assert.match(charts, /External libraries/);
  assert.match(charts, /Utility types project/);
  assert.match(charts, /Compile canaries and incomplete project timings/);
  assert.match(charts, /type-challenges solutions project/);

  const compatibilityDashboard = getProjectCompatibilityDashboard();
  assert.match(compatibilityDashboard, /class="compat-table"/);
  assert.match(compatibilityDashboard, /data-compat-sort="exit"/);
  assert.match(compatibilityDashboard, /data-compat-sort="files"/);
  assert.match(compatibilityDashboard, /data-compat-sort="peak"/);
  assert.match(compatibilityDashboard, /leftRaw === "" \|\| !Number\.isFinite\(leftNumber\)/);
  assert.match(compatibilityDashboard, /utility-types[\s\S]*exit success/);
  assert.match(compatibilityDashboard, /utility-types[\s\S]*10 files/);
  assert.match(compatibilityDashboard, /utility-types[\s\S]*100 MiB peak/);
  assert.match(compatibilityDashboard, /type-challenges solutions[\s\S]*compat-state green/);
  assert.doesNotMatch(compatibilityDashboard, /type-challenges assertions/);

  process.env.TSZ_WEBSITE_BENCHMARK_ARTIFACT = failedOnlyArtifact;
  const failedOnlyCharts = getBenchmarkCharts();
  assert.doesNotMatch(failedOnlyCharts, /No benchmark data/i);
  assert.doesNotMatch(failedOnlyCharts, /No successful project benchmark timing pairs/);
  assert.match(failedOnlyCharts, /Large repositories/);
  assert.match(failedOnlyCharts, /Large ts repo project/);
  assert.match(failedOnlyCharts, /tsgo 100\.0x faster/);
  assert.match(failedOnlyCharts, /Compile canaries and incomplete project timings/);
  assert.match(failedOnlyCharts, /RxJS project/);
  const failedOnlyCompatibility = getProjectCompatibilityDashboard();
  assert.match(failedOnlyCompatibility, /artifact: complete/);
  assert.match(failedOnlyCompatibility, /failure: relations-assignability/);
  assert.match(failedOnlyCompatibility, /owner track: Track 4 relation diagnostics\/compatibility/);
  assert.match(failedOnlyCompatibility, /repro: src\/operators\/map\.ts/);
  assert.match(failedOnlyCompatibility, /source: rxjs @ rxjs-ref/);
  assert.match(failedOnlyCompatibility, /run: 1002 attempt 2 \(cancelled\)/);
  assert.match(failedOnlyCompatibility, /freshness warning: older than latest completed bench run 1003/);
  assert.match(failedOnlyCompatibility, /freshness warning: older than 2026-05-17T00:00:00Z bench artifact/);
  assert.match(failedOnlyCompatibility, /freshness warning: run status: cancelled/);
  assert.match(failedOnlyCompatibility, /failure: tsc oracle unavailable/);
  assert.match(failedOnlyCompatibility, /owner track: Track 1 tsc oracle evidence/);
  assert.match(failedOnlyCompatibility, /source: large-ts-repo @ large-ref/);
  assert.match(failedOnlyCompatibility, /owner track: Tracks 1, 2, 5/);
  assert.match(failedOnlyCompatibility, /compatibility metadata malformed/);
  assert.match(failedOnlyCompatibility, /owner family: mapped\/conditional\/key-space utility surface/);
  assert.equal(
    [...failedOnlyCompatibility.matchAll(/fixture sources missing\/malformed\/unpinned/g)].length,
    3,
  );
  // utility-types-project has peak_memory_bytes: null with a reason; the row
  // must surface "peak RSS: n/a (not measured on platform)" so the residency
  // gap is triageable from the dashboard rather than appearing as a blank.
  assert.match(failedOnlyCompatibility, /peak RSS: n\/a \(not measured on platform\)/);

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
