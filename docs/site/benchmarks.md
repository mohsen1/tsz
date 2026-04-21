---
title: Benchmarks
layout: layouts/base.njk
page_class: benchmarks
permalink: /benchmarks/index.html
---

# Benchmarks

<p class="subtitle">Compiler performance by benchmark category: tsz vs TSGO</p>

Benchmarks are run using [hyperfine](https://github.com/sharkdp/hyperfine) with warmup passes and multiple runs. Each benchmark measures wall-clock time for a full type-check pass (no emit).

tsz is compiled with `--profile dist` (LTO enabled, single codegen unit). tsgo is the native Go compiler from the TypeScript team.

<div class="bench-legend">
  <span class="bench-legend-item"><span class="bench-legend-swatch tsz"></span> tsz (Rust compiler)</span>
  <span class="bench-legend-item"><span class="bench-legend-swatch tsgo"></span> TSGO (Go compiler)</span>
</div>

## Category Breakdown

{{ benchmark_charts | safe }}
