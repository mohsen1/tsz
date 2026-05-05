# [WIP] fix(checker): suppress extra recursive conditional diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/recursive-conditional-extra-diagnostics`
- **PR**: #3399
- **Status**: implemented
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

The canonical picker selected
`TypeScript/tests/cases/compiler/recursiveConditionalTypes.ts`, currently an
only-extra conformance failure: `tsz` emits the expected `TS2322`, `TS2345`,
and `TS2589` diagnostics, but also emits extra `TS2339` and `TS2344`
diagnostics. This PR will identify the root cause in recursive conditional
evaluation, constraint handling, or diagnostic recovery, fix it in the owning
layer, and add a focused Rust regression test for the invariant.

## Files Touched

- `crates/tsz-checker/src/types/type_node_advanced.rs`
- `crates/tsz-checker/src/checkers/generic_checker/constraint_validation.rs`
- `crates/tsz-checker/src/checkers/generic_checker/infer_conditional_constraints.rs`
- `crates/tsz-checker/tests/conditional_infer_tests.rs`

## Verification

- Passed: `CARGO_TARGET_DIR=target-3399 CARGO_BUILD_JOBS=1 cargo nextest run -p tsz-checker --test conditional_infer_tests recursive_conditional_index_access_does_not_report_property_missing nested_tuple_rest_infer_result_satisfies_array_constraint`
- Passed: `cargo check -j 1 --target-dir /var/tmp/tsz-check-3399 -p tsz-checker`
- Blocked: target conformance runner build was interrupted by local build-output cleanup/SIGTERM before a stable `tsz` binary could be produced for `recursiveConditionalTypes`; PR remains draft WIP until the target conformance check is rerun successfully.
- Additional blocked attempt: `cargo build -j 1 --target-dir /var/tmp/tsz-target-3399-one --profile dist-fast -p tsz-cli --bin tsz` exited without a Rust diagnostic while compiling `tsz-binder`, and `/var/tmp/tsz-target-3399-one` was removed before a `tsz` binary was produced.
- Additional blocked attempt: `cargo build -j 1 --target-dir /var/tmp/tsz-check-3399 -p tsz-cli --bin tsz` exited without a Rust diagnostic after reaching `tsz-solver`/`pest_meta`, and `/var/tmp/tsz-check-3399` was removed before a `tsz` binary was produced.
