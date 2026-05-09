# perf(checker): wire compute_type_of_symbol_cache_hits counter

- **Date**: 2026-05-09
- **Branch**: `perf/wire-compute-type-of-symbol-cache-hits`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T1.1 (instrumentation — site #12)

## Intent

Wire site #12 from PERFORMANCE_PLAN.md §4.1.1: bump
`compute_type_of_symbol_cache_hits` on the two cache-hit return paths
in `get_type_of_symbol_inner`. The total `compute_type_of_symbol_calls`
counter is already wired (line 380); the hit ratio is what's missing
from `dump_string()`.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/core.rs` (+8 LOC)

## Verification

- `cargo check -p tsz-checker` (clean)
- Pre-commit will run affected-crate tests.
