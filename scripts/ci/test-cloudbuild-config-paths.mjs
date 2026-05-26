#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const workflowConfigs = new Map([
  [
    ".github/workflows/bench.yml",
    ["cloudbuild-bench-prepare.yaml", "cloudbuild-bench-shard.yaml"],
  ],
  [".github/workflows/ci.yml", ["cloudbuild-unit.yaml"]],
]);
const expectedConfigs = [...workflowConfigs.values()].flat();

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

for (const [workflow, configs] of workflowConfigs) {
  const workflowText = fs.readFileSync(path.join(ROOT, workflow), "utf8");
  const allowedConfigs = new Set(configs);

  for (const config of configs) {
    assert.match(
      workflowText,
      new RegExp(`--config=scripts/cloudbuild/${config}`),
      `${workflow} should reference ${config} through scripts/cloudbuild`,
    );
    assert.doesNotMatch(
      workflowText,
      new RegExp(`--config=${config}`),
      `${workflow} should not reference ${config} from the repository root`,
    );
  }

  for (const config of expectedConfigs) {
    if (allowedConfigs.has(config)) {
      continue;
    }

    assert.doesNotMatch(
      workflowText,
      new RegExp(`--config=scripts/cloudbuild/${config}`),
      `${workflow} should not reference ${config}`,
    );
  }
}

console.log("test-cloudbuild-config-paths: ok");
