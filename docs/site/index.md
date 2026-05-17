---
title: tsz
browser_title: Home
layout: layouts/base.njk
page_class: home
permalink: /index.html
extra_scripts: '<script src="/home.js" defer></script>'
---

<h1 class="home-logo-title"><span class="tsz-logo tsz-logo-hero" role="img" aria-label="tsz"></span></h1>

<p class="subtitle"><code>tsz</code> is a TypeScript checker and language service written in Rust. It is designed to outperform <code>tsgo</code>, and beyond speed, <code>tsz</code> also explores an <a href="/sound-mode">experimental Sound Mode</a> for stricter type-checking.</p>

<div class="hero-actions">
  <a href="/install.html">Install tsz</a>
  <a href="/playground/">Try the playground</a>
  <a href="https://github.com/mohsen1/tsz">GitHub</a>
</div>

## Performance

{{ benchmark_mean_chart | safe }}

<blockquote class="wip-warning">
  <h3>Project Status</h3>
  <p><strong>Work in progress.</strong> The chart above only reflects micro benchmarks. Large-project performance work is still underway, and at the moment <code>tsz</code> is not optimized for large projects.</p>
</blockquote>

## TypeScript compatibility

tsz runs TypeScript's own test suite against `tsc`, comparing diagnostics, JavaScript emit, declaration emit, and language service behavior. Even with 100% of TypeScript's compiler tests passing, we are not yet confident that tsz is fully compatible. Many full-project benchmarks still fail to compile, and many open bugs are not covered by conformance tests.

Currently targeting **TypeScript `{{ metrics.ts_version }}`**

<div class="progress-row">
  <span class="progress-label">Conformance</span>
  <div class="progress-bar" aria-label="Conformance {{ metrics.conformance_rate_label }}, {{ metrics.conformance_passed }} of {{ metrics.conformance_total }}">
    <div class="progress-fill conformance" style="width: {{ metrics.conformance_bar_rate }}%"><span>{{ metrics.conformance_rate_label }}</span></div>
  </div>
</div>

<div class="progress-row">
  <span class="progress-label">JS Emit</span>
  <div class="progress-bar" aria-label="JS Emit {{ metrics.emit_js_rate_label }}, {{ metrics.emit_js_passed }} of {{ metrics.emit_js_total }}{{ metrics.emit_js_extra }}">
    <div class="progress-fill emit-js" style="width: {{ metrics.emit_js_bar_rate }}%"><span>{{ metrics.emit_js_rate_label }}</span></div>
  </div>
</div>

<div class="progress-row">
  <span class="progress-label">Declaration Emit</span>
  <div class="progress-bar" aria-label="Declaration Emit {{ metrics.emit_dts_rate_label }}, {{ metrics.emit_dts_passed }} of {{ metrics.emit_dts_total }}{{ metrics.emit_dts_extra }}">
    <div class="progress-fill emit-dts" style="width: {{ metrics.emit_dts_bar_rate }}%"><span>{{ metrics.emit_dts_rate_label }}</span></div>
  </div>
</div>

<div class="progress-row">
  <span class="progress-label">Language Service</span>
  <div class="progress-bar" aria-label="Language Service {{ metrics.fourslash_rate_label }}, {{ metrics.fourslash_passed }} of {{ metrics.fourslash_total }}">
    <div class="progress-fill fourslash" style="width: {{ metrics.fourslash_bar_rate }}%"><span>{{ metrics.fourslash_rate_label }}</span></div>
  </div>
</div>

<p class="loc-stat">{{ metrics.total_loc }} lines of Rust across {{ metrics.num_crates }} crates</p>
