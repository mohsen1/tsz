---
title: Benchmarks
layout: layouts/base.njk
page_class: benchmarks
permalink: /benchmarks/index.html
---

# Benchmarks

## Project Benchmarks

Comparing type-checking on existing TypeScript projects, with emphasis on projects that use lots of advanced type-system features.

<div class="bench-legend">
  <span class="bench-legend-item"><span class="bench-legend-swatch tsz"></span> tsz (Rust compiler)</span>
  <span class="bench-legend-item"><span class="bench-legend-swatch tsgo"></span> tsgo (Go compiler)</span>
</div>

{{ benchmark_charts | safe }}

## Micro Benchmarks

Focused cases for specific compiler paths: single-file library checks, generated type workloads, and solver stress tests.

<p class="benchmark-micro-link"><a href="/benchmarks/micro/">View micro benchmarks</a></p>
