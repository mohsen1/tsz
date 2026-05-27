#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const BENCH_WORKFLOW = path.join(ROOT, ".github", "workflows", "bench.yml");
const BENCH_SHARD_CLOUDBUILD = path.join(
  ROOT,
  "scripts",
  "cloudbuild",
  "cloudbuild-bench-shard.yaml",
);

const workflow = fs.readFileSync(BENCH_WORKFLOW, "utf8");
const shardCloudbuild = fs.readFileSync(BENCH_SHARD_CLOUDBUILD, "utf8");

const failFastMessages = workflow.match(
  /Cloud Build benchmark prep \$\{cloudbuild_id\} succeeded, but its manifest artifact is for/g,
) ?? [];
assert.equal(
  failFastMessages.length,
  2,
  "both Cloud Build prep artifact paths should fail fast on stale manifest artifacts after build success",
);

assert.doesNotMatch(
  workflow,
  /Cloud Build manifest artifact is for .*waiting for/,
  "successful Cloud Build prep with a stale manifest artifact must not wait until the 150 minute deadline",
);

const unusableArtifactMessages = workflow.match(
  /Cloud Build benchmark prep \$\{cloudbuild_id\} succeeded, but neither SHA-scoped, latest, nor manifest artifacts exposed valid bench-prep env\/tar for/g,
) ?? [];
assert.equal(
  unusableArtifactMessages.length,
  2,
  "both Cloud Build prep artifact paths should fail fast when a successful build exposes no usable prep artifacts",
);

assert.match(
  workflow,
  /"\/bench-prep\/\$\{_BENCH_TARGET_SHA\}\/bench-prep\.env"[\s\S]+"bench-prep\/\$\{_BENCH_TARGET_SHA\}\/bench-prep\.tar"/,
  "Cloud Build manifest parsing should accept literal unsubstituted target path entries from the exact build manifest",
);

assert.match(
  workflow,
  /expected \$\{target_sha\} \/ PGO=1\."\s*\n\s+exit 1/,
  "PGO prep path should report the expected target and exit immediately",
);

assert.match(
  workflow,
  /expected \$\{target_sha\} \/ PGO=1\."\s*\n\s+exit 1/,
  "benchmark prep path should report the expected target and PGO marker before exiting immediately",
);

assert.match(
  workflow,
  /gs:\/\/tsz-ci_cloudbuild\/bench-prep\/\$\{prep_prefix\}\/bench-prep\.env[\s\S]+gs:\/\/tsz-ci_cloudbuild\/bench-prep\/\$\{prep_prefix\}\/bench-prep\.tar[\s\S]+tar -tf bench-prep\.tar \.target-bench\/dist\/tsz[\s\S]+tar -tf bench-prep\.tar \.target-bench\/dist\/\.bench-pgo-optimized[\s\S]+Cloud Build prep artifact already exists/,
  "Cloud Build prep reuse should only skip submit after validating both the env and tar artifacts",
);

assert.match(
  workflow,
  /target_sha="\$\{\{ env\.BENCH_TARGET_SHA \}\}"[\s\S]+gs:\/\/tsz-ci_cloudbuild\/bench-prep\/\$\{target_sha\}\/bench-prep\.env[\s\S]+gs:\/\/tsz-ci_cloudbuild\/bench-prep\/\$\{target_sha\}\/bench-prep\.tar/,
  "benchmark prep artifact polling should keep using the stable workflow target SHA",
);

assert.match(
  workflow,
  /manifest_target_sha="\$\(manifest_value BENCH_TARGET_SHA\)"[\s\S]+manifest_pgo_optimized="\$\(manifest_value BENCH_PGO_OPTIMIZED\)"[\s\S]+\$\{manifest_target_sha\}" == "\$\{target_sha\}" &&[\s\S]+\$\{manifest_pgo_optimized\}" == "1"/,
  "benchmark prep artifact polling should validate manifest target and PGO without sourcing bench-prep.env",
);

const benchmarkPrepDownload = workflow.match(
  /needs\.bench-prepare\.outputs\.should_run != 'true'[\s\S]+?      - name: Validate benchmark prep artifact/,
)?.[0] ?? "";
assert.doesNotMatch(
  benchmarkPrepDownload,
  /source bench-prep\.env/,
  "benchmark prep artifact polling must not source bench-prep.env because it can clobber BENCH_TARGET_SHA",
);

