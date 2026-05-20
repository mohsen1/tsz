#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import {
  COMPILE_CANARY_PROJECT_ROWS,
  REQUIRED_COMPATIBILITY_FIELDS,
  REQUIRED_PROJECT_ROWS,
} from "./project-rows.mjs";
import { isGreen } from "./row-utils.mjs";

const REQUIRED_PROJECT_ROW_SET = new Set(REQUIRED_PROJECT_ROWS);
const PROJECT_COMPATIBILITY_ROW_SET = new Set([
  ...REQUIRED_PROJECT_ROWS,
  ...COMPILE_CANARY_PROJECT_ROWS,
]);

function hasProjectCompatibilityRows(rows) {
  return rows.some((row) => PROJECT_COMPATIBILITY_ROW_SET.has(row?.name));
}

function isNonEmptyString(value) {
  return typeof value === "string" && value.trim() !== "";
}

function isNonEmptyStringArray(value) {
  return Array.isArray(value) && value.some(isNonEmptyString);
}

function parseArgs(argv) {
  const [, , outFile, ...rest] = argv;
  const inputFiles = [];
  const compatibilityJsonlFiles = [];
  let requireRunnerSignature = false;

  for (let index = 0; index < rest.length; index += 1) {
    const arg = rest[index];
    if (arg === "--require-runner-signature") {
      requireRunnerSignature = true;
      continue;
    }
    if (arg === "--compat-jsonl") {
      const file = rest[index + 1];
      if (!file) {
        console.error("Missing value for --compat-jsonl");
        process.exit(2);
      }
      compatibilityJsonlFiles.push(file);
      index += 1;
      continue;
    }
    inputFiles.push(arg);
  }

  return { outFile, inputFiles, compatibilityJsonlFiles, requireRunnerSignature };
}

