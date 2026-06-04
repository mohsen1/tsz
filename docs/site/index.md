---
title: tsz
browser_title: Home
layout: layouts/base.njk
page_class: home
permalink: /index.html
extra_scripts: '<script src="/home.js" defer></script>'
---

<h1 class="home-logo-title"><span class="tsz-logo tsz-logo-hero" role="img" aria-label="tsz"></span></h1>

<p class="subtitle"><code>tsz</code> is a TypeScript checker, emitter, and language service written in Rust. It is closing in on <code>tsc</code> compatibility while making TypeScript feel much faster.</p>

<section class="try-tsz-prompt" aria-labelledby="try-tsz-title">
  <h2 id="try-tsz-title">Try tsz now</h2>
  <p>Run <code>npx try-tsz</code> to try the checker against a project or sample without installing it globally.</p>
</section>

<div class="hero-actions">
  <a href="/install.html">
    <svg aria-hidden="true" viewBox="0 0 24 24"><path d="M12 3v10m0 0 4-4m-4 4-4-4M4 15v3a3 3 0 0 0 3 3h10a3 3 0 0 0 3-3v-3"/></svg>
    <span>Install tsz</span>
  </a>
  <a href="/playground/">
    <svg aria-hidden="true" viewBox="0 0 24 24"><path d="m8 8-4 4 4 4m8-8 4 4-4 4m-2-12-4 16"/></svg>
    <span>Try the playground</span>
  </a>
  <a href="https://github.com/tsz-org/tsz">
    <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8z"/></svg>
    <span>GitHub</span>
  </a>
</div>

## Speed

{{ benchmark_mean_chart | safe }}

<p><a href="/benchmarks/">See the full benchmark page</a> for project timings and focused micro cases.</p>

## Compatibility

<p><code>tsz</code> is close to <code>tsc</code> compatibility across type checking, JavaScript emit, declaration emit, and editor behavior. The Compatibility page tracks the remaining gaps and release gates.</p>

<p><a href="/compatibility/">Read the compatibility status</a></p>