const prepArtifactJob = workflow.match(
  /  bench-prep-artifact:[\s\S]+?  bench:/,
)?.[0] ?? "";
assert.doesNotMatch(
  prepArtifactJob,
  /^\s*["']\/?bench-prep\.(?:env|tar)["'],?$/m,
  "bench-prep-artifact must not trust root-level Cloud Build artifact paths",
);

const latestFallbacks = prepArtifactJob.match(
  /copy_from_latest_prep_artifact\(\) \{[\s\S]+?bench-prep\/latest\/bench-prep\.env[\s\S]+?bench-prep\/latest\/bench-prep\.tar[\s\S]+?validate_downloaded_prep_artifact[\s\S]+?\}/g,
) ?? [];
assert.equal(
  latestFallbacks.length,
  2,
  "both Cloud Build prep artifact paths should recover validated latest prep artifacts",
);

assert.match(
  prepArtifactJob,
  /validate_downloaded_prep_artifact\(\) \{[\s\S]+manifest_target_sha="\$\(manifest_value BENCH_TARGET_SHA\)"[\s\S]+manifest_pgo_optimized="\$\(manifest_value BENCH_PGO_OPTIMIZED\)"[\s\S]+\$\{manifest_target_sha\}" == "\$\{target_sha\}" &&[\s\S]+\$\{manifest_pgo_optimized\}" == "1"/,
  "latest prep artifact fallback must validate the downloaded artifact target SHA and PGO marker",
);

assert.match(
  prepArtifactJob,
  /name: bench-prep-ready[\s\S]+path:\s+\|[\s\S]+bench-prep\.env[\s\S]+bench-prep\.tar/,
  "bench-prep-ready artifact should include both the prep manifest and tarball consumed by shard jobs",
);

assert.match(
  workflow,
  /BENCH_MAX_TARGET_AGE_HOURS: "48"[\s\S]+target_date="\$\(gh api "repos\/\$\{\{ github\.repository \}\}\/commits\/\$\{target_sha\}" --jq '\.commit\.committer\.date' 2>\/dev\/null \|\| true\)"[\s\S]+Benchmark target \$\{target_sha\} is older than \$\{max_target_age_hours\}h/,
  "bench gate should reject genuinely old targets by age instead of exact-main mismatch",
);

assert.match(
  workflow,
  /Another Bench run is already active; letting it finish even if main has moved, and skipping this duplicate run\./,
  "bench gate should let active runs finish and skip duplicate runs",
);

assert.doesNotMatch(
  workflow,
  /gh run cancel/,
  "bench gate must not cancel active benchmark runs just because main moved",
);

assert.doesNotMatch(
  workflow,
  /bench-prep-target-fresh:|bench-target-fresh:|catchup_main_sha|Trigger benchmark catch-up/,
  "benchmark workflow should not kill or chase in-flight runs when main moves a few commits",
);

const benchJob = workflow.match(/  bench:[\s\S]+?  publish:/)?.[0] ?? "";
assert.match(
  benchJob,
  /- name: Download benchmark prep artifact[\s\S]+actions\/download-artifact@v4[\s\S]+name: bench-prep-ready[\s\S]+- name: Validate source benchmark prep artifact[\s\S]+tar -tf bench-prep\.tar \.target-bench\/dist\/tsz[\s\S]+- id: cloudbuild-submit/,
  "benchmark shard jobs should include the validated prep artifact in the Cloud Build source archive before submit",
);

assert.match(
  shardCloudbuild,
  /if \[\[ -f bench-prep\.env && -f bench-prep\.tar \]\]; then[\s\S]+Using benchmark prep artifact from the Cloud Build source archive\.[\s\S]+else[\s\S]+gcloud storage cp[\s\S]+bench-prep\/\$\{_BENCH_TARGET_SHA\}\/bench-prep\.env/,
  "Cloud Build shard prep should prefer source-provided prep artifacts before falling back to GCS",
);

console.log("bench workflow Cloud Build prep artifact tests passed");
