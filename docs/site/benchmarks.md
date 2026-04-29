---
title: Benchmarks
layout: layouts/base.njk
page_class: benchmarks
permalink: /benchmarks/index.html
---

# Benchmarks

## Summary

This page is a quick progress signal for relative performance across benchmark categories. It compares tsz against tsgo, lower time is faster, and successful rows require zero compiler exit codes.

<div class="bench-legend">
  <span class="bench-legend-item"><span class="bench-legend-swatch tsz"></span> tsz (Rust compiler)</span>
  <span class="bench-legend-item"><span class="bench-legend-swatch tsgo"></span> tsgo (Go compiler)</span>
</div>

## Category Breakdown

{{ benchmark_charts | safe }}
