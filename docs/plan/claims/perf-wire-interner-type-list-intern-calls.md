# perf(solver): wire interner_type_list_intern_calls counter

- **Date**: 2026-05-09
- **Branch**: `perf/wire-interner-type-list-intern-calls`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T1.1 (instrumentation — single-counter wiring)

## Intent

The `interner_type_list_intern_calls` counter exists in
`crates/tsz-common/src/perf_counters.rs:382` but neither side is wired:

- The hot callers `intern_type_list` (`crates/tsz-solver/src/intern/core/interner.rs:1188`) and `intern_type_list_from_slice` (`:1194`) do not increment it.
- `dump_string()` prints `n/a (not wired in this PR)` for the row.

Wire both sides. Sites #3 and #4 from PERFORMANCE_PLAN.md §4.1.1 share
the same counter (Vec vs slice variants of the same intern operation),
so they're naturally bundled.

## Files Touched

- `crates/tsz-solver/src/intern/core/interner.rs` (+6 LOC)
- `crates/tsz-common/src/perf_counters.rs` (-1/+2 LOC)

## Verification

- `cargo check -p tsz-common -p tsz-solver` (clean)
- `cargo nextest run -p tsz-common -p tsz-solver --lib` (6124 passed,
  7 skipped)
