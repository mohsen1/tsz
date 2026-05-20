# Benchmarks

<p class="subtitle">Performance comparison: tsz vs tsgo</p>

Benchmarks are run using [hyperfine](https://github.com/sharkdp/hyperfine) with benchmark-specific run counts. Timed rows measure wall-clock time for a full type-check pass (no emit), and successful rows require zero compiler exit codes.

tsz is compiled with `--profile dist` (LTO enabled, single codegen unit). tsgo is the native Go compiler from the TypeScript team.

The page is organized by benchmark shape: project-mode rows first, then real library files and synthetic/compiler fixtures. Known-red project canaries are tracked separately as compile-readiness rows until tsz can compile them reliably enough for timing.

<div class="bench-legend">
  <span class="bench-legend-item"><span class="bench-legend-swatch tsz"></span> tsz (Rust)</span>
  <span class="bench-legend-item"><span class="bench-legend-swatch tsgo"></span> tsgo (Go)</span>
</div>

{{benchmark_charts}}


## Running Benchmarks Locally

To generate benchmark data yourself:

```
./scripts/bench/bench-vs-tsgo.sh --json
```

This produces a JSON file in `artifacts/` that the website build script uses to generate charts. Use `--quick` for faster results with fewer iterations.

See [bench-vs-tsgo.sh](https://github.com/mohsen1/tsz/blob/main/scripts/bench/bench-vs-tsgo.sh) for full usage.
