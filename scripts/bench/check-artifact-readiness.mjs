#!/usr/bin/env node
/**
 * Checks a merged bench artifact for required project-row completeness.
 *
 * Exit codes:
 *   0 — artifact present, all required rows included
 *   1 — artifact present, one or more required rows are missing
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
 *   node scripts/bench/check-artifact-readiness.mjs [--json] <artifact.json>
 */

import fs from "node:fs";

import {
  REQUIRED_PROJECT_ROWS,
  PROJECT_ROWS_BY_NAME,
} from "./project-rows.mjs";
import {
  hasCompletePhaseMetadata,
  isGreen,
} from "./row-utils.mjs";

const args = process.argv.slice(2);
const jsonOutput = args.includes("--json");
const filePath = args.find((a) => !a.startsWith("-")) ?? null;

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

const STATE_ICON = { green: "✅", yellow: "⚠️", red: "❌", gray: "⬜", missing: "🚫" };

function analyzeMeasurementProfile(artifact) {
  const profile = artifact?.measurement_profile;
  if (!profile || typeof profile !== "object") {
    return {
      present: false,
      mode: null,
      tsz_binary_source: null,
      pgo_requested: null,
      pgo_required: null,
      pgo_optimized: null,
      profile_fingerprint: null,
      training_fingerprint: null,
      training_input_count: null,
      training_failure_count: null,
      warning: "measurement_profile missing",
    };
  }

  const pgo = profile.profile_guided_optimization && typeof profile.profile_guided_optimization === "object"
    ? profile.profile_guided_optimization
    : {};
  const mode = typeof profile.mode === "string" && profile.mode.trim()
    ? profile.mode.trim()
    : null;
  const warning = (() => {
    if (!mode) return "measurement_profile.mode missing";
    if (mode === "release-pgo") {
      const missing = [];
      if (pgo.optimized !== true) missing.push("pgo optimized flag");
      if (!pgo.profile_fingerprint) missing.push("profile fingerprint");
      if (!pgo.training_fingerprint) missing.push("training fingerprint");
      if (missing.length) return `release-pgo metadata missing ${missing.join(", ")}`;
    }
    return null;
  })();

  return {
    present: true,
    mode,
    tsz_binary_source: profile.tsz_binary_source ?? null,
    pgo_requested: pgo.requested ?? null,
    pgo_required: pgo.required ?? null,
    pgo_optimized: pgo.optimized ?? null,
    profile_fingerprint: pgo.profile_fingerprint ?? null,
    training_fingerprint: pgo.training_fingerprint ?? null,
    training_input_count: pgo.training_input_count ?? null,
    training_failure_count: pgo.training_failure_count ?? null,
    warning,
  };
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

function analyzeArtifact(artifact) {
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

  const rows = REQUIRED_PROJECT_ROWS.map((name) => {
    const row = byName.get(name) ?? null;
    const duplicateCount = duplicateCounts.get(name) ?? (row ? 1 : 0);
    const duplicate = duplicateCount > 1;
    const state = rowState(row, duplicate);
    const def = PROJECT_ROWS_BY_NAME[name];
    const compatibility = row?.compatibility ?? {};
    return {
      name,
      label: def?.label ?? name,
      state,
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
  });

  return {
    measurementProfile: analyzeMeasurementProfile(artifact),
    validationWarnings: analyzeValidationWarnings(artifact),
    rows,
    missing: rows.filter((r) => r.state === "missing"),
    red: rows.filter((r) => r.state === "red"),
    yellow: rows.filter((r) => r.state === "yellow"),
    gray: rows.filter((r) => r.state === "gray"),
    green: rows.filter((r) => r.state === "green"),
    duplicates: rows.filter((r) => r.duplicate_count > 1),
  };
}

function buildJson({ artifactAbsent, parseError, artifact, measurementProfile, validationWarnings, rows, missing, red, yellow, gray, green, duplicates }) {
  const missingNames = missing?.map((r) => r.name) ?? REQUIRED_PROJECT_ROWS;
  return {
    artifact_absent: artifactAbsent,
    parse_error: parseError ?? null,
    source_commit: artifact?.source_commit ?? null,
    generated_at: artifact?.generated_at ?? null,
    workflow_run_url: artifact?.workflow_run_url ?? null,
    measurement_profile: measurementProfile ?? null,
    validation_warnings: validationWarnings ?? {
      runner_environment: [],
      measurement_profile: [],
      total: 0,
    },
    required_row_count: rows?.length ?? REQUIRED_PROJECT_ROWS.length,
    green: green?.length ?? 0,
    yellow: yellow?.length ?? 0,
    red: red?.length ?? 0,
    gray: gray?.length ?? 0,
    missing: missingNames.length,
    missing_rows: missingNames,
    duplicate_rows: duplicates?.map((r) => ({ name: r.name, count: r.duplicate_count })) ?? [],
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

function artifactAge(generatedAt) {
  if (!generatedAt) return "unknown age";
  const h = Math.round((Date.now() - new Date(generatedAt).getTime()) / 3_600_000);
  if (h < 1) return "< 1 h ago";
  if (h === 1) return "1 h ago";
  return `${h} h ago`;
}

function buildReport({ artifact, measurementProfile, validationWarnings, rows, missing, red, yellow, gray, green, duplicates }) {
  const sourceCommit = artifact?.source_commit?.slice(0, 10) ?? "unknown";
  const generatedAt = artifact?.generated_at ?? null;
  const workflowUrl = artifact?.workflow_run_url ?? null;
  const profile = measurementProfile ?? analyzeMeasurementProfile(artifact);
  const profileLabel = profile.present
    ? `${profile.mode ?? "unknown"}${profile.warning ? ` (${profile.warning})` : ""}`
    : profile.warning;

  const lines = [
    `## Benchmark artifact readiness — ${new Date().toUTCString()}`,
    "",
    "| Field | Value |",
    "|-------|-------|",
    `| Artifact SHA | \`${sourceCommit}\` |`,
    `| Generated | ${generatedAt ?? "—"} (${artifactAge(generatedAt)}) |`,
    `| Workflow run | ${workflowUrl ? `[link](${workflowUrl})` : "—"} |`,
    `| Measurement profile | ${profileLabel} |`,
    `| PGO profile | ${profile.profile_fingerprint ? `\`${profile.profile_fingerprint.slice(0, 12)}\`` : "—"} |`,
    `| PGO training | ${profile.training_fingerprint ? `\`${profile.training_fingerprint.slice(0, 12)}\`` : "—"} |`,
    `| Required rows | ${rows.length} |`,
    `| ✅ green | ${green.length} |`,
    `| ⚠️ yellow | ${yellow.length} |`,
    `| ❌ red | ${red.length} |`,
    `| ⬜ gray | ${gray.length} |`,
    `| 🚫 missing | ${missing.length} |`,
    `| Duplicate rows | ${duplicates.length} |`,
    `| Runner metadata warnings | ${validationWarnings.runner_environment.length} |`,
    `| Measurement profile warnings | ${validationWarnings.measurement_profile.length} |`,
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

  if (validationWarnings.runner_environment.length > 0) {
    lines.push(`### Runner metadata warnings (${validationWarnings.runner_environment.length})`, "");
    for (const warning of validationWarnings.runner_environment) {
      lines.push(`- \`${mdCell(warning.file ?? "unknown input")}\`: ${mdCell(fmtWarningFields(warning))}`);
    }
    lines.push("");
  }

  if (validationWarnings.measurement_profile.length > 0) {
    lines.push(`### Measurement profile warnings (${validationWarnings.measurement_profile.length})`, "");
    for (const warning of validationWarnings.measurement_profile) {
      lines.push(`- \`${mdCell(warning.file ?? "unknown input")}\`: ${mdCell(fmtWarningFields(warning))}`);
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
        rows: null,
        missing: null,
        red: null,
        yellow: null,
        gray: null,
        green: null,
        duplicates: null,
      })) + "\n",
    );
  }
  process.exit(2);
}

const analysis = analyzeArtifact(artifact);
const { measurementProfile, validationWarnings, rows, missing, red, yellow, gray, green, duplicates } = analysis;

writeReport(buildReport({ artifact, ...analysis }));

if (jsonOutput) {
  process.stdout.write(
    JSON.stringify(buildJson({
      artifactAbsent: false,
      parseError: null,
      artifact,
      measurementProfile,
      validationWarnings,
      rows,
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

process.exit(0);
