---
title: tsz
browser_title: Home
layout: layouts/base.njk
page_class: home
permalink: /index.html
extra_scripts: '<script src="/home.js" defer></script>'
---

<h1 class="home-logo-title"><span class="tsz-logo tsz-logo-hero" role="img" aria-label="tsz"></span></h1>

<p class="subtitle"><code>tsz</code> is a TypeScript checker and language service written in Rust. It is designed to outperform <code>tsgo</code>, and beyond speed, <code>tsz</code> also targets a <a href="/sound-mode">Sound Mode</a> for stricter type-checking.</p>

## Performance

{{ benchmark_mean_chart | safe }}

<blockquote class="wip-warning">
  <h3>Project Status</h3>
  <p><strong>Nearly complete.</strong> TypeScript support is in its final compiler stages, with remaining work focused on performance tuning and LSP support in WebAssembly.</p>
</blockquote>

## Progress

tsz runs TypeScript's own test suite to prove it can serve as a drop-in replacement - comparing diagnostics, JavaScript emit, declaration emit, and language service behavior against `tsc`.

Currently targeting **TypeScript `{{ metrics.ts_version }}`**

<div class="progress-row">
  <span class="progress-label">Conformance</span>
  <div class="progress-bar"><div class="progress-fill conformance" style="width: {{ metrics.conformance_bar_rate }}%"></div></div>
  <span class="progress-stat">{{ metrics.conformance_rate_label }} - {{ metrics.conformance_passed }}/{{ metrics.conformance_total }}</span>
</div>

<div class="progress-row">
  <span class="progress-label">JS Emit</span>
  <div class="progress-bar"><div class="progress-fill emit-js" style="width: {{ metrics.emit_js_bar_rate }}%"></div></div>
  <span class="progress-stat">{{ metrics.emit_js_rate_label }} - {{ metrics.emit_js_passed }}/{{ metrics.emit_js_total }}{{ metrics.emit_js_extra }}</span>
</div>

<div class="progress-row">
  <span class="progress-label">Declaration Emit</span>
  <div class="progress-bar"><div class="progress-fill emit-dts" style="width: {{ metrics.emit_dts_bar_rate }}%"></div></div>
  <span class="progress-stat">{{ metrics.emit_dts_rate_label }} - {{ metrics.emit_dts_passed }}/{{ metrics.emit_dts_total }}{{ metrics.emit_dts_extra }}</span>
</div>

<div class="progress-row">
  <span class="progress-label">Language Service</span>
  <div class="progress-bar"><div class="progress-fill fourslash" style="width: {{ metrics.fourslash_bar_rate }}%"></div></div>
  <span class="progress-stat">{{ metrics.fourslash_rate_label }} - {{ metrics.fourslash_passed }}/{{ metrics.fourslash_total }}</span>
</div>

<p class="loc-stat">{{ metrics.total_loc }} lines of Rust across {{ metrics.num_crates }} crates</p>
