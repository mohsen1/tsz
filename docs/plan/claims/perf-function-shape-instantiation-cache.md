# perf(checker): cache function-shape instantiation boundary

- **Date**: 2026-05-01
- **Branch**: `perf/function-shape-instantiation-cache`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Route checker query-boundary `FunctionShape` instantiation through the solver's
cache-aware `instantiate_type_cached` entry point. The only checker caller
already provides a `QueryDatabase`, so repeated parameter/return/this-type
substitutions can reuse the existing instantiation cache.

## Files Touched

- `crates/tsz-checker/src/query_boundaries/common.rs` (~small signature/body update)
- `docs/plan/claims/perf-function-shape-instantiation-cache.md`

## Verification

- `cargo fmt --check` (pass)
- `cargo check -p tsz-checker` (pass)
- `cargo test -p tsz-solver instantiation_cache` (15 passed)
- `cargo test -p tsz-checker --lib architecture_contract_tests_src::test_checker_file_size_ceiling` (pass)
- `scripts/bench/perf-hotspots.sh --quick` blocked: `hyperfine not found` in this environment.
