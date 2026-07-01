#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";

const workflow = fs.readFileSync(".github/workflows/bench.yml", "utf8");

assert.doesNotMatch(
  workflow,
  /scripts\/cloudbuild|cloudbuild-|bench-prep|bench-shards|artifacts-\$\{|BUILD_ID|PROJECT_ID|LOCATION/,
  "Bench should not submit Cloud Build prep or shard jobs",
);
assert.doesNotMatch(
  workflow,
  /Download Cloud Build|Wait for Cloud Build|capture_cloudbuild_log|cloudbuild_status|copy_from_cloudbuild_manifest/,
  "Bench should not poll Cloud Build artifacts or logs",
);
assert.match(
  workflow,
  /permissions:\s*\n\s*contents: read\s*\n\s*actions: write\s*\n\s*issues: read/,
  "Bench should keep only the permissions needed to upload artifacts and dispatch Pages",
);
assert.match(
  workflow,
  /concurrency:\s*\n\s*group: bench-\$\{\{ github\.ref \}\}\s*\n\s*cancel-in-progress: false/,
  "Bench should stay single-flight per ref without throwing away in-flight benchmark work",
);
assert.match(
  workflow,
  /sudo apt-get install -y jq bc git curl pkg-config libssl-dev[\s\S]+hyperfine/,
  "GitHub-hosted Bench should install the local benchmark runner dependencies",
);
assert.match(
  workflow,
  /actions\/upload-artifact@\S+[\s\S]+compression-level: 1/,
  "Bench should publish results through GitHub Actions artifacts",
);

console.log("bench workflow GCP-free shard policy tests passed");
