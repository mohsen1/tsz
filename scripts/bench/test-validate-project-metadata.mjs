#!/usr/bin/env node
import assert from "node:assert/strict";
import {
  validateCurrentProjectMetadata,
  validateProjectMetadata,
} from "./validate-project-metadata.mjs";

function validRow(overrides = {}) {
  return {
    name: "example-project",
    label: "Example",
    owner: "Tracks 1",
    family: "fixture metadata smoke",
    fixture_dir: "example",
    source_dir: "src",
    repo_env: "EXAMPLE_REPO",
    ref_env: "EXAMPLE_REF",
    repo: "https://example.com/example.git",
    ref: "1234567",
    guard_set: "required",
    benchmark_set: "required",
    category: "external",
    readme_candidates: ["README.md"],
    ...overrides,
  };
}

function validate(rows, exports = {}) {
  return validateProjectMetadata({
    projectRows: rows,
    requiredProjectRows: exports.requiredProjectRows ?? [],
    compileGuardRequiredRows: exports.compileGuardRequiredRows ?? [],
    compileCanaryProjectRows: exports.compileCanaryProjectRows ?? [],
    compileGuardCanaryProjectRows: exports.compileGuardCanaryProjectRows ?? [],
  });
}

assert.deepEqual(validateCurrentProjectMetadata(), []);

assert.deepEqual(validate([validRow({ name: " " })]), [
  "project row has invalid or empty name",
]);

assert.deepEqual(validate([validRow(), validRow()]), [
  "duplicate project row name: example-project",
]);

assert.deepEqual(
  validate([
    validRow({
      label: "",
      owner: " ",
      family: "",
      fixture_dir: "",
      source_dir: " ",
      repo_env: "",
      ref_env: " ",
      guard_set: "nightly",
      benchmark_set: "manual",
      category: "fixture",
      readme_candidates: ["README.md", ""],
    }),
  ]),
  [
    "example-project: label must be a non-empty string",
    "example-project: owner must be a non-empty string",
    "example-project: family must be a non-empty string",
    "example-project: fixture_dir must be a non-empty string",
    "example-project: source_dir must be a non-empty string",
    "example-project: invalid benchmark_set manual",
    "example-project: invalid guard_set nightly",
    "example-project: invalid category fixture",
    "example-project: readme_candidates[1] must be a non-empty string",
    "example-project: repo_env must be a non-empty string when present",
    "example-project: ref_env must be a non-empty string when present",
  ],
);

assert.deepEqual(
  validate([
    validRow({
      repo: "git@example.com:repo.git",
      ref: "main",
      expected_generated_env: "EXAMPLE_EXPECTED_GENERATED",
      expected_generated: 0,
      expected_test_cases_env: "EXAMPLE_EXPECTED_TEST_CASES",
      expected_test_cases: undefined,
    }),
  ]),
  [
    "example-project: repo_env is set but repo is missing or not an https:// URL",
    "example-project: ref_env is set but ref is missing or not a valid commit hash",
    "example-project: expected_generated_env is set but expected_generated is missing or not a positive number",
    "example-project: expected_test_cases_env is set but expected_test_cases is missing or not a positive number",
  ],
);

assert.deepEqual(
  validate([validRow({ readme_candidates: [] })]),
  ["example-project: readme_candidates must be a non-empty array"],
);

assert.deepEqual(
  validate([validRow()], {
    requiredProjectRows: ["missing-required"],
    compileGuardRequiredRows: ["missing-guard-required"],
    compileCanaryProjectRows: ["missing-canary"],
    compileGuardCanaryProjectRows: ["missing-guard-canary"],
  }),
  [
    "missing-required: referenced by generated export but not present in definitions",
    "missing-guard-required: referenced by generated export but not present in definitions",
    "missing-canary: referenced by generated export but not present in definitions",
    "missing-guard-canary: referenced by generated export but not present in definitions",
  ],
);
