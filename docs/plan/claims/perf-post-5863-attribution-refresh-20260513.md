# perf: refresh post-#5863 attribution data

- **Date**: 2026-05-13
- **Branch**: `codex/perf-post-5863-attribution-20260513`
- **PR**: #6071
- **Status**: ready
- **Workstream**: `PERFORMANCE_PLAN.md` §7 Tier 2.2 decision input

## Intent

Run the attribution-mode scale-cliff benchmarks after the #5863
`cross_file_cache_miss_causes` wiring so the next T2.2 architecture PR targets
the dominant measured miss cause instead of guessing between gate, cache-key,
and `TypeId` visibility hypotheses.

## Planned Scope

- Capture fresh attribution JSON for the available scale-cliff fixtures.
- Summarize the new `cross_file_cache_miss_causes` buckets alongside delegate
  construction counters.
- Update the performance plan/decision-record docs with the chosen next T2.2
  target.

## Result

The 2026-05-13 run is checked in at
`docs/plan/perf-runs/2026-05-13-post-5863-attribution.md`. The new
`cross_file_cache_miss_causes` rows are present but all zero: the hot
`DelegateCrossArenaSymbol` misses bypass the canonical `cached_cross_file_*`
readers because they come from `binder.symbol_arenas`, not
`resolve_symbol_file_index`.

Next target: route symbol-arena-sourced source-file delegations through the
canonical cross-file query bucket before changing cache keys or `TypeId`
validation.

## Verification Plan

- `cargo build --release --features perf-tools --bin tsz`
- `scripts/bench/scale-cliff/run-cliff.sh`
- `scripts/bench/bench-vs-tsgo.sh` if the large fixture is available without
  unsafe local fallback assumptions
