# perf(checker): correct counter gate comment to say "runtime", not "build"

- **Date**: 2026-05-10
- **Branch**: `perf/t0-counter-comment-runtime-precision-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: T0 perf-counter wiring

## Intent

Address a Copilot review comment on merged PR #5015. The terse one-liner
above the gated `record_root_checker_construction` increment said:

```
// PERF: per-file checker construction; gated to skip `counters()` deref in disabled builds.
```

The phrase "disabled builds" implies a compile-time gate, but counters
are toggled at *runtime* via the `TSZ_PERF_COUNTERS` env var read by
`enabled_fast()`. Compile-time gating exists only for the
`perf-counters-timing` feature, which is a different layer.

## Files Touched

- `crates/tsz-checker/src/state/state.rs` (one comment word change:
  "in disabled builds" → "when disabled at runtime")

## Verification

- `cargo check -p tsz-checker` clean
- File LOC unchanged at 2000 (still under arch guard limit)
- Pre-commit hook passes
