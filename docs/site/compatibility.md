---
title: Compatibility
layout: layouts/base.njk
page_class: compatibility
permalink: /compatibility/index.html
---

# Compatibility

`tsz` is not a drop-in replacement yet. Even though TypeScript's own compiler tests pass, some benchmark projects still do not compile cleanly with `tsz`. There is still a lot of compatibility work to do before it can replace `tsc` on real projects.

Currently tracking **TypeScript `{{ metrics.ts_version }}`**.

## Type Checking

The conformance suite compares `tsz` diagnostics with `tsc` on TypeScript's own compiler tests. This is the main signal for whether `tsz` agrees with `tsc` about types and errors.

<div class="progress-row">
  <span class="progress-label">Type checking</span>
  <div class="progress-bar" aria-label="Type checking {{ metrics.conformance_rate_label }}, {{ metrics.conformance_passed }} of {{ metrics.conformance_total }}">
    <div class="progress-fill conformance" style="width: {{ metrics.conformance_bar_rate }}%"><span>{{ metrics.conformance_rate_label }}</span></div>
  </div>
</div>

<p class="compat-note">{{ metrics.conformance_passed }} of {{ metrics.conformance_total }} compiler tests match <code>tsc</code>.</p>

## Emit

Emit compatibility means the generated output matches what TypeScript users expect from `tsc`. JavaScript emit and declaration emit are tracked separately because they fail for different reasons and matter to different users.

<div class="progress-row">
  <span class="progress-label">JavaScript emit</span>
  <div class="progress-bar" aria-label="JavaScript emit {{ metrics.emit_js_rate_label }}, {{ metrics.emit_js_passed }} of {{ metrics.emit_js_total }}{{ metrics.emit_js_extra }}">
    <div class="progress-fill emit-js" style="width: {{ metrics.emit_js_bar_rate }}%"><span>{{ metrics.emit_js_rate_label }}</span></div>
  </div>
</div>

<div class="progress-row">
  <span class="progress-label">Declaration emit</span>
  <div class="progress-bar" aria-label="Declaration emit {{ metrics.emit_dts_rate_label }}, {{ metrics.emit_dts_passed }} of {{ metrics.emit_dts_total }}{{ metrics.emit_dts_extra }}">
    <div class="progress-fill emit-dts" style="width: {{ metrics.emit_dts_bar_rate }}%"><span>{{ metrics.emit_dts_rate_label }}</span></div>
  </div>
</div>

<p class="compat-note">JavaScript emit is at {{ metrics.emit_js_rate_label }}{{ metrics.emit_js_extra }}. Declaration emit is at {{ metrics.emit_dts_rate_label }}{{ metrics.emit_dts_extra }}.</p>

## Language Service

The language-service suite checks editor-facing behavior: completions, hover, go-to-definition, diagnostics, and related workflows that developers feel in an IDE.

<div class="progress-row">
  <span class="progress-label">Editor behavior</span>
  <div class="progress-bar" aria-label="Editor behavior {{ metrics.fourslash_rate_label }}, {{ metrics.fourslash_passed }} of {{ metrics.fourslash_total }}">
    <div class="progress-fill fourslash" style="width: {{ metrics.fourslash_bar_rate }}%"><span>{{ metrics.fourslash_rate_label }}</span></div>
  </div>
</div>

<p class="compat-note">{{ metrics.fourslash_passed }} of {{ metrics.fourslash_total }} editor tests match the TypeScript suite.</p>
