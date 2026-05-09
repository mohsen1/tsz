# perf(cli): wire resolver_candidate_paths_total counter

- **Date**: 2026-05-09
- **Branch**: `perf/wire-resolver-candidate-paths-total`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T1.1 (instrumentation — site #15)

## Intent

Wire site #15 from PERFORMANCE_PLAN.md §4.1.1: bump
`resolver_candidate_paths_total` at the two `candidates.push(...)`
sites in `crates/tsz-cli/src/driver/resolution.rs` (lines 1758, 1912)
and replace the matching `n/a` row in `dump_string()`.

## Files Touched

- `crates/tsz-cli/src/driver/resolution.rs` (+6 LOC)
- `crates/tsz-common/src/perf_counters.rs` (-1/+2 LOC)

## Verification

- `cargo check -p tsz-common -p tsz-cli` (clean)
- Pre-commit will run affected-crate tests.
