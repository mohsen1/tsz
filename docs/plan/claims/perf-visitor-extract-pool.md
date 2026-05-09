# perf(solver): pool visitor_extract DFS scratch sets

- **Date**: 2026-05-09
- **Branch**: `perf/visitor-extract-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (small-fixture polish — visitor allocator reuse)

## Intent

Two recursive DFS walkers in
`crates/tsz-solver/src/visitors/visitor_extract.rs` allocated a fresh
`FxHashSet<TypeId>` per top-level call:

- `contains_unresolved_application` (line 462)
- `collect_infer_bindings` (line 659)

Apply the same thread-local pool pattern as #4722 / #4790, scoped to
this module via a private `with_extract_visited` helper.

## Files Touched

- `crates/tsz-solver/src/visitors/visitor_extract.rs` (~30 LOC)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
