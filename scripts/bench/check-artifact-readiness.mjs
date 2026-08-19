#!/usr/bin/env node
/**
 * Checks a merged bench artifact for required project-row completeness.
 *
 * Exit codes:
 *   0 — artifact present, all required rows included
 *   1 — artifact present, one or more required rows are missing, the required
 *       corpus collapsed (present but not one required-measured row is green, or
 *       fewer than the --require-corpus-health floor), the
 *       --require-green release gate found non-green required rows, the
 *       --require-clean-metadata gate found artifact metadata warnings,
 *       --require-application-compat found missing/incomplete benchmark_set:"required"
 *       application rows (canary application gaps are advisory, never blocking),
 *       --require-green-project-timing-pairs found green perf-timed rows
 *       missing tsz/tsgo timing pairs,
 *       --require-required-coverage found declared benchmark_set:"required" rows
 *       absent from the artifact,
 *       or the --require-source-current gate found a stale artifact source commit
 *   2 — artifact file absent or unparseable
 *
 * Without --json: writes a markdown report to stdout (and GITHUB_STEP_SUMMARY
 * when that env var is set).
 *
 * With --json: writes only JSON to stdout; markdown goes to stderr so that
 * `--json > out.json` is always clean. JSON is emitted even when the artifact
 * is absent (exit 2) so callers reliably get machine-readable status in all cases.
 *
 * Usage:
 *   node scripts/bench/check-artifact-readiness.mjs [--json] [--require-green] [--require-clean-metadata] [--require-application-compat] [--require-green-project-timing-pairs] [--require-required-coverage] [--require-corpus-health[=<minGreen>]] [--require-project-timing-pairs[=<n>]] [--expect-source-commit=<sha>] [--require-source-current] <artifact.json>
 */

import fs from "node:fs";
import { execFileSync } from "node:child_process";

import {
  REQUIRED_PROJECT_ROWS,
  PROJECT_ROW_DEFINITIONS,
  PROJECT_ROWS_BY_NAME,
  PERF_TIMED_PROJECT_ROWS,
} from "./project-rows.mjs";
import { BENCH_RUNNER_EXCLUDED_ROWS } from "./project-row-summary.mjs";
import {
  hasCompletePhaseMetadata,
  isGreen,
  isSpeedRatioEligible,
  missingDeclaredRows,
} from "./row-utils.mjs";
import { measurementProfileStatus } from "./measurement-profile.mjs";

// The bench artifact readiness gate must not demand rows that the bench runner
// never measures, or it fails every publish on a permanently-missing row. The
// never measures as a required row, or it fails every publish on a permanently
// missing/optional row. Rows in BENCH_RUNNER_EXCLUDED_ROWS are never measured;
// application rows may be timed by an optional shard but must not be required,
// because their package-manager installs are intentionally outside the public
// publish completeness gate. Examples that broke publishing:
// type-challenges-solutions-project (excluded-set, #13549) and
// infisical/payload/medusa (category:application, promoted to benchmark_set
// "required" by #13775). Only rows that are required AND part of the required
// timing shards belong in the readiness required-set.
//
// RUNTIME_GATED_REQUIRED_ROWS is a distinct, narrower exclusion: rows that
// ARE present in bench-vs-tsgo.sh (so they must stay out of
// BENCH_RUNNER_EXCLUDED_ROWS, whose structural-presence contract is enforced
// by test-project-rows.mjs) but whose runner function only executes behind a
// runtime kill-switch. `nextjs` (`run_nextjs_benchmarks`) only runs when
// NEXTJS_BENCHMARK_ENABLED=1 or an explicit --filter reaches it — a
// kill-switch for an unstable sparse fixture — so the daily scheduled run
// (no filter) never produces a result for it. Treating it as
// required-and-measured made it permanently "missing" and tripped this
// gate's unconditional missing-row check on every scheduled run, so
// bench.yml's readiness step never reported ready=true (#17561).
export const RUNTIME_GATED_REQUIRED_ROWS = new Set(["nextjs"]);

const REQUIRED_MEASURED_ROWS = REQUIRED_PROJECT_ROWS.filter(
  (name) =>
    !BENCH_RUNNER_EXCLUDED_ROWS.has(name) &&
    !RUNTIME_GATED_REQUIRED_ROWS.has(name) &&
    PROJECT_ROWS_BY_NAME[name]?.category !== "application",
);
const REQUIRED_MEASURED_ROW_SET = new Set(REQUIRED_MEASURED_ROWS);

// Corpus-collapse floor (#17561, point 4). The readiness gate historically
// blocked only on *missing* required rows, so a run where every required row is
// PRESENT but errored (state "red"/"yellow"/"gray") passed as ready — a whole
// all-`error` corpus published reading "ok" (2026-08-15, a fixture tag-object
// pin bug erroring every fetched row). This floor closes that hole: a
// publishable dataset must contain at least one fully green required-measured
// row. It is unconditional (like the missing-row and duplicate-row guards) and
// freeze-proof — the trivially-compiling utility and locally-generated rows
// (utility-types, vite-vanilla-ts-app, nextjs-fresh-app, ...) always land green
// in any non-catastrophic run, so this never fires in steady state where some
// external rows legitimately error behind open compiler issues (#16055 zod,
// #14101 large-ts-repo). --require-corpus-health=<n> raises the floor above 1
// for a caller that wants to block a partial collapse too; that threshold is an
// outward-facing policy choice left to the caller rather than baked in here.
const DEFAULT_MIN_GREEN_CORPUS_FLOOR = 1;

const APPLICATION_PROJECT_ROWS = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.category === "application")
  .map((row) => row.name);

// A green perf-timed row missing its tsz/tsgo timing pair only blocks the public
// publish when the row is part of the required publish completeness set. Every
// `perf_timed` row today is a canary/advisory shard (external libraries and
// application canaries) that is intentionally outside that gate (see the
// REQUIRED_MEASURED_ROWS note above and the optional-shard contract in
// project-rows.mjs). A canary perf benchmark legitimately errors or is skipped
// from time to time; treating that as publish-blocking froze the whole
// benchmark site for ~half a day when one green canary application row
// (infisical) produced no timing pair. #15004 demoted that single row to
// perf_timed:false; this guard generalizes the rule so any flaky canary timing
// pair is advisory, while a future required perf-timed row still blocks.
function isBlockingTimedRow(name) {
  return REQUIRED_MEASURED_ROW_SET.has(name);
}

