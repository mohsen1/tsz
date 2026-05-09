# perf(solver): pool return_context placeholder visited (2 sites)

- **Date**: 2026-05-09
- **Branch**: `perf/return-context-visited-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (visitor allocator reuse)

## Intent

Two `type_contains_placeholder` call-sites in `return_context.rs`
allocated fresh visited sets per call. Apply the same thread-local
pool pattern. The third site at line 892 uses a different value type
(`FxHashSet<(TypeId, TypeId)>`) and is left unchanged.

## Files Touched

- `crates/tsz-solver/src/operations/generic_call/return_context.rs` (~30 LOC)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
