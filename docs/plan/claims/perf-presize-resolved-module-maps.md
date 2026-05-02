# perf(cli): pre-size resolved module maps

- **Date**: 2026-05-02
- **Branch**: `perf/presize-resolved-module-maps`
- **PR**: #2228
- **Status**: ready
- **Workstream**: 5 (large-repo residency/runtime)

## Intent

Pre-size the driver's resolved-module maps from the cached module specifier
lists already collected for the compilation. This avoids repeated hash-map and
hash-set growth during module resolution on large repositories while preserving
the existing resolution behavior and checker-facing data shape.

## Planned Scope

- `crates/tsz-cli/src/driver/check.rs`
- `docs/plan/claims/perf-presize-resolved-module-maps.md`

## Verification

- `cargo fmt --check` (pass)
- `cargo check -p tsz-cli` (pass)
- `cargo test -p tsz-cli driver_tests_ts2307` (pass; 8/8)
- `scripts/bench/perf-hotspots.sh --quick` (pass; tsz beat tsgo on all 5 fixtures)
  - 100 classes: 2.17x
  - Constraint conflicts N=30: 1.70x
  - DeepPartial optional-chain N=50: 1.40x
  - 50 generic functions: 1.38x
  - Shallow optional-chain N=50: 1.28x
- Guarded large-repo RSS sample:
  `RUST_BACKTRACE=1 scripts/safe-run.sh --limit 75% --interval 2 --verbose -- .target-bench/dist/tsz --extendedDiagnostics --noEmit -p ~/code/large-ts-repo/tsconfig.flat.bench.json`;
  manual stop after stable sample window, exit 143, peak sampled physical footprint ~11.32 GB / 12.29 GB guard.
