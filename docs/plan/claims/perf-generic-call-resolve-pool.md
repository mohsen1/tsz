# perf(solver): pool generic_call/resolve placeholder visited

- **Date**: 2026-05-09
- **Branch**: `perf/generic-call-resolve-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (visitor allocator reuse)

## Intent

`generic_call/resolve.rs` allocated a fresh `FxHashSet<TypeId>` for a
`type_contains_placeholder` check. Apply the same thread-local pool
pattern.

## Files Touched

- `crates/tsz-solver/src/operations/generic_call/resolve.rs` (~30 LOC)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
