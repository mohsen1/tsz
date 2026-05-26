#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";

const workflow = fs.readFileSync(".github/workflows/gh-pages.yml", "utf8");

assert.match(
  workflow,
  /WORKFLOW_RUN_ID:\s*\$\{\{ github\.event\.workflow_run\.id \}\}/,
  "Pages deploy check should know the exact triggering workflow_run id",
);

assert.match(
  workflow,
  /WORKFLOW_RUN_NAME" = "Bench"[\s\S]+actions\/runs\/\$\{WORKFLOW_RUN_ID\}\/artifacts[\s\S]+bench-results-merged/,
  "Bench-triggered Pages deploys must inspect the exact Bench run artifact list",
);

assert.match(
  workflow,
  /select\(\.name == "bench-results-merged" and \.expired == false\)/,
  "Pages deploy should require a non-expired merged benchmark artifact",
);

assert.match(
  workflow,
  /did not publish bench-results-merged; skipping stale benchmark redeploy\.[\s\S]+should_deploy=false/,
  "Bench-triggered Pages deploys without benchmark data must not redeploy stale fallback charts",
);

assert.match(
  workflow,
  /WORKFLOW_RUN_NAME" = "CI"[\s\S]+should_deploy=true/,
  "Successful main CI workflow_run events should still deploy normal website changes",
);

console.log("gh-pages benchmark artifact gate tests passed");
