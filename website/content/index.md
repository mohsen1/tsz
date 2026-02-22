# Project Zang

<p class="subtitle">Project Zang (<code>tsz</code>) is an ambitious project to write a complete TypeScript type checker and language service in Rust that is faster than <code>tsgo</code>.</p>

<blockquote class="wip-warning">
<p><strong>Work in progress.</strong> This project is not ready for general use yet. Many TypeScript features are still being implemented.</p>
</blockquote>

## Progress

Currently targeting **TypeScript `{{ts_version}}`**

<div class="progress-row">
  <span class="progress-label">Conformance</span>
  <div class="progress-bar"><div class="progress-fill conformance" style="width: {{conformance_rate}}%"></div></div>
  <span class="progress-stat">{{conformance_rate}}% &mdash; {{conformance_passed}}/{{conformance_total}}</span>
</div>

<div class="progress-row">
  <span class="progress-label">JS Emit</span>
  <div class="progress-bar"><div class="progress-fill emit-js" style="width: {{emit_js_rate}}%"></div></div>
  <span class="progress-stat">{{emit_js_rate}}% &mdash; {{emit_js_passed}}/{{emit_js_total}}</span>
</div>

<div class="progress-row">
  <span class="progress-label">Declaration Emit</span>
  <div class="progress-bar"><div class="progress-fill emit-dts" style="width: {{emit_dts_rate}}%"></div></div>
  <span class="progress-stat">{{emit_dts_rate}}% &mdash; {{emit_dts_passed}}/{{emit_dts_total}}</span>
</div>

<div class="progress-row">
  <span class="progress-label">Language Service</span>
  <div class="progress-bar"><div class="progress-fill fourslash" style="width: {{fourslash_rate}}%"></div></div>
  <span class="progress-stat">{{fourslash_rate}}% &mdash; {{fourslash_passed}}/{{fourslash_total}}</span>
</div>

<p class="loc-stat">{{total_loc}} lines of Rust across {{num_crates}} crates</p>

Conformance is measured by diagnostic fingerprint comparison: each diagnostic must match `tsc` in error code, file, line, column, and message.

---

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
