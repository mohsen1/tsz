# perf(solver): wire interner_function_shape_intern_calls counter

- **Date**: 2026-05-09
- **Branch**: `perf/wire-interner-function-shape-intern-calls`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T1.1 (instrumentation — single-counter wiring)

## Intent

Wire site #6 from PERFORMANCE_PLAN.md §4.1.1.

## Files Touched

- `crates/tsz-solver/src/intern/core/interner.rs` (+3 LOC)
- `crates/tsz-common/src/perf_counters.rs` (-1/+2 LOC)

## Verification

- `cargo check -p tsz-common -p tsz-solver` (clean)
- Pre-commit will run affected-crate tests.
