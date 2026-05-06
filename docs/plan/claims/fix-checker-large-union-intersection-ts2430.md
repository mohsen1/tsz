# fix(checker): align large-union intersection diagnostics

- **Date**: 2026-05-05 22:11:01 UTC
- **Branch**: `fix/checker-large-union-intersection-ts2430`
- **PR**: #3390
- **Status**: implemented
- **Workstream**: 1 (Conformance - diagnostic pass-rate fix)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/compiler/intersectionsOfLargeUnions2.ts`. On current
`origin/main`, the focused runner still fails: `tsc` reports `TS2300`,
`TS2430`, and `TS2536`, while `tsz` reports `TS2300`, `TS2536`, and an extra
`TS2677`.

This PR fixes the root cause behind the missing lib inheritance diagnostic and
the extra type-predicate assignability diagnostic in the owning solver/checker
boundary layer, and adds focused Rust regression coverage.

## Files Touched

- `crates/tsz-checker/src/checkers/generic_checker/constraint_validation.rs`
- `crates/tsz-checker/src/classes/class_checker_compat.rs`
- `crates/tsz-cli/src/driver/check.rs`
- `crates/tsz-solver/src/relations/subtype/helpers.rs`
- `crates/tsz-checker/tests/large_union_index_merge_regression_tests.rs`
- `crates/tsz-checker/Cargo.toml`

## Verification

- `git diff --check`
- `rustfmt --edition 2024 --check crates/tsz-checker/src/checkers/generic_checker/constraint_validation.rs crates/tsz-checker/src/classes/class_checker_compat.rs crates/tsz-cli/src/driver/check.rs crates/tsz-solver/src/relations/subtype/helpers.rs crates/tsz-checker/tests/large_union_index_merge_regression_tests.rs`
- `cargo test -p tsz-checker --test large_union_index_merge_regression_tests -- --nocapture`