// Application compatibility rows are real apps cloned + installed by the
// optional, best-effort bench-applications shard (their compat lands from that
// shard or, as a fallback, a matching-CI compat artifact). BOTH sources can
// legitimately be absent for a given run: a flaky package-manager install, or a
// workflow_dispatch Bench with no matching main CI push run — the exact failure
// that froze the public site when infisical's only compat source vanished
// (#15004 dropped infisical from the perf-timed shard, so it stopped being
// benched, and that run's matching-CI compat was absent, leaving the row
// entirely missing). Mirror the perf-timing split (isBlockingTimedRow): a
// canary application row with missing/incomplete/duplicate compat is advisory
// (reported, never publish-blocking), while a future benchmark_set:"required"
// application row still blocks the publish.
function isBlockingApplicationRow(name) {
  return PROJECT_ROWS_BY_NAME[name]?.benchmark_set === "required";
}

const args = process.argv.slice(2);

function parseArgs(rawArgs) {
  const options = {
    jsonOutput: false,
    requireGreen: false,
    requireCleanMetadata: false,
    requireApplicationCompat: false,
    requireGreenProjectTimingPairs: false,
    requireRequiredCoverage: false,
    requireSourceCurrent: false,
    // The zero-green collapse floor is always enforced (DEFAULT_MIN_GREEN_CORPUS_FLOOR);
    // --require-corpus-health[=<n>] only ever raises it, never lowers it.
    corpusHealthMinGreen: DEFAULT_MIN_GREEN_CORPUS_FLOOR,
    requiredProjectTimingPairs: 0,
    expectedSourceCommit: process.env.TSZ_BENCH_EXPECT_SOURCE_COMMIT ?? null,
    filePath: null,
  };

  for (let i = 0; i < rawArgs.length; i += 1) {
    const arg = rawArgs[i];
    if (arg === "--json") {
      options.jsonOutput = true;
    } else if (arg === "--require-green") {
      options.requireGreen = true;
    } else if (arg === "--require-clean-metadata") {
      options.requireCleanMetadata = true;
    } else if (arg === "--require-application-compat") {
      options.requireApplicationCompat = true;
    } else if (arg === "--require-green-project-timing-pairs") {
      options.requireGreenProjectTimingPairs = true;
    } else if (arg === "--require-required-coverage") {
      options.requireRequiredCoverage = true;
    } else if (arg === "--require-corpus-health") {
      // Bare flag keeps the default floor; a following integer raises it. The
      // floor can only be raised, never lowered (Math.max), so the unconditional
      // zero-green collapse guard always holds.
      const next = rawArgs[i + 1] ?? "";
      if (/^\d+$/.test(next)) {
        options.corpusHealthMinGreen = Math.max(DEFAULT_MIN_GREEN_CORPUS_FLOOR, Number(next));
        i += 1;
      }
    } else if (arg.startsWith("--require-corpus-health=")) {
      const value = Number(arg.slice("--require-corpus-health=".length));
      if (Number.isFinite(value)) {
        options.corpusHealthMinGreen = Math.max(DEFAULT_MIN_GREEN_CORPUS_FLOOR, value);
      }
    } else if (arg === "--require-source-current") {
      options.requireSourceCurrent = true;
    } else if (arg === "--require-project-timing-pairs") {
      const next = rawArgs[i + 1] ?? "";
      if (/^\d+$/.test(next)) {
        options.requiredProjectTimingPairs = Number(next);
        i += 1;
      } else {
        options.requiredProjectTimingPairs = 1;
      }
    } else if (arg.startsWith("--require-project-timing-pairs=")) {
      options.requiredProjectTimingPairs = Number(arg.slice("--require-project-timing-pairs=".length));
    } else if (arg === "--expect-source-commit") {
      options.expectedSourceCommit = rawArgs[i + 1] ?? "";
      i += 1;
    } else if (arg.startsWith("--expect-source-commit=")) {
      options.expectedSourceCommit = arg.slice("--expect-source-commit=".length);
    } else if (!arg.startsWith("-") && !options.filePath) {
      options.filePath = arg;
    }
  }

  return options;
}

const {
  jsonOutput,
  requireGreen,
  requireCleanMetadata,
  requireApplicationCompat,
  requireGreenProjectTimingPairs,
  requireRequiredCoverage,
  requireSourceCurrent,
  corpusHealthMinGreen,
  requiredProjectTimingPairs: rawRequiredProjectTimingPairs,
  expectedSourceCommit: rawExpectedSourceCommit,
  filePath,
} = parseArgs(args);

const requiredProjectTimingPairs = Number.isFinite(rawRequiredProjectTimingPairs)
  ? Math.max(0, rawRequiredProjectTimingPairs)
  : 0;

