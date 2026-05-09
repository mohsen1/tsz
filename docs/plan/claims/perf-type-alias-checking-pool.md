# perf(checker): pool type_alias_checking DefId-visited

- **Date**: 2026-05-09
- **Branch**: `perf/type-alias-checking-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (visitor allocator reuse)

## Intent

The alias-resolution DFS in `type_alias_checking.rs` allocated a fresh
`FxHashSet<DefId>` per call. Apply the same thread-local pool pattern.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/type_alias_checking.rs` (~30 LOC)

## Verification

- `cargo check -p tsz-checker` (clean)
- File LOC: 1465 (well under the 2000 checker boundary).
