# perf(checker): avoid cloning JS class body statements for summary scan

- **Date**: 2026-05-05
- **Branch**: `perf/js-class-summary-no-statement-clone`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Avoid cloning a JS class body block's statement vector while recording
assignment-derived member kinds. Alias collection can borrow the statement
slice, and the member scan can copy one `NodeIndex` at a time after ending the
immutable arena borrow.

## Planned Scope

- `crates/tsz-checker/src/classes/class_summary.rs`
- `docs/plan/claims/perf-js-class-summary-no-statement-clone.md`

## Verification Plan

- `cargo test -p tsz-checker --lib js_constructor_property`
- `scripts/bench/perf-hotspots.sh --quick`
- `cargo fmt --check`
- `git diff --check`
