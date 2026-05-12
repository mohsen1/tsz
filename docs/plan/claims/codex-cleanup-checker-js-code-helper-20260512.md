# [WIP] chore(checker-tests): share JS diagnostic code helper

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-checker-js-code-helper-20260512`
- **PR**: TBD
- **Status**: claim
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

- Pending
