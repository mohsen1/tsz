#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";

const workflow = fs.readFileSync(".github/workflows/bench.yml", "utf8");

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
  /bench-postmortem-\$\{\{ matrix\.label \}\}\.log\s*\n\s*bench-cloudbuild-\$\{\{ matrix\.label \}\}\.log\s*\n\s*retention-days: 7/,
  "bench shard artifacts should include the captured Cloud Build log",
);

console.log("test-bench-workflow-cloudbuild-shards: ok");
