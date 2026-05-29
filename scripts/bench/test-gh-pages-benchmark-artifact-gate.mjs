#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";

const workflow = fs.readFileSync(".github/workflows/gh-pages.yml", "utf8");
const gcpFullCi = fs.readFileSync("scripts/ci/gcp-full-ci.sh", "utf8");

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
  /github\.event_name \}\}" = "workflow_dispatch"[\s\S]+allowing it to queue behind any active deploy[\s\S]+in_flight=false/,
  "Explicit Pages redeploy dispatches should not be dropped behind an older deploy",
);

assert.match(
  workflow,
  /actions\/artifacts\?name=bench-results-merged&per_page=20[\s\S]+workflow_run\.head_branch[\s\S]+Latest benchmark merged artifact/,
  "Pages deploy should find merged benchmark artifacts directly instead of scanning a small window of successful Bench runs",
);

assert.doesNotMatch(
  workflow,
  /gh run list[\s\S]+--workflow bench\.yml[\s\S]+--limit 50/,
  "Pages deploy must not rely on a 50-run success window that gate-only Bench runs can crowd out",
);

assert.match(
  workflow,
  /select\(\.name == "bench-results-merged" and \.expired == false\)/,
  "Pages deploy should require a non-expired merged benchmark artifact",
);

assert.match(
  workflow,
  /Download latest benchmark data from GCS[\s\S]+bench-runs\/latest\.json[\s\S]+bench-vs-tsgo-gcs-latest\.json[\s\S]+bench-runs\/latest\.tsgo-winners\.json/,
  "Pages deploy should use published GCS benchmark truth when credentials are available",
);

assert.match(
  workflow,
  /selectLatestBenchmarkArtifact[\s\S]+bench-vs-tsgo-github-latest\.json[\s\S]+bench-vs-tsgo-gcs-latest\.json/,
  "Pages readiness status should describe the selected fresh benchmark artifact",
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

assert.match(
  gcpFullCi,
  /node scripts\/bench\/test-gh-pages-benchmark-artifact-gate\.mjs/,
  "gcp-full-ci lint should run the benchmark artifact gate test",
);

console.log("gh-pages benchmark artifact gate tests passed");
