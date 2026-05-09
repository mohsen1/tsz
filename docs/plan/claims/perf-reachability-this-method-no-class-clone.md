Status: ready

# perf(checker): avoid cloning enclosing class info in reachability checks

- **Date**: 2026-05-05
- **Branch**: `perf/reachability-this-method-no-class-clone`
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Remove an avoidable `EnclosingClassInfo` clone from reachability analysis for
`this.method()` calls. The current path clones the whole enclosing-class record,
including member vectors and cached metadata, only so it can scan member nodes
before calling the mutable never-return checker.

## Planned Scope

- `crates/tsz-checker/src/flow/reachability_checker.rs`
- Focused checker test or existing targeted test for never-return reachability,
  depending on local coverage.

## Verification Plan

- `cargo fmt --check`
- Focused checker test covering `this.method()` never-return reachability
- `scripts/bench/perf-hotspots.sh --quick` before/after comparison

## Verification

- Baseline quick benchmark:
  `scripts/bench/perf-hotspots.sh --quick`
  (`artifacts/perf/hotspots-20260505-010757.json`): tsz beat tsgo on all five
  quick fixtures; factors ranged from 1.20x to 2.16x.
- Focused regression:
  `cargo test -p tsz-checker this_method_returning_never_marks_following_code_unreachable -- --nocapture`
- Broader existing reachability filter:
  `cargo test -p tsz-checker unreachable -- --nocapture`
- `cargo fmt --check`
- `git diff --check`
- Post-change quick benchmark:
  `scripts/bench/perf-hotspots.sh --quick`
  (`artifacts/perf/hotspots-20260505-011931.json`): tsz beat tsgo on all five
  quick fixtures; factors ranged from 1.12x to 2.30x. Absolute timings were
  noisier/slower than the baseline across both tsz and tsgo, so this slice
  claims allocation removal rather than a measurable benchmark win.
