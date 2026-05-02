# perf(solver): cache contextual call-evaluator instantiation

- **Date**: 2026-05-02
- **Branch**: `perf/call-evaluator-contextual-instantiation-cache`
- **PR**: #2204
- **Status**: ready
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Route contextual call-signature application instantiation through the solver's
cache-aware instantiation entry point. The contextual signature visitor already
has a `QueryDatabase`, so repeated parameter, return, `this`, and type-predicate
substitutions can reuse `QueryCache` instead of bypassing it.

## Planned Scope

- `crates/tsz-solver/src/operations/core/call_evaluator.rs`
- `crates/tsz-solver/src/operations/core/call_resolution.rs`
- `crates/tsz-solver/src/operations/{call_args.rs,generic_call/inference_helpers.rs,generic_call/return_context.rs}`
- `crates/tsz-solver/src/{lib.rs,operations/mod.rs}`
- `crates/tsz-checker/src/query_boundaries/checkers/call.rs`
- `crates/tsz-checker/src/tests/architecture_contract_tests.rs`
- `docs/plan/claims/perf-call-evaluator-contextual-instantiation-cache.md`

## Verification Plan

- `cargo fmt --check`
- `cargo check -p tsz-solver`
- `cargo test -p tsz-solver instantiation_cache`
- `scripts/bench/perf-hotspots.sh --quick`
- Guarded large-repo RSS sample if the change reaches the large-repo path.

## Verification

- `cargo fmt --check` (pass)
- `cargo check -p tsz-solver` (pass)
- `cargo check -p tsz-checker` (pass)
- `cargo test -p tsz-solver instantiation_cache` (16 passed)
- `cargo test -p tsz-checker --lib architecture_contract_tests_src::test_assignment_and_binding_default_assignability_use_central_gateway_helpers -- --nocapture`
  (pass)
- `scripts/bench/perf-hotspots.sh --quick`
  (`artifacts/perf/hotspots-20260501-183345.json`): tsz beat tsgo on all five
  quick fixtures: 100 classes 2.21x, 50 generic functions 1.38x,
  DeepPartial optional-chain N=50 1.34x, Shallow optional-chain N=50 1.40x,
  Constraint conflicts N=30 1.69x.
- Guarded large-repo sample:
  `RUST_BACKTRACE=1 scripts/safe-run.sh --limit 75% --interval 2 --verbose -- .target-bench/dist/tsz --extendedDiagnostics --noEmit -p ~/code/large-ts-repo/tsconfig.flat.bench.json`;
  manually stopped after a stable sample window, exit 137, peak sampled
  physical footprint 11395 MB / 12288 MB guard.
