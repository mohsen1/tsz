---
title: Micro Benchmarks
layout: layouts/base.njk
page_class: benchmarks
permalink: /benchmarks/micro/index.html
---

<nav class="bench-breadcrumb" aria-label="Breadcrumb">
  <a href="/benchmarks/">Benchmarks</a>
  <span>/</span>
  <span>Micro benchmarks</span>
</nav>

# Micro Benchmarks

Focused single-file, generated, and solver stress cases for isolating compiler hot spots after the full-project pass.

<div class="bench-legend">
  <span class="bench-legend-item"><span class="bench-legend-swatch tsz"></span> tsz</span>
  <span class="bench-legend-item"><span class="bench-legend-swatch tsgo"></span> tsgo</span>
</div>

{{ benchmark_micro_charts | safe }}
