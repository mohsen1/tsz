import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { marked } from "marked";
import {
  COMPILE_CANARY_PROJECT_ROWS,
  COMPATIBILITY_CORPUS_ROWS,
  PROJECT_ROWS_BY_NAME,
  REQUIRED_PROJECT_ROWS,
} from "../../../../scripts/bench/project-rows.mjs";
import { selectLatestBenchmarkArtifact } from "../../../../scripts/bench/benchmark-artifact-selection.mjs";
import { didNotFinish } from "../../../../scripts/bench/row-utils.mjs";
import { subsystemForCode } from "../../../../scripts/ci/diagnostic-subsystems.mjs";
import { fmt } from "./loc.js";
import { generatedBenchmarkSource } from "./benchmark_generated_sources.js";
import { PROJECT_DESCRIPTIONS } from "./project_descriptions.js";

const ROOT = path.resolve(import.meta.dirname, "..", "..", "..", "..");

function formatUtcTimestamp(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  return date.toISOString().replace(/\.\d{3}Z$/, "Z");
}

function formatMemory(bytes) {
  const value = Number(bytes);
  if (!Number.isFinite(value) || value <= 0) return null;
  return `${(value / (1024 ** 3)).toFixed(1)} GiB RAM`;
}

function measurementProfileSummary(data) {
  const profile = data?.measurement_profile;
  if (!profile || typeof profile !== "object") return null;

  const mode = String(profile.mode || "").trim();
  if (!mode) return null;

  const pgo = profile.profile_guided_optimization || {};
  if (mode === "release-pgo" && pgo.optimized) {
    const parts = ["tsz release-pgo"];
    if (Number.isFinite(Number(pgo.training_input_count))) {
      parts.push(`${Number(pgo.training_input_count)} PGO training inputs`);
    }
    if (pgo.profile_fingerprint) {
      parts.push(`profile ${String(pgo.profile_fingerprint).slice(0, 12)}`);
    }
    if (pgo.profile_data_source === "cache") {
      parts.push("cached profile data");
    }
    if (pgo.training_metadata_available === false) {
      parts.push("training metadata unavailable");
    }
    return parts.join(", ");
  }

  if (mode === "release-untrained") return "tsz release build without PGO";
  if (mode === "quick-untrained") return "quick-mode tsz build without PGO";
  if (mode === "tsz-override") return "caller-provided tsz binary";
  return `tsz ${mode}`;
}

// Builds the "show runner info" line as an HTML string. Each part is either a
// pre-escaped text fragment or intentional markup: the generated timestamp is a
// GitHub <relative-time> web component (relative display, absolute ISO as
// fallback text + title), and the source sha links to its GitHub commit. The
// caller embeds this without further escaping, so every dynamic text value is
// escaped here.
const TSZ_COMMIT_URL_BASE = "https://github.com/tsz-org/tsz/commit/";

function runnerEnvironmentSummary(data) {
  const parts = [];
  const generatedAt = formatUtcTimestamp(data?.generated_at);
  if (generatedAt) {
    const iso = escapeHtml(generatedAt);
    parts.push(
      `Generated <relative-time datetime="${iso}" tense="past" format="relative" title="${iso}">${iso}</relative-time>`,
    );
  }
  const sourceCommit = normalizedCommit(data?.source_commit);
  if (sourceCommit) {
    const href = escapeHtml(`${TSZ_COMMIT_URL_BASE}${sourceCommit}`);
    parts.push(
      `sha <a href="${href}" target="_blank" rel="noreferrer noopener"><code>${escapeHtml(sourceCommit.slice(0, 12))}</code></a>`,
    );
  }
  const measurement = measurementProfileSummary(data);
  if (measurement) parts.push(escapeHtml(measurement));

  const env = data?.runner_environment;
  if (!env || typeof env !== "object") {
    return parts.join(" · ");
  }

  const platform = [env.platform, env.arch].filter(Boolean).join("/");
  if (platform) parts.push(escapeHtml(platform));
  if (env.cpu_count) {
    const cpuModel = env.cpu_model ? ` ${env.cpu_model}` : "";
    parts.push(escapeHtml(`${env.cpu_count} CPU${env.cpu_count === 1 ? "" : "s"}${cpuModel}`));
  }
  const memory = formatMemory(env.total_memory_bytes);
  if (memory) parts.push(escapeHtml(memory));
  if (env.github_actions?.runner_os || env.github_actions?.runner_arch) {
    const runner = [
      env.github_actions.runner_os,
      env.github_actions.runner_arch,
    ].filter(Boolean).join("/");
    parts.push(escapeHtml(`GitHub Actions ${runner}`));
  } else if (env.ci) {
    parts.push(escapeHtml("CI runner"));
  }
  if (env.cloud_build?.machine_type) {
    parts.push(escapeHtml(`Cloud Build ${env.cloud_build.machine_type}`));
  }

  return parts.join(" · ");
}

function formatDurationMs(value, fractionDigits = 0) {
  const ms = Number(value);
  if (!Number.isFinite(ms)) return "";
  if (ms > 1000) {
    return `${(ms / 1000).toLocaleString("en-US", { maximumFractionDigits: 1 })}s`;
  }
  return `${ms.toFixed(fractionDigits)}ms`;
}

