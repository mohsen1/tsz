#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";

const workflow = fs.readFileSync(".github/workflows/ci-health.yml", "utf8");

const readinessStep = workflow.match(
  /- name: Run artifact readiness check[\s\S]+?exit 0/,
)?.[0] ?? "";

assert.match(
  readinessStep,
  /node scripts\/bench\/check-artifact-readiness\.mjs "\$\{artifact\}"[\s\S]+--expect-source-commit="\$\{GITHUB_SHA\}"/,
  "CI-health benchmark readiness should compare artifacts with the workflow checkout SHA",
);

assert.match(
  readinessStep,
  /--require-clean-metadata/,
  "CI-health benchmark readiness should warn when artifact metadata is not clean",
);

assert.match(
  readinessStep,
  /--require-source-current/,
  "CI-health benchmark readiness should warn when the artifact source commit is stale",
);

assert.match(
  readinessStep,
  /not current release truth; inspect the readiness report above/,
  "CI-health warning should cover stale source and dirty metadata, not only missing rows",
);

assert.doesNotMatch(
  readinessStep,
  /present but missing required project rows/,
  "CI-health warning must not misclassify metadata or source freshness failures as missing rows",
);

console.log("ci-health benchmark readiness tests passed");
