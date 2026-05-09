# perf(solver): wire conditional/mapped/application intern counters

- **Date**: 2026-05-09
- **Branch**: `perf/wire-interner-conditional-mapped-application`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T1.1 (instrumentation — counter wiring)

## Intent

Wire the remaining three intern-call counters from PERFORMANCE_PLAN.md
§4.1.1 in one bundled PR (sites #7, #8, #9):

- `interner_conditional_intern_calls` at `intern_conditional_type` (line 1688).
- `interner_mapped_intern_calls` at `intern_mapped_type` (line 1695).
- `interner_application_intern_calls` at `intern_application` (line 1699).

These three are sibling factories on `TypeInterner` with identical
shapes; bundling avoids three near-identical PRs.

## Files Touched

- `crates/tsz-solver/src/intern/core/interner.rs` (+9 LOC)
- `crates/tsz-common/src/perf_counters.rs` (-3/+6 LOC)

## Verification

- `cargo check -p tsz-common -p tsz-solver` (clean)
- Pre-commit will run affected-crate tests.
