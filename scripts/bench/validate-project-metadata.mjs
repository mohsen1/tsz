#!/usr/bin/env node
import { PROJECT_ROW_DEFINITIONS, REQUIRED_PROJECT_ROWS, COMPILE_GUARD_REQUIRED_ROWS, COMPILE_CANARY_PROJECT_ROWS, COMPILE_GUARD_CANARY_PROJECT_ROWS } from "./project-rows.mjs";

const requiredFields = [
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
const failures = [];
const seen = new Set();

for (const row of PROJECT_ROW_DEFINITIONS) {
  if (typeof row.name !== "string" || !row.name.trim()) {
    failures.push("project row has invalid or empty name");
    continue;
  }

  if (seen.has(row.name)) {
    failures.push(`duplicate project row name: ${row.name}`);
    continue;
  }
  seen.add(row.name);

  for (const field of requiredFields) {
    if (!(field in row) || row[field] === undefined) {
      failures.push(`${row.name}: missing required field ${field}`);
    }
  }

  if (typeof row.benchmark_set !== "string" || row.benchmark_set.length === 0) {
    failures.push(`${row.name}: invalid benchmark_set`);
  }

  if (!allowedGuardSets.has(row.guard_set)) {
    failures.push(`${row.name}: invalid guard_set ${String(row.guard_set)}`);
  }

  if (!Array.isArray(row.readme_candidates) || row.readme_candidates.length === 0) {
    failures.push(`${row.name}: readme_candidates must be a non-empty array`);
  }
}

const rowNames = new Set(PROJECT_ROW_DEFINITIONS.map((row) => row.name));
const expectedRows = [
  ...REQUIRED_PROJECT_ROWS,
  ...COMPILE_GUARD_REQUIRED_ROWS,
  ...COMPILE_CANARY_PROJECT_ROWS,
  ...COMPILE_GUARD_CANARY_PROJECT_ROWS,
];

for (const rowName of expectedRows) {
  if (!rowNames.has(rowName)) {
    failures.push(`${rowName}: referenced by generated export but not present in definitions`);
  }
}

if (failures.length > 0) {
  for (const failure of failures) {
    console.error(failure);
  }
  process.exit(1);
}

process.exit(0);
