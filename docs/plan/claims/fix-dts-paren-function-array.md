# [WIP] fix(emitter): preserve class returns in inferred function arrays

- **Date**: 2026-05-02
- **Branch**: `fix/dts-paren-function-array`
- **PR**: #2214
- **Status**: ready
- **Workstream**: 2 (declaration emit pass rate)

## Intent

Fix the declaration emit mismatch in `declFileTypeAnnotationParenType` where
an inferred array of arrow functions returning a private class is emitted as
`(() => any)[]` instead of preserving the nameable local class return type.
The slice should stay in AST/type inference and avoid broad printed-string
post-processing.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/helpers/type_inference.rs`
- `crates/tsz-emitter/src/declaration_emitter/tests/type_formatting.rs`
- `crates/tsz-cli/src/bin/tsz.rs`
- `crates/tsz-cli/tests/tsc_compat_tests.rs`

## Verification

- Focused emit repro for `declFileTypeAnnotationParenType`.
- `cargo fmt --check`
- `cargo nextest run -p tsz-emitter declaration_emitter::tests::type_formatting::test_inferred_function_array_preserves_new_expression_return_type`
- `cargo nextest run -p tsz-emitter` (1712 passed, 5 skipped)
- `TSZ_BIN=/tmp/tsz-tail-failures/.target/release/tsz scripts/safe-run.sh ./scripts/emit/run.sh --dts-only --filter=declFileTypeAnnotationParenType --verbose --json-out=/tmp/tsz-tail-failures/.tmp-decl-paren-type-final.json`
- `cargo nextest run -p tsz-cli batch_mode_uses_project_cwd_for_jsdoc_required_constructor_types`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter typeFromParamTagForFunction --verbose`
