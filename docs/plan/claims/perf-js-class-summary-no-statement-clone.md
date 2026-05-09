# perf(checker): avoid cloning JS class body statements for summary scan

- **Date**: 2026-05-05
- **Branch**: `perf/js-class-summary-no-statement-clone`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Avoid cloning a JS class body block's statement vector while recording
assignment-derived member kinds. Alias collection can borrow the statement
slice, and the member scan can copy one `NodeIndex` at a time after ending the
immutable arena borrow.

## Planned Scope

- `crates/tsz-checker/src/classes/class_summary.rs`
- `docs/plan/claims/perf-js-class-summary-no-statement-clone.md`

## Verification

- `cargo test -p tsz-checker --lib js_constructor_property`
  (pass: 70 passed)
- `scripts/bench/perf-hotspots.sh --quick`
  (pass, artifact: `artifacts/perf/hotspots-20260505-064330.json`; tsz beat
  tsgo on all five quick fixtures: 100 classes 2.14x, Constraint conflicts
  N=30 1.57x, 50 generic functions 1.26x, Shallow optional-chain N=50 1.23x,
  DeepPartial optional-chain N=50 1.21x)
- `cargo fmt --check` (pass)
- `git diff --check` (pass)
