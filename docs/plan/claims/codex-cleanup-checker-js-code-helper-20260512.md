# [WIP] chore(checker-tests): share JS diagnostic code helper

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-checker-js-code-helper-20260512`
- **PR**: #5934
- **Status**: ready
- **Workstream**: DRY checker test helpers

## Intent

Consolidate the remaining JS checker-test wrappers that only run the shared
parse/bind/check pipeline and project diagnostics down to codes. This keeps JS
test setup behavior in `test_utils` and trims local boilerplate from focused
diagnostic regression tests.

## Files Touched

- `crates/tsz-checker/src/test_utils.rs`
- `crates/tsz-checker/tests/ts1210_class_arguments_tests.rs`
- `crates/tsz-checker/tests/ts7006_broad_jsdoc_type_cast.rs`
- `docs/plan/claims/codex-cleanup-checker-js-code-helper-20260512.md`

## Verification

- `cargo fmt --check`
- `cargo clippy -p tsz-checker --lib -- -D warnings`
- `cargo nextest run -p tsz-checker --test ts1210_class_arguments_tests --no-fail-fast` (3 tests pass)
- `cargo nextest run -p tsz-checker --lib -E 'test(jsdoc_type_cast_star_does_not_suppress_ts7006_for_nested_closure) | test(jsdoc_type_cast_any_does_not_suppress_ts7006_for_nested_closure) | test(jsdoc_type_cast_capital_object_does_not_suppress_ts7006_for_nested_closure) | test(jsdoc_type_cast_function_does_not_suppress_ts7006_for_nested_closure) | test(jsdoc_type_cast_specific_signature_still_suppresses_ts7006)' --no-fail-fast` (5 tests pass)
