# perf(solver): pool instantiate_type_params_to_constraints visited set

- **Date**: 2026-05-09
- **Branch**: `perf/instantiate-constraints-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (small-fixture polish — visitor allocator reuse)

## Intent

`instantiate_type_params_to_constraints` allocated a fresh
`FxHashSet<TypeId>` for `collect_type_param_constraint_substitutions`'s
recursive DFS on every call. Apply the same thread-local pool pattern
as #4722 / #4790 / #4801, scoped to this module via a private
`with_constraint_visited` helper.

## Files Touched

- `crates/tsz-solver/src/instantiation/instantiate.rs` (~30 LOC)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
