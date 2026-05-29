#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";

import { REQUIRED_PROJECT_ROWS } from "./project-rows.mjs";

const workflow = fs.readFileSync(".github/workflows/bench.yml", "utf8");
const shardCloudbuild = fs.readFileSync(
  "scripts/cloudbuild/cloudbuild-bench-shard.yaml",
  "utf8",
);

assert.match(
  workflow,
  /capture_cloudbuild_log\(\)\s*\{\s*gcloud builds log "\$\{\{ steps\.cloudbuild-submit\.outputs\.build_id \}\}"\s*\\\s*\n\s*--project=tsz-ci\s*\\\s*\n\s*--region=us-central1\s*\\\s*\n\s*>\s*"bench-cloudbuild-\$\{\{ matrix\.label \}\}\.log" 2>&1 \|\| true\s*\}/,
  "bench shard wait step should capture Cloud Build logs into a shard-local file",
);

assert.match(
  workflow,
  /Cloud Build shard succeeded but did not publish a status artifact for \$\{\{ matrix\.label \}\}\."\s*\n\s*capture_cloudbuild_log\s*\n\s*exit 1/,
  "missing shard status artifacts should upload the Cloud Build log",
);

assert.match(
  workflow,
  /Cloud Build benchmark shard \$\{\{ steps\.cloudbuild-submit\.outputs\.build_id \}\} ended with status \$\{status\}\."\s*\n\s*download_shard_artifacts \|\| true\s*\n\s*capture_cloudbuild_log\s*\n\s*exit 1/,
  "terminal Cloud Build shard failures should upload the Cloud Build log",
);

assert.match(
  workflow,
  /No finished Cloud Build benchmark shard artifact is available for \$\{\{ matrix\.label \}\} after \$\{\{ matrix\.timeout \}\} minutes\."\s*\n\s*\[\[ -z "\$status" \]\] \|\| echo "Last Cloud Build benchmark shard status: \$\{status\} \(\$\{\{ steps\.cloudbuild-submit\.outputs\.build_id \}\}\)\."\s*\n\s*capture_cloudbuild_log\s*\n\s*exit 1/,
  "timed-out shard waits should upload the Cloud Build log",
);

assert.match(
  workflow,
  /bench-results-\$\{\{ matrix\.label \}\}\.json\s*\n\s*bench-status-\$\{\{ matrix\.label \}\}\.env\s*\n\s*bench-run-\$\{\{ matrix\.label \}\}\.log\s*\n\s*bench-postmortem-\$\{\{ matrix\.label \}\}\.log\s*\n\s*bench-prep-fetch-\$\{\{ matrix\.label \}\}\.log\s*\n\s*bench-cloudbuild-\$\{\{ matrix\.label \}\}\.log\s*\n\s*retention-days: 7/,
  "bench shard artifacts should include status, result, and captured Cloud Build logs",
);

assert.match(
  workflow,
  /storage_cp "\$\{prefix\}\/bench-prep-fetch-\$\{\{ matrix\.label \}\}\.log" "bench-prep-fetch-\$\{\{ matrix\.label \}\}\.log" \|\| true/,
  "bench shard waits should download the prep-fetch log when Cloud Build publishes it",
);

assert.match(
  workflow,
  /copy_from_cloudbuild_manifest\(\)[\s\S]+artifacts-\$\{\{ steps\.cloudbuild-submit\.outputs\.build_id \}\}\.json[\s\S]+manifest_status[\s\S]+download_shard_artifacts\(\)[\s\S]+copy_from_cloudbuild_manifest/,
  "bench shard waits should fall back to the Cloud Build artifact manifest when object paths are flattened",
);

assert.match(
  workflow,
  /shard_status_value\(\)[\s\S]+index\(\$0, key "="\) == 1[\s\S]+status_target_sha="\$\(shard_status_value BENCH_TARGET_SHA\)"[\s\S]+status_shard_label="\$\(shard_status_value BENCH_SHARD_LABEL\)"[\s\S]+status_exit="\$\(shard_status_value BENCH_SHARD_STATUS\)"[\s\S]+exit "\$\{status_exit\}"/,
  "bench shard waits should parse status env files without sourcing unquoted filter values",
);

assert.match(
  workflow,
  /shard_published_complete_results\(\)[\s\S]+grep -q "Results Summary" "\$run_log"[\s\S]+grep -q "JSON results written:" "\$run_log"[\s\S]+Number\(data\.totals\.rows\) !== results\.length[\s\S]+if \[\[ "\$\{status_exit\}" != "0" \]\] && shard_published_complete_results; then[\s\S]+allowing publish to merge the artifact\.[\s\S]+exit 0/,
  "bench shard waits should allow a nonzero shard status when the runner produced a complete benchmark artifact",
);

assert.doesNotMatch(
  workflow,
  /source "bench-status-\$\{\{ matrix\.label \}\}\.env"/,
  "bench shard waits must not source status env files because BENCH_SHARD_FILTER can contain spaces or shell metacharacters",
);

const shardFilters = [...workflow.matchAll(/^\s+filter: '([^']+)'/gm)]
  .map((match) => new RegExp(match[1]));
const missingRequiredProjectRows = REQUIRED_PROJECT_ROWS
  .filter((row) => !shardFilters.some((filter) => filter.test(row)));
assert.deepEqual(
  missingRequiredProjectRows,
  [],
  "bench matrix filters should select every required project row so publish does not fail on missing compatibility rows",
);

assert.match(
  shardCloudbuild,
  /id: download-bench-prep[\s\S]+env:\s*\n\s*- '_BENCH_TARGET_SHA=\$\{_BENCH_TARGET_SHA\}'[\s\S]+#!\/bin\/sh[\s\S]+\) > bench-prep-fetch\.log 2>&1[\s\S]+BENCH_PREP_FETCH_STATUS=%s[\s\S]+exit 0/,
  "Cloud Build prep-fetch step should receive the benchmark target SHA, use the shell available in cloud-sdk:slim, record status, and never fail the build before shard status artifacts can be written",
);

assert.match(
  shardCloudbuild,
  /output_dir="bench-shards\/\$\{_BENCH_TARGET_SHA\}\/\$\{_BENCH_SHARD_LABEL\}"[\s\S]+mkdir -p "\$output_dir"[\s\S]+run_shard\(\)[\s\S]+apt-get update[\s\S]+hyperfine[\s\S]+pnpm config set store-dir/,
  "Cloud Build shard status directory should be prepared before setup commands that can fail, and shard images should install benchmark runtime tools",
);

assert.ok(
  shardCloudbuild.includes('run_shard 2>&1 | tee "/workspace/${run_log}"') &&
    shardCloudbuild.includes('shard_status="${PIPESTATUS[0]}"') &&
    shardCloudbuild.includes("printf 'BENCH_SHARD_STATUS=%s\\n' \"$shard_status\"") &&
    shardCloudbuild.includes("bench-prep-fetch-${_BENCH_SHARD_LABEL}.log") &&
    shardCloudbuild.includes("exit 0"),
  "Cloud Build shard should publish status/log artifacts and exit successfully so GitHub can consume BENCH_SHARD_STATUS",
);

console.log("test-bench-workflow-cloudbuild-shards: ok");
