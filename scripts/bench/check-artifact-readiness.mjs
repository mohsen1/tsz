#!/usr/bin/env node
/**
 * Checks a merged bench artifact for required project-row completeness.
 *
 * Exit codes:
 *   0 — artifact present, all required rows included
 *   1 — artifact present, one or more required rows are missing, the
 *       --require-green release gate found non-green required rows, the
 *       --require-clean-metadata gate found artifact metadata warnings,
 *       --require-application-compat found missing/incomplete application rows,
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
 *   node scripts/bench/check-artifact-readiness.mjs [--json] [--require-green] [--require-clean-metadata] [--require-application-compat] [--require-project-timing-pairs[=<n>]] [--expect-source-commit=<sha>] [--require-source-current] <artifact.json>
 */

import fs from "node:fs";
import { execFileSync } from "node:child_process";

import {
  REQUIRED_PROJECT_ROWS,
  PROJECT_ROW_DEFINITIONS,
  PROJECT_ROWS_BY_NAME,
} from "./project-rows.mjs";
import { BENCH_RUNNER_EXCLUDED_ROWS } from "./project-row-summary.mjs";
import {
  hasCompletePhaseMetadata,
  isGreen,
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
const REQUIRED_MEASURED_ROWS = REQUIRED_PROJECT_ROWS.filter(
  (name) =>
    !BENCH_RUNNER_EXCLUDED_ROWS.has(name) &&
    PROJECT_ROWS_BY_NAME[name]?.category !== "application",
);
const APPLICATION_PROJECT_ROWS = PROJECT_ROW_DEFINITIONS
  .filter((row) => row.category === "application")
  .map((row) => row.name);

const args = process.argv.slice(2);

function parseArgs(rawArgs) {
  const options = {
    jsonOutput: false,
    requireGreen: false,
    requireCleanMetadata: false,
    requireApplicationCompat: false,
    requireSourceCurrent: false,
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
  requireSourceCurrent,
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

  return {
    measurementProfile: measurementProfileStatus(artifact),
    validationWarnings: analyzeValidationWarnings(artifact),
    sourceFreshness: analyzeSourceFreshness(artifact, expectedCommit),
    rows,
    applicationRows,
    applicationMissing: applicationRows.filter((r) => r.state === "missing"),
    applicationIncomplete: applicationRows.filter((r) => (
      r.state !== "missing" &&
      r.duplicate_count <= 1 &&
      r.metadata_complete !== true
    )),
    applicationDuplicates: applicationRows.filter((r) => r.duplicate_count > 1),
    successfulProjectTimingPairs: rows.filter((row) => (
      row.state === "green" &&
      Number.isFinite(Number(row.tsz_ms)) &&
      Number(row.tsz_ms) > 0 &&
      Number.isFinite(Number(row.tsgo_ms)) &&
      Number(row.tsgo_ms) > 0 &&
      row.winner !== "error"
    )),
    missing: rows.filter((r) => r.state === "missing"),
    red: rows.filter((r) => r.state === "red"),
    yellow: rows.filter((r) => r.state === "yellow"),
    gray: rows.filter((r) => r.state === "gray"),
    green: rows.filter((r) => r.state === "green"),
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
  measurementProfile,
  validationWarnings,
  sourceFreshness,
  rows,
  applicationRows,
  applicationMissing,
  applicationIncomplete,
  applicationDuplicates,
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
    required_row_count: rows?.length ?? REQUIRED_MEASURED_ROWS.length,
    successful_project_timing_pairs: successfulProjectTimingPairs?.length ?? 0,
    required_project_timing_pairs: requiredProjectTimingPairs,
    application_compatibility: applicationRows
      ? {
          required: requireApplicationCompat,
          row_count: applicationRows.length,
          present: applicationRows.length - (applicationMissing?.length ?? 0),
          complete: applicationRows.length - (applicationMissing?.length ?? 0) - (applicationIncomplete?.length ?? 0),
          missing: applicationMissing?.length ?? 0,
          incomplete: applicationIncomplete?.length ?? 0,
          duplicates: applicationDuplicates?.length ?? 0,
          missing_rows: applicationMissing?.map((r) => r.name) ?? [],
          incomplete_rows: applicationIncomplete?.map((r) => ({ name: r.name, state: r.state })) ?? [],
          duplicate_rows: applicationDuplicates?.map((r) => ({ name: r.name, count: r.duplicate_count })) ?? [],
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
  measurementProfile,
  validationWarnings,
  sourceFreshness,
  rows,
  applicationRows,
  applicationMissing,
  applicationIncomplete,
  applicationDuplicates,
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
    `| Successful project timing pairs | ${successfulProjectTimingPairs.length} |`,
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

  if (duplicates.length > 0) {
    lines.push(`### ⬜ Duplicate required rows (${duplicates.length})`, "");
    for (const r of duplicates) lines.push(`- \`${r.name}\` appears ${r.duplicate_count} times`);
    lines.push("");
  }

  if (applicationMissing.length > 0 || applicationIncomplete.length > 0 || applicationDuplicates.length > 0) {
    lines.push(
      `### Application compatibility gaps (${applicationMissing.length + applicationIncomplete.length + applicationDuplicates.length})`,
      "",
    );
    for (const r of applicationMissing) lines.push(`- \`${r.name}\`: missing compatibility row`);
    for (const r of applicationIncomplete) lines.push(`- \`${r.name}\`: incomplete compatibility metadata`);
    for (const r of applicationDuplicates) lines.push(`- \`${r.name}\`: duplicate compatibility row (${r.duplicate_count})`);
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
        measurementProfile: null,
        sourceFreshness: null,
        rows: null,
        applicationRows: null,
        applicationMissing: null,
        applicationIncomplete: null,
        applicationDuplicates: null,
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
  measurementProfile,
  validationWarnings,
  sourceFreshness,
  rows,
  applicationRows,
  applicationMissing,
  applicationIncomplete,
  applicationDuplicates,
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
      measurementProfile,
      validationWarnings,
      sourceFreshness,
      rows,
      applicationRows,
      applicationMissing,
      applicationIncomplete,
      applicationDuplicates,
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

if (requireApplicationCompat && (
  applicationMissing.length > 0 ||
  applicationIncomplete.length > 0 ||
  applicationDuplicates.length > 0
)) {
  const gaps = [
    ...applicationMissing.map((r) => `${r.name} (missing)`),
    ...applicationIncomplete.map((r) => `${r.name} (incomplete)`),
    ...applicationDuplicates.map((r) => `${r.name} (${r.duplicate_count} duplicates)`),
  ];
  process.stderr.write(
    `bench-artifact-readiness: application compatibility incomplete for ${gaps.length} row(s): ` +
      gaps.join(", ") + "\n",
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

process.exit(0);
