Status: ready
Branch: codex/website-benchmark-pages
Created: 2026-05-01T11:46:17Z

## Intent

Improve the website benchmark presentation by organizing the benchmark list more clearly and adding a generated detail page for each benchmark row.

## Scope

- `crates/tsz-website/src/_data/benchmark_charts.js`
- benchmark detail page templates under `crates/tsz-website/src/`
- benchmark CSS in `crates/tsz-website/static/style.css`
- docs/site benchmark copy if needed

## Verification

- Build the Eleventy website.
- Browse the benchmark list and detail pages locally.
