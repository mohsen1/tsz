# perf(solver): pool walker.rs placeholder-check visited sets

- **Date**: 2026-05-09
- **Branch**: `perf/walker-placeholder-visited-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (visitor allocator reuse)

## Intent

Five `type_contains_placeholder` call-sites in
`crates/tsz-solver/src/operations/constraints/walker.rs` each
allocated a fresh `FxHashSet<TypeId>` per call. Apply the same
thread-local pool pattern as #4722 / #4790 / #4801 / #4805 / #4807,
bundled via a shared `with_placeholder_visited` helper.

## Files Touched

- `crates/tsz-solver/src/operations/constraints/walker.rs` (~50 LOC change)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
