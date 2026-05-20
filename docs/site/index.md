---
title: tsz
browser_title: Home
layout: layouts/base.njk
page_class: home
permalink: /index.html
extra_scripts: '<script src="/home.js" defer></script>'
---

<h1 class="home-logo-title"><span class="tsz-logo tsz-logo-hero" role="img" aria-label="tsz"></span></h1>

<p class="subtitle"><code>tsz</code> is a TypeScript checker, emitter, and language service written in Rust. The goal is simple: keep moving toward <code>tsc</code> compatibility while making TypeScript feel much faster.</p>

<div class="hero-actions">
  <a href="/install.html">Install tsz</a>
  <a href="/playground/">Try the playground</a>
  <a href="https://github.com/mohsen1/tsz">GitHub</a>
</div>

## Speed

{{ benchmark_mean_chart | safe }}

<p><a href="/benchmarks/">See the full benchmark page</a> for project timings and focused micro cases.</p>

## Compatibility

<p><code>tsz</code> is not a drop-in <code>tsc</code> replacement yet. The Compatibility page tracks how close it is for type checking, JavaScript emit, declaration emit, and editor behavior.</p>

<p><a href="/compatibility/">Read the compatibility status</a></p>
