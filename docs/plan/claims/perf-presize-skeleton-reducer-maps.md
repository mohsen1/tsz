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

- Pending
