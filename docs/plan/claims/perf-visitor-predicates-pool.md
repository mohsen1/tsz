# perf(solver): pool visitor_predicates DFS scratch buffers

- **Date**: 2026-05-09
- **Branch**: `perf/visitor-predicates-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (small-fixture polish — visitor allocator reuse)

## Intent

Four predicate functions in `crates/tsz-solver/src/visitors/visitor_predicates.rs`
each allocated a fresh `FxHashSet<TypeId>` + `Vec<TypeId>` per call.
Apply the same thread-local pool pattern PR #4722 used for
`walk_referenced_types`, bundled via a shared `with_predicate_buffers`
helper.

## Files Touched

- `crates/tsz-solver/src/visitors/visitor_predicates.rs` (~50 LOC change)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
