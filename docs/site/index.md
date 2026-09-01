---
title: tsz
browser_title: Home
layout: layouts/base.njk
page_class: home
permalink: /index.html
extra_scripts: '<script src="/home.js" defer></script>'
---

<h1 class="home-logo-title"><span class="tsz-logo tsz-logo-hero" role="img" aria-label="tsz"></span></h1>

<p class="subtitle"><code>tsz</code> is a clean-slate TypeScript compiler experiment in Rust, targeting the pinned TypeScript <code>7.0.2</code> compiler exactly.</p>

<section class="try-tsz-prompt" aria-labelledby="rewrite-status-title">
  <h2 id="rewrite-status-title">R0: validation only</h2>
  <p>The fresh compiler proves a narrow end-to-end seed slice. It is not a supported install, package release, WASM build, or drop-in replacement.</p>
</section>

<div class="hero-actions">
  <a href="https://github.com/tsz-org/tsz/blob/main/docs/plan/ROADMAP.md">
    <svg aria-hidden="true" viewBox="0 0 24 24"><path d="M5 4h14v16H5zM8 8h8M8 12h8M8 16h5"/></svg>
    <span>Read the roadmap</span>
  </a>
  <a href="https://github.com/tsz-org/tsz">
    <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>
    <span>GitHub</span>
  </a>
</div>

## What works now

The R0 seed matrix covers declarations and literal widening, annotations and
assignments, function calls and returns, object properties, a bounded union
subset, and JavaScript emit. It checks exact codes, spans, messages, ordering,
exit status, and emitted bytes against TypeScript `7.0.2`.

<p><a href="/compatibility/">See the capability and legacy-checkpoint split</a></p>

## Performance

There is no current rewrite speed claim. The eventual target is at least 3x
`tsgo` throughput on every row that first produces the same result as the
TypeScript oracle. The pre-reset benchmark dashboard is retained as frozen
historical evidence and is not rendered as R0 performance.

<p><a href="/benchmarks/">Read the R0 benchmark policy</a></p>
