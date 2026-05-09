# perf(solver): pool signatures.rs placeholder-check visited sets

- **Date**: 2026-05-09
- **Branch**: `perf/signatures-visited-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (visitor allocator reuse)

## Intent

Two `type_contains_placeholder` call-sites in `signatures.rs` allocated
fresh visited sets per call. Apply the same thread-local pool pattern.

## Files Touched

- `crates/tsz-solver/src/operations/constraints/signatures.rs` (~30 LOC)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
