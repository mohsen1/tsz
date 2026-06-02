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

const successGraceWindows = workflow.match(
  /success_seen_at=""[\s\S]+?success_grace_seconds=7200/g,
) ?? [];
assert.equal(
  successGraceWindows.length,
  2,
  "both Cloud Build prep artifact paths should use the extended post-success artifact visibility grace window",
);

const postSuccessMessages = workflow.match(
  /Cloud Build benchmark prep \$\{cloudbuild_id\} succeeded; waiting up to \$\{success_grace_seconds\}s for prep artifacts to become visible\./g,
) ?? [];
assert.equal(
  postSuccessMessages.length,
  2,
  "both Cloud Build prep artifact paths should log when entering the post-success artifact visibility grace window",
);

const staleManifestWaits = workflow.match(
  /Cloud Build benchmark prep \$\{cloudbuild_id\} manifest artifact is for \$\{manifest_target_sha:-unknown\} \/ PGO=\$\{manifest_pgo_optimized:-0\}; waiting for \$\{target_sha\} \/ PGO=1\./g,
) ?? [];
assert.equal(
  staleManifestWaits.length,
  2,
  "successful Cloud Build prep with a stale manifest artifact should keep polling during the post-success grace window",
);

const unusableArtifactMessages = workflow.match(
  /Cloud Build benchmark prep \$\{cloudbuild_id\} succeeded, but neither SHA-scoped, latest, nor manifest artifacts exposed valid bench-prep env\/tar for \$\{target_sha\} after \$\{success_grace_seconds\}s\./g,
) ?? [];
assert.equal(
  unusableArtifactMessages.length,
  2,
  "both Cloud Build prep artifact paths should fail after the bounded post-success grace window when no usable prep artifacts appear",
);

const prepLogCaptureHelpers = workflow.match(
  /capture_cloudbuild_log\(\) \{[\s\S]+?bench-prep-cloudbuild\.log 2>&1 \|\| true[\s\S]+?\}/g,
) ?? [];
assert.equal(
  prepLogCaptureHelpers.length,
  2,
  "both Cloud Build prep artifact paths should capture the prep build log before terminal handoff failures",
);

assert.match(
  workflow,
  /- name: Upload benchmark prep diagnostics[\s\S]+if: failure\(\)[\s\S]+name: bench-prep-diagnostics[\s\S]+bench-prep-cloudbuild\.log[\s\S]+cloudbuild-artifacts\.json[\s\S]+latest-bench-prep\.env/,
  "bench-prep-artifact should upload Cloud Build handoff diagnostics when prep artifact polling fails",
);

assert.match(
  workflow,
  /"\/bench-prep\/\$\{_BENCH_TARGET_SHA\}\/bench-prep\.env"[\s\S]+"bench-prep\/\$\{_BENCH_TARGET_SHA\}\/bench-prep\.tar"/,
  "Cloud Build manifest parsing should accept literal unsubstituted target path entries from the exact build manifest",
);

assert.match(
  workflow,
  /"\/bench-prep\/latest\/bench-prep\.env"[\s\S]+"bench-prep\/latest\/bench-prep\.tar"/,
  "Cloud Build manifest parsing should accept latest prep entries and still validate their manifest target before use",
);

assert.match(
  workflow,
  /waiting for \$\{target_sha\} \/ PGO=1\."\s*\n\s+fi\s*\n\s+if \(\( SECONDS - success_seen_at >= success_grace_seconds \)\)/,
  "PGO prep path should report the expected target and wait until the post-success grace deadline",
);

assert.match(
  workflow,
  /waiting for \$\{target_sha\} \/ PGO=1\."\s*\n\s+fi\s*\n\s+if \(\( SECONDS - success_seen_at >= success_grace_seconds \)\)/,
  "benchmark prep path should report the expected target and PGO marker before the post-success grace deadline",
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
  /"\$\{\{ github\.event_name \}\}" == "workflow_run"[\s\S]+Another Bench run is already active; letting it finish even if main has moved, and skipping this duplicate run\./,
  "bench gate should let active runs finish and skip duplicate automatic runs",
);

assert.doesNotMatch(
  workflow,
  /gh run cancel/,
  "bench gate must not cancel active benchmark runs just because main moved",
);

assert.match(
  workflow,
  /catch-up:[\s\S]+needs: \[bench-gate, bench-prep-artifact, bench, publish\][\s\S]+github\.event_name == 'workflow_run'[\s\S]+needs\.bench-gate\.outputs\.should_run == 'true'[\s\S]+needs\.publish\.result != 'success'/,
  "benchmark workflow should schedule catch-up only after a publish-capable workflow_run exits without publishing",
);

assert.match(
  workflow,
  /target_sha="\$\{\{ env\.BENCH_TARGET_SHA \}\}"[\s\S]+main_sha="\$\(gh api "repos\/\$\{GITHUB_REPOSITORY\}\/git\/ref\/heads\/main" --jq '\.object\.sha'[\s\S]+if \[\[ "\$\{target_sha\}" == "\$\{main_sha\}" \]\]; then[\s\S]+skipping catch-up dispatch/,
  "benchmark catch-up should only dispatch when main moved beyond the non-publishing target",
);

assert.match(
  workflow,
  /gh run list --repo "\$\{GITHUB_REPOSITORY\}" --workflow bench\.yml --branch main --status in_progress[\s\S]+select\(\.databaseId != \$\{\{ github\.run_id \}\}\)[\s\S]+gh run list --repo "\$\{GITHUB_REPOSITORY\}" --workflow bench\.yml --branch main --status queued[\s\S]+Another Bench run is already active; skipping catch-up dispatch/,
  "benchmark catch-up should avoid dispatching when another Bench run is already active",
);

assert.match(
  workflow,
  /Bench target \$\{target_sha\} did not publish and main is now \$\{main_sha\}; dispatching one catch-up Bench run\.[\s\S]+actions\/workflows\/bench\.yml\/dispatches[\s\S]+'{"ref":"main","inputs":\{"publish_latest_pgo":false\}}'/,
  "benchmark catch-up should dispatch one fresh main benchmark run after a stale non-publish",
);

const benchJob = workflow.match(/  bench:[\s\S]+?  publish:/)?.[0] ?? "";
assert.match(
  benchJob,
  /- name: Download benchmark prep artifact[\s\S]+actions\/download-artifact@v4[\s\S]+name: bench-prep-ready[\s\S]+- name: Validate source benchmark prep artifact[\s\S]+tar -tf bench-prep\.tar \.target-bench\/dist\/tsz[\s\S]+- id: cloudbuild-submit/,
  "benchmark shard jobs should include the validated prep artifact in the Cloud Build source archive before submit",
);

assert.match(
  shardCloudbuild,
  /if \[ -f bench-prep\.env \] && \[ -f bench-prep\.tar \]; then[\s\S]+Using benchmark prep artifact from the Cloud Build source archive\.[\s\S]+else[\s\S]+gcloud storage cp[\s\S]+bench-prep\/\$\{_BENCH_TARGET_SHA\}\/bench-prep\.env/,
  "Cloud Build shard prep should prefer source-provided prep artifacts before falling back to GCS",
);

console.log("bench workflow Cloud Build prep artifact tests passed");
