# fix(checker): suppress full-name TS2304 for known JSDoc generics

- **Date**: 2026-05-06
- **Branch**: `fix/jsdoc-generic-param-full-name-ts2304`
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the live conformance false positive in
`contravariantOnlyInferenceFromAnnotatedFunctionJs.ts`, where `tsz` reported
`TS2304 Cannot find name 'Funcs<A, B>'` for a JSDoc generic typedef reference
whose base typedef `Funcs` is known and whose type arguments are in scope.

## Files Touched

- `crates/tsz-checker/src/jsdoc/diagnostics.rs`
- `crates/tsz-checker/tests/jsdoc_reference_kernel_tests.rs`
- `crates/tsz-cli/src/bin/tsz_server/handlers_structure.rs` (clippy-only cleanup required by the pre-commit parity lint)

## Verification

- `cargo fmt --all --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/next-slice CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUSTFLAGS='-Cdebuginfo=0' cargo test -p tsz-checker --lib template_after_typedef_binds_as_generic_params -- --nocapture`
  - `1 test passed`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/next-slice CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUSTFLAGS='-Cdebuginfo=0' cargo test -p tsz-checker --test jsdoc_type_expression_tests jsdoc_nongeneric_instantiation_reports_ts2315_and_ts2304 -- --nocapture`
  - `1 test passed`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/next-slice CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUSTFLAGS='-Cdebuginfo=0' cargo build -p tsz-cli -p tsz-conformance`
- `/Users/mohsen/code/tsz-build-targets/next-slice/debug/tsz-conformance --test-dir /Users/mohsen/code/tsz-worktrees/origin-main-20260505-7/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary /Users/mohsen/code/tsz-build-targets/next-slice/debug/tsz --workers 1 --print-test --print-fingerprints --verbose --filter contravariantOnlyInferenceFromAnnotatedFunctionJs`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
