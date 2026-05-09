# perf(checker): wire delegate_max_recursion_depth via RAII guard

- **Date**: 2026-05-09
- **Branch**: `perf/wire-delegate-depth-guard`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T1.1 (instrumentation — site #11)

## Intent

Wire site #11 from PERFORMANCE_PLAN.md §4.1.1: track the running peak
recursion depth into `delegate_cross_arena_symbol_resolution` via a
thread-local + RAII guard.

The `delegate_max_recursion_depth` counter is already declared and
already loaded in `dump_string()` — only the source-side instrumentation
was missing. Add a `DelegateDepthGuard` RAII helper in `perf_counters.rs`
(thread_local depth counter + `enter_delegate()` + `Drop` impl) and call
it once at the entry of the delegate path in
`crates/tsz-checker/src/state/type_analysis/cross_file.rs:644`.

Disabled-path overhead unchanged (each branch returns immediately when
`enabled_fast()` is false).

## Files Touched

- `crates/tsz-common/src/perf_counters.rs` (+30 LOC: helper)
- `crates/tsz-checker/src/state/type_analysis/cross_file.rs` (+4 LOC)

## Verification

- `cargo check -p tsz-common -p tsz-checker` (clean)
- Pre-commit will run affected-crate tests.
