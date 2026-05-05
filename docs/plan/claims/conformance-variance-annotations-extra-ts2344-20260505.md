# [WIP] fix(conformance): suppress extra TS2344 in variance annotations

- **Date**: 2026-05-05
- **Branch**: `conformance/variance-annotations-extra-ts2344-20260505`
- **PR**: #3274
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

The random conformance picker selected
`TypeScript/tests/cases/conformance/types/typeParameters/typeParameterLists/varianceAnnotations.ts`.
`tsc` reports the expected syntax, variance, and assignability diagnostics, but
`tsz` emits one extra TS2344. This PR will identify which variance-related
constraint check is too eager and route the fix through the owning checker or
solver boundary instead of filtering the diagnostic by test name.

## Files Touched

- `crates/tsz-solver/src/instantiation/application.rs`
- `crates/tsz-checker/src/checkers/generic_checker/instantiation_expression_constraints.rs`
- `crates/tsz-checker/tests/ts2344_class_constructor_constraint.rs`

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=.target-3274 CARGO_BUILD_JOBS=1 cargo check --package tsz-solver`
- `CARGO_TARGET_DIR=.target-3274 CARGO_BUILD_JOBS=1 cargo check --package tsz-checker`
- `CARGO_TARGET_DIR=.target-3274 CARGO_BUILD_JOBS=1 cargo test --package tsz-checker --test ts2344_class_constructor_constraint`
- `CARGO_TARGET_DIR=.target-3274 CARGO_BUILD_JOBS=1 cargo build -p tsz-cli --bin tsz`
- `.target-3274/debug/tsz --target es2015 --strict --declaration --pretty false TypeScript/tests/cases/conformance/types/typeParameters/typeParameterLists/varianceAnnotations.ts`
  - Verified no `TS2344` is emitted for the file.

`cargo nextest run --package tsz-checker test_instance_type_of_generic_class_expression_type_query_no_ts2344`
and the dist-fast CLI build were attempted first but were killed or raced with
target-directory cleanup before completion; the single integration-test target
and dev CLI build completed successfully.
