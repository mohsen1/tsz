# perf(solver): pool deep_reduce_for_display visited

- **Date**: 2026-05-09
- **Branch**: `perf/diagnostics-reduce-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (visitor allocator reuse)

## Intent

`deep_reduce_for_display` allocated a fresh `FxHashSet<TypeId>` per
call. Apply the same thread-local pool pattern.

## Files Touched

- `crates/tsz-solver/src/diagnostics/reduce.rs` (~30 LOC)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
