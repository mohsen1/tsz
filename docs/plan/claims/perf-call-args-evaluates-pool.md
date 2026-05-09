# perf(solver): pool type_evaluates_to_function visited set

- **Date**: 2026-05-09
- **Branch**: `perf/call-args-evaluates-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (visitor allocator reuse)

## Intent

`type_evaluates_to_function` allocated a fresh `FxHashSet<TypeId>` per
call. Apply the same thread-local pool pattern as #4722 / #4790 / #4801
/ #4805.

## Files Touched

- `crates/tsz-solver/src/operations/call_args.rs` (~30 LOC)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
