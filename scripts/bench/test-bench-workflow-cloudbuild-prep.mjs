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
const BENCH_PREPARE_CLOUDBUILD = path.join(
  ROOT,
  "scripts",
  "cloudbuild",
  "cloudbuild-bench-prepare.yaml",
);

const workflow = fs.readFileSync(BENCH_WORKFLOW, "utf8");
const shardCloudbuild = fs.readFileSync(BENCH_SHARD_CLOUDBUILD, "utf8");
const prepareCloudbuild = fs.readFileSync(BENCH_PREPARE_CLOUDBUILD, "utf8");

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
  prepareCloudbuild,
  /BENCH_RUST_TARGET_CPU=x86-64-v3/,
  "Cloud Build benchmark prep should build portable-but-representative (x86-64-v3) PGO binaries: " +
    "target-cpu=native SIGILLs on older shard hosts and bare x86-64 costs hot rows ~1.7-1.9x (#13248)",
);
assert.match(
  shardCloudbuild,
  /BENCH_RUST_TARGET_CPU=x86-64-v3/,
  "Cloud Build benchmark shards should inherit the same portable bench target CPU",
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

const cloudbuildPrepSubmit = workflow.match(
  /  bench-prepare-cloudbuild:[\s\S]+?  publish-latest-pgo:/,
)?.[0] ?? "";
assert.match(
  cloudbuildPrepSubmit,
  /target_sha="\$\{\{ env\.BENCH_TARGET_SHA \}\}"[\s\S]+manifest_target_sha="\$\(manifest_value BENCH_TARGET_SHA\)"[\s\S]+manifest_build_date="\$\(manifest_value BENCH_BUILD_DATE\)"[\s\S]+manifest_pgo_optimized="\$\(manifest_value BENCH_PGO_OPTIMIZED\)"[\s\S]+--substitutions=_BENCH_TARGET_SHA="\$\{target_sha\}"/,
  "Cloud Build prep submit should preserve the workflow target SHA when checking reusable artifacts",
);
assert.doesNotMatch(
  cloudbuildPrepSubmit,
  /source bench-prep\.env/,
  "Cloud Build prep submit must not source a reusable prep env because it can clobber BENCH_TARGET_SHA before submit",
);
assert.match(
  cloudbuildPrepSubmit,
  /Existing Cloud Build prep artifact is incomplete or stale; submitting a fresh build\.[\s\S]+rm -f bench-prep\.env bench-prep\.tar[\s\S]+gcloud builds submit/,
  "Cloud Build prep submit should remove stale reusable artifacts before uploading a fresh source archive",
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
assert.match(
  prepArtifactJob,
  /copy_from_cloudbuild_manifest\(\) \{[\s\S]+["']\/bench-prep\.env["'][\s\S]+["']bench-prep\.env["'][\s\S]+["']\/bench-prep\.tar["'][\s\S]+["']bench-prep\.tar["'][\s\S]+if copy_from_cloudbuild_manifest[\s\S]+if validate_downloaded_prep_artifact; then[\s\S]+exit 0/,
  "bench-prep-artifact may recover exact-build root-level Cloud Build artifacts only after validating target SHA and PGO",
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

// Emergency cost mode: perf fanout is manual-only. The per-commit
// workflow_run trigger + defer-to-oldest gate + age/stale/catch-up machinery
// stay removed, but the scheduled cadence is disabled until Cloud Build spend
// is intentionally budgeted again.
assert.doesNotMatch(
  workflow,
  /schedule:\s*\n\s*- cron:/,
  "bench should not run scheduled Cloud Build fanout during emergency cost scale-down",
);
assert.match(
  workflow,
  /workflow_dispatch:\s*\n\s*inputs:\s*\n\s*publish_latest_pgo:/,
  "bench should remain manually dispatchable",
);
assert.doesNotMatch(
  workflow,
  /workflow_run:\s*\n\s*workflows: \[CI\]/,
  "bench must no longer trigger on per-CI-completion (workflow_run)",
);
assert.match(
  workflow,
  /concurrency:\s*\n\s*group: bench-\$\{\{[\s\S]+?\}\}\s*\n\s*cancel-in-progress: false/,
  "bench should be single-flight via a per-channel concurrency group (cancel-in-progress:false)",
);
assert.match(
  workflow,
  /BENCH_TARGET_SHA: \$\{\{ github\.sha \}\}/,
  "bench should target the selected dispatch ref (github.sha) under manual mode",
);
assert.doesNotMatch(
  workflow,
  /BENCH_MAX_TARGET_AGE_HOURS|BENCH_STALE_ACTIVE_RUN_MINUTES|BENCH_CATCH_UP_MIN_INTERVAL_HOURS|active_run_id|deferring to the oldest/,
  "the defer/age/stale/catch-up mutex knobs and gate outputs should be gone",
);

assert.doesNotMatch(
  workflow,
  /gh run cancel/,
  "bench gate must not cancel active benchmark runs just because main moved",
);

assert.doesNotMatch(
  workflow,
  /\n  catch-up:\n|\n  catch-up-after-active:\n/,
  "the bench-catch-up and bench-catch-up-after-active jobs should be deleted",
);

assert.match(
  workflow,
  /- id: merge[\s\S]+expected_shards=9[\s\S]+echo "complete=false" >> "\$GITHUB_OUTPUT"[\s\S]+node scripts\/bench\/merge-results\.mjs/,
  "bench publish should merge partial shard sets into a diagnostic artifact instead of discarding completed shard JSON",
);

assert.match(
  workflow,
  /- id: app-compat-source[\s\S]+GITHUB_TOKEN: \$\{\{ github\.token \}\}[\s\S]+run_id="\$\{\{ github\.event\.workflow_run\.id \}\}"[\s\S]+run_id="\$\{candidate_id\}"/,
  "bench publish should resolve application compatibility from the triggering CI or an exact-SHA successful main CI without requiring gh on the publish runner",
);
assert.match(
  workflow,
  /- id: app-compat-source[\s\S]+node -e[\s\S]+actions\/runs\?branch=main&event=push&status=success&per_page=50[\s\S]+hostname: "api\.github\.com"[\s\S]+run\.name !== "CI"/,
  "bench publish should query successful main CI runs through the GitHub API instead of shelling out to gh",
);
assert.match(
  workflow,
  /- name: Download required compile compatibility from matching CI \(best-effort\)[\s\S]+if: steps\.app-compat-source\.outputs\.run_id != ''[\s\S]+uses: actions\/download-artifact@\S+[\s\S]+name: project-compile-compatibility[\s\S]+path: \.target\/app-compile-required-compat[\s\S]+run-id: \$\{\{ steps\.app-compat-source\.outputs\.run_id \}\}[\s\S]+github-token: \$\{\{ github\.token \}\}/,
  "bench publish should download matching-CI required compile compatibility with actions/download-artifact",
);
assert.match(
  workflow,
  /- name: Download canary compile compatibility from matching CI \(best-effort\)[\s\S]+if: steps\.app-compat-source\.outputs\.run_id != ''[\s\S]+uses: actions\/download-artifact@\S+[\s\S]+name: project-compile-canary-logs[\s\S]+path: \.target\/app-compile-canary-compat[\s\S]+run-id: \$\{\{ steps\.app-compat-source\.outputs\.run_id \}\}[\s\S]+github-token: \$\{\{ github\.token \}\}[\s\S]+- name: List application compile compatibility/,
  "bench publish should download matching-CI canary compile compatibility with actions/download-artifact",
);
assert.match(
  workflow,
  /mapfile -t triggering_compat_files[\s\S]+\.target\/app-compile-required-compat[\s\S]+\.target\/app-compile-canary-compat[\s\S]+compat_args\+=\( --compat-jsonl "\$app_compat" \)/,
  "bench publish should merge both required and canary matching-CI project compatibility JSONL files",
);
assert.doesNotMatch(
  workflow,
  /gh run download "\$CI_RUN_ID" -n project-compile-canary-logs/,
  "bench publish must not depend on gh being installed on the self-hosted publish runner",
);

assert.match(
  workflow,
  /- id: readiness[\s\S]+continue-on-error: true[\s\S]+check-artifact-readiness\.mjs[\s\S]+- name: Upload merged benchmark artifact/,
  "bench publish should keep the merged artifact upload path reachable when public readiness fails",
);

assert.match(
  workflow,
  /Fail non-published partial benchmark artifact[\s\S]+steps\.merge\.outputs\.complete != 'true' \|\| steps\.readiness\.outcome != 'success'[\s\S]+refusing to publish it as latest[\s\S]+exit 1[\s\S]+- name: Publish results[\s\S]+steps\.merge\.outputs\.complete == 'true' && steps\.readiness\.outcome == 'success'/,
  "partial or readiness-failing benchmark artifacts should upload for diagnosis but still fail before latest publication",
);


const benchJob = workflow.match(/  bench:[\s\S]+?  publish:/)?.[0] ?? "";
assert.match(
  benchJob,
  /- name: Download benchmark prep artifact[\s\S]+actions\/download-artifact@\S+[\s\S]+name: bench-prep-ready[\s\S]+- name: Validate source benchmark prep artifact[\s\S]+tar -tf bench-prep\.tar \.target-bench\/dist\/tsz[\s\S]+- id: cloudbuild-submit/,
  "benchmark shard jobs should include the validated prep artifact in the Cloud Build source archive before submit",
);

assert.match(
  shardCloudbuild,
  /if \[ -f bench-prep\.env \] && \[ -f bench-prep\.tar \]; then[\s\S]+Using benchmark prep artifact from the Cloud Build source archive\.[\s\S]+else[\s\S]+gcloud storage cp[\s\S]+bench-prep\/\$\{_BENCH_TARGET_SHA\}\/bench-prep\.env/,
  "Cloud Build shard prep should prefer source-provided prep artifacts before falling back to GCS",
);

console.log("bench workflow Cloud Build prep artifact tests passed");
