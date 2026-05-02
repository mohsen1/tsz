# perf(solver): cache contextual call-evaluator instantiation

- **Date**: 2026-05-02
- **Branch**: `perf/call-evaluator-contextual-instantiation-cache`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Route contextual call-signature application instantiation through the solver's
cache-aware instantiation entry point. The contextual signature visitor already
has a `QueryDatabase`, so repeated parameter, return, `this`, and type-predicate
substitutions can reuse `QueryCache` instead of bypassing it.

## Planned Scope

- `crates/tsz-solver/src/operations/core/call_evaluator.rs`
- `docs/plan/claims/perf-call-evaluator-contextual-instantiation-cache.md`

## Verification Plan

- `cargo fmt --check`
- `cargo check -p tsz-solver`
- `cargo test -p tsz-solver instantiation_cache`
- `scripts/bench/perf-hotspots.sh --quick`
- Guarded large-repo RSS sample if the change reaches the large-repo path.
