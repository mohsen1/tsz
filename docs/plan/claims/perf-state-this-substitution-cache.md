# perf(checker): cache state this-type substitution boundary

- **Date**: 2026-05-01
- **Branch**: `perf/state-this-substitution-cache`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Route the state type-environment query-boundary `substitute_this_type` helper
through the solver's cache-aware entry point. The current caller already holds
a `QueryDatabase`, so this avoids bypassing the existing instantiation cache
for repeated flow/control-flow `this` substitutions.

## Files Touched

- `crates/tsz-checker/src/query_boundaries/state/type_environment.rs` (~small signature/body update)
- `docs/plan/claims/perf-state-this-substitution-cache.md`

## Verification

- `cargo fmt --check` (pass)
- `cargo check -p tsz-checker` (pass)
- `cargo test -p tsz-solver instantiation_cache` (15 passed)
- `cargo test -p tsz-checker --lib architecture_contract_tests_src::test_checker_file_size_ceiling` (pass)
- `scripts/bench/perf-hotspots.sh --quick` blocked: `hyperfine not found` in this environment.
