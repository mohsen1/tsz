# perf(solver): pool reverse_mapped placeholder visited

- **Date**: 2026-05-09
- **Branch**: `perf/reverse-mapped-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (visitor allocator reuse)

## Intent

`find_keyof_inference_target` allocated a fresh `FxHashSet<TypeId>` for
its `type_contains_placeholder` check. Apply the same thread-local
pool pattern.

## Files Touched

- `crates/tsz-solver/src/operations/constraints/reverse_mapped.rs` (~30 LOC)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
