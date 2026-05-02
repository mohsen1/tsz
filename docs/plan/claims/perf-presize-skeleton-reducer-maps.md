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

- `cargo fmt --check`
- `cargo test -p tsz-core skeleton_reduction_capacities_count_distinct_map_keys -- --nocapture`
- `cargo test -p tsz-core skeleton`
- `CARGO_TARGET_DIR=.target-bench cargo build --profile dist -p tsz-cli --bin tsz`
- Guarded large-repo sample after rebasing onto `origin/main`:
  `RUST_BACKTRACE=1 scripts/safe-run.sh --limit 75% --interval 2 --verbose -- .target-bench/dist/tsz --extendedDiagnostics --noEmit -p ~/code/large-ts-repo/tsconfig.flat.bench.json`
  manually stopped after the sample window with exit 143. The branch stayed
  under the local 12.29GB physical-footprint guard; peak sampled footprint was
  11.74GB. It still did not finish the fixture, so this PR remains WIP and must
  not be marked ready.

## 2026-05-02 Update

The original capacity pre-pass sized hash-map buckets from total augmentation
entry counts. That over-reserved when many files shared the same augmentation
target/specifier. The pre-pass now counts distinct map keys instead and has a
unit test locking that behavior. This removes the obvious bucket over-allocation
bug. A follow-up guarded sample no longer exceeded the local memory guard, but
the large-repo fixture still did not finish during the manual sample window, so
separate runtime/residency reduction is still required before promotion.
