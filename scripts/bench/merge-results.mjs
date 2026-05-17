#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import {
  COMPILE_CANARY_PROJECT_ROWS,
  REQUIRED_COMPATIBILITY_FIELDS,
  REQUIRED_PROJECT_ROWS,
} from "./project-rows.mjs";

const [, , outFile, ...inputFiles] = process.argv;

if (!outFile || inputFiles.length === 0) {
  console.error("Usage: scripts/bench/merge-results.mjs <out-file> <input-json...>");
  process.exit(2);
}

const missingInputFiles = inputFiles.filter((file) => !fs.existsSync(file));
if (missingInputFiles.length > 0) {
  console.error("Missing benchmark JSON inputs:");
  for (const file of missingInputFiles) {
    console.error(`  ${file}`);
  }
  process.exit(1);
}

const payloads = inputFiles.map((file) => {
  const payload = JSON.parse(fs.readFileSync(file, "utf8"));
  return { file, payload };
});

if (payloads.length === 0) {
  console.error("No benchmark JSON inputs found.");
  process.exit(1);
}

const results = payloads.flatMap(({ payload }) => payload.results || []);
const tszWins = results.filter((row) => row.winner === "tsz").length;
const tsgoWins = results.filter((row) => row.winner === "tsgo").length;
const errorCases = results.filter((row) => row.status).length;
const hyperfineExitCodesRequired = payloads.every(
  ({ payload }) => payload.validation?.hyperfine_exit_codes_required === true,
);
const runnerEnvironments = payloads
  .map(({ file, payload }) => ({ file, environment: payload.runner_environment }))
  .filter(({ environment }) => environment && typeof environment === "object");
const runnerEnvironment = runnerEnvironments[0]?.environment || null;
const runnerEnvironmentWarnings = validateRunnerEnvironmentConsistency(runnerEnvironments);

const REQUIRED_PROJECT_ROW_SET = new Set(REQUIRED_PROJECT_ROWS);
const PROJECT_COMPATIBILITY_ROW_SET = new Set([
  ...REQUIRED_PROJECT_ROWS,
  ...COMPILE_CANARY_PROJECT_ROWS,
]);

function hasProjectCompatibilityRows(rows) {
  return rows.some((row) => PROJECT_COMPATIBILITY_ROW_SET.has(row?.name));
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
    const mismatchedFields = Object.keys(baseline)
      .filter((key) => baseline[key] !== current[key]);
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

function validateProjectCompatibilityRows(rows) {
  const projectRows = rows.filter((row) => PROJECT_COMPATIBILITY_ROW_SET.has(row?.name));
  if (projectRows.length === 0) return;

  const byName = new Map(projectRows.map((row) => [row.name, row]));
  const failures = [];

  if (projectRows.some((row) => REQUIRED_PROJECT_ROW_SET.has(row.name))) {
    for (const name of REQUIRED_PROJECT_ROWS) {
      if (!byName.has(name)) {
        failures.push(`${name}: missing project row`);
      }
    }
  }

  for (const row of projectRows) {
    if (!row.compatibility || typeof row.compatibility !== "object") {
      failures.push(`${row.name}: missing compatibility object`);
      continue;
    }
    for (const field of REQUIRED_COMPATIBILITY_FIELDS) {
      if (!Object.prototype.hasOwnProperty.call(row.compatibility, field)) {
        failures.push(`${row.name}: missing compatibility.${field}`);
      }
    }
  }

  if (failures.length > 0) {
    console.error("Project compatibility artifact validation failed:");
    for (const failure of failures) {
      console.error(`  - ${failure}`);
    }
    process.exit(1);
  }
}

validateProjectCompatibilityRows(results);
const projectCompatibilityRequiredFields = hasProjectCompatibilityRows(results);

const merged = {
  generated_at: new Date().toISOString(),
  benchmark_runner: "scripts/bench/bench-vs-tsgo.sh",
  merged_from: payloads.map(({ file }) => path.basename(file)).sort(),
  validation: {
    hyperfine_exit_codes_required: hyperfineExitCodesRequired,
    project_compatibility_required_fields: projectCompatibilityRequiredFields,
    runner_environment_warnings: runnerEnvironmentWarnings,
  },
  ...(runnerEnvironment ? { runner_environment: runnerEnvironment } : {}),
  quick_mode: payloads.every(({ payload }) => payload.quick_mode === true),
  filter: null,
  binaries: payloads.find(({ payload }) => payload.binaries)?.payload.binaries || {},
  totals: {
    benchmarks_run: payloads.reduce(
      (sum, { payload }) => sum + Number(payload.totals?.benchmarks_run || 0),
      0,
    ),
    rows: results.length,
    tsz_wins: tszWins,
    tsgo_wins: tsgoWins,
    error_cases: errorCases,
  },
  results,
};

fs.mkdirSync(path.dirname(outFile), { recursive: true });
fs.writeFileSync(outFile, `${JSON.stringify(merged, null, 2)}\n`, "utf8");
console.log(`Merged ${payloads.length} benchmark files into ${outFile}`);
