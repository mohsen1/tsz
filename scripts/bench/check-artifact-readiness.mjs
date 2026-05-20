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

function rowState(row) {
  if (!row) return "missing";
  if (row.status) return "red";
  const compat = row.compatibility;
  if (!compat) return "gray";
  return compat.state ?? "gray";
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

function analyzeArtifact(artifact) {
  const byName = new Map(
    (artifact.results ?? []).map((r) => [r?.name, r]),
  );

  const rows = REQUIRED_PROJECT_ROWS.map((name) => {
    const row = byName.get(name) ?? null;
    const state = rowState(row);
    const def = PROJECT_ROWS_BY_NAME[name];
    const compatibility = row?.compatibility ?? {};
    return {
      name,
      label: def?.label ?? name,
      state,
      tsz_ms: row?.tsz_ms ?? null,
      tsgo_ms: row?.tsgo_ms ?? null,
      winner: row?.winner ?? null,
      exit_class: compatibility.exit_class ?? null,
      phase: compatibility.phase ?? null,
      last_successful_phase: compatibility.last_successful_phase ?? null,
      first_failure_class: compatibility.first_failure_class ?? null,
      owner_family: compatibility.semantic_owner_family ?? compatibility.owner_family ?? null,
      known_blockers: Array.isArray(compatibility.known_blockers)
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
    rows,
    missing: rows.filter((r) => r.state === "missing"),
    red: rows.filter((r) => r.state === "red"),
    yellow: rows.filter((r) => r.state === "yellow"),
    gray: rows.filter((r) => r.state === "gray"),
    green: rows.filter((r) => r.state === "green"),
  };
}

function buildJson({ artifactAbsent, parseError, artifact, measurementProfile, rows, missing, red, yellow, gray, green }) {
  const missingNames = missing?.map((r) => r.name) ?? REQUIRED_PROJECT_ROWS;
  return {
    artifact_absent: artifactAbsent,
    parse_error: parseError ?? null,
    source_commit: artifact?.source_commit ?? null,
    generated_at: artifact?.generated_at ?? null,
    workflow_run_url: artifact?.workflow_run_url ?? null,
    measurement_profile: measurementProfile ?? null,
    required_row_count: rows?.length ?? REQUIRED_PROJECT_ROWS.length,
    green: green?.length ?? 0,
    yellow: yellow?.length ?? 0,
    red: red?.length ?? 0,
    gray: gray?.length ?? 0,
    missing: missingNames.length,
    missing_rows: missingNames,
    red_rows: red?.map((r) => r.name) ?? [],
    yellow_rows: yellow?.map((r) => r.name) ?? [],
    rows: rows?.map((r) => ({
      name: r.name,
      label: r.label,
      state: r.state,
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

function artifactAge(generatedAt) {
  if (!generatedAt) return "unknown age";
  const h = Math.round((Date.now() - new Date(generatedAt).getTime()) / 3_600_000);
  if (h < 1) return "< 1 h ago";
  if (h === 1) return "1 h ago";
  return `${h} h ago`;
}

function buildReport({ artifact, measurementProfile, rows, missing, red, yellow, gray, green }) {
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
    "",
  ];

  if (missing.length > 0) {
    lines.push(`### 🚫 Missing required rows (${missing.length})`, "");
    for (const r of missing) lines.push(`- \`${r.name}\``);
    lines.push("");
  }

  lines.push("### All required rows", "");
  lines.push("| State | Row | tsz | tsgo | Winner | Exit | Phase | Last phase | Files | Peak RSS | Failure | Blocker family | Diagnostics |");
  lines.push("|:-----:|-----|----:|----:|--------|------|-------|------------|------:|---------:|---------|----------------|-------------|");
  for (const r of rows) {
    const icon = STATE_ICON[r.state] ?? "?";
    const blockerFamily = r.owner_family ?? r.known_blockers?.[0] ?? "—";
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
      })) + "\n",
    );
  }
  process.exit(2);
}

const analysis = analyzeArtifact(artifact);
const { measurementProfile, rows, missing, red, yellow, gray, green } = analysis;

writeReport(buildReport({ artifact, ...analysis }));

if (jsonOutput) {
  process.stdout.write(
    JSON.stringify(buildJson({
      artifactAbsent: false,
      parseError: null,
      artifact,
      measurementProfile,
      rows,
      missing,
      red,
      yellow,
      gray,
      green,
    })) + "\n",
  );
}

if (missing.length > 0) {
  process.stderr.write(
    `bench-artifact-readiness: ${missing.length} required row(s) missing from artifact: ` +
      missing.map((r) => r.name).join(", ") + "\n",
  );
  process.exit(1);
}

process.exit(0);
