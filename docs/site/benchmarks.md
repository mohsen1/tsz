---
title: Benchmarks
layout: layouts/base.njk
page_class: benchmarks
permalink: /benchmarks/index.html
---

# Benchmarks

`tsz` has focused on single-file performance so far. Work is underway to make it fast for full projects too.

## Summary

{{ benchmark_environment | safe }}

{{ benchmark_mean_chart | safe }}

<p class="benchmark-data-link"><a href="/benchmark-data/latest.json">View the raw benchmark artifact</a></p>

Benchmark artifacts are trend-comparable only when runner provenance is
consistent. Cloud Build timing shards need same-SHA calibration before they are
treated as the public trend line; calibration readiness requires green timed
rows across compiler-file, synthetic, solver-stress, project, and large-repo
families.

<div class="bench-legend">
  <span class="bench-legend-item"><span class="bench-legend-swatch tsz"></span> tsz (Rust compiler)</span>
  <span class="bench-legend-item"><span class="bench-legend-swatch tsgo"></span> tsgo (Go compiler)</span>
</div>

## Full Project Type Checking

Full-project rows use real repositories and generated app fixtures. If a project has a timing pair in the latest artifact, it is shown here even when compatibility tracking has more work left.

{{ benchmark_charts | safe }}

## Micro Benchmarks

Focused cases for specific compiler paths: single-file library checks, generated type workloads, and solver stress tests.

<p class="benchmark-micro-link"><a href="/benchmarks/micro/">View micro benchmarks</a></p>
