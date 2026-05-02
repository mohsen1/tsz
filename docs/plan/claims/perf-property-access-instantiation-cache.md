Status: ready

# perf(solver): cache property-access instantiation calls

- **Date**: 2026-05-02
- **Branch**: `perf/property-access-instantiation-cache`
- **PR**: #2199
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

## Verification

- `cargo fmt --check` (pass)
- `cargo check -p tsz-solver` (pass)
- `cargo test -p tsz-solver instantiation_cache` (16 passed)
- `scripts/bench/perf-hotspots.sh --quick`
  (`artifacts/perf/hotspots-20260501-175326.json`): tsz beat tsgo on all five
  quick fixtures: 100 classes 3.07x, 50 generic functions 1.41x,
  DeepPartial optional-chain N=50 1.39x, Shallow optional-chain N=50 1.40x,
  Constraint conflicts N=30 1.72x.
- Guarded large-repo sample:
  `RUST_BACKTRACE=1 scripts/safe-run.sh --limit 75% --interval 2 --verbose -- .target-bench/dist/tsz --extendedDiagnostics --noEmit -p ~/code/large-ts-repo/tsconfig.flat.bench.json`;
  manually stopped after a stable sample window, exit 143, peak sampled
  physical footprint 11098 MB / 12288 MB guard.
