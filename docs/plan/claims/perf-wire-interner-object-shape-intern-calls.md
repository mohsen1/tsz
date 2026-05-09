# perf(solver): wire interner_object_shape_intern_calls counter

- **Date**: 2026-05-09
- **Branch**: `perf/wire-interner-object-shape-intern-calls`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T1.1 (instrumentation — single-counter wiring)

## Intent

Wire site #5 from PERFORMANCE_PLAN.md §4.1.1: `intern_object_shape` at
`crates/tsz-solver/src/intern/core/interner.rs:1212` and the matching
`dump_string()` row.

## Files Touched

- `crates/tsz-solver/src/intern/core/interner.rs` (+3 LOC)
- `crates/tsz-common/src/perf_counters.rs` (-1/+2 LOC)

## Verification

- `cargo check -p tsz-common -p tsz-solver` (clean)
- Pre-commit will run affected-crate tests.
