#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";

const workflow = fs.readFileSync(".github/workflows/gh-pages.yml", "utf8");
const fullCi = fs.readFileSync("scripts/ci/full-ci.sh", "utf8");

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
  /actions\/artifacts\?name=bench-results-merged&per_page=20[\s\S]+workflow_run\.head_branch[\s\S]+Latest readiness-clean benchmark merged artifact/,
  "Pages deploy should find readiness-clean merged benchmark artifacts directly instead of scanning a small window of successful Bench runs",
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
  /artifact_ready_for_pages\(\)[\s\S]+application_compatibility\.required == true[\s\S]+blocking_application_compatibility_gaps == 0[\s\S]+blocking_project_timing_pair_gaps == 0[\s\S]+successful_project_timing_pairs >= \.required_project_timing_pairs[\s\S]+\(\.corpus_health\.collapsed \/\/ false\) == false/,
  "Pages deploy should require benchmark readiness JSON with no blocking (required) application-compat or project timing-pair gaps and a non-collapsed required corpus before using merged artifacts; canary application gaps are advisory",
);

assert.match(
  workflow,
  /Bench run \$\{WORKFLOW_RUN_ID\} published bench-results-merged, but readiness failed; skipping benchmark redeploy\.[\s\S]+should_deploy=false/,
  "Bench-triggered Pages deploys must skip diagnostic benchmark artifacts whose readiness gate failed",
);

assert.match(
  workflow,
  /Download latest benchmark data from GitHub artifact[\s\S]+actions\/download-artifact@\S+[\s\S]+name: bench-results-merged[\s\S]+path: artifacts/,
  "Pages deploy should download benchmark data only from the GitHub Actions artifact",
);

assert.doesNotMatch(
  workflow,
  /Download latest benchmark data from GCS|Download latest suite metrics from GCS|SCCACHE_GCS_KEY_JSON|gcloud auth|gs:\/\/|bench-vs-tsgo-gcs-latest/,
  "Pages deploy must not download benchmark or suite data from GCS",
);

assert.match(
  workflow,
  /No benchmark artifact downloaded; bench charts will use repository\/local fallback data\./,
  "Pages deploy should fall back to repository/local benchmark data when no GitHub artifact is available",
);

assert.match(
  workflow,
  /selectLatestBenchmarkArtifact[\s\S]+bench-vs-tsgo-github-latest\.json/,
  "Pages readiness status should describe the selected GitHub artifact",
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
  fullCi,
  /node scripts\/bench\/test-gh-pages-benchmark-artifact-gate\.mjs/,
  "full-ci lint should run the benchmark artifact gate test",
);

console.log("gh-pages benchmark artifact gate tests passed");
