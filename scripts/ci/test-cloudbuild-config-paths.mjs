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
const workflowDir = path.join(ROOT, ".github", "workflows");
const workflowFiles = fs
  .readdirSync(workflowDir)
  .filter((entry) => /\.ya?ml$/.test(entry))
  .map((entry) => `.github/workflows/${entry}`)
  .sort();
const expectedConfigs = [...workflowConfigs.values()].flat();
const expectedConfigSet = new Set(expectedConfigs);

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function configFlagPattern(configPath) {
  return new RegExp(`--config(?:=|\\s+)${escapeRegExp(configPath)}`);
}

const rootCloudbuildConfigs = fs
  .readdirSync(ROOT)
  .filter((entry) => /^cloudbuild.*\.ya?ml$/.test(entry))
  .sort();
assert.deepEqual(
  rootCloudbuildConfigs,
  [],
  "Cloud Build configs should not live at repository root",
);

const scriptsCloudbuildConfigs = fs
  .readdirSync(path.join(ROOT, "scripts", "cloudbuild"))
  .filter((entry) => /^cloudbuild.*\.ya?ml$/.test(entry))
  .sort();
assert.deepEqual(
  scriptsCloudbuildConfigs,
  [...expectedConfigSet].sort(),
  "scripts/cloudbuild should contain exactly the expected Cloud Build configs",
);

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

for (const workflow of workflowFiles) {
  const configs = workflowConfigs.get(workflow) ?? [];
  const workflowText = fs.readFileSync(path.join(ROOT, workflow), "utf8");
  const allowedConfigs = new Set(configs);

  for (const config of configs) {
    assert.match(
      workflowText,
      configFlagPattern(`scripts/cloudbuild/${config}`),
      `${workflow} should reference ${config} through scripts/cloudbuild`,
    );
    assert.doesNotMatch(
      workflowText,
      configFlagPattern(config),
      `${workflow} should not reference ${config} from the repository root`,
    );
  }

  for (const config of expectedConfigs) {
    assert.doesNotMatch(
      workflowText,
      configFlagPattern(config),
      `${workflow} should not reference ${config} from the repository root`,
    );

    if (allowedConfigs.has(config)) {
      continue;
    }

    assert.doesNotMatch(
      workflowText,
      configFlagPattern(`scripts/cloudbuild/${config}`),
      `${workflow} should not reference ${config}`,
    );
  }
}

console.log("test-cloudbuild-config-paths: ok");
