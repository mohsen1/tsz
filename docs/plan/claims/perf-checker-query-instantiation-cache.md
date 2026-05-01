# perf(checker): route query-boundary instantiation through QueryCache

- **Date**: 2026-05-01
- **Branch**: `perf/checker-query-instantiation-cache`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Route the checker's shared query-boundary instantiation helpers through the
cache-aware solver entry points when callers already provide a `QueryDatabase`.
This keeps the existing cache leaf fast paths intact while allowing repeated
checker-side type substitutions to hit `QueryCache` in the large-repo path.

## Files Touched

- `crates/tsz-checker/src/query_boundaries/common.rs` (~small signature/body updates)
- Targeted checker call sites if needed for type alignment.

## Verification

- `cargo fmt --check` (pass)
- `cargo check -p tsz-checker` (pass)
- `cargo test -p tsz-checker --lib architecture_contract_tests_src::test_checker_file_size_ceiling` (pass)
- `cargo test -p tsz-solver instantiation_cache` (15 passed)
- `cargo test -p tsz-checker --lib` (3102 passed, 1 failed before line-count trim: `architecture_contract_tests_src::test_checker_file_size_ceiling`; rerun of the failed guard passed after trimming `common.rs` back to 1999 lines)
- `scripts/bench/perf-hotspots.sh --quick` blocked: `hyperfine not found` in this environment.
