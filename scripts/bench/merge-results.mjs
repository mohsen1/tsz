#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

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

const REQUIRED_PROJECT_ROWS = [
  "type-fest-project",
  "ts-essentials-project",
  "large-ts-repo",
  "nextjs",
];

const REQUIRED_COMPATIBILITY_FIELDS = [
  "exit_class",
  "phase",
  "last_successful_phase",
  "diagnostic_status",
  "diagnostic_deltas",
  "diagnostic_subsystems",
  "known_blockers",
  "exit_codes",
  "files_reached",
  "peak_memory_bytes",
  "emit_status",
  "dts_status",
];

function hasProjectCompatibilityRows(rows) {
  return rows.some((row) => REQUIRED_PROJECT_ROWS.includes(row?.name));
}

function validateProjectCompatibilityRows(rows) {
  const projectRows = rows.filter((row) => REQUIRED_PROJECT_ROWS.includes(row?.name));
  if (projectRows.length === 0) return;

  const byName = new Map(projectRows.map((row) => [row.name, row]));
  const failures = [];
  for (const name of REQUIRED_PROJECT_ROWS) {
    const row = byName.get(name);
    if (!row) {
      failures.push(`${name}: missing project row`);
      continue;
    }
    if (!row.compatibility || typeof row.compatibility !== "object") {
      failures.push(`${name}: missing compatibility object`);
      continue;
    }
    for (const field of REQUIRED_COMPATIBILITY_FIELDS) {
      if (!Object.prototype.hasOwnProperty.call(row.compatibility, field)) {
        failures.push(`${name}: missing compatibility.${field}`);
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
  },
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
