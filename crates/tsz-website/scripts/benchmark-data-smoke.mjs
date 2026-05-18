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
    },
    {
      name: "type-challenges-assertion-candidates",
      lines: 78,
      kb: 8,
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
      status: "diagnostic mismatch",
      compatibility: {
        exit_class: "diagnostic mismatch",
        phase: "assertion-classification",
        last_successful_phase: null,
        diagnostic_status: "tsz rejects tsc-accepted assertion candidates",
        diagnostic_deltas: ["tsz: assertions/two.ts(2,3): error TS2589: deep"],
        diagnostic_subsystems: [
          {
            subsystem: "type-challenges recursive conditionals",
            codes: ["TS2589"],
            count: 1,
            examples: [],
          },
        ],
        known_blockers: ["tsz rejects tsc-accepted assertion candidates"],
        exit_codes: { tsc: [0], tsz: [1], tsgo: [] },
        files_reached: 78,
        peak_memory_bytes: null,
        fixture_sources: [
          {
            name: "type-challenges",
            repository: "https://github.com/type-challenges/type-challenges.git",
            ref: "type-ref",
          },
          {
            name: "type-challenges-solutions",
            repository: "https://github.com/ghaiklor/type-challenges-solutions.git",
            ref: "solutions-ref",
          },
        ],
        emit_status: "not in scope (noEmit assertion check)",
        dts_status: "not in scope (noEmit assertion check)",
        assertion_candidates: {
          sources: {
            templates: { repository: "type", ref: "type-ref" },
            testCases: { repository: "type", ref: "type-ref" },
            solutions: { repository: "solutions", ref: "solutions-ref" },
          },
          paired_solutions: 78,
          generated_assertions: 78,
          assertions_referencing_solution_declaration: 76,
          assertions_missing_solution_declaration_reference: 2,
          tsc_diagnostic_free: 10,
          tsc_with_diagnostics: 68,
          tsz_diagnostic_free: 7,
          diagnostic_free_candidate_delta: -3,
          both_accepted: 5,
          both_rejected: 60,
          tsc_accepted_tsz_rejected: 3,
          tsc_rejected_tsz_accepted: 2,
          tsc_clean_subset: {
            manifest_path:
              "type-challenges-assertions-tsc-clean/type-challenges-assertions-tsc-clean-manifest.json",
            classification_path:
              "type-challenges-assertions-tsc-clean/type-challenges-assertions-tsc-clean-classification.json",
            tsconfig_path: "type-challenges-assertions-tsc-clean/tsconfig.tsz-guard.json",
            total_candidates: 78,
            generated_assertions: 10,
            assertions_referencing_solution_declaration: 9,
            assertions_missing_solution_declaration_reference: 1,
            rejected_from_full_corpus: 68,
            tsc_status: "pass",
            tsz_status: "fail",
            comparison_status: "tsz-rejects-tsc-accepted",
            tsc_diagnostic_free: 10,
            tsz_diagnostic_free: 7,
          },
          file_comparison: {
            counts: {
              bothAccepted: 5,
              bothRejected: 60,
              tscAcceptedTszRejected: 3,
              tscRejectedTszAccepted: 2,
            },
          },
          diagnostic_candidate_examples: [
            {
              compiler: "tsz",
              file: "type-challenges-assertions/assertions/two.ts",
              candidate_id: "00002-medium-recursive",
              codes: ["TS2589"],
            },
          ],
        },
      },
    },
    {
      name: "type-challenges-assertions-tsc-clean",
      lines: 10,
      kb: 2,
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
      status: "compile canary tracked in CI; not timed by vs-tsgo benchmarks",
      compatibility: {
        exit_class: "exit success",
        phase: "check",
        last_successful_phase: "check",
        diagnostic_status: "none",
        diagnostic_deltas: [],
        diagnostic_subsystems: [],
        known_blockers: [],
        exit_codes: { tsc: [0], tsz: [0], tsgo: [] },
        files_reached: 10,
        peak_memory_bytes: null,
        fixture_sources: [
          {
            name: "type-challenges",
            repository: "https://github.com/type-challenges/type-challenges.git",
            ref: "type-ref",
          },
          {
            name: "type-challenges-solutions",
            repository: "https://github.com/ghaiklor/type-challenges-solutions.git",
            ref: "solutions-ref",
          },
        ],
        emit_status: "not in scope (noEmit assertion check)",
        dts_status: "not in scope (noEmit assertion check)",
        assertion_clean_subset: {
          manifest_path:
            "type-challenges-assertions-tsc-clean/type-challenges-assertions-tsc-clean-manifest.json",
          classification_path:
            "type-challenges-assertions-tsc-clean/type-challenges-assertions-tsc-clean-classification.json",
          total_candidates: 78,
          generated_assertions: 10,
          assertions_referencing_solution_declaration: 9,
          assertions_missing_solution_declaration_reference: 1,
          rejected_from_full_corpus: 68,
          tsc_status: "pass",
          tsz_status: "pass",
          comparison_status: "both-pass",
          tsc_diagnostic_free: 10,
          tsz_diagnostic_free: 10,
        },
      },
    },
  ],
}, null, 2)}\n`, "utf8");

await fs.writeFile(failedOnlyArtifact, `${JSON.stringify({
  generated_at: "2026-05-16T00:00:00.000Z",
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
        peak_memory_bytes: 104857600,
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
  ],
}, null, 2)}\n`, "utf8");

process.env.TSZ_WEBSITE_BENCHMARK_ARTIFACT = artifact;

try {
  const {
    getBenchmarkCharts,
    getBenchmarkPages,
    getProjectCompatibilityDashboard,
  } = await import("../src/_data/benchmark_data.js");
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

  const typeChallengesPage = pages.find((page) => page.name === "type-challenges-project");
  assert.ok(typeChallengesPage, "expected compile-canary type-challenges page");
  assert.equal(typeChallengesPage.failed, true);
  assert.match(typeChallengesPage.status_label, /compile canary/i);

  const typeChallengesSolutionsPage = pages.find((page) => page.name === "type-challenges-solutions-project");
  assert.ok(typeChallengesSolutionsPage, "expected compile-canary type-challenges solutions page");
  assert.equal(typeChallengesSolutionsPage.failed, true);
  assert.match(typeChallengesSolutionsPage.status_label, /compile canary/i);

  const typeChallengesAssertionPage = pages.find((page) => page.name === "type-challenges-assertion-candidates");
  assert.ok(typeChallengesAssertionPage, "expected type-challenges assertion candidates page");
  assert.equal(
    typeChallengesAssertionPage.display_name,
    "type-challenges assertion candidates",
  );
  assert.equal(typeChallengesAssertionPage.failed, true);
  assert.match(typeChallengesAssertionPage.status_label, /diagnostic mismatch/i);

  const typeChallengesCleanPage = pages.find((page) => page.name === "type-challenges-assertions-tsc-clean");
  assert.ok(typeChallengesCleanPage, "expected compile-canary type-challenges tsc-clean assertions page");
  assert.equal(
    typeChallengesCleanPage.display_name,
    "type-challenges tsc-clean assertions",
  );
  assert.equal(typeChallengesCleanPage.failed, true);
  assert.match(typeChallengesCleanPage.status_label, /compile canary/i);

  const charts = getBenchmarkCharts();
  assert.match(charts, /External libraries/);
  assert.match(charts, /Compile canaries and incomplete project timings/);
  assert.match(charts, /type-challenges project/);
  assert.match(charts, /type-challenges solutions project/);
  assert.match(charts, /type-challenges assertion candidates/);
  assert.match(charts, /type-challenges tsc-clean assertions/);

  const compatibilityDashboard = getProjectCompatibilityDashboard();
  assert.match(compatibilityDashboard, /type-challenges assertions/);
  assert.match(compatibilityDashboard, /paired solutions: 78/);
  assert.match(compatibilityDashboard, /assertions generated: 78/);
  assert.match(compatibilityDashboard, /assertions referencing solutions: 76/);
  assert.match(compatibilityDashboard, /assertions missing solution references: 2/);
  assert.match(compatibilityDashboard, /templates ref: type-ref/);
  assert.match(compatibilityDashboard, /test cases ref: type-ref/);
  assert.match(compatibilityDashboard, /solutions ref: solutions-ref/);
  assert.match(compatibilityDashboard, /source: type-challenges @ type-ref/);
  assert.match(compatibilityDashboard, /source: type-challenges-solutions @ solutions-ref/);
  assert.match(compatibilityDashboard, /tsc clean: 10/);
  assert.match(compatibilityDashboard, /tsz clean: 7/);
  assert.match(
    compatibilityDashboard,
    /tsc-clean manifest: type-challenges-assertions-tsc-clean\/type-challenges-assertions-tsc-clean-manifest\.json/,
  );
  assert.match(
    compatibilityDashboard,
    /tsc-clean classification: type-challenges-assertions-tsc-clean\/type-challenges-assertions-tsc-clean-classification\.json/,
  );
  assert.match(
    compatibilityDashboard,
    /tsc-clean tsconfig: type-challenges-assertions-tsc-clean\/tsconfig\.tsz-guard\.json/,
  );
  assert.match(compatibilityDashboard, /tsc-clean total candidates: 78/);
  assert.match(compatibilityDashboard, /tsc-clean subset: 10/);
  assert.match(compatibilityDashboard, /tsc-clean rejected: 68/);
  assert.match(compatibilityDashboard, /tsc-clean tsc: pass/);
  assert.match(compatibilityDashboard, /tsc-clean tsz: fail/);
  assert.match(compatibilityDashboard, /tsc-clean comparison: tsz-rejects-tsc-accepted/);
  assert.match(compatibilityDashboard, /tsc-clean tsc diagnostic-free: 10/);
  assert.match(compatibilityDashboard, /tsc-clean tsz diagnostic-free: 7/);
  assert.match(
    compatibilityDashboard,
    /tsc-clean manifest: type-challenges-assertions-tsc-clean\/type-challenges-assertions-tsc-clean-manifest\.json/,
  );
  assert.match(
    compatibilityDashboard,
    /tsc-clean classification: type-challenges-assertions-tsc-clean\/type-challenges-assertions-tsc-clean-classification\.json/,
  );
  assert.match(compatibilityDashboard, /tsc-clean total candidates: 78/);
  assert.match(compatibilityDashboard, /tsc-clean subset: 10/);
  assert.match(compatibilityDashboard, /tsc-clean references solutions: 9/);
  assert.match(compatibilityDashboard, /tsc-clean rejected: 68/);
  assert.match(compatibilityDashboard, /tsc-clean tsc: pass/);
  assert.match(compatibilityDashboard, /tsc-clean tsz: pass/);
  assert.match(compatibilityDashboard, /tsc-clean comparison: both-pass/);
  assert.match(compatibilityDashboard, /tsc-clean tsc diagnostic-free: 10/);
  assert.match(compatibilityDashboard, /tsc-clean tsz diagnostic-free: 10/);
  assert.match(compatibilityDashboard, /both accepted: 5/);
  assert.match(compatibilityDashboard, /both rejected: 60/);
  assert.match(compatibilityDashboard, /tsc accepted\/tsz rejected: 3/);
  assert.match(compatibilityDashboard, /tsc rejected\/tsz accepted: 2/);
  assert.match(
    compatibilityDashboard,
    /tsz: TS2589 type-challenges-assertions\/assertions\/two\.ts/,
  );

  process.env.TSZ_WEBSITE_BENCHMARK_ARTIFACT = failedOnlyArtifact;
  const failedOnlyCharts = getBenchmarkCharts();
  assert.doesNotMatch(failedOnlyCharts, /No benchmark data/i);
  assert.match(failedOnlyCharts, /Compile canaries and incomplete project timings/);
  assert.match(failedOnlyCharts, /RxJS project/);
  const failedOnlyCompatibility = getProjectCompatibilityDashboard();
  assert.match(failedOnlyCompatibility, /artifact: complete/);
  assert.match(failedOnlyCompatibility, /failure: relations-assignability/);
  assert.match(failedOnlyCompatibility, /owner track: Track 4 relation diagnostics\/compatibility/);
  assert.match(failedOnlyCompatibility, /repro: src\/operators\/map\.ts/);
  assert.match(failedOnlyCompatibility, /source: rxjs @ rxjs-ref/);
  assert.equal(
    [...failedOnlyCompatibility.matchAll(/fixture sources missing\/malformed\/unpinned/g)].length,
    3,
  );

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
