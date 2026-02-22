---
title: Project Zang
layout: layouts/base.njk
page_class: home
permalink: /index.html
---

# Project Zang

<p class="subtitle">Project Zang (<code>tsz</code>) is an ambitious project to write a complete TypeScript type checker and language service in Rust that is faster than <code>tsgo</code>.</p>

<blockquote class="wip-warning">
<p><strong>Work in progress.</strong> This project is not ready for general use yet. Many TypeScript features are still being implemented.</p>
</blockquote>

## Performance

{{ benchmark_mean_chart | safe }}

## Progress

We run TypeScript's own test suite to ensure tsz can serve as a drop-in replacement - comparing diagnostics, JavaScript emit, declaration emit, and language service behavior against `tsc`.

Currently targeting **TypeScript `{{ metrics.ts_version }}`**

<div class="progress-row">
  <span class="progress-label">Conformance</span>
  <div class="progress-bar"><div class="progress-fill conformance" style="width: {{ metrics.conformance_rate }}%"></div></div>
  <span class="progress-stat">{{ metrics.conformance_rate }}% - {{ metrics.conformance_passed }}/{{ metrics.conformance_total }}</span>
</div>

<div class="progress-row">
  <span class="progress-label">JS Emit</span>
  <div class="progress-bar"><div class="progress-fill emit-js" style="width: {{ metrics.emit_js_rate }}%"></div></div>
  <span class="progress-stat">{{ metrics.emit_js_rate }}% - {{ metrics.emit_js_passed }}/{{ metrics.emit_js_total }}</span>
</div>

<div class="progress-row">
  <span class="progress-label">Declaration Emit</span>
  <div class="progress-bar"><div class="progress-fill emit-dts" style="width: {{ metrics.emit_dts_rate }}%"></div></div>
  <span class="progress-stat">{{ metrics.emit_dts_rate }}% - {{ metrics.emit_dts_passed }}/{{ metrics.emit_dts_total }}</span>
</div>

<div class="progress-row">
  <span class="progress-label">Language Service</span>
  <div class="progress-bar"><div class="progress-fill fourslash" style="width: {{ metrics.fourslash_rate }}%"></div></div>
  <span class="progress-stat">{{ metrics.fourslash_rate }}% - {{ metrics.fourslash_passed }}/{{ metrics.fourslash_total }}</span>
</div>

<p class="loc-stat">{{ metrics.total_loc }} lines of Rust across {{ metrics.num_crates }} crates</p>

Conformance is measured by diagnostic fingerprint comparison: each diagnostic must match `tsc` in error code, file, line, column, and message.

## Install

<div class="install-block">
  <span class="prompt">$</span>
  <span class="cmd">npm install -g @mohsen-azimi/tsz-dev</span>
</div>

Or with Cargo:

<div class="install-block">
  <span class="prompt">$</span>
  <span class="cmd">cargo install tsz-cli</span>
</div>

Then run it just like `tsc`:

<div class="install-block">
  <span class="prompt">$</span>
  <span class="cmd">tsz --project ./tsconfig.json</span>
</div>
