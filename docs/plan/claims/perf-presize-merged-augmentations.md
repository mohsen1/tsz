# perf(core,cli): pre-size merged augmentation builders

- **Date**: 2026-05-02
- **Branch**: `perf/presize-merged-augmentations`
- **PR**: TBD
- **Status**: claim
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

- `cargo fmt --check`
- `cargo check -p tsz-cli`
- `cargo check -p tsz-core`
- `scripts/bench/perf-hotspots.sh --quick`
