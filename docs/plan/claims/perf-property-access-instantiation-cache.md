Status: claim

# perf(solver): cache property-access instantiation calls

- **Date**: 2026-05-02
- **Branch**: `perf/property-access-instantiation-cache`
- **PR**: TBD
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Route property-access application/mapped-type instantiation and `this`
substitution through the solver's cache-aware entry points. The
`PropertyAccessEvaluator` already carries a `QueryDatabase`, so these hot
property paths can reuse `QueryCache` instead of bypassing it.

## Planned Scope

- `crates/tsz-solver/src/operations/property_helpers.rs`
- `docs/plan/claims/perf-property-access-instantiation-cache.md`

## Verification Plan

- `cargo fmt --check`
- `cargo check -p tsz-solver`
- `cargo test -p tsz-solver instantiation_cache`
- `scripts/bench/perf-hotspots.sh --quick`
- Guarded large-repo RSS sample if the change reaches the large-repo path.
