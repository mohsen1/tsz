---
title: Benchmarks
layout: layouts/base.njk
page_class: benchmarks
permalink: /benchmarks/index.html
---

# Benchmarks

<p class="subtitle">A practical snapshot of tsz vs tsgo, with focus on speed for large TypeScript projects.</p>

## Summary

This page is a quick progress signal for relative performance across benchmark categories. It compares tsz against tsgo, and lower time is faster.

<div class="bench-legend">
  <span class="bench-legend-item"><span class="bench-legend-swatch tsz"></span> tsz (Rust compiler)</span>
  <span class="bench-legend-item"><span class="bench-legend-swatch tsgo"></span> tsgo (Go compiler)</span>
</div>

## Category Breakdown

{{ benchmark_charts | safe }}
