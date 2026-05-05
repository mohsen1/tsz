# fix(checker): align object element-access diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-object-element-access-diagnostics`
- **PR**: https://github.com/mohsen1/tsz/pull/3087
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-05

## Intent

Fix the validated random conformance pick
`TypeScript/tests/cases/compiler/objectCreationOfElementAccessExpression.ts`.
`tsz` reported the expected `TS2348`, `TS2538`, and `TS2564` codes, but the
diagnostic fingerprints diverged from `tsc`: annotated variable initializers
lost the inner `TS2348` non-callable constructor diagnostic and `TS2538` invalid
index-type diagnostic during the pre-contextual diagnostic reset.

## Files Touched

- `docs/plan/claims/fix-checker-object-element-access-diagnostics.md`
- `crates/tsz-checker/Cargo.toml`
- `crates/tsz-checker/src/state/variable_checking/core.rs`
- `crates/tsz-checker/tests/object_element_access_diagnostics_tests.rs`

## Verification

- `cargo fmt --all -- --check`
- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --test object_element_access_diagnostics_tests annotated_element_access_initializer_preserves_inner_call_and_index_errors`
- `./scripts/conformance/conformance.sh run --filter "objectCreationOfElementAccessExpression" --verbose`
- `CARGO_TARGET_DIR=.target/nextest-local cargo check --package tsz-checker`
- `./scripts/conformance/conformance.sh run --max 200`
