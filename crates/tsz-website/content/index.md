# tsz

<p class="subtitle"><code>tsz</code> is a complete TypeScript type checker and language service in Rust that aims to be faster than <code>tsgo</code>.</p>

<blockquote class="wip-warning">
<p><strong>Work in progress.</strong> This project is not ready for general use yet. Many TypeScript features are still being implemented.</p>
</blockquote>

## Progress

tsz runs TypeScript's own test suite to prove it can serve as a drop-in replacement - comparing diagnostics, JavaScript emit, declaration emit, and language service behavior against `tsc`.

Currently targeting **TypeScript `{{ts_version}}`**

<div class="progress-row">
  <span class="progress-label">Conformance</span>
  <div class="progress-bar"><div class="progress-fill conformance" style="width: {{conformance_rate}}%"></div></div>
  <span class="progress-stat">{{conformance_rate}}% - {{conformance_passed}}/{{conformance_total}}</span>
</div>

<div class="progress-row">
  <span class="progress-label">JS Emit</span>
  <div class="progress-bar"><div class="progress-fill emit-js" style="width: {{emit_js_rate}}%"></div></div>
  <span class="progress-stat">{{emit_js_rate}}% - {{emit_js_passed}}/{{emit_js_total}}</span>
</div>

<div class="progress-row">
  <span class="progress-label">Declaration Emit</span>
  <div class="progress-bar"><div class="progress-fill emit-dts" style="width: {{emit_dts_rate}}%"></div></div>
  <span class="progress-stat">{{emit_dts_rate}}% - {{emit_dts_passed}}/{{emit_dts_total}}</span>
</div>

<div class="progress-row">
  <span class="progress-label">Language Service</span>
  <div class="progress-bar"><div class="progress-fill fourslash" style="width: {{fourslash_rate}}%"></div></div>
  <span class="progress-stat">{{fourslash_rate}}% - {{fourslash_passed}}/{{fourslash_total}}</span>
</div>

<p class="loc-stat">{{total_loc}} lines of Rust across {{num_crates}} crates</p>

Conformance is measured by diagnostic fingerprint comparison: each diagnostic must match `tsc` in error code, file, line, column, and message.

## Experimental Sound Mode

tsz also has an experimental **Sound Mode** for stricter, opt-in TypeScript checking.

It is still in exploration, stays behind explicit flags, and is being rolled out in a deliberately narrow way:

1. user-authored TypeScript source first
2. migration flags before ecosystem-wide guarantees
3. later declaration-boundary work behind separate pilots

Read more on the [Sound Mode page](/sound-mode/) or try the [Playground](/playground/).
