# perf(solver): pool extended.rs index-type DFS visited (3 sites)

- **Date**: 2026-05-09
- **Branch**: `perf/extended-visited-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (visitor allocator reuse)

## Intent

Three index-type DFS walkers in `type_queries/extended.rs`
(`get_invalid_index_type_member`, `is_index_key_anchor`,
`get_invalid_index_type_member_strict`) each allocated a fresh
`FxHashSet<TypeId>` per call. Apply the same thread-local pool
pattern, bundled via `with_extended_visited`.

## Files Touched

- `crates/tsz-solver/src/type_queries/extended.rs` (~30 LOC)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