function finiteNumber(value) {
  if (value === null || value === undefined || value === "") return null;
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

function formatFilesReached(value) {
  const count = finiteNumber(value);
  return count === null ? null : `${fmt(count)} files`;
}

function formatPeakMemoryMiB(value) {
  const bytes = finiteNumber(value);
  if (bytes === null || bytes <= 0) return null;
  return `${(bytes / (1024 * 1024)).toLocaleString("en-US", { maximumFractionDigits: 0 })} MiB peak`;
}

function durationLabelFitsBar(label, widthPx) {
  const width = Number(widthPx);
  if (!Number.isFinite(width) || width <= 0) return false;

  // Bench labels use the monospace 0.8rem style plus 0.45rem horizontal padding
  // on each side. Estimate conservatively so labels move outside before clipping.
  const approximateTextWidth = String(label).length * 8;
  const horizontalPadding = 14.5;
  return width >= approximateTextWidth + horizontalPadding;
}

// A real bar never renders below this width, so a large row in the same chart
// cannot crush a smaller (but genuine) row to an invisible sub-pixel sliver.
// A zero value (no timing) stays zero -- no bar is drawn.
const MIN_VISIBLE_BAR_PX = 3;
function renderBenchmarkBar(kind, widthPx, label) {
  const raw = Number.isFinite(Number(widthPx)) ? Math.max(0, Number(widthPx)) : 0;
  const width = raw > 0 ? Math.max(MIN_VISIBLE_BAR_PX, raw) : 0;
  const placementClass = durationLabelFitsBar(label, width) ? "" : " value-outside";
  return `<div class="bench-bar ${kind}${placementClass}" style="width: ${width.toFixed(2)}px">
          <span class="bench-bar-value">${label}</span>
        </div>`;
}

function formatSpeedupLabel(tszMs, tsgoMs) {
  const tsz = Number(tszMs);
  const tsgo = Number(tsgoMs);
  if (!Number.isFinite(tsz) || !Number.isFinite(tsgo) || tsz <= 0 || tsgo <= 0) return "";

  const factor = Math.max(tsz, tsgo) / Math.min(tsz, tsgo);
  if (factor < 1.05) return "equal";

  return tsz < tsgo
    ? `tsz ${factor.toFixed(1)}x faster`
    : `tsgo ${factor.toFixed(1)}x faster`;
}

function hasTiming(value) {
  const time = Number(value);
  return Number.isFinite(time) && time > 0;
}

function isProjectBenchmark(row) {
  return Boolean(row?.name && PROJECT_ROWS_BY_NAME[row.name]);
}

function hasGreenProjectCompatibility(row) {
  if (!isProjectBenchmark(row)) return true;

  const compatibility = row?.compatibility;
  if (!compatibility || typeof compatibility !== "object") return false;

  const state = String(compatibility.state || "").toLowerCase();
  const exitClass = String(compatibility.exit_class || "").toLowerCase();
  const diagnosticStatus = String(compatibility.diagnostic_status || "").toLowerCase();
  return state === "green"
    && exitClass === "exit success"
    && (!diagnosticStatus || diagnosticStatus === "none")
    && hasCompleteCompatibilityMetadata(compatibility);
}

function fastestTiming(row) {
  const timings = [row?.tsz_ms, row?.tsgo_ms].map(Number).filter((time) => Number.isFinite(time) && time > 0);
  return timings.length ? Math.min(...timings) : Infinity;
}

function tszSpeedupScore(row) {
  const tsz = Number(row?.tsz_ms);
  const tsgo = Number(row?.tsgo_ms);
  if (!Number.isFinite(tsz) || !Number.isFinite(tsgo) || tsz <= 0 || tsgo <= 0) {
    return -Infinity;
  }
  return tsgo / tsz;
}

function compareByTszSpeedup(a, b) {
  const aScore = tszSpeedupScore(a);
  const bScore = tszSpeedupScore(b);
  if (aScore !== bScore) return bScore - aScore;

  const aFastest = fastestTiming(a);
  const bFastest = fastestTiming(b);
  if (aFastest !== bFastest) return aFastest - bFastest;

  return String(a?.name || "").localeCompare(String(b?.name || ""));
}

function hasSuccessfulTimingPair(row) {
  // `didNotFinish` (which subsumes `winner === "error"`) keeps a killed/errored
  // row's ceiling/error timing out of any speed ratio — see it for the #16196
  // rationale and why the exclusion must be structural, not incidental.
  return !row?.status
    && !didNotFinish(row)
    && hasTiming(row?.tsz_ms)
    && hasTiming(row?.tsgo_ms);
}

// We run every benchmark and let individual ones fail; the chart renders only
// the ones that "succeeded", where success means SPEED, not tsc-compatibility:
// both compilers produced a timing AND tsgo is not >= 1.5x faster than tsz.
// A row that keeps up with tsgo renders even if it diverges from tsc (yellow);
// a row where tsz is >= 1.5x slower, errored, or timed out simply does not
// render — it never blocks the rest of the chart.
const CHART_MAX_TSZ_TO_TSGO_RATIO = 1.5;
function isChartEligible(row) {
  const tsz = Number(row?.tsz_ms);
  const tsgo = Number(row?.tsgo_ms);
  if (!(tsz > 0) || !(tsgo > 0)) return false;
  // A short-ceiling timeout can land under 1.5x tsgo with finite timings and
  // would otherwise leak a `ceiling / tsgo_time` win into the chart (#16196);
  // `didNotFinish` (which subsumes `winner === "error"`) drops it structurally.
  if (didNotFinish(row)) return false;
  return tsz < tsgo * CHART_MAX_TSZ_TO_TSGO_RATIO;
}

// A row that produced a real timing pair but is too slow to chart (tsgo is
// >= 1.5x faster, so isChartEligible() drops it) still has a timing pair, so the
// normal isFailedBenchmark() check would treat it as "successful" and drop it
// from the failures list too. Surface it explicitly in the excluded/incomplete
// section so a timed-but-too-slow row stays visible instead of vanishing.
function isExcludedSlowTimedRow(row) {
  return hasSuccessfulTimingPair(row) && !isChartEligible(row);
}

function isFailedBenchmark(row) {
  if (!row || hasSuccessfulTimingPair(row)) return false;
  return Boolean(row.status) || row.winner === "error" || hasTiming(row.tsz_ms) || hasTiming(row.tsgo_ms);
}

function statusLabel(row) {
  if (row?.status) return String(row.status);
  // Say "did not finish" rather than deriving a slower/faster label from a
  // killed/errored row's sentinel timing (#16196).
  if (didNotFinish(row)) return "did not finish";
  const tsz = Number(row?.tsz_ms);
  const tsgo = Number(row?.tsgo_ms);
  if (tsz > 0 && tsgo > 0 && tsz >= tsgo * CHART_MAX_TSZ_TO_TSGO_RATIO) {
    return `tsz ${(tsz / tsgo).toFixed(1)}x slower than tsgo`;
  }
  return "timing unavailable";
}

function firstPresent(...values) {
  for (const value of values) {
    if (value !== undefined && value !== null && value !== "") return value;
  }
  return null;
}


function diagnosticSubsystemsFromDeltas(deltas) {
  const groups = new Map();
  for (const line of deltas) {
    const codes = [...String(line || "").matchAll(/\bTS\d{4,5}\b/g)].map((match) => match[0]);
    const lineCodes = codes.length ? codes : ["uncoded"];
    for (const code of lineCodes) {
      const subsystem = code === "uncoded" ? "uncoded diagnostic" : subsystemForCode(code);
      if (!groups.has(subsystem)) {
        groups.set(subsystem, { subsystem, codes: [], count: 0, examples: [] });
      }
      const group = groups.get(subsystem);
      group.count += 1;
      if (code !== "uncoded" && !group.codes.includes(code) && group.codes.length < 8) {
        group.codes.push(code);
      }
      if (group.examples.length < 3) {
        group.examples.push(String(line || ""));
      }
    }
  }
  return [...groups.values()];
}

function normalizedDiagnosticSubsystems(compatibility) {
  const existing = Array.isArray(compatibility?.diagnostic_subsystems)
    ? compatibility.diagnostic_subsystems
    : [];
  if (existing.length) {
    return existing
      .map((group) => ({
        subsystem: String(group?.subsystem || "unclassified diagnostic"),
        codes: Array.isArray(group?.codes) ? group.codes.map(String).filter(Boolean).slice(0, 8) : [],
        count: Number.isFinite(Number(group?.count)) ? Number(group.count) : 0,
        examples: Array.isArray(group?.examples) ? group.examples.map(String).filter(Boolean).slice(0, 3) : [],
      }))
      .filter((group) => group.count > 0 || group.codes.length || group.examples.length)
      .slice(0, 8);
  }
  const deltas = Array.isArray(compatibility?.diagnostic_deltas)
    ? compatibility.diagnostic_deltas
    : compatibility?.diagnostic_deltas
      ? [compatibility.diagnostic_deltas]
      : [];
  return diagnosticSubsystemsFromDeltas(deltas).slice(0, 8);
}

function diagnosticCodesFromDeltas(deltas) {
  const codes = [];
  const seen = new Set();
  for (const line of deltas) {
    for (const match of String(line || "").matchAll(/\bTS\d{4,5}\b/g)) {
      const code = match[0];
      if (seen.has(code)) continue;
      seen.add(code);
      codes.push(code);
      if (codes.length >= 8) return codes;
    }
  }
  return codes;
}

function normalizedKnownBlockers(compatibility, diagnosticSubsystems, fallbackBlockers = []) {
  const existing = Array.isArray(compatibility?.known_blockers) ? compatibility.known_blockers : [];
  if (existing.length) {
    return existing.map(String).filter(Boolean).slice(0, 8);
  }

  const blockers = [];
  const add = (blocker) => {
    if (blocker && !blockers.includes(blocker) && blockers.length < 8) blockers.push(blocker);
  };
  const exitClass = String(compatibility?.exit_class || "");
  const phase = String(compatibility?.phase || "");

  if (exitClass === "timeout") add("timeout during project check");
  if (exitClass === "oom") add("OOM or killed during project check");
  if (exitClass === "crash") add("compiler crash during project check");
  if (exitClass === "fixture invalid") add("reference fixture invalid");
  if (exitClass === "runner error") add("benchmark runner error");
  if (exitClass === "tsz unavailable") add("tsz unavailable in benchmark runner");
  if (exitClass === "oracle unavailable") add("tsc oracle unavailable");
  if (phase && phase !== "check") add(`${phase} phase blocker`);

  for (const group of diagnosticSubsystems) {
    add(String(group?.subsystem || ""));
  }

  const deltas = Array.isArray(compatibility?.diagnostic_deltas)
    ? compatibility.diagnostic_deltas
    : compatibility?.diagnostic_deltas
      ? [compatibility.diagnostic_deltas]
      : [];
  if (!blockers.length && diagnosticCodesFromDeltas(deltas).length) {
    add("unclassified diagnostic mismatch");
  }
  for (const blocker of fallbackBlockers) {
    add(blocker);
  }

  return blockers;
}

function normalizedLastSuccessfulPhase(compatibility) {
  if (compatibility?.last_successful_phase !== undefined && compatibility.last_successful_phase !== "") {
    return compatibility.last_successful_phase;
  }
  if (compatibility?.exit_class === "exit success" && compatibility?.diagnostic_status === "none") return "check";
  return null;
}

const COMPATIBILITY_METADATA_FIELDS = [
  ["generated_at", "artifact generated at"],
  ["source_commit", "source commit"],
  ["workflow_name", "workflow name"],
  ["workflow_run_id", "workflow run id"],
  ["workflow_run_url", "workflow run URL"],
  ["workflow_run_attempt", "workflow run attempt"],
  ["run_status", "run status"],
  ["state", "state"],
  ["exit_class", "exit class"],
  ["first_failure_class", "first failure class"],
  ["owner_track", "owner track"],
  ["phase", "phase"],
  ["last_successful_phase", "last successful phase"],
  ["diagnostic_status", "diagnostic status"],
  ["diagnostic_deltas", "diagnostic deltas"],
  ["diagnostic_subsystems", "diagnostic subsystems"],
  ["known_blockers", "known blockers"],
  ["reduced_repro_path", "reduced repro path"],
  ["repro", "repro metadata"],
  ["exit_codes", "exit codes"],
  ["files_reached", "files reached"],
  ["files_reached_reason", "files reached reason"],
  ["peak_memory_bytes", "peak memory"],
  ["peak_memory_bytes_reason", "peak memory reason"],
  ["fixture_sources", "fixture sources"],
  ["emit_status", "emit status"],
  ["dts_status", "dts status"],
];

const COMPATIBILITY_FRESHNESS_FIELDS = new Set([
  "generated_at",
  "source_commit",
  "workflow_name",
  "workflow_run_id",
  "workflow_run_url",
  "workflow_run_attempt",
  "run_status",
]);

function hasArtifactField(artifact, field) {
  return Object.prototype.hasOwnProperty.call(artifact || {}, field);
}

function missingCompatibilityMetadata(row, artifact) {
  const compatibility = row?.compatibility;
  if (!compatibility || typeof compatibility !== "object") return ["compatibility artifact"];
  const missing = COMPATIBILITY_METADATA_FIELDS
    .filter(([field]) => (
      !Object.prototype.hasOwnProperty.call(compatibility, field) &&
      !(COMPATIBILITY_FRESHNESS_FIELDS.has(field) && hasArtifactField(artifact, field))
    ))
    .map(([, label]) => label);
  if (
    Object.prototype.hasOwnProperty.call(compatibility, "fixture_sources") &&
    !hasCompleteFixtureSources(compatibility)
  ) {
    missing.push("fixture sources missing/malformed/unpinned");
  }
  return missing;
}

function hasCompleteCompatibilityMetadata(compatibility) {
  if (!compatibility || typeof compatibility !== "object") return false;
  return COMPATIBILITY_METADATA_FIELDS.every(([field]) => (
    Object.prototype.hasOwnProperty.call(compatibility, field)
  )) && (
    !Object.prototype.hasOwnProperty.call(compatibility, "fixture_sources") ||
    hasCompleteFixtureSources(compatibility)
  );
}

function hasCompleteFixtureSources(compatibility) {
  const sources = Array.isArray(compatibility?.fixture_sources)
    ? compatibility.fixture_sources
    : [];
  return sources.length > 0 && sources.every((source) => (
    String(source?.name || "").trim() &&
    String(source?.repository || "").trim() &&
    String(source?.ref || "").trim()
  ));
}

let currentCheckoutCommitCache;

function currentCheckoutCommit() {
  if (currentCheckoutCommitCache !== undefined) return currentCheckoutCommitCache;
  try {
    currentCheckoutCommitCache = execFileSync("git", ["rev-parse", "HEAD"], {
      cwd: ROOT,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim() || null;
  } catch {
    currentCheckoutCommitCache = null;
  }
  return currentCheckoutCommitCache;
}

function normalizedCommit(value) {
  const commit = String(value || "").trim().toLowerCase();
  return /^[0-9a-f]{7,40}$/.test(commit) ? commit : null;
}

function commitsMatch(left, right) {
  const a = normalizedCommit(left);
  const b = normalizedCommit(right);
  if (!a || !b) return true;
  return a.startsWith(b) || b.startsWith(a);
}

function shortCommit(value) {
  const commit = String(value || "").trim();
  return commit && commit !== "local" ? commit.slice(0, 12) : commit;
}

function artifactMetadataFor(row, artifact) {
  const compatibility = row?.compatibility || {};
  const get = (field) => {
    if (Object.prototype.hasOwnProperty.call(compatibility, field)) return compatibility[field];
    if (Object.prototype.hasOwnProperty.call(artifact || {}, field)) return artifact[field];
    return null;
  };
  return {
    generatedAt: formatUtcTimestamp(get("generated_at")),
    sourceCommit: get("source_commit"),
    workflowName: get("workflow_name"),
    workflowRunId: get("workflow_run_id"),
    workflowRunUrl: get("workflow_run_url"),
    workflowRunAttempt: get("workflow_run_attempt"),
    runStatus: get("run_status"),
    latestCompletedBenchmarkRunId: get("latest_completed_benchmark_run_id"),
    latestCompletedBenchmarkGeneratedAt: formatUtcTimestamp(get("latest_completed_benchmark_generated_at")),
  };
}

function artifactFreshnessWarnings(metadata) {
  const warnings = [];
  const currentCommit = currentCheckoutCommit();
  if (
    metadata.sourceCommit &&
    metadata.sourceCommit !== "local" &&
    currentCommit &&
    !commitsMatch(metadata.sourceCommit, currentCommit)
  ) {
    warnings.push(`source older than checkout ${shortCommit(currentCommit)}`);
  }

  if (
    metadata.latestCompletedBenchmarkRunId &&
    metadata.workflowRunId &&
    String(metadata.latestCompletedBenchmarkRunId) !== String(metadata.workflowRunId)
  ) {
    warnings.push(`older than latest completed bench run ${metadata.latestCompletedBenchmarkRunId}`);
  }

  if (
    metadata.latestCompletedBenchmarkGeneratedAt &&
    metadata.generatedAt &&
    new Date(metadata.latestCompletedBenchmarkGeneratedAt).getTime() > new Date(metadata.generatedAt).getTime()
  ) {
    warnings.push(`older than ${metadata.latestCompletedBenchmarkGeneratedAt} bench artifact`);
  }

  const runStatus = String(metadata.runStatus || "").toLowerCase();
  if (runStatus && !["completed", "manually merged", "local"].includes(runStatus)) {
    warnings.push(`run status: ${metadata.runStatus}`);
  }
  return warnings;
}

function normalizedFixtureSources(compatibility) {
  const sources = Array.isArray(compatibility?.fixture_sources)
    ? compatibility.fixture_sources
    : [];
  const seen = new Set();
  return sources
    .map((source) => ({
      name: String(source?.name || "").trim(),
      repository: String(source?.repository || "").trim(),
      ref: String(source?.ref || "").trim(),
    }))
    .filter((source) => source.name && source.repository && source.ref)
    .filter((source) => {
      const key = `${source.name}\0${source.repository}\0${source.ref || ""}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    })
    .slice(0, 4);
}

function withExpectedProjectRows(results) {
  const rows = Array.isArray(results) ? results.slice() : [];
  const existingNames = new Set(rows.map((row) => row?.name).filter(Boolean));

  for (const name of REQUIRED_PROJECT_ROWS) {
    if (existingNames.has(name)) continue;
    rows.push({
      name,
      lines: 0,
      kb: 0,
      tsz_ms: null,
      tsgo_ms: null,
      tsz_lps: null,
      tsgo_lps: null,
      winner: "error",
      ratio: 0,
      status: "not recorded in latest benchmark artifact",
    });
  }

  for (const name of COMPILE_CANARY_PROJECT_ROWS) {
    if (existingNames.has(name)) continue;
    rows.push({
      name,
      lines: 0,
      kb: 0,
      tsz_ms: null,
      tsgo_ms: null,
      tsz_lps: null,
      tsgo_lps: null,
      winner: "error",
      ratio: 0,
      status: "compile canary tracked in CI; not timed by vs-tsgo benchmarks",
    });
  }

  return rows;
}

function compatibilityState(row) {
  const compatibility = row?.compatibility || {};
  const diagnosticStatus = String(compatibility.diagnostic_status || "").toLowerCase();
  const recordedState = String(compatibility.state || "").toLowerCase();
  if (!Object.keys(compatibility).length) {
    return {
      className: "gray",
      stateLabel: "Not measured",
      exitClass: "missing or incomplete artifact",
      phase: "artifact",
      diagnosticDeltas: "not available",
    };
  }
  if (recordedState === "gray") {
    return {
      className: "gray",
      stateLabel: "Not measured",
      exitClass: firstPresent(compatibility.exit_class, "missing or incomplete artifact"),
      phase: firstPresent(compatibility.phase, "artifact"),
      diagnosticDeltas: firstPresent(compatibility.diagnostic_deltas, "not available"),
    };
  }
  const compatibilityGreen = (
    recordedState === "green" ||
    String(compatibility.exit_class || "").toLowerCase() === "exit success"
  ) && diagnosticStatus === "none";
  if (compatibilityGreen && hasCompleteCompatibilityMetadata(compatibility)) {
    return {
      className: "green",
      stateLabel: "Passing",
      exitClass: firstPresent(compatibility.exit_class, "exit success"),
      phase: firstPresent(compatibility.phase, "check"),
      diagnosticDeltas: firstPresent(compatibility.diagnostic_deltas, "none recorded"),
    };
  }

  if (hasSuccessfulTimingPair(row) && hasGreenProjectCompatibility(row)) {
    if (diagnosticStatus && diagnosticStatus !== "none") {
      return {
        className: "yellow",
        stateLabel: "Errors",
        exitClass: firstPresent(compatibility.exit_class, "diagnostic mismatch"),
        phase: firstPresent(compatibility.phase, "check"),
        diagnosticDeltas: firstPresent(compatibility.diagnostic_deltas, "not captured by latest artifact"),
      };
    }
    return {
      className: "green",
      stateLabel: "Passing",
      exitClass: firstPresent(compatibility.exit_class, "exit success"),
      phase: firstPresent(compatibility.phase, "check"),
      diagnosticDeltas: firstPresent(compatibility.diagnostic_deltas, "none recorded"),
    };
  }

  const status = String(row?.status || "").toLowerCase();
  if (!row || status.includes("not recorded") || status.includes("fixture") || status.includes("tsc fixture")) {
    return {
      className: "gray",
      stateLabel: "Not measured",
      exitClass: firstPresent(
        compatibility.exit_class,
        status.includes("tsc fixture") ? "fixture invalid" : "missing or incomplete artifact",
      ),
      phase: firstPresent(compatibility.phase, status.includes("fixture") ? "fixture setup" : "artifact"),
      diagnosticDeltas: firstPresent(compatibility.diagnostic_deltas, "not available"),
    };
  }

  if (status.includes("diagnostic mismatch") || diagnosticStatus.includes("diagnostic mismatch")) {
    return {
      className: "yellow",
      stateLabel: "Errors",
      exitClass: firstPresent(compatibility.exit_class, "diagnostic mismatch"),
      phase: firstPresent(compatibility.phase, "check"),
      diagnosticDeltas: firstPresent(compatibility.diagnostic_deltas, "not captured by latest artifact"),
    };
  }

  const isTimeout = status.includes("timeout") ||
    String(compatibility.exit_class || "").toLowerCase().includes("timeout");
  return {
    className: "red",
    stateLabel: isTimeout ? "Timeout" : "Fails",
    exitClass: firstPresent(compatibility.exit_class, isTimeout ? "timeout" : "nonzero exit"),
    phase: firstPresent(compatibility.phase, "check"),
    diagnosticDeltas: firstPresent(compatibility.diagnostic_deltas, "not captured by latest artifact"),
  };
}

function compatibilityRowFor(definition, allResults, artifact) {
  const row = allResults.find((candidate) => candidate?.name === definition.name);
  const artifactFamily = firstPresent(row?.compatibility?.semantic_owner_family, row?.compatibility?.owner_family);
  const ownerFamily = artifactFamily || definition.family;
  const compatibility = row?.compatibility || {};
  const diagnosticSubsystems = normalizedDiagnosticSubsystems(compatibility);
  const missingMetadata = missingCompatibilityMetadata(row, artifact);
  const artifactMetadata = artifactMetadataFor(row, artifact);
  const state = compatibilityState(row);
  const fallbackBlockers = state.className === "green"
    ? []
    : [
        row?.status ? String(row.status) : "",
        ownerFamily ? `owner family: ${ownerFamily}` : "",
      ];
  return {
    ...definition,
    family: ownerFamily,
    ...state,
    row,
    lines: row?.lines || 0,
    filesReached: compatibility.files_reached ?? null,
    filesReachedReason: compatibility.files_reached_reason ?? null,
    firstFailureClass: compatibility.first_failure_class || null,
    ownerTrack: firstPresent(compatibility.owner_track, definition.owner),
    reducedReproPath: compatibility.reduced_repro_path || null,
    lastSuccessfulPhase: normalizedLastSuccessfulPhase(compatibility),
    peakMemoryBytes: compatibility.peak_memory_bytes ?? null,
    peakMemoryBytesReason: compatibility.peak_memory_bytes_reason ?? null,
    emitStatus: compatibility.emit_status || "not in scope (noEmit project check)",
    dtsStatus: compatibility.dts_status || "not in scope (noEmit project check)",
    knownBlockers: normalizedKnownBlockers(compatibility, diagnosticSubsystems, fallbackBlockers),
    exitCodes: compatibility.exit_codes && typeof compatibility.exit_codes === "object"
      ? {
          tsc: Array.isArray(compatibility.exit_codes.tsc) ? compatibility.exit_codes.tsc.slice(0, 8) : [],
          tsz: Array.isArray(compatibility.exit_codes.tsz) ? compatibility.exit_codes.tsz.slice(0, 8) : [],
          tsgo: Array.isArray(compatibility.exit_codes.tsgo) ? compatibility.exit_codes.tsgo.slice(0, 8) : [],
        }
      : { tsc: [], tsz: [], tsgo: [] },
    diagnosticCodes: Array.isArray(compatibility.diagnostic_codes) ? compatibility.diagnostic_codes.slice(0, 8) : [],
    diagnosticSubsystems,
    primarySubsystem: compatibility.primary_subsystem || diagnosticSubsystems[0]?.subsystem || null,
    fixtureSources: normalizedFixtureSources(compatibility),
    reductionCandidates: Array.isArray(compatibility.reduction_candidates)
      ? compatibility.reduction_candidates.slice(0, 5)
      : [],
    artifactMetadata,
    freshnessWarnings: artifactFreshnessWarnings(artifactMetadata),
    missingMetadata,
    status: row?.status || "not recorded in latest benchmark artifact",
    url: benchmarkUrl({ name: definition.name }),
  };
}

const PROJECT_README_PATHS = {
  "large-ts-repo": [".target-bench/external/large-ts-repo/README.md"],
  nextjs: [".target-bench/external/next.js/README.md"],
  "nextjs-fresh-app": [".target-bench/external/next-app-live/README.md"],
  "vite-vanilla-ts-app": [".target-bench/external/vite-vanilla-ts-live/README.md"],
  "rxjs-project": [".target-bench/external/rxjs/README.md"],
  "type-fest-project": [".target-bench/external/type-fest/readme.md", ".target-bench/external/type-fest/README.md"],
  "zod-project": [".target-bench/external/zod/README.md"],
  "kysely-project": [".target-bench/external/kysely/README.md"],
  "utility-types-project": [".target-bench/external/utility-types/README.md"],
  "ts-toolbelt-project": [".target-bench/external/ts-toolbelt/README.md"],
  "ts-essentials-project": [".target-bench/external/ts-essentials/README.md"],
  "type-challenges-solutions-project": [".target/project-compile-guard/type-challenges-solutions/README.md"],
};

const PROJECT_README_URLS = {
  "large-ts-repo": "https://raw.githubusercontent.com/mohsen1/large-ts-repo/e1b22bda18664a507ed0da19c155e0365d585b18/README.md",
  "rxjs-project": "https://raw.githubusercontent.com/ReactiveX/rxjs/e5351d02e225e275ac0e497c7b66eaa5f0c88791/README.md",
  "zod-project": "https://raw.githubusercontent.com/colinhacks/zod/93b0b6892cc0cfee8d0bec4e2e1242c7df771f95/README.md",
  "utility-types-project": "https://raw.githubusercontent.com/piotrwitek/utility-types/2ee1f6ecb241651ab22390fee7ee5349942efda2/README.md",
  "ts-toolbelt-project": "https://raw.githubusercontent.com/millsp/ts-toolbelt/b8a49285e3ed3a7d8bb8e0b433389eac46a5f140/README.md",
  "ts-essentials-project": "https://raw.githubusercontent.com/ts-essentials/ts-essentials/5abe8700b42068048bd3c368e0531b6defe56558/README.md",
  "type-challenges-solutions-project": "https://raw.githubusercontent.com/ghaiklor/type-challenges-solutions/91a6d2986650475f29eeb3bd18ebd025128aa07e/README.md",
};

const NEXTJS_FRESH_APP_README = `# Fresh Next.js app benchmark

This fixture is generated by \`scripts/bench/generate-next-app-fixture.mjs\`.

Each benchmark run recreates the app, installs current npm versions, and type-checks the generated Next.js project with:

\`\`\`sh
tsz --noEmit -p tsconfig.json
tsgo --noEmit -p tsconfig.json
\`\`\`

The app intentionally imports and uses common type-heavy dependencies:

- \`zod\`
- \`@tanstack/react-query\`
- \`react-hook-form\`
- \`type-fest\`
- \`ts-pattern\`
- \`superjson\`
- \`date-fns\`
- \`clsx\`
- \`zustand\`
- \`valibot\`

The generated source mixes App Router pages, server actions, schema inference, discriminated unions, form helpers, query typing, store typing, and JSON-safe utility types so the benchmark reflects a modern application rather than a tiny startup file.`;

const REMOTE_FIXTURE_REFS = {
  "utility-types": "2ee1f6ecb241651ab22390fee7ee5349942efda2",
  "ts-toolbelt": "b8a49285e3ed3a7d8bb8e0b433389eac46a5f140",
  "ts-essentials": "5abe8700b42068048bd3c368e0531b6defe56558",
};

const TYPESCRIPT_VERSIONS_PATH = path.join(ROOT, "scripts/conformance/typescript-versions.json");

function currentTypeScriptRef() {
  const versions = readJsonIfExists(TYPESCRIPT_VERSIONS_PATH);
  return versions?.current || "4d4f005c8541e0255a9d8791205fdce326e462bc";
}

const TYPESCRIPT_FIXTURE_DIRS = [
  "tests/cases/compiler",
  "tests/cases/conformance",
];

const remoteSourceCache = new Map();

function escapeHtml(str) {
  return String(str)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function escapeAttributeJson(value) {
  return escapeHtml(JSON.stringify(value));
}

function readJsonIfExists(p) {
  try {
    return JSON.parse(fs.readFileSync(p, "utf8"));
  } catch {
    return null;
  }
}

function benchmarkArtifactFiles() {
  const artifactsDir = path.join(ROOT, "artifacts");
  const ciLatest = [
    "bench-vs-tsgo-github-latest.json",
    "bench-results.json",
  ].map((file) => path.join(artifactsDir, file));

  try {
    const localArtifacts = fs.readdirSync(artifactsDir)
      .filter((file) => file.startsWith("bench-vs-tsgo-") && file.endsWith(".json"))
      .filter((file) => !["bench-vs-tsgo-github-latest.json", "bench-results.json"].includes(file))
      .sort()
      .reverse()
      .map((file) => path.join(artifactsDir, file));
    return [...ciLatest, ...localArtifacts];
  } catch {
    return ciLatest;
  }
}

function sanitizeLegacyBenchmarkData(data) {
  if (data?.validation?.hyperfine_exit_codes_required === true) {
    return data;
  }
  if (!data?.results?.length) {
    return data;
  }
  return {
    ...data,
    results: data.results.filter((row) => row.name !== "large-ts-repo"),
  };
}

function loadBenchmarks() {
  const overrideArtifact = process.env.TSZ_WEBSITE_BENCHMARK_ARTIFACT;
  if (overrideArtifact) {
    const data = readJsonIfExists(overrideArtifact);
    if (data?.results) {
      return sanitizeLegacyBenchmarkData(data);
    }
  }

  const snapshotPath = path.join(ROOT, "crates/tsz-website/bench-snapshot.json");
  // Always use the latest available data. We do NOT gate selection on every app
  // being clean, on green-only project timing pairs, or on a minimum count —
  // benchmarks are allowed to fail individually; isChartEligible() decides which
  // rows render (tsz within 1.5x of tsgo). Gating selection here is what left the
  // whole dashboard empty whenever a canary/app legitimately crashed or timed out.
  const selectedArtifact = selectLatestBenchmarkArtifact([
    ...benchmarkArtifactFiles(),
    snapshotPath,
  ], {
    minimumProjectTimingPairs: 0,
  });
  if (selectedArtifact) {
    return sanitizeLegacyBenchmarkData(selectedArtifact.data);
  }

  return null;
}

function categoryFor(name, lines) {
  if (name === "large-ts-repo" || name === "nextjs") return "Projects: large repositories";
  const projectRow = PROJECT_ROWS_BY_NAME[name];
  if (projectRow) {
    if (projectRow.category === "generated") return "Projects: generated apps";
    if (projectRow.category === "application") return "Projects: applications";
    return "Projects: external libraries";
  }
  if (name.startsWith("utility-types/")) return "Single file: utility-types";
  if (name.startsWith("ts-toolbelt/")) return "Single file: ts-toolbelt";
  if (name.startsWith("ts-essentials/")) return "Single file: ts-essentials";
  if (/Recursive utility aliases|Indexed access hotspot|Remapped accessor hotspot|Conditional infer hotspot|Object spread hotspot|Contextual callback hotspot/i.test(name)) {
    return "Project Hotspot Microbenchmarks";
  }
  if (/Recursive generic|Conditional dist|Mapped type/i.test(name)) return "Solver Stress Tests";
  if (/\d+\s+classes|\d+\s+generic functions|\d+\s+union members|DeepPartial|Shallow optional/i.test(name)) {
    return "Synthetic Type Workloads";
  }
  return "General Benchmarks";
}

function categorySlug(category) {
  return String(category)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/(^-|-$)/g, "");
}

function isProjectCategory(category) {
  return String(category).startsWith("Projects:");
}

function isExternalLibraryCategory(category) {
  return (
    category === "Single file: utility-types" ||
    category === "Single file: ts-toolbelt" ||
    category === "Single file: ts-essentials"
  );
}

function libraryNameForCategory(category) {
  if (category.startsWith("Libraries: ")) {
    return category.slice("Libraries: ".length);
  }
  if (category.startsWith("Single file: ")) {
    return category.slice("Single file: ".length);
  }
  return "";
}

function categoryMeta(category) {
  return {
    "Projects: large repositories": {
      title: "Large repositories",
      description: "Full repository type-checks that stress project graph setup, residency, and cross-file analysis.",
    },
    "Projects: generated apps": {
      title: "Generated apps",
      description: "Programmatically created app projects with framework defaults and common TypeScript dependencies.",
    },
    "Projects: applications": {
      title: "Applications",
      description: "Pinned real-world applications checked with their own project configuration.",
    },
    "Projects: external libraries": {
      title: "External libraries",
      description: "Pinned real-world libraries and type-heavy repositories checked as project-mode fixtures.",
    },
    "Single file: utility-types": {
      title: "utility-types files",
      description: "Real-world utility-types file-level benchmark set from pinned snapshot.",
      repo: "https://github.com/piotrwitek/utility-types",
      repoLabel: "piotrwitek/utility-types",
    },
    "Single file: ts-toolbelt": {
      title: "ts-toolbelt files",
      description: "Real-world ts-toolbelt file-level benchmark set with type-heavy examples.",
      repo: "https://github.com/millsp/ts-toolbelt",
      repoLabel: "millsp/ts-toolbelt",
    },
    "Single file: ts-essentials": {
      title: "ts-essentials files",
      description: "Real-world ts-essentials file-level benchmark set from pinned snapshot.",
      repo: "https://github.com/ts-essentials/ts-essentials",
      repoLabel: "ts-essentials/ts-essentials",
    },
    "General Benchmarks": {
      title: "Compiler scenarios",
      description: "Focused compiler behavior on representative mixed workloads.",
    },
    "Synthetic Type Workloads": {
      title: "Generated type workloads",
      description: "Generated stress tests that isolate specific type-system patterns.",
    },
    "Project Hotspot Microbenchmarks": {
      title: "Project hotspot probes",
      description: "Focused synthetic rows that isolate hot patterns found in real project benchmark regressions.",
    },
    "Solver Stress Tests": {
      title: "Solver stress",
      description: "Upper-bound tests for recursive, mapped, and conditional type complexity.",
    },
  }[category] || { description: "" };
}

function displayName(name) {
  if (name === "privacyFunctionParameterDeclFile.ts") {
    return "Privacy function parameter declaration file";
  }
  if (name === "rxjs-project") return "RxJS project";
  if (name === "type-fest-project") return "type-fest project";
  if (name === "zod-project") return "Zod project";
  if (name === "nextjs-fresh-app") return "Fresh Next.js app";
  if (name === "vite-vanilla-ts-app") return "Fresh Vite app";
  if (name === "kysely-project") return "Kysely project";
  if (name === "type-challenges-solutions-project") return "type-challenges solutions project";

  const cleaned = String(name || "")
    .replace(/^utility-types\//, "")
    .replace(/^ts-toolbelt\//, "")
    .replace(/^ts-essentials\//, "")
    .replace(/^utility-types-project$/, "utility-types project")
    .replace(/^ts-toolbelt-project$/, "ts-toolbelt project")
    .replace(/^ts-essentials-project$/, "ts-essentials project")
    .replace(/^large-ts-repo$/, "large-ts-repo project")
    .replace(/^nextjs$/, "next.js full project")
    .replace(/\.ts$/, "")
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/_/g, " ")
    .replace(/-/g, " ");
  return cleaned.charAt(0).toUpperCase() + cleaned.slice(1);
}

function isTypeScriptFixtureName(name) {
  return String(name || "").endsWith(".ts") && !String(name || "").includes("/");
}

function displayBaseName(name) {
  return displayName(name)
    .replace(/\s+Speed Reasonable$/i, "")
    .replace(/\s+Not Too Large$/i, "")
    .trim();
}

function benchmarkTitle(row, category) {
  const name = String(row?.name || "");
  if (isProjectCategory(category)) return displayName(name);
  if (isExternalLibraryCategory(category)) return `${libraryNameForCategory(category)} file: ${displayBaseName(name)}`;
  if (isTypeScriptFixtureName(name)) return displayBaseName(name);
  return displayName(name);
}

function benchmarkSlug(name) {
  return String(name || "benchmark")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/(^-|-$)/g, "") || "benchmark";
}

function benchmarkUrl(row) {
  return `/benchmarks/${benchmarkSlug(row.name)}/`;
}

function benchmarkKind(category) {
  if (isProjectCategory(category)) return "project";
  if (isExternalLibraryCategory(category)) return "library file";
  if (category === "Project Hotspot Microbenchmarks") return "hotspot";
  if (category === "Solver Stress Tests") return "solver stress";
  if (category === "Synthetic Type Workloads") return "synthetic";
  return "benchmark";
}

function benchmarkFocus(row, category) {
  const name = String(row.name || "");
  if (name === "conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable.ts") {
    return "Official TypeScript compiler fixture that stresses conditional type discrimination across a large union without falling off a performance cliff.";
  }
  if (name === "manyConstExports.ts") {
    return "Official TypeScript compiler fixture that stresses binder/export-table setup for many constant exports.";
  }
  if (name === "binderBinaryExpressionStress.ts" || name === "binderBinaryExpressionStressJs.ts") {
    return "Official TypeScript compiler fixture that stresses binder traversal over a very large binary-expression tree.";
  }
  if (name === "binaryArithmeticControlFlowGraphNotTooLarge.ts") {
    return "Official TypeScript compiler fixture that keeps arithmetic control-flow graph construction bounded.";
  }
  if (name === "enumLiteralsSubtypeReduction.ts") {
    return "Official TypeScript compiler fixture that exercises enum literal subtype reduction and related assignability checks.";
  }
  if (name === "controlFlowArrays.ts") {
    return "Official TypeScript compiler fixture for array-sensitive control-flow analysis.";
  }
  if (/privacy/i.test(name)) {
    return "Official TypeScript compiler fixture for declaration privacy checks on public APIs.";
  }
  if (name === "typedArrays.ts") {
    return "Generated fixture that type-checks typed-array constructor and from() overload surfaces.";
  }
  if (isProjectCategory(category)) {
    return PROJECT_DESCRIPTIONS[name] ?? "Full project type-check throughput, including module graph setup and cross-file type analysis.";
  }
  if (name.includes("Recursive generic")) {
    return "Recursive generic instantiation and cache behavior under deep type expansion.";
  }
  if (name.includes("Conditional dist")) {
    return "Distributive conditional types over broad unions.";
  }
  if (name.includes("Mapped type") || /DeepPartial|Shallow optional/i.test(name)) {
    return "Mapped-type and property traversal behavior in the solver.";
  }
  if (name.includes("union members")) {
    return "Union construction, reduction, and assignability checks.";
  }
  if (name.includes("Recursive utility aliases")) {
    return "Recursive utility alias applications that stress generic instantiation, substitution, and cache reuse.";
  }
  if (name.includes("Indexed access hotspot")) {
    return "Indexed access over mapped reader helpers, a reduced shape from project-row property access pressure.";
  }
  if (name.includes("Remapped accessor hotspot")) {
    return "Mapped-type key remapping with accessor-like property surfaces.";
  }
  if (name.includes("Conditional infer hotspot")) {
    return "Conditional infer extraction chains that probe repeated evaluation and inference reuse.";
  }
  if (name.includes("Object spread hotspot")) {
    return "Object spread inference and property merging from project-style update pipelines.";
  }
  if (name.includes("Contextual callback hotspot")) {
    return "Contextual typing through callback dispatch tables with repeated generic payload shapes.";
  }
  if (name.includes("classes")) {
    return "Class declaration binding plus constructor/member shape checking.";
  }
  if (name.includes("generic functions")) {
    return "Generic signature checking and type-parameter environment setup.";
  }
  if (isExternalLibraryCategory(category)) {
    return `Single-file type-check from ${libraryNameForCategory(category)} with real-world helper types.`;
  }
  if (/privacy/i.test(name)) {
    return "Declaration emit privacy checks for public APIs that reference private parameter types.";
  }
  if (/binder/i.test(name)) {
    return "Binder and symbol-table setup for syntax-heavy TypeScript input.";
  }
  if (/controlflow|cfa/i.test(name)) {
    return "Control-flow graph construction and narrowing analysis.";
  }
  if (/enum/i.test(name)) {
    return "Enum literal subtype reduction and related assignability checks.";
  }
  return `No-emit type-check timing for ${displayName(name).toLowerCase()}.`;
}

function snippetForBenchmark(row, category) {
  const name = String(row.name || "");
  const generatedSource = generatedBenchmarkSource(name);
  if (generatedSource) return generatedSource;

  if (name.includes("Recursive generic")) {
    return `type Recurse<T, N extends number> =
  N extends 0 ? T : Recurse<{ value: T }, N>;

type Result = Recurse<string, 40>;`;
  }
  if (name.includes("Conditional dist")) {
    return `type Dist<T> = T extends unknown
  ? { value: T; optional?: T }
  : never;

type Result = Dist<"a" | "b" | "c">;`;
  }
  if (name.includes("Mapped type") || /DeepPartial/i.test(name)) {
    return `type DeepPartial<T> = {
  [K in keyof T]?: T[K] extends object
    ? DeepPartial<T[K]>
    : T[K];
};`;
  }
  if (/Shallow optional/i.test(name)) {
    return `type Optional<T> = {
  [K in keyof T]?: T[K];
};`;
  }
  if (name.includes("union members")) {
    return `type Variant =
  | { kind: "a"; value: string }
  | { kind: "b"; value: number }
  | { kind: "c"; value: boolean };`;
  }
  if (name.includes("classes")) {
    return `class Example {
  constructor(public id: string) {}
  read(): string { return this.id; }
}`;
  }
  if (name.includes("generic functions")) {
    return `function map<T, U>(
  value: T,
  fn: (value: T) => U,
): U {
  return fn(value);
}`;
  }
  if (isProjectCategory(category)) {
    return `# Project benchmark
tsz --noEmit -p tsconfig.json
tsgo --noEmit -p tsconfig.json`;
  }
  if (isExternalLibraryCategory(category)) {
    return `import type { DeepPartial } from "./helpers";

type Fixture<T> = DeepPartial<T> & {
  readonly id: string;
};`;
  }
  return `type Fixture<T> = {
  [K in keyof T]: T[K] extends string
    ? K
    : never;
};`;
}

function readFixtureSource(name) {
  const fixtureName = String(name || "");
  if (!fixtureName.endsWith(".ts") || fixtureName.includes("/")) return null;

  const candidates = TYPESCRIPT_FIXTURE_DIRS.map((dir) => path.join(ROOT, "TypeScript", dir, fixtureName));

  for (const candidate of candidates) {
    try {
      return fs.readFileSync(candidate, "utf8").trimEnd();
    } catch {
      // Keep looking in the next known TypeScript fixture location.
    }
  }

  const ref = currentTypeScriptRef();
  for (const dir of TYPESCRIPT_FIXTURE_DIRS) {
    const remote = readRemoteText(`https://raw.githubusercontent.com/microsoft/TypeScript/${ref}/${dir}/${fixtureName}`);
    if (remote) return remote;
  }

  return null;
}

function externalFixturePath(name) {
  const fixtureName = String(name || "");
  if (fixtureName.startsWith("utility-types/")) {
    return path.join(ROOT, ".target-bench/external/utility-types/src", fixtureName.slice("utility-types/".length));
  }
  if (fixtureName.startsWith("ts-toolbelt/")) {
    return path.join(ROOT, ".target-bench/external/ts-toolbelt/sources", fixtureName.slice("ts-toolbelt/".length));
  }
  if (fixtureName.startsWith("ts-essentials/")) {
    const rel = fixtureName.slice("ts-essentials/".length).replace(/\.ts$/, "/index.ts");
    return path.join(ROOT, ".target-bench/external/ts-essentials/lib", rel);
  }
  return null;
}

function externalFixtureUrl(name) {
  const fixtureName = String(name || "");
  if (fixtureName.startsWith("utility-types/")) {
    const rel = fixtureName.slice("utility-types/".length);
    return `https://raw.githubusercontent.com/piotrwitek/utility-types/${REMOTE_FIXTURE_REFS["utility-types"]}/src/${rel}`;
  }
  if (fixtureName.startsWith("ts-toolbelt/")) {
    const rel = fixtureName.slice("ts-toolbelt/".length);
    return `https://raw.githubusercontent.com/millsp/ts-toolbelt/${REMOTE_FIXTURE_REFS["ts-toolbelt"]}/sources/${rel}`;
  }
  if (fixtureName.startsWith("ts-essentials/")) {
    const rel = fixtureName.slice("ts-essentials/".length).replace(/\.ts$/, "/index.ts");
    return `https://raw.githubusercontent.com/ts-essentials/ts-essentials/${REMOTE_FIXTURE_REFS["ts-essentials"]}/lib/${rel}`;
  }
  return null;
}

function readRemoteText(url) {
  if (!url) return null;
  if (remoteSourceCache.has(url)) return remoteSourceCache.get(url);

  try {
    const text = execFileSync("curl", ["-fsSL", url], {
      encoding: "utf8",
      timeout: 10000,
      maxBuffer: 1024 * 1024,
      stdio: ["ignore", "pipe", "ignore"],
    }).trimEnd();
    remoteSourceCache.set(url, text);
    return text;
  } catch {
    remoteSourceCache.set(url, null);
    return null;
  }
}

function readExternalFixtureSource(name) {
  const sourcePath = externalFixturePath(name);
  if (sourcePath) {
    try {
      return fs.readFileSync(sourcePath, "utf8").trimEnd();
    } catch {
      // Deployed static builds may not have the prepared benchmark fixtures.
    }
  }

  return readRemoteText(externalFixtureUrl(name));
}

function sourceFilesForBenchmark(row, category) {
  if (isProjectCategory(category)) return [];

  const name = String(row.name || "fixture.ts");
  const fixtureName = name.endsWith(".ts") ? name : `${name}.ts`;
  const artifactSource = typeof row?.source?.content === "string" && row.source.content
    ? row.source.content.trimEnd()
    : null;
  if (artifactSource) {
    return [{
      name: row.source.path || fixtureName,
      language: "typescript",
      source: artifactSource,
    }];
  }

  const externalSource = isExternalLibraryCategory(category)
    ? readExternalFixtureSource(fixtureName)
    : null;
  const snippet = externalSource || readFixtureSource(fixtureName) || snippetForBenchmark(row, category);

  if (isExternalLibraryCategory(category)) {
    if (!externalSource) return [];
    return [{
      name: fixtureName,
      language: "typescript",
      source: externalSource,
    }];
  }

  return [
    {
      name: fixtureName,
      language: "typescript",
      source: snippet,
    },
  ];
}

function benchmarkCommand(row, category, compiler) {
  if (isProjectCategory(category)) {
    return `${compiler} --noEmit -p tsconfig.json`;
  }
  const name = String(row.name || "fixture.ts");
  return `${compiler} --noEmit ${name.endsWith(".ts") ? name : `${name}.ts`}`;
}

function projectReadmeRemoteUrls(definition) {
  // Derive raw.githubusercontent.com URLs from the project row's repo + ref +
  // readme_candidates. Only works for GitHub-hosted repos.
  const { repo, ref, readme_candidates: candidates } = definition;
  if (!repo || !ref || !Array.isArray(candidates) || !candidates.length) return [];
  const match = String(repo).match(/github\.com\/([^/]+\/[^/]+?)(?:\.git)?$/);
  if (!match) return [];
  const slug = match[1];
  return candidates.map(
    (candidate) => `https://raw.githubusercontent.com/${slug}/${ref}/${candidate}`,
  );
}

function readProjectReadme(row, category) {
  if (!isProjectCategory(category)) return null;

  if (row.readme) return truncateReadme(row.readme);

  // 1. Try hardcoded local paths (legacy entries that pre-date project-rows.mjs
  //    metadata, or that use non-standard fixture paths).
  const localCandidates = PROJECT_README_PATHS[row.name] || [];
  for (const candidate of localCandidates) {
    try {
      const text = fs.readFileSync(path.join(ROOT, candidate), "utf8").trim();
      if (!text) continue;
      return truncateReadme(text);
    } catch {
      // README is optional for local benchmark fixtures that have not been prepared.
    }
  }

  // 2. Try dynamic local paths derived from fixture_dir + readme_candidates
  //    (covers all project-rows.mjs entries automatically).
  const definition = PROJECT_ROWS_BY_NAME[row.name];
  if (definition?.fixture_dir && Array.isArray(definition.readme_candidates)) {
    const fixtureBase = path.join(ROOT, ".target-bench/external", definition.fixture_dir);
    for (const candidate of definition.readme_candidates) {
      try {
        const text = fs.readFileSync(path.join(fixtureBase, candidate), "utf8").trim();
        if (!text) continue;
        return truncateReadme(text);
      } catch {
        // Fixture may not be cloned locally.
      }
    }
  }

  if (row.name === "nextjs-fresh-app") return NEXTJS_FRESH_APP_README;

  // 3. Try hardcoded remote URLs (legacy, kept for compatibility).
  const hardcodedRemote = readRemoteText(PROJECT_README_URLS[row.name]);
  if (hardcodedRemote) return truncateReadme(hardcodedRemote);

  // 4. Try dynamic remote URLs derived from project-rows.mjs metadata.
  if (definition) {
    for (const url of projectReadmeRemoteUrls(definition)) {
      const remote = readRemoteText(url);
      if (remote) return truncateReadme(remote);
    }
  }

  return null;
}

function truncateReadme(text) {
  const trimmed = String(text || "").trim();
  if (!trimmed) return null;
  return trimmed.length > 18000 ? `${trimmed.slice(0, 18000).trimEnd()}\n\n...` : trimmed;
}

function comparison(row) {
  const tsz = Number(row.tsz_ms);
  const tsgo = Number(row.tsgo_ms);
  if (!Number.isFinite(tsz) || !Number.isFinite(tsgo) || tsz <= 0 || tsgo <= 0) {
    return {
      available: false,
      winner: row.status ? "unavailable" : "unknown",
      factor: null,
      deltaMs: null,
      percent: null,
    };
  }
  const winner = tsz < tsgo ? "tsz" : tsgo < tsz ? "tsgo" : "tie";
  const factor = Math.max(tsz, tsgo) / Math.min(tsz, tsgo);
  return {
    available: true,
    winner,
    factor,
    deltaMs: Math.abs(tsz - tsgo),
    percent: ((tsz - tsgo) / tsgo) * 100,
  };
}

function decorateRow(row, category, options = {}) {
  const maxMs = Math.max(Number(row.tsz_ms) || 0, Number(row.tsgo_ms) || 0);
  // A killed/errored row never carries a speed ratio (#16196); compute the flag
  // once and gate the labels themselves, so no downstream view reads a win off it.
  const rowDidNotFinish = didNotFinish(row);
  const sourceFiles = sourceFilesForBenchmark(row, category);
  const focus = benchmarkFocus(row, category);
  const readme = readProjectReadme(row, category);
  const decorated = {
    ...row,
    category,
    category_slug: categorySlug(category),
    display_name: benchmarkTitle(row, category),
    slug: benchmarkSlug(row.name),
    url: benchmarkUrl(row),
    kind: benchmarkKind(category),
    focus,
    detail_focus: focus,
    snippet: sourceFiles[0]?.source || snippetForBenchmark(row, category),
    source_files: sourceFiles,
    readme,
    readme_html: readme ? marked.parse(readme) : "",
    tsz_command: benchmarkCommand(row, category, "tsz"),
    tsgo_command: benchmarkCommand(row, category, "tsgo"),
    tsz_time: row.tsz_ms ? formatDurationMs(row.tsz_ms, 2) : "",
    tsgo_time: row.tsgo_ms ? formatDurationMs(row.tsgo_ms, 2) : "",
    tsz_width: maxMs > 0 && row.tsz_ms ? Math.max(1, (row.tsz_ms / maxMs) * 100).toFixed(2) : "1.00",
    tsgo_width: maxMs > 0 && row.tsgo_ms ? Math.max(1, (row.tsgo_ms / maxMs) * 100).toFixed(2) : "1.00",
    status_label: (row.status || rowDidNotFinish) ? statusLabel(row) : "",
    failed: isFailedBenchmark(row),
    is_aggregate: Boolean(options.isAggregate),
  };
  decorated.source_files_json = escapeAttributeJson(decorated.source_files);
  decorated.comparison = comparison(decorated);
  decorated.speedup_label = rowDidNotFinish
    ? ""
    : formatSpeedupLabel(decorated.tsz_ms, decorated.tsgo_ms);
  return decorated;
}

function buildGroupedBenchmarks(data) {
  const allResults = withExpectedProjectRows(data?.results);
  const results = allResults.filter(isChartEligible);
  const grouped = new Map();

  for (const row of results) {
    const category = categoryFor(row.name || "", row.lines);
    const bucket = grouped.get(category) || [];
    bucket.push(row);
    grouped.set(category, bucket);
  }

  const successfulNames = new Set([
    ...results.map((row) => row.name),
    ...[...grouped.values()].flat().map((row) => row.name),
  ]);
  const failedResults = allResults.filter((row) => (
    (isFailedBenchmark(row) || isExcludedSlowTimedRow(row)) && !successfulNames.has(row.name)
  ));

  const order = [
    "Projects: external libraries",
    "Projects: applications",
    "Projects: generated apps",
    "Projects: large repositories",
    "Single file: utility-types",
    "Single file: ts-toolbelt",
    "Single file: ts-essentials",
    "General Benchmarks",
    "Synthetic Type Workloads",
    "Project Hotspot Microbenchmarks",
    "Solver Stress Tests",
  ];

  const categories = [...grouped.keys()].sort((a, b) => {
    const ia = order.indexOf(a);
    const ib = order.indexOf(b);
    if (ia === -1 && ib === -1) return a.localeCompare(b);
    if (ia === -1) return 1;
    if (ib === -1) return -1;
    return ia - ib;
  });

  return { allResults, results, failedResults, grouped, categories };
}

export function getBenchmarkPages() {
  const data = loadBenchmarks();
  if (!data?.results?.length) return [];

  const { grouped, categories, failedResults } = buildGroupedBenchmarks(data);
  const pages = [];
  const seen = new Set();

  for (const category of categories) {
    const entries = (grouped.get(category) || []).slice();

    entries.sort((a, b) => {
      const aLines = Number(a.lines) || 0;
      const bLines = Number(b.lines) || 0;
      if (bLines !== aLines) return bLines - aLines;
      return String(a.name || "").localeCompare(String(b.name || ""));
    });

    for (const row of entries) {
      if (seen.has(row.name)) continue;
      seen.add(row.name);
      pages.push(decorateRow(row, category, { isAggregate: row.is_aggregate }));
    }
  }

  for (const row of failedResults) {
    if (seen.has(row.name)) continue;
    seen.add(row.name);
    const category = categoryFor(row.name || "", row.lines);
    pages.push(decorateRow(row, category));
  }

  return pages;
}

function categoryDescription(category) {
  return categoryMeta(category).description || "";
}

function categoryTitle(category) {
  return categoryMeta(category).title || category;
}

function categoryBelongsToMode(category, mode) {
  if (mode === "projects") return isProjectCategory(category);
  if (mode === "micro") return !isProjectCategory(category);
  return true;
}

function failedBelongsToMode(row, mode) {
  const category = categoryFor(row.name || "", row.lines);
  return categoryBelongsToMode(category, mode);
}

function generateCharts(data, mode = "projects") {
  if (!data?.results?.length) {
    return `<div class="bench-placeholder">No benchmark data is available for this local build.</div>`;
  }

  const { results, failedResults, grouped, categories } = buildGroupedBenchmarks(data);
  if (!results.length && !failedResults.length) return "";

  const barMaxWidth = 420;
  const entriesForCategory = (category) => {
    return (grouped.get(category) || []).slice();
  };
  const categoryTszSpeedupScore = (category) => Math.max(
    -Infinity,
    ...entriesForCategory(category).map(tszSpeedupScore),
  );
  const visibleCategories = categories
    .filter((category) => categoryBelongsToMode(category, mode))
    .sort((a, b) => {
      if (mode !== "projects") return 0;
      const aScore = categoryTszSpeedupScore(a);
      const bScore = categoryTszSpeedupScore(b);
      if (aScore !== bScore) return bScore - aScore;
      return categoryTitle(a).localeCompare(categoryTitle(b));
    });
  const visibleFailedResults = failedResults.filter((row) => failedBelongsToMode(row, mode));
  // Scale bars only against rows that actually render bars. Failed/excluded rows
  // (too slow, incomplete) are listed as text below, so their timings must NOT
  // inflate the bar scale -- otherwise one off-chart slow row (e.g. a 7s outlier)
  // shrinks every on-chart bar to an unreadable sliver.
  const chartMaxMs = Math.max(
    1,
    ...visibleCategories
      .flatMap((category) => entriesForCategory(category))
      .flatMap((row) => [Number(row.tsz_ms) || 0, Number(row.tsgo_ms) || 0]),
  );

  let html = "";
  if (mode === "projects" && visibleCategories.length === 0 && visibleFailedResults.length === 0) {
    html += `<div class="bench-placeholder">No successful project benchmark timing pairs are available in this artifact yet. Project rows below are still tracked for compile readiness.</div>\n`;
  }

  for (const category of visibleCategories) {
    const entries = entriesForCategory(category);
    const slug = categorySlug(category);
    const meta = categoryMeta(category);
    const isProject = isProjectCategory(category);
    if (!entries.length) continue;

    entries.sort((a, b) => {
      if (isProject) {
        return compareByTszSpeedup(a, b);
      } else {
        const aLines = Number(a.lines) || 0;
        const bLines = Number(b.lines) || 0;
        if (bLines !== aLines) return bLines - aLines;
      }
      return (String(a.name || "") > String(b.name || "") ? 1 : -1);
    });
    const desc = category === "Projects: generated apps" || category === "Projects: applications" || !isProject
      ? categoryDescription(category)
      : "";
    const repoLink = meta.repo
      ? ` <a class="bench-category-repo" href="${meta.repo}" target="_blank" rel="noopener noreferrer">${escapeHtml(meta.repoLabel || meta.repo)}</a>`
      : "";
    const title = categoryTitle(category);

    html += `<section class="bench-category${isProject ? " bench-project-category" : ""}">
  <h3 class="bench-category-title" id="${slug}">${escapeHtml(title)}${repoLink}</h3>
  ${desc ? `<p class="bench-category-desc">${escapeHtml(desc)}</p>` : ""}
  <div class="bench-chart">\n`;

    for (const r of entries) {
      const decorated = decorateRow(r, category, { isAggregate: r.is_aggregate });
      const tszWidth = (r.tsz_ms / chartMaxMs) * barMaxWidth;
      const tsgoWidth = (r.tsgo_ms / chartMaxMs) * barMaxWidth;
      const winnerLabel = formatSpeedupLabel(r.tsz_ms, r.tsgo_ms);
      const tszLabel = formatDurationMs(r.tsz_ms);
      const tsgoLabel = formatDurationMs(r.tsgo_ms);

      const metaParts = isProject
        ? [`${fmt(r.lines || 0)} lines`, `${fmt(r.kb || 0)} KB`]
        : [decorated.kind, `${fmt(r.lines || 0)} lines`, `${fmt(r.kb || 0)} KB`];

      html += `  <div class="bench-row">
    <div class="bench-name"><a href="${decorated.url}">${escapeHtml(decorated.display_name)}</a></div>
    <div class="bench-meta">${escapeHtml(metaParts.join(" · "))}</div>
    ${isProject ? "" : `<p class="bench-focus">${escapeHtml(decorated.focus)}</p>`}
    <div class="bench-bars">
      <div class="bench-bar-row">
        <span class="bench-bar-label">tsz</span>
        ${renderBenchmarkBar("tsz", tszWidth, tszLabel)}
      </div>
      <div class="bench-bar-row">
        <span class="bench-bar-label">tsgo</span>
        ${renderBenchmarkBar("tsgo", tsgoWidth, tsgoLabel)}
      </div>
      ${winnerLabel ? `<div class="bench-winner">${winnerLabel}</div>` : ""}
    </div>
    <a class="bench-detail-link" href="${decorated.url}">View details</a>
  </div>\n`;
    }

    html += `  </div>
 </section>\n`;
  }

  if (visibleFailedResults.length > 0) {
    const failedTitle = mode === "projects"
      ? "Not charted: canaries, incomplete, or tsz slower than tsgo"
      : "Not charted: incomplete or tsz slower than tsgo";
    const failedDescription = mode === "projects"
      ? "Rows that ran but are not in the timed chart: compile canaries, rows without a full tsz and tsgo timing pair, or rows where tsz is at least 1.5x slower than tsgo."
      : "Rows without a full tsz and tsgo timing pair, or where tsz is at least 1.5x slower than tsgo.";
    html += `<section class="bench-category bench-failures">
  <h3 class="bench-category-title" id="failures">${escapeHtml(failedTitle)}</h3>
  <p class="bench-category-desc">${escapeHtml(failedDescription)}</p>
  <ul class="bench-failure-list">\n`;
    for (const r of visibleFailedResults) {
      const category = categoryFor(r.name || "", r.lines);
      const decorated = decorateRow(r, category);
      html += `  <li>
    <a href="${decorated.url}">${escapeHtml(displayName(r.name))}</a>
    <span>${escapeHtml(statusLabel(r))}</span>
  </li>\n`;
    }
    html += `  </ul>
 </section>\n`;
  }

  return html;
}

export function getBenchmarkCharts() {
  return generateCharts(loadBenchmarks(), "projects");
}

export function getBenchmarkMicroCharts() {
  return generateCharts(loadBenchmarks(), "micro");
}

export function getBenchmarkEnvironmentSummary() {
  const summary = runnerEnvironmentSummary(loadBenchmarks());
  if (!summary) return "";
  // summary is already HTML-safe (text fragments escaped; the timestamp and sha
  // carry intentional <relative-time>/<a> markup), so it is embedded raw here.
  return `<details class="bench-runner-details">
  <summary>show runner info</summary>
  <p class="bench-runner-meta">${summary}</p>
</details>`;
}

export function getProjectCompatibilityDashboard() {
  const data = loadBenchmarks();
  const allResults = withExpectedProjectRows(data?.results);
  // Render EVERY defined corpus row, including unmeasured ("gray") ones (#16310).
  // A row that is never measured is otherwise indistinguishable, downstream,
  // from a row that does not exist — so coverage can silently shrink and nothing
  // reports it. An unmeasured row is shown as "Not measured" and still carries
  // its reason in the Exit class column ("missing or incomplete artifact" /
  // "fixture invalid"), which is honest and creates pressure to fix it.
  const rows = COMPATIBILITY_CORPUS_ROWS
    .map((definition) => compatibilityRowFor(definition, allResults, data));

  const measuredCount = rows.filter((row) => row.className !== "gray").length;
  const notMeasuredCount = rows.length - measuredCount;

  if (!rows.length) {
    return `<section class="compat-dashboard">
  <h2>Project compatibility</h2>
  <p class="compat-dashboard-intro">No measured project compatibility rows are available in this build yet.</p>
</section>`;
  }

  const numericSortValue = (value) => {
    const number = finiteNumber(value);
    return number === null ? "" : String(number);
  };

  const sortableHeader = (key, label, type = "text") =>
    `<button type="button" class="compat-sort-button" data-compat-sort="${key}" data-sort-type="${type}" aria-label="Sort project compatibility by ${escapeHtml(label)}">${escapeHtml(label)}</button>`;

  const sortScript = `<script>
(() => {
  for (const table of document.querySelectorAll("[data-compat-sortable]")) {
    const tbody = table.tBodies[0];
    if (!tbody) continue;
    const buttons = Array.from(table.querySelectorAll("[data-compat-sort]"));
    for (const button of buttons) {
      button.addEventListener("click", () => {
        const key = button.dataset.compatSort;
        const type = button.dataset.sortType || "text";
        const direction = button.dataset.direction === "asc" ? "desc" : "asc";
        for (const candidate of buttons) {
          candidate.dataset.direction = "";
          candidate.removeAttribute("aria-sort");
        }
        button.dataset.direction = direction;
        button.setAttribute("aria-sort", direction === "asc" ? "ascending" : "descending");
        const rows = Array.from(tbody.rows);
        rows.sort((left, right) => {
          const leftCell = left.querySelector(\`[data-sort-key="\${key}"]\`);
          const rightCell = right.querySelector(\`[data-sort-key="\${key}"]\`);
          const leftRaw = leftCell?.dataset.sortValue ?? "";
          const rightRaw = rightCell?.dataset.sortValue ?? "";
          let comparison = 0;
          if (type === "number") {
            const leftNumber = Number(leftRaw);
            const rightNumber = Number(rightRaw);
            const leftMissing = leftRaw === "" || !Number.isFinite(leftNumber);
            const rightMissing = rightRaw === "" || !Number.isFinite(rightNumber);
            if (leftMissing || rightMissing) {
              comparison = leftMissing === rightMissing ? 0 : leftMissing ? 1 : -1;
            } else {
              comparison = leftNumber - rightNumber;
            }
          } else {
            comparison = leftRaw.localeCompare(rightRaw, undefined, { sensitivity: "base", numeric: true });
          }
          if (comparison === 0) {
            comparison = (left.dataset.project || "").localeCompare(right.dataset.project || "", undefined, { sensitivity: "base" });
          }
          return direction === "asc" ? comparison : -comparison;
        });
        for (const row of rows) tbody.append(row);
      });
    }
  }
})();
</script>`;

  const runStatusRaw = String(data?.run_status || "").trim();
  let runStatusLabel = "";
  if (runStatusRaw === "local") runStatusLabel = "local snapshot (not a CI run)";
  else if (runStatusRaw) runStatusLabel = `run status: ${runStatusRaw}`;
  const provenanceSummary = runnerEnvironmentSummary(data);
  const provenanceParts = [provenanceSummary, escapeHtml(runStatusLabel)].filter(Boolean);
  const provenanceLine = provenanceParts.length
    ? `\n  <p class="compat-dashboard-meta">${provenanceParts.join(" · ")}</p>`
    : "";
  const coverageLine = `<p class="compat-dashboard-coverage">${measuredCount} of ${rows.length} defined corpus rows measured${notMeasuredCount ? ` · ${notMeasuredCount} not measured` : ""}.</p>`;

  return `<section class="compat-dashboard">
  <h2>Project compatibility</h2>
  <p class="compat-dashboard-intro">These rows track real project fixtures that <code>tsc</code> accepts. A green row means <code>tsz</code> completed the same project check; red or yellow rows identify the current compatibility blocker. A "Not measured" row is a defined corpus project with no measurement in the latest artifact.</p>
  ${coverageLine}${provenanceLine}
  <div class="compat-table-wrap">
    <table class="compat-table" data-compat-sortable>
      <thead>
        <tr>
          <th scope="col">${sortableHeader("project", "Project")}</th>
          <th scope="col">${sortableHeader("state", "State")}</th>
          <th scope="col">${sortableHeader("exit", "Exit class")}</th>
          <th scope="col">${sortableHeader("phase", "Phase")}</th>
          <th scope="col">${sortableHeader("files", "Files", "number")}</th>
          <th scope="col">${sortableHeader("peak", "Peak RSS", "number")}</th>
        </tr>
      </thead>
      <tbody>
        ${rows.map((row) => `<tr class="compat-item" data-project="${escapeHtml(row.label)}">
          <td class="compat-project" data-sort-key="project" data-sort-value="${escapeHtml(row.label)}"><a href="${row.url}">${escapeHtml(row.label)}</a></td>
          <td data-sort-key="state" data-sort-value="${escapeHtml(row.className)}"><span class="compat-state ${row.className}">${escapeHtml(row.stateLabel)}</span></td>
          <td data-sort-key="exit" data-sort-value="${escapeHtml(row.exitClass || "")}"><span class="compat-detail">${escapeHtml(row.exitClass || "unknown")}</span></td>
          <td data-sort-key="phase" data-sort-value="${escapeHtml(row.phase || "")}"><span class="compat-detail">${escapeHtml(row.phase || "unknown")}</span></td>
          <td data-sort-key="files" data-sort-value="${numericSortValue(row.filesReached)}">${escapeHtml(formatFilesReached(row.filesReached) || "—")}</td>
          <td data-sort-key="peak" data-sort-value="${numericSortValue(row.peakMemoryBytes)}">${escapeHtml(formatPeakMemoryMiB(row.peakMemoryBytes) || "—")}</td>
        </tr>`).join("\n")}
      </tbody>
    </table>
  </div>
  ${sortScript}
</section>`;
}
