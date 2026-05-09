# perf(solver): pool flow.rs has_type_query_for_symbol visited

- **Date**: 2026-05-09
- **Branch**: `perf/flow-visited-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (visitor allocator reuse)

## Intent

`has_type_query_for_symbol` (used for TS2502 detection) allocated a
fresh `FxHashSet<TypeId>` per call. Apply the same thread-local pool
pattern.

## Files Touched

- `crates/tsz-solver/src/type_queries/flow.rs` (~30 LOC)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