function readJsonl(file) {
  return fs.readFileSync(file, "utf8")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function mergeCompatibilityCanaries(results, compatibilityJsonlFiles) {
  if (compatibilityJsonlFiles.length === 0) return results;

  const canaryNames = new Set(COMPILE_CANARY_PROJECT_ROWS);
  const byName = new Map();
  const merged = results.map((row) => {
    if (row?.name) byName.set(row.name, row);
    return row;
  });

  for (const file of compatibilityJsonlFiles) {
    for (const compatibility of readJsonl(file)) {
      const name = compatibility?.name;
      if (!canaryNames.has(name)) continue;
      const existing = byName.get(name);
      if (existing) {
        existing.compatibility = compatibility;
        existing.status ||= "compile canary tracked in CI; not timed by vs-tsgo benchmarks";
        continue;
      }

      const row = {
        name,
        lines: Number.isFinite(Number(compatibility.files_reached)) ? Number(compatibility.files_reached) : 0,
        kb: 0,
        tsz_ms: null,
        tsgo_ms: null,
        tsz_lps: null,
        tsgo_lps: null,
        winner: "error",
        ratio: 0,
        status: "compile canary tracked in CI; not timed by vs-tsgo benchmarks",
        compatibility,
      };
      byName.set(name, row);
      merged.push(row);
    }
  }

  return merged;
}

function runnerHardwareSignature(environment) {
  return {
    platform: environment?.platform || null,
    arch: environment?.arch || null,
    release: environment?.release || null,
    cpu_count: environment?.cpu_count ?? null,
    cpu_model: environment?.cpu_model || null,
    total_memory_bytes: environment?.total_memory_bytes ?? null,
    github_runner_os: environment?.github_actions?.runner_os || null,
    github_runner_arch: environment?.github_actions?.runner_arch || null,
    cloud_build_machine_type: environment?.cloud_build?.machine_type || null,
  };
}

function validateRunnerEnvironmentConsistency(environments) {
  if (environments.length <= 1) return [];

  const baseline = runnerHardwareSignature(environments[0].environment);
  const warnings = [];
  for (const { file, environment } of environments.slice(1)) {
    const current = runnerHardwareSignature(environment);
    const mismatchedFields = Object.keys(baseline).filter((key) => baseline[key] !== current[key]);
    if (mismatchedFields.length > 0) {
      warnings.push({
        file: path.basename(file),
        mismatched_fields: mismatchedFields,
        expected: baseline,
        actual: current,
      });
    }
  }
  return warnings;
}

function isMissingSignatureValue(value) {
  return value === undefined || value === null || value === "";
}

function nestedValue(object, pathParts) {
  let current = object;
  for (const part of pathParts) {
    if (!current || typeof current !== "object") return undefined;
    current = current[part];
  }
  return current;
}

function collectMissingFields(object, prefix, fields) {
  const missing = [];
  for (const field of fields) {
    const value = nestedValue(object, field.split("."));
    if (isMissingSignatureValue(value)) {
      missing.push(`${prefix}.${field}`);
    }
  }
  return missing;
}

function collectRunnerSignatureFailures(payloads, runnerEnvironmentWarnings) {
  const failures = [];
  const seenShardLabels = new Map();

  for (const { file, payload } of payloads) {
    const basename = path.basename(file);
    const environment = payload.runner_environment;
    if (!environment || typeof environment !== "object") {
      failures.push(`${basename}: missing runner_environment`);
    } else {
      const missingEnvironmentFields = collectMissingFields(environment, "runner_environment", [
        "platform",
        "arch",
        "release",
        "cpu_count",
        "cpu_model",
        "total_memory_bytes",
      ]);
      if (environment.github_actions && typeof environment.github_actions === "object") {
        missingEnvironmentFields.push(...collectMissingFields(environment.github_actions, "runner_environment.github_actions", [
          "run_id",
          "run_attempt",
          "runner_os",
          "runner_arch",
          "workflow",
          "job",
          "sha",
        ]));
      }
      if (environment.cloud_build && typeof environment.cloud_build === "object") {
        missingEnvironmentFields.push(...collectMissingFields(environment.cloud_build, "runner_environment.cloud_build", [
          "machine_type",
        ]));
      }
      for (const field of missingEnvironmentFields) {
        failures.push(`${basename}: missing ${field}`);
      }
    }

    const missingPayloadFields = collectMissingFields(payload, "", [
      "source_commit",
      "workflow_run_id",
      "filter",
      "shard.label",
      "shard.filter",
    ]).map((field) => field.slice(1));
    for (const field of missingPayloadFields) {
      failures.push(`${basename}: missing ${field}`);
    }

    const shardLabel = payload.shard?.label;
    if (!isMissingSignatureValue(shardLabel)) {
      const previous = seenShardLabels.get(shardLabel);
      if (previous) {
        failures.push(`${basename}: duplicate shard.label ${JSON.stringify(shardLabel)} also used by ${previous}`);
      } else {
        seenShardLabels.set(shardLabel, basename);
      }
    }
  }

  for (const warning of runnerEnvironmentWarnings) {
    failures.push(`${warning.file}: runner_environment mismatch (${warning.mismatched_fields.join(", ")})`);
  }

  return failures;
}

function measurementProfileSignature(profile) {
  const pgo = profile?.profile_guided_optimization || {};
  return {
    mode: profile?.mode || null,
    tsz_binary_source: profile?.tsz_binary_source || null,
    profile_guided_optimization: {
      requested: pgo.requested ?? null,
      required: pgo.required ?? null,
      optimized: pgo.optimized ?? null,
      profile_fingerprint: pgo.profile_fingerprint || null,
      training_fingerprint: pgo.training_fingerprint || null,
      profile_data_source: pgo.profile_data_source || null,
      training_metadata_available: pgo.training_metadata_available ?? null,
      training_input_count: pgo.training_input_count ?? null,
      training_failure_count: pgo.training_failure_count ?? null,
      training_inputs: Array.isArray(pgo.training_inputs) ? pgo.training_inputs : [],
      training_failed_inputs: Array.isArray(pgo.training_failed_inputs) ? pgo.training_failed_inputs : [],
      config: {
        synthetic: pgo.config?.synthetic ?? null,
        fetch_utility_types: pgo.config?.fetch_utility_types ?? null,
        fetch_core_projects: pgo.config?.fetch_core_projects ?? null,
        panic_unwind: pgo.config?.panic_unwind ?? null,
        extra_inputs: pgo.config?.extra_inputs || null,
        training_timeout_seconds: pgo.config?.training_timeout_seconds ?? null,
        cache_enabled: pgo.config?.cache_enabled ?? null,
      },
    },
  };
}

function flattenForComparison(value, prefix = "", output = {}) {
  if (Array.isArray(value)) {
    output[prefix] = JSON.stringify(value);
    return output;
  }
  if (value && typeof value === "object") {
    for (const key of Object.keys(value).sort()) {
      flattenForComparison(value[key], prefix ? `${prefix}.${key}` : key, output);
    }
    return output;
  }
  output[prefix] = value;
  return output;
}

function validateMeasurementProfileConsistency(profiles) {
  if (profiles.length <= 1) return [];

  const baseline = flattenForComparison(measurementProfileSignature(profiles[0].profile));
  const baselineSignature = measurementProfileSignature(profiles[0].profile);
  const warnings = [];
  for (const { file, profile } of profiles.slice(1)) {
    const currentSignature = measurementProfileSignature(profile);
    const current = flattenForComparison(currentSignature);
    const allKeys = new Set([...Object.keys(baseline), ...Object.keys(current)]);
    const mismatchedFields = [...allKeys].filter((key) => baseline[key] !== current[key]).sort();
    if (mismatchedFields.length > 0) {
      warnings.push({
        file: path.basename(file),
        mismatched_fields: mismatchedFields,
        expected: baselineSignature,
        actual: currentSignature,
      });
    }
  }
  return warnings;
}

function collectProjectCompatibilityFailures(rows) {
  const projectRows = rows.filter((row) => PROJECT_COMPATIBILITY_ROW_SET.has(row?.name));
  if (projectRows.length === 0) return [];

  const failures = [];
  const seenNames = new Set();
  const duplicateNames = new Set();

  for (const row of projectRows) {
    if (seenNames.has(row.name)) {
      duplicateNames.add(row.name);
    } else {
      seenNames.add(row.name);
    }
  }
  for (const name of [...duplicateNames].sort()) {
    failures.push(`${name}: duplicate project row`);
  }

  if (projectRows.some((row) => REQUIRED_PROJECT_ROW_SET.has(row.name))) {
    for (const name of REQUIRED_PROJECT_ROWS) {
      if (!seenNames.has(name)) {
        failures.push(`${name}: missing project row`);
      }
    }
  }

  for (const row of projectRows) {
    // Rows explicitly marked artifact-missing are exempt from compatibility validation.
    // They represent runs where the compatibility artifact was not collected (e.g. timeout
    // before the artifact step ran). They must never appear as speed wins.
    if (row.artifact_missing === true) continue;
    if (!row.compatibility || typeof row.compatibility !== "object") {
      failures.push(`${row.name}: missing compatibility object`);
      continue;
    }
    for (const field of REQUIRED_COMPATIBILITY_FIELDS) {
      if (!Object.hasOwn(row.compatibility, field)) {
        failures.push(`${row.name}: missing compatibility.${field}`);
      }
    }
    const state = String(row.compatibility.state || "").toLowerCase();
    if (state === "red" || state === "yellow") {
      if (!isNonEmptyString(row.compatibility.first_failure_class)) {
        failures.push(`${row.name}: red/yellow compatibility.first_failure_class must name the first blocker`);
      }
      if (!isNonEmptyStringArray(row.compatibility.known_blockers)) {
        failures.push(`${row.name}: red/yellow compatibility.known_blockers must name at least one blocker`);
      }
    }
  }

  return failures;
}

function firstNonEmpty(...values) {
  for (const value of values) {
    const normalized = String(value ?? "").trim();
    if (normalized) return normalized;
  }
  return null;
}

function githubRunUrl(runId) {
  if (!runId || runId === "local") return null;
  const serverUrl = firstNonEmpty(process.env.GITHUB_SERVER_URL, "https://github.com");
  const repository = firstNonEmpty(process.env.GITHUB_REPOSITORY);
  if (!repository) return null;
  return `${serverUrl}/${repository}/actions/runs/${runId}`;
}

function mergedArtifactMetadata(payloads, generatedAt) {
  const payloadMetadata = payloads.find(({ payload }) => payload?.source_commit)?.payload;
  const runId = firstNonEmpty(payloadMetadata?.workflow_run_id, process.env.GITHUB_RUN_ID, "local");
  return {
    generated_at: generatedAt,
    source_commit: firstNonEmpty(
      payloadMetadata?.source_commit,
      process.env.BENCH_TARGET_SHA,
      process.env.GITHUB_SHA,
      "local",
    ),
    workflow_name: firstNonEmpty(payloadMetadata?.workflow_name, process.env.GITHUB_WORKFLOW, "local"),
    workflow_run_id: runId,
    workflow_run_url: firstNonEmpty(payloadMetadata?.workflow_run_url, githubRunUrl(runId)),
    workflow_run_attempt: firstNonEmpty(payloadMetadata?.workflow_run_attempt, process.env.GITHUB_RUN_ATTEMPT),
    run_status: firstNonEmpty(
      payloadMetadata?.run_status,
      process.env.GITHUB_ACTIONS === "true" ? "completed" : "local",
    ),
  };
}

function tallyWins(results) {
  let tszWins = 0;
  let tsgoWins = 0;
  let greenTszWins = 0;
  let greenTsgoWins = 0;
  let errorCases = 0;
  for (const row of results) {
    if (row.status) errorCases += 1;
    if (row.winner === "tsz") {
      tszWins += 1;
      if (isGreen(row)) greenTszWins += 1;
    } else if (row.winner === "tsgo") {
      tsgoWins += 1;
      if (isGreen(row)) greenTsgoWins += 1;
    }
  }
  return { tszWins, tsgoWins, greenTszWins, greenTsgoWins, errorCases };
}

function main() {
  const { outFile, inputFiles, compatibilityJsonlFiles, requireRunnerSignature } = parseArgs(process.argv);

  if (!outFile || inputFiles.length === 0) {
    console.error("Usage: scripts/bench/merge-results.mjs <out-file> [--require-runner-signature] [--compat-jsonl <file>] <input-json...>");
    process.exit(2);
  }

  const missingInputFiles = [...inputFiles, ...compatibilityJsonlFiles].filter((file) => !fs.existsSync(file));
  if (missingInputFiles.length > 0) {
    console.error("Missing benchmark inputs:");
    for (const file of missingInputFiles) {
      console.error(`  ${file}`);
    }
    process.exit(1);
  }

  const payloads = inputFiles.map((file) => ({
    file,
    payload: JSON.parse(fs.readFileSync(file, "utf8")),
  }));

  if (payloads.length === 0) {
    console.error("No benchmark JSON inputs found.");
    process.exit(1);
  }

  const results = mergeCompatibilityCanaries(
    payloads.flatMap(({ payload }) => payload.results || []),
    compatibilityJsonlFiles,
  );

  const failures = collectProjectCompatibilityFailures(results);
  if (failures.length > 0) {
    console.error("Project compatibility artifact validation failed:");
    for (const failure of failures) {
      console.error(`  - ${failure}`);
    }
    process.exit(1);
  }

  const wins = tallyWins(results);

  const hyperfineExitCodesRequired = payloads.every(
    ({ payload }) => payload.validation?.hyperfine_exit_codes_required === true,
  );
  const runnerEnvironments = payloads
    .map(({ file, payload }) => ({ file, environment: payload.runner_environment }))
    .filter(({ environment }) => environment && typeof environment === "object");
  const runnerEnvironment = runnerEnvironments[0]?.environment ?? null;
  const runnerEnvironmentWarnings = validateRunnerEnvironmentConsistency(runnerEnvironments);
  const measurementProfiles = payloads
    .map(({ file, payload }) => ({ file, profile: payload.measurement_profile }))
    .filter(({ profile }) => profile && typeof profile === "object");
  const measurementProfile = measurementProfiles[0]?.profile ?? null;
  const measurementProfileWarnings = validateMeasurementProfileConsistency(measurementProfiles);
  const runnerSignatureFailures = requireRunnerSignature
    ? collectRunnerSignatureFailures(payloads, runnerEnvironmentWarnings)
    : [];
  if (runnerSignatureFailures.length > 0) {
    console.error("Benchmark runner signature validation failed:");
    for (const failure of runnerSignatureFailures) {
      console.error(`  - ${failure}`);
    }
    process.exit(1);
  }

  const merged = {
    ...mergedArtifactMetadata(payloads, new Date().toISOString()),
    benchmark_runner: "scripts/bench/bench-vs-tsgo.sh",
    merged_from: payloads.map(({ file }) => path.basename(file)).sort(),
    validation: {
      hyperfine_exit_codes_required: hyperfineExitCodesRequired,
      project_compatibility_required_fields: hasProjectCompatibilityRows(results),
      runner_signature_required: requireRunnerSignature,
      runner_environment_warnings: runnerEnvironmentWarnings,
      measurement_profile_warnings: measurementProfileWarnings,
    },
    ...(runnerEnvironment ? { runner_environment: runnerEnvironment } : {}),
    ...(measurementProfile ? { measurement_profile: measurementProfile } : {}),
    quick_mode: payloads.every(({ payload }) => payload.quick_mode === true),
    filter: null,
    binaries: payloads.find(({ payload }) => payload.binaries)?.payload.binaries || {},
    totals: {
      benchmarks_run: payloads.reduce(
        (sum, { payload }) => sum + Number(payload.totals?.benchmarks_run || 0),
        0,
      ),
      rows: results.length,
      tsz_wins: wins.tszWins,
      tsgo_wins: wins.tsgoWins,
      green_tsz_wins: wins.greenTszWins,
      green_tsgo_wins: wins.greenTsgoWins,
      error_cases: wins.errorCases,
    },
    results,
  };

  fs.mkdirSync(path.dirname(outFile), { recursive: true });
  fs.writeFileSync(outFile, `${JSON.stringify(merged, null, 2)}\n`, "utf8");
  console.log(`Merged ${payloads.length} benchmark files into ${outFile}`);
}

main();
