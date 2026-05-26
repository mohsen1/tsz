#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const expectedConfigs = [
  "cloudbuild-bench-prepare.yaml",
  "cloudbuild-bench-shard.yaml",
  "cloudbuild-unit.yaml",
];

for (const config of expectedConfigs) {
  assert.ok(
    fs.existsSync(path.join(ROOT, "scripts", "cloudbuild", config)),
    `${config} should live under scripts/cloudbuild`,
  );
  assert.ok(
    !fs.existsSync(path.join(ROOT, config)),
    `${config} should not remain at repository root`,
  );
}

const workflowText = [
  fs.readFileSync(path.join(ROOT, ".github", "workflows", "bench.yml"), "utf8"),
  fs.readFileSync(path.join(ROOT, ".github", "workflows", "ci.yml"), "utf8"),
].join("\n");

for (const config of expectedConfigs) {
  assert.match(
    workflowText,
    new RegExp(`--config=scripts/cloudbuild/${config}`),
    `${config} should be referenced through scripts/cloudbuild`,
  );
  assert.doesNotMatch(
    workflowText,
    new RegExp(`--config=${config}`),
    `${config} should not be referenced from the repository root`,
  );
}

console.log("test-cloudbuild-config-paths: ok");
