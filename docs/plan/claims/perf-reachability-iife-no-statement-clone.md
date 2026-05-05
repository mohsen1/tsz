# perf(checker): avoid cloning IIFE block statements in reachability

- **Date**: 2026-05-05
- **Branch**: `perf/reachability-iife-no-statement-clone`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Avoid cloning a function-body block's statement vector while checking whether a
terminating IIFE anchors an unreachable statement. The reachability check only
needs to scan for the first statement that always throws, so it can iterate over
the existing statement list directly.

## Planned Scope

- `crates/tsz-checker/src/flow/reachability_checker.rs`
- `docs/plan/claims/perf-reachability-iife-no-statement-clone.md`

## Verification

- `cargo test -p tsz-checker --test control_flow_type_guard_tests typeof_primitive_checks_narrow_explicit_any_only_in_true_branch -- --nocapture`
  (pass: 1 passed)
- `cargo test -p tsz-checker --lib`
  (pass: 3377 passed, 10 ignored)
- `scripts/bench/perf-hotspots.sh --quick`
  (pass, artifact: `artifacts/perf/hotspots-20260505-041947.json`; tsz beat
  tsgo on all five quick fixtures: 100 classes 1.96x, Constraint conflicts
  N=30 1.58x, DeepPartial optional-chain N=50 1.45x, 50 generic functions
  1.12x, Shallow optional-chain N=50 1.09x)
- `cargo fmt --check` (pass)
- `git diff --check` (pass)
