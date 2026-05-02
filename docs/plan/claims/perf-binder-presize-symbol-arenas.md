# perf(core): pre-size symbol_arenas + declaration_arenas at merge time

- **Date**: 2026-05-02
- **Branch**: `perf/binder-presize-symbol-arenas`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (large-repo residency)

## Intent

`merge_bind_results_from_source` builds two large `FxHashMap`s during
the merge phase:

- `symbol_arenas: FxHashMap<SymbolId, Arc<NodeArena>>` — one entry per
  merged symbol.
- `declaration_arenas: FxHashMap<(SymbolId, NodeIndex),
  SmallVec<[Arc<NodeArena>; 1]>>` — one entry per (symbol, declaration)
  pair, dominated by the single-declaration case so its size is
  approximately `total_symbols`.

Both started at `FxHashMap::default()` (zero capacity). On a 6086-file
project `total_symbols` lands in the hundreds of thousands range, so
the default doubling schedule rehashes ~17–18 times during the merge
loop, each rehash allocating a fresh bucket array and re-inserting
every prior entry.

The exact upper bound is known up-front (`total_symbols` is computed
just above the map declarations), so this change pre-sizes both maps
with `with_capacity_and_hasher(total_symbols, Default::default())` to
skip the rehash chain. No behavior change.

This builds on the same shape as the recently-merged pre-size patches
(#2166, #2191, #2157, #2211).

## Files Touched

- `crates/tsz-core/src/parallel/core.rs` (~12 LOC)

## Verification

- `cargo check -p tsz-core` — clean
- `cargo nextest run -p tsz-core -E 'test(test_merge)'` — 13/13
