# perf(checker): avoid cloning static block statements during access scan

- **Date**: 2026-05-05
- **Branch**: `perf/static-block-access-no-statement-clone`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Avoid cloning a static block's statement vector while collecting `this.X` and
`ClassName.X` accesses for use-before-initialization checks. The scan only needs
to visit each statement once, so it can copy one `NodeIndex` at a time after
ending the immutable arena borrow.

## Planned Scope

- `crates/tsz-checker/src/state/state_checking_members/property_init.rs`
- `docs/plan/claims/perf-static-block-access-no-statement-clone.md`

## Verification Plan

- `cargo test -p tsz-checker --lib property_init`
- `scripts/bench/perf-hotspots.sh --quick`
- `cargo fmt --check`
- `git diff --check`
