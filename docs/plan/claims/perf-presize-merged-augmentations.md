# perf(core,cli): pre-size merged augmentation builders

- **Date**: 2026-05-02
- **Branch**: `perf/presize-merged-augmentations`
- **PR**: #2220
- **Status**: ready
- **Workstream**: 5 (large-repo residency/runtime)

## Intent

Pre-size the merged augmentation hash maps built once per compilation in the
CLI and core binder reconstruction paths. This avoids repeated hash-map growth
while preserving the existing Arc-sharing behavior for per-file binders.

## Planned Scope

- `crates/tsz-cli/src/driver/check_utils.rs`
- `crates/tsz-core/src/parallel/core.rs`
- `docs/plan/claims/perf-presize-merged-augmentations.md`

## Verification

- `cargo fmt --check` (pass)
- `cargo check -p tsz-cli` (pass)
- `cargo check -p tsz-core` (pass)
- `scripts/bench/perf-hotspots.sh --quick` (pass; tsz beat tsgo on all 5 fixtures)
  - 100 classes: 2.29x
  - Constraint conflicts N=30: 1.52x
  - Shallow optional-chain N=50: 1.44x
  - DeepPartial optional-chain N=50: 1.38x
  - 50 generic functions: 1.38x
- Guarded large-repo RSS sample:
  `RUST_BACKTRACE=1 scripts/safe-run.sh --limit 75% --interval 2 --verbose -- .target-bench/dist/tsz --extendedDiagnostics --noEmit -p ~/code/large-ts-repo/tsconfig.flat.bench.json`;
  manual stop after stable sample window, exit 143, peak sampled physical footprint ~11.29 GB / 12.29 GB guard.
