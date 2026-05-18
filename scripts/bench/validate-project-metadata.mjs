#!/usr/bin/env node
import { pathToFileURL } from "node:url";
import { PROJECT_ROW_DEFINITIONS, REQUIRED_PROJECT_ROWS, COMPILE_GUARD_REQUIRED_ROWS, COMPILE_CANARY_PROJECT_ROWS, COMPILE_GUARD_CANARY_PROJECT_ROWS } from "./project-rows.mjs";

export const REQUIRED_METADATA_FIELDS = [
  "name",
  "label",
  "owner",
  "family",
  "fixture_dir",
  "source_dir",
  "guard_set",
  "benchmark_set",
  "category",
  "readme_candidates",
];

const allowedGuardSets = new Set(["required", "canary", null]);
const allowedBenchmarkSets = new Set(["required", "canary"]);
const allowedCategories = new Set(["external", "generated"]);

const pinCouplings = [
  {
    envField: "repo_env",
    valueField: "repo",
    valid: (v) => typeof v === "string" && v.startsWith("https://"),
    message: "repo is missing or not an https:// URL",
  },
  {
    envField: "ref_env",
    valueField: "ref",
    valid: (v) => typeof v === "string" && /^[0-9a-f]{7,}$/i.test(v),
    message: "ref is missing or not a valid commit hash",
  },
  {
    envField: "expected_generated_env",
    valueField: "expected_generated",
    valid: (v) => typeof v === "number" && v > 0,
    message: "expected_generated is missing or not a positive number",
  },
  {
    envField: "expected_test_cases_env",
    valueField: "expected_test_cases",
    valid: (v) => typeof v === "number" && v > 0,
    message: "expected_test_cases is missing or not a positive number",
  },
];

function isNonEmptyString(value) {
  return typeof value === "string" && value.trim().length > 0;
}

export function validateProjectMetadata({
  projectRows,
  requiredProjectRows,
  compileGuardRequiredRows,
  compileCanaryProjectRows,
  compileGuardCanaryProjectRows,
}) {
  const failures = [];
  const seen = new Set();

  for (const row of projectRows) {
    if (!isNonEmptyString(row.name)) {
      failures.push("project row has invalid or empty name");
      continue;
    }

    if (seen.has(row.name)) {
      failures.push(`duplicate project row name: ${row.name}`);
      continue;
    }
    seen.add(row.name);

    for (const field of REQUIRED_METADATA_FIELDS) {
      if (!(field in row) || row[field] === undefined) {
        failures.push(`${row.name}: missing required field ${field}`);
      }
    }

    for (const field of ["label", "owner", "family", "fixture_dir", "source_dir"]) {
      if (!isNonEmptyString(row[field])) {
        failures.push(`${row.name}: ${field} must be a non-empty string`);
      }
    }

    if (!allowedBenchmarkSets.has(row.benchmark_set)) {
      failures.push(`${row.name}: invalid benchmark_set ${String(row.benchmark_set)}`);
    }

    if (!allowedGuardSets.has(row.guard_set)) {
      failures.push(`${row.name}: invalid guard_set ${String(row.guard_set)}`);
    }

    if (!allowedCategories.has(row.category)) {
      failures.push(`${row.name}: invalid category ${String(row.category)}`);
    }

    if (!Array.isArray(row.readme_candidates) || row.readme_candidates.length === 0) {
      failures.push(`${row.name}: readme_candidates must be a non-empty array`);
    } else {
      row.readme_candidates.forEach((candidate, index) => {
        if (!isNonEmptyString(candidate)) {
          failures.push(`${row.name}: readme_candidates[${index}] must be a non-empty string`);
        }
      });
    }

    for (const field of ["repo_env", "ref_env"]) {
      if (field in row && !isNonEmptyString(row[field])) {
        failures.push(`${row.name}: ${field} must be a non-empty string when present`);
      }
    }

    for (const { envField, valueField, valid, message } of pinCouplings) {
      if (row[envField] !== undefined && !valid(row[valueField])) {
        failures.push(`${row.name}: ${envField} is set but ${message}`);
      }
    }
  }

  const rowNames = new Set(projectRows.map((row) => row.name));
  const expectedRows = [
    ...requiredProjectRows,
    ...compileGuardRequiredRows,
    ...compileCanaryProjectRows,
    ...compileGuardCanaryProjectRows,
  ];

  for (const rowName of expectedRows) {
    if (!rowNames.has(rowName)) {
      failures.push(`${rowName}: referenced by generated export but not present in definitions`);
    }
  }

  return failures;
}

export function validateCurrentProjectMetadata() {
  return validateProjectMetadata({
    projectRows: PROJECT_ROW_DEFINITIONS,
    requiredProjectRows: REQUIRED_PROJECT_ROWS,
    compileGuardRequiredRows: COMPILE_GUARD_REQUIRED_ROWS,
    compileCanaryProjectRows: COMPILE_CANARY_PROJECT_ROWS,
    compileGuardCanaryProjectRows: COMPILE_GUARD_CANARY_PROJECT_ROWS,
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const failures = validateCurrentProjectMetadata();

  if (failures.length > 0) {
    for (const failure of failures) {
      console.error(failure);
    }
    process.exit(1);
  }

  process.exit(0);
}
