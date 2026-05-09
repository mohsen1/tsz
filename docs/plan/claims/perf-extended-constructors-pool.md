# perf(solver): pool resolve_abstract_constructor_anchor visited

- **Date**: 2026-05-09
- **Branch**: `perf/extended-constructors-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (visitor allocator reuse)

## Intent

`resolve_abstract_constructor_anchor` allocated a fresh
`FxHashSet<TypeId>` per call. Apply the same thread-local pool
pattern.

## Files Touched

- `crates/tsz-solver/src/type_queries/extended_constructors.rs` (~30 LOC)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
