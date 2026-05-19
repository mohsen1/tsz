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

assert.deepEqual(validate([validRow({ name: "Example_Project" })]), [
  "Example_Project: name must be a lowercase hyphenated slug",
]);

assert.deepEqual(validate([validRow(), validRow()]), [
  "duplicate project row name: example-project",
]);

assert.deepEqual(
  validate([
    validRow({ name: "first-project", label: "Shared Label" }),
    validRow({
      name: "second-project",
      label: "Shared Label",
      fixture_dir: "second-project",
      repo_env: "SECOND_REPO",
      ref_env: "SECOND_REF",
    }),
  ]),
  ["second-project: label duplicates first-project: Shared Label"],
);

assert.deepEqual(
  validate([
    validRow({ name: "first-project", label: "First", fixture_dir: "shared-fixture" }),
    validRow({
      name: "second-project",
      label: "Second",
      fixture_dir: "shared-fixture",
      repo_env: "SECOND_REPO",
      ref_env: "SECOND_REF",
    }),
  ]),
  ["second-project: fixture_dir duplicates first-project: shared-fixture"],
);

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
      expected_generated_env: "bad-env-name",
      expected_test_cases_env: "1BAD_ENV",
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
    "example-project: expected_generated_env must be a valid shell variable name when present",
    "example-project: expected_test_cases_env must be a valid shell variable name when present",
    "example-project: expected_generated_env is set but expected_generated is missing or not a positive number",
    "example-project: expected_test_cases_env is set but expected_test_cases is missing or not a positive number",
  ],
);

assert.deepEqual(
  validate([
    validRow({ name: "first-project", label: "First", repo_env: "SHARED_REPO" }),
    validRow({
      name: "second-project",
      label: "Second",
      repo_env: "SHARED_REPO",
      ref_env: "SECOND_REF",
      fixture_dir: "second-project",
    }),
    validRow({
      name: "third-project",
      label: "Third",
      ref_env: "SHARED_REPO",
      fixture_dir: "third-project",
    }),
  ]),
  [
    "second-project: repo_env duplicates first-project.repo_env: SHARED_REPO",
    "third-project: ref_env duplicates first-project.repo_env: SHARED_REPO",
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
  validate([
    (() => {
      const row = validRow({
        expected_generated: 10,
        expected_test_cases: 20,
      });
      delete row.repo_env;
      delete row.ref_env;
      return row;
    })(),
  ]),
  [
    "example-project: external row is missing required pin field repo_env",
    "example-project: external row is missing required pin field ref_env",
    "example-project: expected_generated is set but expected_generated_env is missing",
    "example-project: expected_test_cases is set but expected_test_cases_env is missing",
  ],
);

assert.deepEqual(
  validate([
    (() => {
      const row = validRow({ name: "external-without-pins" });
      delete row.repo;
      delete row.ref;
      delete row.repo_env;
      delete row.ref_env;
      return row;
    })(),
  ]),
  [
    "external-without-pins: external row is missing required pin field repo",
    "external-without-pins: external row is missing required pin field ref",
    "external-without-pins: external row is missing required pin field repo_env",
    "external-without-pins: external row is missing required pin field ref_env",
  ],
);

assert.deepEqual(
  validate([validRow({ readme_candidates: [] })]),
  ["example-project: readme_candidates must be a non-empty array"],
);

assert.deepEqual(
  validate([
    validRow({
      fixture_dir: "/tmp/example",
      source_dir: "../src",
      readme_candidates: ["README.md", "docs//README.md", "docs\\README.md", "../README.md"],
    }),
  ]),
  [
    "example-project: fixture_dir must be a relative POSIX path",
    "example-project: source_dir must not contain empty, dot, or parent segments",
    "example-project: readme_candidates[1] must not contain empty, dot, or parent segments",
    "example-project: readme_candidates[2] must be a relative POSIX path",
    "example-project: readme_candidates[3] must not contain empty, dot, or parent segments",
  ],
);

assert.deepEqual(
  validate([validRow({ source_dir: "." })]),
  [],
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
