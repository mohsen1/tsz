# perf(checker): avoid cloning static block statements during access scan

- **Date**: 2026-05-05
- **Branch**: `perf/static-block-access-no-statement-clone`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Avoid cloning a static block's statement vector while collecting `this.X` and
`ClassName.X` accesses for use-before-initialization checks. The scan only needs
to visit each statement once, so it can copy one `NodeIndex` at a time after
ending the immutable arena borrow.

## Planned Scope

- `crates/tsz-checker/src/state/state_checking_members/property_init.rs`
- `docs/plan/claims/perf-static-block-access-no-statement-clone.md`

## Verification

- `cargo test -p tsz-checker --lib property_init`
  (pass: 12 passed)
- `scripts/bench/perf-hotspots.sh --quick`
  (pass after rebasing onto current `origin/main`, artifact:
  `artifacts/perf/hotspots-20260505-062725.json`; tsz beat tsgo on all five
  quick fixtures: 100 classes 2.07x, Constraint conflicts N=30 1.72x,
  50 generic functions 1.27x, DeepPartial optional-chain N=50 1.23x, Shallow
  optional-chain N=50 1.22x)
- `cargo fmt --check` (pass)
- `git diff --check` (pass)
