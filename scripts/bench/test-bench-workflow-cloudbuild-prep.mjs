#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";

const workflow = fs.readFileSync(".github/workflows/bench.yml", "utf8");

assert.match(
  workflow,
  /workflow_dispatch:\s*\n\s*inputs:[\s\S]+filter:[\s\S]+full_suite:[\s\S]+redeploy_site:/,
  "Bench should be manually dispatchable with a selected row filter and site redeploy switch",
);
assert.doesNotMatch(
  workflow,
  /workflow_run:\s*\n|schedule:\s*\n/,
  "Bench must not auto-fan-out from CI or schedules while GCP spend is shut down",
);
assert.match(
  workflow,
  /\n\s{2}bench:\n[\s\S]+runs-on: ubuntu-latest/,
  "Bench should run on GitHub-hosted Ubuntu",
);
assert.doesNotMatch(
  workflow,
  /self-hosted|tsz-cloud-run|gcloud|gsutil|gs:\/\/|Cloud Build|Cloud Run|_TSZ_CI_CACHE_BUCKET|SCCACHE_GCS_KEY_JSON/,
  "Bench workflow must not reference GCP runners, commands, buckets, or credentials",
);
assert.match(
  workflow,
  /BENCH_PGO: "0"[\s\S]+BENCH_REQUIRE_PGO: "0"/,
  "GitHub-hosted Bench should default to non-PGO mode to keep manual runs bounded",
);
assert.match(
  workflow,
  /scripts\/bench\/bench-vs-tsgo\.sh "\$\{args\[@\]\}"/,
  "Bench should execute the normal bench-vs-tsgo runner directly on GitHub Actions",
);
assert.match(
  workflow,
  /if \[\[ "\$\{FULL_SUITE\}" != "true" \]\]; then[\s\S]+args\+=\(--quick\)/,
  "Bench should default to quick mode unless full_suite is explicitly selected",
);
assert.match(
  workflow,
  /node scripts\/bench\/check-artifact-readiness\.mjs[\s\S]+--require-project-timing-pairs=1[\s\S]+bench-results-readiness\.json/,
  "Bench should emit readiness JSON for the Pages artifact gate",
);
assert.match(
  workflow,
  /name: bench-results-merged[\s\S]+bench-results\.json[\s\S]+bench-results-tsgo-winners\.json[\s\S]+bench-results-missing-attribution\.md[\s\S]+bench-results-readiness\.json/,
  "Bench should upload the merged artifact shape consumed by gh-pages",
);
assert.match(
  workflow,
  /actions\/workflows\/gh-pages\.yml\/dispatches/,
  "Readiness-clean bench runs should dispatch the GitHub Pages rebuild",
);

console.log("bench workflow GitHub Actions policy tests passed");
