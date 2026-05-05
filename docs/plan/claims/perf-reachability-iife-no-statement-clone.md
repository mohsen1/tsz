# perf(checker): avoid cloning IIFE block statements in reachability

- **Date**: 2026-05-05
- **Branch**: `perf/reachability-iife-no-statement-clone`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Avoid cloning a function-body block's statement vector while checking whether a
terminating IIFE anchors an unreachable statement. The reachability check only
needs to scan for the first statement that always throws, so it can iterate over
the existing statement list directly.

## Planned Scope

- `crates/tsz-checker/src/flow/reachability_checker.rs`
- `docs/plan/claims/perf-reachability-iife-no-statement-clone.md`

## Verification Plan

- `cargo test -p tsz-checker --test control_flow_type_guard_tests typeof_primitive_checks_narrow_explicit_any_only_in_true_branch -- --nocapture`
- `cargo test -p tsz-checker --lib`
- `scripts/bench/perf-hotspots.sh --quick`
- `cargo fmt --check`
- `git diff --check`
