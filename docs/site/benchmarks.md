---
title: Benchmarks
layout: layouts/base.njk
page_class: benchmarks
permalink: /benchmarks/index.html
---

# Benchmarks

## Project Benchmarks

Comparing type-checking on existing TypeScript projects, with emphasis on projects that use lots of advanced type-system features.
Known-red project canaries are kept out of timed vs-tsgo charts until they compile reliably; the small incomplete-timings section below tracks their compile-readiness status.

{{ benchmark_environment | safe }}

## Benchmark Artifact Validity

The public `latest.json` benchmark artifact is only useful when every timing
shard came from the same kind of runner and the same source revision. The
publish merge therefore treats runner provenance as part of the artifact
contract, not as optional decoration.

Every shard must record its source commit, workflow run, shard label, shard
filter, operating system, CPU model/count, and total memory. When a shard runs
inside Cloud Build it must also record the Cloud Build machine type. The publish
step refuses artifacts with missing runner signatures, duplicate shard labels,
or hardware signatures that differ between shards.

This keeps mixed-runner results out of the public trend line. Current Cloud
Build prep artifacts and future Cloud Build timing shards are not comparable to
the historical runner series until a same-SHA calibration artifact documents the
speed/noise difference.

<div class="bench-legend">
  <span class="bench-legend-item"><span class="bench-legend-swatch tsz"></span> tsz (Rust compiler)</span>
  <span class="bench-legend-item"><span class="bench-legend-swatch tsgo"></span> tsgo (Go compiler)</span>
</div>

{{ benchmark_charts | safe }}

## Micro Benchmarks

Focused cases for specific compiler paths: single-file library checks, generated type workloads, and solver stress tests.

<p class="benchmark-micro-link"><a href="/benchmarks/micro/">View micro benchmarks</a></p>

{{ project_compatibility_dashboard | safe }}
