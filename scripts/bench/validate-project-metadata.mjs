#!/usr/bin/env node
import { pathToFileURL } from "node:url";
import { PROJECT_ROW_DEFINITIONS, REQUIRED_PROJECT_ROWS, COMPILE_GUARD_REQUIRED_ROWS, COMPILE_CANARY_PROJECT_ROWS, COMPILE_GUARD_CANARY_PROJECT_ROWS } from "./project-rows.mjs";
import { GENERATOR_SCRIPTS_PREFIX } from "./fixture-provenance.mjs";

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

const pairedMetadataFields = [
  ["repo", "repo_env"],
  ["ref", "ref_env"],
  ["expected_generated", "expected_generated_env"],
  ["expected_test_cases", "expected_test_cases_env"],
];

function isNonEmptyString(value) {
  return typeof value === "string" && value.trim().length > 0;
}

function isShellVariableName(value) {
  return typeof value === "string" && /^[A-Za-z_][A-Za-z0-9_]*$/.test(value);
}

function isProjectRowName(value) {
  return typeof value === "string" && /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(value);
}

function safeRelativePathError(value, { allowDotRoot = false } = {}) {
  if (!isNonEmptyString(value)) {
    return "must be a non-empty relative path";
  }

  if (value.startsWith("/") || value.includes("\\")) {
    return "must be a relative POSIX path";
  }

  if (allowDotRoot && value === ".") {
    return null;
  }

  const segments = value.split("/");
  if (segments.some((segment) => segment === "" || segment === "." || segment === "..")) {
    return "must not contain empty, dot, or parent segments";
  }

  return null;
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
  const labels = new Map();
  const fixtureDirs = new Map();
  const envNames = new Map();

  for (const row of projectRows) {
    if (!isNonEmptyString(row.name)) {
      failures.push("project row has invalid or empty name");
      continue;
    } else if (!isProjectRowName(row.name)) {
      failures.push(`${row.name}: name must be a lowercase hyphenated slug`);
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
    if (isNonEmptyString(row.label)) {
      const previousRowName = labels.get(row.label);
      if (previousRowName) {
        failures.push(`${row.name}: label duplicates ${previousRowName}: ${row.label}`);
      } else {
        labels.set(row.label, row.name);
      }
    }

    for (const field of ["fixture_dir", "source_dir"]) {
      if (isNonEmptyString(row[field])) {
        const error = safeRelativePathError(row[field], { allowDotRoot: field === "source_dir" });
        if (error) {
          failures.push(`${row.name}: ${field} ${error}`);
        }
      }
    }
    if (isNonEmptyString(row.fixture_dir)) {
      const previousRowName = fixtureDirs.get(row.fixture_dir);
      if (previousRowName) {
        failures.push(`${row.name}: fixture_dir duplicates ${previousRowName}: ${row.fixture_dir}`);
      } else {
        fixtureDirs.set(row.fixture_dir, row.name);
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

    const missingExternalPinFields = new Set();
    if (row.category === "external") {
      for (const field of ["repo", "ref", "repo_env", "ref_env"]) {
        if (!(field in row) || row[field] === undefined) {
          missingExternalPinFields.add(field);
          failures.push(`${row.name}: external row is missing required pin field ${field}`);
        }
      }
    }

    if (!Array.isArray(row.readme_candidates) || row.readme_candidates.length === 0) {
      failures.push(`${row.name}: readme_candidates must be a non-empty array`);
    } else {
      row.readme_candidates.forEach((candidate, index) => {
        if (!isNonEmptyString(candidate)) {
          failures.push(`${row.name}: readme_candidates[${index}] must be a non-empty string`);
          return;
        }
        const error = safeRelativePathError(candidate);
        if (error) {
          failures.push(`${row.name}: readme_candidates[${index}] ${error}`);
        }
      });
    }

    for (const field of ["repo_env", "ref_env", "expected_generated_env", "expected_test_cases_env"]) {
      if (field in row && !isNonEmptyString(row[field])) {
        failures.push(`${row.name}: ${field} must be a non-empty string when present`);
      } else if (field in row && !isShellVariableName(row[field])) {
        failures.push(`${row.name}: ${field} must be a valid shell variable name when present`);
      } else if (field in row) {
        const previous = envNames.get(row[field]);
        if (previous) {
          failures.push(`${row.name}: ${field} duplicates ${previous}: ${row[field]}`);
        } else {
          envNames.set(row[field], `${row.name}.${field}`);
        }
      }
    }

    for (const [valueField, envField] of pairedMetadataFields) {
      if (
        row[valueField] !== undefined
        && row[envField] === undefined
        && !missingExternalPinFields.has(envField)
      ) {
        failures.push(`${row.name}: ${valueField} is set but ${envField} is missing`);
      }
    }

    for (const { envField, valueField, valid, message } of pinCouplings) {
      if (row[envField] !== undefined && !valid(row[valueField])) {
        failures.push(`${row.name}: ${envField} is set but ${message}`);
      }
    }

    if (row.generated_by === undefined) continue;
    if (
      typeof row.generated_by !== "string" ||
      !row.generated_by.startsWith(GENERATOR_SCRIPTS_PREFIX) ||
      !row.generated_by.endsWith(".mjs")
    ) {
      failures.push(
        `${row.name}: generated_by must point to a ${GENERATOR_SCRIPTS_PREFIX}*.mjs generator script`,
      );
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