function currentGitHead() {
  try {
    return execFileSync("git", ["rev-parse", "HEAD"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
  } catch {
    return null;
  }
}

const expectedSourceCommit =
  rawExpectedSourceCommit ?? (requireSourceCurrent ? currentGitHead() : null);

function loadArtifact() {
  if (!filePath) {
    return { artifact: null, artifactAbsent: true, parseError: null };
  }
  let raw;
  try {
    raw = fs.readFileSync(filePath, "utf8");
  } catch {
    return { artifact: null, artifactAbsent: true, parseError: null };
  }
  try {
    return { artifact: JSON.parse(raw), artifactAbsent: false, parseError: null };
  } catch (err) {
    return { artifact: null, artifactAbsent: false, parseError: String(err.message) };
  }
}

function rowState(row, duplicate = false) {
  if (!row) return "missing";
  if (duplicate) return "gray";
  if (row.artifact_missing === true) return "gray";
  const compat = row.compatibility;
  if (!compat) return "gray";
  if (!hasCompletePhaseMetadata(compat)) return "gray";
  if (row.status) return "red";
  if (isGreen(row)) return "green";
  if (compat.state === "yellow" || compat.state === "red") return compat.state;
  return "gray";
}

function applicationCompatibilityState(row, duplicate = false) {
  if (!row) return "missing";
  if (duplicate) return "gray";
  if (row.artifact_missing === true) return "gray";
  const compat = row.compatibility;
  if (!compat) return "gray";
  if (!hasCompletePhaseMetadata(compat)) return "gray";
  if (compat.state === "green" || compat.state === "yellow" || compat.state === "red") {
    return compat.state;
  }
  return "gray";
}

const STATE_ICON = { green: "✅", yellow: "⚠️", red: "❌", gray: "⬜", missing: "🚫" };

// Shared bench gate (`row-utils.mjs`). This readiness check previously omitted
// the did-not-finish guard, so a row killed at the ceiling with a finite
// sentinel timing could count here while the website/README dropped it — the
// exact drift #17302 fixes. `isSpeedRatioEligible` adds that guard (#16196).
function hasSuccessfulTimingPair(row) {
  return isSpeedRatioEligible(row);
}

function hasGreenCompatibilityEvidence(row) {
  const compatibility = row?.compatibility;
  if (!compatibility || typeof compatibility !== "object") return false;
  const state = String(compatibility.state || "").toLowerCase();
  const exitClass = String(compatibility.exit_class || "").toLowerCase();
  const diagnosticStatus = String(compatibility.diagnostic_status || "").toLowerCase();
  return state === "green"
    && exitClass === "exit success"
    && (!diagnosticStatus || diagnosticStatus === "none")
    && hasCompletePhaseMetadata(compatibility);
}

function cleanValidationWarnings(warnings) {
  if (!Array.isArray(warnings)) return [];
  return warnings
    .filter((warning) => warning && typeof warning === "object" && !Array.isArray(warning))
    .map((warning) => ({
      file: typeof warning.file === "string" && warning.file.trim() ? warning.file.trim() : null,
      mismatched_fields: Array.isArray(warning.mismatched_fields)
        ? warning.mismatched_fields
          .filter((field) => typeof field === "string" && field.trim())
          .map((field) => field.trim())
        : [],
      expected: warning.expected ?? null,
      actual: warning.actual ?? null,
    }));
}

function analyzeValidationWarnings(artifact) {
  const validation = artifact?.validation && typeof artifact.validation === "object"
    ? artifact.validation
    : {};
  const runnerEnvironment = cleanValidationWarnings(validation.runner_environment_warnings);
  const measurementProfile = cleanValidationWarnings(validation.measurement_profile_warnings);
  return {
    runner_environment: runnerEnvironment,
    measurement_profile: measurementProfile,
    total: runnerEnvironment.length + measurementProfile.length,
  };
}

function normalizedCommit(value) {
  const commit = String(value || "").trim().toLowerCase();
  return /^[0-9a-f]{7,40}$/.test(commit) ? commit : null;
}

function commitsMatch(left, right) {
  const a = normalizedCommit(left);
  const b = normalizedCommit(right);
  if (!a || !b) return false;
  return a.startsWith(b) || b.startsWith(a);
}

function shortCommit(value) {
  const commit = String(value || "").trim();
  return commit && commit !== "local" ? commit.slice(0, 12) : commit || null;
}

function analyzeSourceFreshness(artifact, expectedCommit) {
  const expected = normalizedCommit(expectedCommit);
  const source = normalizedCommit(artifact?.source_commit);
  if (!expected) {
    return {
      expected_source_commit: null,
      source_commit: artifact?.source_commit ?? null,
      current: null,
      warning: null,
    };
  }
  if (!source) {
    return {
      expected_source_commit: expected,
      source_commit: artifact?.source_commit ?? null,
      current: false,
      warning: `source_commit missing; expected ${shortCommit(expected)}`,
    };
  }
  if (commitsMatch(source, expected)) {
    return {
      expected_source_commit: expected,
      source_commit: source,
      current: true,
      warning: null,
    };
  }
  return {
    expected_source_commit: expected,
    source_commit: source,
    current: false,
    warning: `source ${shortCommit(source)} differs from expected ${shortCommit(expected)}`,
  };
}

// Corpus-health summary over the required-measured rows (#17561, point 4).
// `collapsed` is the always-enforced floor: present rows, but not one is green.
// The counts are surfaced in the JSON/markdown so a consumer can see a degraded
// (but not collapsed) dataset — e.g. several external rows erroring behind open
// compiler issues — even when it still publishes. `byState` is the state
// partition analyzeArtifact already computes, reused here rather than re-scanned.
function buildCorpusHealth(measured, byState) {
  return {
    measured,
    green: byState.green.length,
    errored: byState.red.length,
    yellow: byState.yellow.length,
    gray: byState.gray.length,
    missing: byState.missing.length,
    errored_rows: byState.red.map((r) => r.name),
    collapsed: measured > 0 && byState.green.length === 0,
  };
}

function analyzeArtifact(artifact, expectedCommit) {
  const byName = new Map();
  const duplicateCounts = new Map();
  for (const row of Array.isArray(artifact?.results) ? artifact.results : []) {
    const name = row?.name;
    if (typeof name !== "string") continue;
    if (byName.has(name)) {
      duplicateCounts.set(name, (duplicateCounts.get(name) ?? 1) + 1);
    } else {
      byName.set(name, row);
    }
  }

  const compatibilityRow = (name, stateFn) => {
    const row = byName.get(name) ?? null;
    const duplicateCount = duplicateCounts.get(name) ?? (row ? 1 : 0);
    const duplicate = duplicateCount > 1;
    const state = stateFn(row, duplicate);
    const def = PROJECT_ROWS_BY_NAME[name];
    const compatibility = row?.compatibility ?? {};
    const metadataComplete = (
      row != null &&
      duplicate !== true &&
      row.artifact_missing !== true &&
      row.compatibility != null &&
      hasCompletePhaseMetadata(row.compatibility)
    );
    return {
      name,
      label: def?.label ?? name,
      state,
      metadata_complete: metadataComplete,
      duplicate_count: duplicateCount,
      tsz_ms: row?.tsz_ms ?? null,
      tsgo_ms: row?.tsgo_ms ?? null,
      winner: row?.winner ?? null,
      exit_class: duplicate ? "duplicate row" : compatibility.exit_class ?? null,
      phase: compatibility.phase ?? null,
      last_successful_phase: compatibility.last_successful_phase ?? null,
      first_failure_class: duplicate
        ? `${duplicateCount} entries found`
        : compatibility.first_failure_class ?? null,
      owner_family: compatibility.semantic_owner_family ?? compatibility.owner_family ?? null,
      known_blockers: duplicate
        ? ["duplicate project row"]
        : Array.isArray(compatibility.known_blockers)
        ? compatibility.known_blockers.filter(Boolean).slice(0, 8)
        : [],
      diagnostic_status: compatibility.diagnostic_status ?? null,
      files_reached: compatibility.files_reached ?? null,
      files_reached_reason: compatibility.files_reached_reason ?? null,
      peak_memory_bytes: compatibility.peak_memory_bytes ?? null,
      peak_memory_bytes_reason: compatibility.peak_memory_bytes_reason ?? null,
    };
  };

  const rows = REQUIRED_MEASURED_ROWS.map((name) => compatibilityRow(name, rowState));
  const applicationRows = APPLICATION_PROJECT_ROWS.map((name) =>
    compatibilityRow(name, applicationCompatibilityState),
  );
  const greenTimedRowsMissingTimingPairs = PERF_TIMED_PROJECT_ROWS
    .map((name) => compatibilityRow(name, applicationCompatibilityState))
    .filter((row) => {
      const sourceRow = byName.get(row.name);
      return row.duplicate_count <= 1 &&
        hasGreenCompatibilityEvidence(sourceRow) &&
        !hasSuccessfulTimingPair(sourceRow);
    });
  const blockingTimedRowsMissingTimingPairs = greenTimedRowsMissingTimingPairs.filter(
    (row) => isBlockingTimedRow(row.name),
  );
  const advisoryTimedRowsMissingTimingPairs = greenTimedRowsMissingTimingPairs.filter(
    (row) => !isBlockingTimedRow(row.name),
  );

  const applicationMissing = applicationRows.filter((r) => r.state === "missing");
  const applicationIncomplete = applicationRows.filter((r) => (
    r.state !== "missing" &&
    r.duplicate_count <= 1 &&
    r.metadata_complete !== true
  ));
  const applicationDuplicates = applicationRows.filter((r) => r.duplicate_count > 1);
  // Every application-compat gap, tagged with the kind of gap, then split into
  // publish-blocking (benchmark_set:"required") and advisory (canary) buckets so
  // a flaky canary app install can never freeze the public benchmark site while
  // a required application row still gates the publish.
  const applicationGaps = [
    ...applicationMissing.map((r) => ({ ...r, gap_kind: "missing" })),
    ...applicationIncomplete.map((r) => ({ ...r, gap_kind: "incomplete" })),
    ...applicationDuplicates.map((r) => ({ ...r, gap_kind: "duplicate" })),
  ];
  const blockingApplicationGaps = applicationGaps.filter((r) => isBlockingApplicationRow(r.name));
  const advisoryApplicationGaps = applicationGaps.filter((r) => !isBlockingApplicationRow(r.name));

  // Required-set coverage (#17025): every declared benchmark_set:"required" row
  // that carries no result row at all. This is a PRESENCE check over the full
  // required set, strictly wider than REQUIRED_MEASURED_ROWS (which drops the
  // never-timed excluded/application rows): a required row can stop being
  // measured indefinitely — no timing shard, no compile-guard compat — and pass
  // the shard-count gate as a `completed` run with a smaller results array. The
  // artifact's own run_status is surfaced alongside so a merge-stamped `partial`
  // is visible in the readiness report.
  // Recomputed from the persisted results rather than trusting the merge-written
  // validation.missing_required_rows: the readiness gate independently re-judges
  // an arbitrary artifact (as it does for every other row signal) so a stale or
  // wrong-version merge cannot self-certify. No all-absent guard here — unlike
  // the per-shard merge, readiness always sees the final merged artifact, so an
  // empty/degenerate one legitimately reports the whole required set absent.
  const missingRequiredCoverageRows = missingDeclaredRows(REQUIRED_PROJECT_ROWS, artifact?.results);
  const requiredCoverage = {
    declared: REQUIRED_PROJECT_ROWS.length,
    present: REQUIRED_PROJECT_ROWS.length - missingRequiredCoverageRows.length,
    missing: missingRequiredCoverageRows.length,
    missing_rows: missingRequiredCoverageRows,
    run_status: artifact?.run_status ?? null,
  };

  // Partition the required-measured rows by state once; both the corpus-health
  // summary and the state buckets below read from it.
  const byState = {
    missing: rows.filter((r) => r.state === "missing"),
    red: rows.filter((r) => r.state === "red"),
    yellow: rows.filter((r) => r.state === "yellow"),
    gray: rows.filter((r) => r.state === "gray"),
    green: rows.filter((r) => r.state === "green"),
  };

  return {
    requiredCoverage,
    corpusHealth: buildCorpusHealth(rows.length, byState),
    measurementProfile: measurementProfileStatus(artifact),
    validationWarnings: analyzeValidationWarnings(artifact),
    sourceFreshness: analyzeSourceFreshness(artifact, expectedCommit),
    rows,
    applicationRows,
    applicationMissing,
    applicationIncomplete,
    applicationDuplicates,
    applicationGaps,
    blockingApplicationGaps,
    advisoryApplicationGaps,
    greenTimedRowsMissingTimingPairs,
    blockingTimedRowsMissingTimingPairs,
    advisoryTimedRowsMissingTimingPairs,
    successfulProjectTimingPairs: byState.green.filter((row) => hasSuccessfulTimingPair(row)),
    missing: byState.missing,
    red: byState.red,
    yellow: byState.yellow,
    gray: byState.gray,
    green: byState.green,
    duplicates: rows.filter((r) => r.duplicate_count > 1),
  };
}

function uniqueRowsByName(rows) {
  const byName = new Map();
  for (const row of rows) {
    if (!row?.name || byName.has(row.name)) continue;
    byName.set(row.name, row);
  }
  return [...byName.values()];
}

function buildJson({
  artifactAbsent,
  parseError,
  artifact,
  requiredCoverage,
  corpusHealth,
  measurementProfile,
  validationWarnings,
  sourceFreshness,
  rows,
  applicationRows,
  applicationMissing,
  applicationIncomplete,
  applicationDuplicates,
  blockingApplicationGaps,
  advisoryApplicationGaps,
  greenTimedRowsMissingTimingPairs,
  blockingTimedRowsMissingTimingPairs,
  advisoryTimedRowsMissingTimingPairs,
  successfulProjectTimingPairs,
  missing,
  red,
  yellow,
  gray,
  green,
  duplicates,
}) {
  const missingNames = missing?.map((r) => r.name) ?? REQUIRED_MEASURED_ROWS;
  const metadataWarningsList = metadataWarnings(measurementProfile, validationWarnings);
  const nonGreenRows = rows
    ? uniqueRowsByName([
      ...(red ?? []),
      ...(yellow ?? []),
      ...(gray ?? []),
      ...(missing ?? []),
      ...(duplicates ?? []),
    ])
    : missingNames.map((name) => ({ name, state: "missing" }));
  return {
    artifact_absent: artifactAbsent,
    parse_error: parseError ?? null,
    source_commit: artifact?.source_commit ?? null,
    generated_at: artifact?.generated_at ?? null,
    workflow_run_url: artifact?.workflow_run_url ?? null,
    measurement_profile: measurementProfile ?? null,
    source_freshness: sourceFreshness ?? {
      expected_source_commit: normalizedCommit(expectedSourceCommit),
      source_commit: artifact?.source_commit ?? null,
      current: null,
      warning: null,
    },
    validation_warnings: validationWarnings ?? {
      runner_environment: [],
      measurement_profile: [],
      total: 0,
    },
    metadata_clean: metadataWarningsList.length === 0,
    metadata_warnings_total: metadataWarningsList.length,
    // Full declared benchmark_set:"required" presence coverage (#17025), wider
    // than the required-timing row set below. `run_status` echoes the artifact's
    // own merge-stamped status (`partial` when the merge saw the same gap). Null
    // only on the artifact-absent path, alongside `measurement_profile` below.
    required_coverage: requiredCoverage ?? null,
    // Corpus-health verdict (#17561, point 4): the required-measured green/errored
    // split, so a consumer can see a degraded dataset and the gh-pages publish
    // gate can refuse a collapsed one (corpus_health.collapsed). Null only on the
    // artifact-absent path.
    corpus_health: corpusHealth ?? null,
    required_row_count: rows?.length ?? REQUIRED_MEASURED_ROWS.length,
    successful_project_timing_pairs: successfulProjectTimingPairs?.length ?? 0,
    required_project_timing_pairs: requiredProjectTimingPairs,
    require_green_project_timing_pairs: requireGreenProjectTimingPairs,
    green_project_timing_pair_gaps: greenTimedRowsMissingTimingPairs?.length ?? 0,
    green_project_timing_pair_gap_rows: greenTimedRowsMissingTimingPairs?.map((r) => ({
      name: r.name,
      label: r.label,
      state: r.state,
      tsz_ms: r.tsz_ms,
      tsgo_ms: r.tsgo_ms,
      blocking: isBlockingTimedRow(r.name),
    })) ?? [],
    // Only required perf-timed rows missing a timing pair block the publish;
    // canary/advisory perf-timed rows are reported but never freeze latest.json.
    blocking_project_timing_pair_gaps: blockingTimedRowsMissingTimingPairs?.length ?? 0,
    blocking_project_timing_pair_gap_rows: blockingTimedRowsMissingTimingPairs?.map((r) => ({
      name: r.name,
      label: r.label,
      state: r.state,
      tsz_ms: r.tsz_ms,
      tsgo_ms: r.tsgo_ms,
    })) ?? [],
    advisory_project_timing_pair_gaps: advisoryTimedRowsMissingTimingPairs?.length ?? 0,
    // Only benchmark_set:"required" application rows missing/incomplete compat
    // block the publish; canary application gaps are reported but never freeze
    // latest.json. gh-pages.yml gates on blocking_application_compatibility_gaps.
    blocking_application_compatibility_gaps: blockingApplicationGaps?.length ?? 0,
    advisory_application_compatibility_gaps: advisoryApplicationGaps?.length ?? 0,
    application_compatibility: applicationRows
      ? {
          required: requireApplicationCompat,
          row_count: applicationRows.length,
          present: applicationRows.length - (applicationMissing?.length ?? 0),
          complete: applicationRows.length - (applicationMissing?.length ?? 0) - (applicationIncomplete?.length ?? 0),
          missing: applicationMissing?.length ?? 0,
          incomplete: applicationIncomplete?.length ?? 0,
          duplicates: applicationDuplicates?.length ?? 0,
          blocking_gaps: blockingApplicationGaps?.length ?? 0,
          advisory_gaps: advisoryApplicationGaps?.length ?? 0,
          missing_rows: applicationMissing?.map((r) => r.name) ?? [],
          incomplete_rows: applicationIncomplete?.map((r) => ({ name: r.name, state: r.state })) ?? [],
          duplicate_rows: applicationDuplicates?.map((r) => ({ name: r.name, count: r.duplicate_count })) ?? [],
          blocking_gap_rows: blockingApplicationGaps?.map((r) => ({ name: r.name, state: r.state, gap_kind: r.gap_kind })) ?? [],
          advisory_gap_rows: advisoryApplicationGaps?.map((r) => ({ name: r.name, state: r.state, gap_kind: r.gap_kind })) ?? [],
        }
      : null,
    green: green?.length ?? 0,
    yellow: yellow?.length ?? 0,
    red: red?.length ?? 0,
    gray: gray?.length ?? 0,
    missing: missingNames.length,
    missing_rows: missingNames,
    duplicate_rows: duplicates?.map((r) => ({ name: r.name, count: r.duplicate_count })) ?? [],
    all_required_rows_green: rows
      ? nonGreenRows.length === 0 && green?.length === rows.length
      : false,
    non_green_required_rows: nonGreenRows.map((r) => ({
      name: r.name,
      state: r.state,
    })),
    red_rows: red?.map((r) => r.name) ?? [],
    yellow_rows: yellow?.map((r) => r.name) ?? [],
    rows: rows?.map((r) => ({
      name: r.name,
      label: r.label,
      state: r.state,
      duplicate_count: r.duplicate_count,
      tsz_ms: r.tsz_ms,
      tsgo_ms: r.tsgo_ms,
      winner: r.winner,
      exit_class: r.exit_class,
      phase: r.phase,
      last_successful_phase: r.last_successful_phase,
      first_failure_class: r.first_failure_class,
      owner_family: r.owner_family,
      known_blockers: r.known_blockers,
      diagnostic_status: r.diagnostic_status,
      files_reached: r.files_reached,
      files_reached_reason: r.files_reached_reason,
      peak_memory_bytes: r.peak_memory_bytes,
      peak_memory_bytes_reason: r.peak_memory_bytes_reason,
    })) ?? [],
  };
}

function fmtMs(ms) {
  if (ms == null) return "—";
  return `${Number(ms).toFixed(0)} ms`;
}

function mdCell(value) {
  return String(value ?? "—").replace(/\|/g, "\\|").replace(/\r?\n/g, " ");
}

function fmtFilesReached(value, reason) {
  if (Number.isFinite(Number(value))) return String(Number(value));
  return reason ? `n/a (${reason})` : "—";
}

function fmtPeakMemory(value, reason) {
  if (Number.isFinite(Number(value))) {
    return `${(Number(value) / (1024 * 1024)).toFixed(1)} MiB`;
  }
  return reason ? `n/a (${reason})` : "—";
}

function fmtWarningFields(warning) {
  return warning.mismatched_fields.length > 0
    ? warning.mismatched_fields.join(", ")
    : "metadata mismatch";
}

function measurementProfileReportWarnings(profile, validationWarnings) {
  const warnings = [];
  if (profile?.warning) {
    warnings.push({
      file: "artifact measurement_profile",
      message: profile.warning,
    });
  }
  for (const warning of validationWarnings.measurement_profile) {
    warnings.push({
      file: warning.file ?? "unknown input",
      message: fmtWarningFields(warning),
    });
  }
  return warnings;
}

function metadataWarnings(profile, validationWarnings) {
  const runnerWarnings = (validationWarnings?.runner_environment ?? []).map((warning) => ({
    kind: "runner metadata",
    file: warning.file ?? "unknown input",
    message: fmtWarningFields(warning),
  }));
  const measurementWarnings = measurementProfileReportWarnings(
    profile ?? { warning: "measurement_profile missing" },
    validationWarnings ?? { measurement_profile: [] },
  ).map((warning) => ({
    kind: "measurement profile",
    file: warning.file,
    message: warning.message,
  }));
  return [...runnerWarnings, ...measurementWarnings];
}

function artifactAge(generatedAt) {
  if (!generatedAt) return "unknown age";
  const h = Math.round((Date.now() - new Date(generatedAt).getTime()) / 3_600_000);
  if (h < 1) return "< 1 h ago";
  if (h === 1) return "1 h ago";
  return `${h} h ago`;
}

function buildReport({
  artifact,
  requiredCoverage,
  corpusHealth,
  measurementProfile,
  validationWarnings,
  sourceFreshness,
  rows,
  applicationRows,
  applicationMissing,
  applicationIncomplete,
  applicationDuplicates,
  blockingApplicationGaps,
  advisoryApplicationGaps,
  greenTimedRowsMissingTimingPairs,
  blockingTimedRowsMissingTimingPairs,
  advisoryTimedRowsMissingTimingPairs,
  successfulProjectTimingPairs,
  missing,
  red,
  yellow,
  gray,
  green,
  duplicates,
}) {
  const sourceCommit = artifact?.source_commit?.slice(0, 10) ?? "unknown";
  const generatedAt = artifact?.generated_at ?? null;
  const workflowUrl = artifact?.workflow_run_url ?? null;
  const profile = measurementProfile ?? measurementProfileStatus(artifact);
  const profileLabel = profile.present
    ? `${profile.mode ?? "unknown"}${profile.warning ? ` (${profile.warning})` : ""}`
    : profile.warning;
  const measurementWarnings = measurementProfileReportWarnings(profile, validationWarnings);
  const sourceFreshnessLabel = sourceFreshness?.expected_source_commit
    ? sourceFreshness.current === true
      ? `current for ${shortCommit(sourceFreshness.expected_source_commit)}`
      : sourceFreshness.warning
    : "not checked";

  const lines = [
    `## Benchmark artifact readiness — ${new Date().toUTCString()}`,
    "",
    "| Field | Value |",
    "|-------|-------|",
    `| Artifact SHA | \`${sourceCommit}\` |`,
    `| Source freshness | ${sourceFreshnessLabel ?? "not checked"} |`,
    `| Generated | ${generatedAt ?? "—"} (${artifactAge(generatedAt)}) |`,
    `| Workflow run | ${workflowUrl ? `[link](${workflowUrl})` : "—"} |`,
    `| Measurement profile | ${profileLabel} |`,
    `| PGO profile | ${profile.profile_fingerprint ? `\`${profile.profile_fingerprint.slice(0, 12)}\`` : "—"} |`,
    `| PGO training | ${profile.training_fingerprint ? `\`${profile.training_fingerprint.slice(0, 12)}\`` : "—"} |`,
    `| Binary target CPU | ${profile.rust_target_cpu ? `\`${profile.rust_target_cpu}\`` : "—"} |`,
    `| Required rows | ${rows.length} |`,
    `| Required-set coverage | ${requiredCoverage.present}/${requiredCoverage.declared} declared present${requiredCoverage.missing ? ` (${requiredCoverage.missing} absent)` : ""}${requiredCoverage.run_status ? ` · run status: ${requiredCoverage.run_status}` : ""} |`,
    `| Corpus health | ${corpusHealth.green}/${corpusHealth.measured} required-measured green${corpusHealth.errored ? `, ${corpusHealth.errored} errored` : ""}${corpusHealth.collapsed ? " · ⛔ COLLAPSED (0 green)" : ""} |`,
    `| Successful project timing pairs | ${successfulProjectTimingPairs.length} |`,
    `| Green perf-timed rows missing timing | ${greenTimedRowsMissingTimingPairs.length} |`,
    `| Application compatibility rows | ${applicationRows.length - applicationMissing.length}/${applicationRows.length} present, ${applicationIncomplete.length} incomplete |`,
    `| ✅ green | ${green.length} |`,
    `| ⚠️ yellow | ${yellow.length} |`,
    `| ❌ red | ${red.length} |`,
    `| ⬜ gray | ${gray.length} |`,
    `| 🚫 missing | ${missing.length} |`,
    `| Duplicate rows | ${duplicates.length} |`,
    `| Runner metadata warnings | ${validationWarnings.runner_environment.length} |`,
    `| Measurement profile warnings | ${measurementWarnings.length} |`,
    "",
  ];

  if (missing.length > 0) {
    lines.push(`### 🚫 Missing required rows (${missing.length})`, "");
    for (const r of missing) lines.push(`- \`${r.name}\``);
    lines.push("");
  }

  if (corpusHealth.collapsed || corpusHealth.errored > 0) {
    lines.push(
      ...(corpusHealth.collapsed
        ? [
            `### ⛔ Required corpus collapsed (0/${corpusHealth.measured} green)`,
            "",
            "Every required-measured row is present but non-green — a publishable dataset must carry at least one green required row. This is the whole-corpus failure that let an all-`error` run publish reading \"ok\" (#17561).",
          ]
        : [
            `### ❌ Errored required rows (${corpusHealth.errored})`,
            "",
            `${corpusHealth.green}/${corpusHealth.measured} required-measured rows are green; the following errored (advisory unless below the --require-corpus-health floor):`,
          ]),
      "",
    );
    for (const name of corpusHealth.errored_rows) lines.push(`- \`${name}\``);
    lines.push("");
  }

  if (requiredCoverage.missing > 0) {
    lines.push(
      `### 📉 Required-set coverage gap (${requiredCoverage.missing})`,
      "",
      `Declared \`benchmark_set:"required"\` rows with no result row in the artifact — never measured by any shard, so "the benchmark is green" excludes them (#17025):`,
      "",
    );
    for (const name of requiredCoverage.missing_rows) lines.push(`- \`${name}\``);
    lines.push("");
  }

  if (duplicates.length > 0) {
    lines.push(`### ⬜ Duplicate required rows (${duplicates.length})`, "");
    for (const r of duplicates) lines.push(`- \`${r.name}\` appears ${r.duplicate_count} times`);
    lines.push("");
  }

  const blockingAppGaps = blockingApplicationGaps ?? [];
  const advisoryAppGaps = advisoryApplicationGaps ?? [];
  const gapText = (r) =>
    r.gap_kind === "missing"
      ? "missing compatibility row"
      : r.gap_kind === "duplicate"
        ? `duplicate compatibility row (${r.duplicate_count})`
        : "incomplete compatibility metadata";
  if (blockingAppGaps.length > 0) {
    lines.push(`### 🚫 Required application compatibility gaps (${blockingAppGaps.length})`, "");
    for (const r of blockingAppGaps) lines.push(`- \`${r.name}\`: ${gapText(r)} (blocks publish)`);
    lines.push("");
  }
  if (advisoryAppGaps.length > 0) {
    lines.push(`### Canary application compatibility gaps (advisory, ${advisoryAppGaps.length})`, "");
    for (const r of advisoryAppGaps) {
      lines.push(`- \`${r.name}\`: ${gapText(r)}; reported only, never blocks publish`);
    }
    lines.push("");
  }

  const blockingGaps = blockingTimedRowsMissingTimingPairs ?? [];
  const advisoryGaps = advisoryTimedRowsMissingTimingPairs ?? [];
  if (blockingGaps.length > 0) {
    lines.push(`### 🚫 Required perf-timed rows missing timing (${blockingGaps.length})`, "");
    for (const r of blockingGaps) {
      lines.push(`- \`${r.name}\`: required row is green but no tsz/tsgo timing pair was recorded (blocks publish)`);
    }
    lines.push("");
  }
  if (advisoryGaps.length > 0) {
    lines.push(`### Canary perf-timed rows missing timing (advisory, ${advisoryGaps.length})`, "");
    for (const r of advisoryGaps) {
      lines.push(`- \`${r.name}\`: green compat but no tsz/tsgo timing pair; charted only when timed, never blocks publish`);
    }
    lines.push("");
  }

  if (validationWarnings.runner_environment.length > 0) {
    lines.push(`### Runner metadata warnings (${validationWarnings.runner_environment.length})`, "");
    for (const warning of validationWarnings.runner_environment) {
      lines.push(`- \`${mdCell(warning.file ?? "unknown input")}\`: ${mdCell(fmtWarningFields(warning))}`);
    }
    lines.push("");
  }

  if (measurementWarnings.length > 0) {
    lines.push(`### Measurement profile warnings (${measurementWarnings.length})`, "");
    for (const warning of measurementWarnings) {
      lines.push(`- \`${mdCell(warning.file)}\`: ${mdCell(warning.message)}`);
    }
    lines.push("");
  }

  lines.push("### All required rows", "");
  lines.push("| State | Row | tsz | tsgo | Winner | Exit | Phase | Last phase | Files | Peak RSS | Failure | Blocker family | Diagnostics |");
  lines.push("|:-----:|-----|----:|----:|--------|------|-------|------------|------:|---------:|---------|----------------|-------------|");
  for (const r of rows) {
    const icon = STATE_ICON[r.state] ?? "?";
    const blockerFamily = r.known_blockers?.[0] ?? r.first_failure_class ?? r.owner_family ?? "—";
    lines.push(
      `| ${icon} | \`${mdCell(r.label)}\` | ${fmtMs(r.tsz_ms)} | ${fmtMs(r.tsgo_ms)} | ${mdCell(r.winner)} | ${mdCell(r.exit_class)} | ${mdCell(r.phase)} | ${mdCell(r.last_successful_phase)} | ${mdCell(fmtFilesReached(r.files_reached, r.files_reached_reason))} | ${mdCell(fmtPeakMemory(r.peak_memory_bytes, r.peak_memory_bytes_reason))} | ${mdCell(r.first_failure_class)} | ${mdCell(blockerFamily)} | ${mdCell(r.diagnostic_status)} |`,
    );
  }

  return lines.join("\n");
}

function buildAbsentReport(parseError) {
  const header = `## Benchmark artifact readiness — ${new Date().toUTCString()}`;
  if (parseError) {
    return `${header}\n\n> ❌ **Artifact present but could not be parsed:** ${parseError}\n`;
  }
  return (
    `${header}\n\n` +
    `> 🚫 **Artifact missing** — no bench-results-merged artifact found for latest main.\n` +
    `>\n` +
    `> bench.yml did not complete successfully for the current main SHA,\n` +
    `> or the artifact has expired (30-day retention window).\n`
  );
}

function writeReport(text) {
  if (jsonOutput) {
    process.stderr.write(text + "\n");
  } else {
    process.stdout.write(text + "\n");
  }
  const summaryFile = process.env.GITHUB_STEP_SUMMARY;
  if (summaryFile) {
    try {
      fs.appendFileSync(summaryFile, text + "\n");
    } catch (err) {
      process.stderr.write(`warn: could not write GITHUB_STEP_SUMMARY: ${err.message}\n`);
    }
  }
}

const { artifact, artifactAbsent, parseError } = loadArtifact();

if (artifactAbsent || parseError) {
  writeReport(buildAbsentReport(parseError));
  if (jsonOutput) {
    process.stdout.write(
      JSON.stringify(buildJson({
        artifactAbsent: true,
        parseError,
        artifact: null,
        requiredCoverage: null,
        corpusHealth: null,
        measurementProfile: null,
        sourceFreshness: null,
        rows: null,
        applicationRows: null,
        applicationMissing: null,
        applicationIncomplete: null,
        applicationDuplicates: null,
        blockingApplicationGaps: null,
        advisoryApplicationGaps: null,
        greenTimedRowsMissingTimingPairs: null,
        blockingTimedRowsMissingTimingPairs: null,
        advisoryTimedRowsMissingTimingPairs: null,
        missing: null,
        red: null,
        yellow: null,
        gray: null,
        green: null,
        duplicates: null,
        successfulProjectTimingPairs: null,
      })) + "\n",
    );
  }
  process.exit(2);
}

const analysis = analyzeArtifact(artifact, expectedSourceCommit);
const {
  requiredCoverage,
  corpusHealth,
  measurementProfile,
  validationWarnings,
  sourceFreshness,
  rows,
  applicationRows,
  applicationMissing,
  applicationIncomplete,
  applicationDuplicates,
  blockingApplicationGaps,
  advisoryApplicationGaps,
  greenTimedRowsMissingTimingPairs,
  blockingTimedRowsMissingTimingPairs,
  advisoryTimedRowsMissingTimingPairs,
  successfulProjectTimingPairs,
  missing,
  red,
  yellow,
  gray,
  green,
  duplicates,
} = analysis;

writeReport(buildReport({ artifact, ...analysis }));

if (jsonOutput) {
  process.stdout.write(
    JSON.stringify(buildJson({
      artifactAbsent: false,
      parseError: null,
      artifact,
      requiredCoverage,
      corpusHealth,
      measurementProfile,
      validationWarnings,
      sourceFreshness,
      rows,
      applicationRows,
      applicationMissing,
      applicationIncomplete,
      applicationDuplicates,
      blockingApplicationGaps,
      advisoryApplicationGaps,
      greenTimedRowsMissingTimingPairs,
      blockingTimedRowsMissingTimingPairs,
      advisoryTimedRowsMissingTimingPairs,
      successfulProjectTimingPairs,
      missing,
      red,
      yellow,
      gray,
      green,
      duplicates,
    })) + "\n",
  );
}

if (missing.length > 0 || duplicates.length > 0) {
  if (duplicates.length > 0) {
    process.stderr.write(
      `bench-artifact-readiness: ${duplicates.length} required row(s) duplicated in artifact: ` +
        duplicates.map((r) => `${r.name} (${r.duplicate_count})`).join(", ") + "\n",
    );
  }
  if (missing.length > 0) {
    process.stderr.write(
      `bench-artifact-readiness: ${missing.length} required row(s) missing from artifact: ` +
        missing.map((r) => r.name).join(", ") + "\n",
    );
  }
  process.exit(1);
}

// Corpus-collapse floor (#17561, point 4): unconditional, in the same class as
// the missing-row guard above. A required-measured corpus that is PRESENT but
// carries fewer green rows than the floor (default 1) is structurally
// unpublishable — the all-`error` run that read "ok" on 2026-08-15, where every
// required row errored on a fixture-pin bug yet nothing blocked because the gate
// only counted *missing* rows. --require-corpus-health=<n> raises the floor
// above 1; the default floor never fires in a healthy run (see the constant).
if (corpusHealth.measured > 0 && corpusHealth.green < corpusHealthMinGreen) {
  const detail = corpusHealth.errored_rows.length > 0
    ? ` (errored: ${corpusHealth.errored_rows.join(", ")})`
    : "";
  process.stderr.write(
    `bench-artifact-readiness: required corpus health below floor — ` +
      `${corpusHealth.green}/${corpusHealth.measured} required-measured row(s) green, ` +
      `floor ${corpusHealthMinGreen}${detail}\n`,
  );
  process.exit(1);
}

if (requireApplicationCompat && advisoryApplicationGaps.length > 0) {
  // Canary application rows (every category:"application" row today) are real
  // apps installed by the optional best-effort bench-applications shard; a
  // flaky install or an absent matching-CI compat artifact legitimately leaves
  // one missing/incomplete. Surface it but never block the publish — exactly as
  // a missing canary perf-timing pair is advisory. A single such gap (infisical)
  // froze the public benchmark site; this keeps latest.json flowing.
  process.stderr.write(
    `::warning::bench-artifact-readiness: ${advisoryApplicationGaps.length} canary application ` +
      `compatibility gap(s) (advisory, not blocking publish): ` +
      advisoryApplicationGaps.map((r) => `${r.name} (${r.gap_kind})`).join(", ") + "\n",
  );
}

if (requireApplicationCompat && blockingApplicationGaps.length > 0) {
  process.stderr.write(
    `bench-artifact-readiness: application compatibility incomplete for ` +
      `${blockingApplicationGaps.length} required row(s): ` +
      blockingApplicationGaps.map((r) => `${r.name} (${r.gap_kind})`).join(", ") + "\n",
  );
  process.exit(1);
}

if (requireGreen && (red.length > 0 || yellow.length > 0 || gray.length > 0)) {
  const nonGreen = [...red, ...yellow, ...gray];
  process.stderr.write(
    `bench-artifact-readiness: ${nonGreen.length} required row(s) are not green: ` +
      nonGreen.map((r) => `${r.name} (${r.state})`).join(", ") + "\n",
  );
  process.exit(1);
}

if (requireCleanMetadata) {
  const warnings = metadataWarnings(measurementProfile, validationWarnings);
  if (warnings.length > 0) {
    process.stderr.write(
      `bench-artifact-readiness: ${warnings.length} metadata warning(s) present: ` +
        warnings.map((warning) => `${warning.kind} ${warning.file}: ${warning.message}`).join("; ") + "\n",
    );
    process.exit(1);
  }
}

if (requireSourceCurrent && sourceFreshness.current !== true) {
  process.stderr.write(
    `bench-artifact-readiness: source freshness failed: ${sourceFreshness.warning ?? "no expected source commit provided"}\n`,
  );
  process.exit(1);
}

if (successfulProjectTimingPairs.length < requiredProjectTimingPairs) {
  process.stderr.write(
    `bench-artifact-readiness: ${successfulProjectTimingPairs.length} successful project timing pair(s); ` +
      `required ${requiredProjectTimingPairs} before publishing latest benchmark data\n`,
  );
  process.exit(1);
}

if (requireGreenProjectTimingPairs && advisoryTimedRowsMissingTimingPairs.length > 0) {
  // Canary/advisory perf-timed rows missing a timing pair are surfaced but never
  // block the publish: the website only charts a perf-timed row once it is both
  // green and timed, so a missing canary pair simply omits that row from the
  // chart. A single such gap previously froze the whole benchmark site for half
  // a day; this keeps the latest.json publish flowing.
  process.stderr.write(
    `::warning::bench-artifact-readiness: ${advisoryTimedRowsMissingTimingPairs.length} canary perf-timed project row(s) ` +
      `missing tsz/tsgo timing pairs (advisory, not blocking publish): ` +
      `${advisoryTimedRowsMissingTimingPairs.map((r) => r.name).join(", ")}\n`,
  );
}

if (requireGreenProjectTimingPairs && blockingTimedRowsMissingTimingPairs.length > 0) {
  process.stderr.write(
    `bench-artifact-readiness: ${blockingTimedRowsMissingTimingPairs.length} required perf-timed project row(s) ` +
      `missing tsz/tsgo timing pairs: ${blockingTimedRowsMissingTimingPairs.map((r) => r.name).join(", ")}\n`,
  );
  process.exit(1);
}

// Required-set coverage is always surfaced (report + JSON) and the merge step
// already downgrades run_status to `partial`, but blocking on it is opt-in:
// today `nextjs` and `type-challenges-solutions-project` have no measurement
// path, so a default-on gate would freeze every publish (the #14398/#15004
// anti-freeze contract). A caller that has given the full required set a
// measurement path can pass --require-required-coverage to enforce it.
if (requireRequiredCoverage && requiredCoverage.missing > 0) {
  process.stderr.write(
    `bench-artifact-readiness: ${requiredCoverage.missing} declared benchmark_set:"required" row(s) ` +
      `absent from artifact: ${requiredCoverage.missing_rows.join(", ")}\n`,
  );
  process.exit(1);
}

process.exit(0);
