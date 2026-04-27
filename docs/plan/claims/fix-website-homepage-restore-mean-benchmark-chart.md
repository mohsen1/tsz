# Restore the average benchmark rates chart on the homepage

- **Date**: 2026-04-27
- **Branch**: `fix/website-homepage-restore-mean-benchmark-chart`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 6 (LSP And WASM As Service Clients — website-adjacent docs/UX)

## Intent

Bring the average-across-all-benchmarks chart back to the homepage. PR #1390
introduced a "Featured benchmark" highlight (`large-ts-repo`) that takes
priority over the arithmetic mean and shows only when the highlight has no
data. Restore the original `benchmark_mean_chart.js` behavior so the homepage
again shows the arithmetic mean across all benchmark cases.

The full benchmarks page (`/benchmarks/`) is unaffected — `large-ts-repo`
remains spotlighted there. Only the homepage chart variable
(`{{ benchmark_mean_chart | safe }}` in `docs/site/index.md`) is touched.

## Files Touched

- `crates/tsz-website/src/_data/benchmark_mean_chart.js` (~50 LOC removed):
  drop the `renderHighlightedBenchmark` helper + its `toNumber` helper, and
  export `renderMeanChart(loadBenchmarks())` directly.

## Verification

- `node --check crates/tsz-website/src/_data/benchmark_mean_chart.js` passes.
- Module-import smoke with synthetic 3-benchmark `artifacts/bench-vs-tsgo-*.json`
  produced the expected `<section class="benchmark-mean-card">` mean card with
  "Arithmetic mean across 3 benchmark cases" and a speedup label.
- Empty-artifacts case returns "" (renderMeanChart's existing no-data guard).
