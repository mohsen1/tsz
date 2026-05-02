# perf(core): pre-size skeleton reducer maps

- **Date**: 2026-05-02
- **Branch**: `perf/presize-skeleton-reducer-maps`
- **PR**: #2230
- **Status**: claim
- **Workstream**: 5 (large-repo residency/runtime)

## Intent

Pre-size `reduce_skeletons` aggregation maps from the per-file skeleton counts
already retained for the reduction pass. This avoids repeated hash-map and
hash-set growth while building the large-repo skeleton index, without changing
the reduced topology or post-merge consumers.

## Planned Scope

- `crates/tsz-core/src/parallel/skeleton.rs`
- `docs/plan/claims/perf-presize-skeleton-reducer-maps.md`

## Verification

- `cargo fmt`
- `cargo nextest run -p tsz-core skeleton_reduction_capacities_count_distinct_map_keys skeleton`
- `CARGO_TARGET_DIR=.target-bench cargo build --profile dist -p tsz-cli --bin tsz`
- Guarded large-repo sample after rebasing onto `origin/main`:
  `RUST_BACKTRACE=1 scripts/safe-run.sh --limit 75% --interval 2 --verbose -- .target-bench/dist/tsz --extendedDiagnostics --noEmit -p ~/code/large-ts-repo/tsconfig.flat.bench.json`
  still exceeded the local 36.86GB physical-footprint guard (peak sample
  39.94GB, exit 143), so this PR remains WIP and must not be marked ready.

## 2026-05-02 Update

The original capacity pre-pass sized hash-map buckets from total augmentation
entry counts. That over-reserved when many files shared the same augmentation
target/specifier. The pre-pass now counts distinct map keys instead and has a
unit test locking that behavior. This removes the obvious bucket over-allocation
bug, but the large-repo sample still fails the memory guard, so a separate
residency reduction is still required before promotion.
