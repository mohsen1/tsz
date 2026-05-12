# fix(audit): scope unresolved computed recursive-alias TS2589 checks to conditional aliases

- **Date**: 2026-05-12
- **Branch**: `codex/review-audit-batch5-20260512`
- **PR**: #5889
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close a missed important review note from PR #4977: `conditional_body_has_unresolved_computed_recursive_alias_ref(...)` was invoked even when the alias body was not a conditional type. This could route non-conditional aliases through conditional-definition TS2589 detection and produce the wrong error surface.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/type_alias_checking.rs`
- `crates/tsz-checker/tests/ts2589_tests.rs`

## Verification

- `cargo test -p tsz-checker --lib ts2589_tests::non_conditional_recursive_alias_with_unresolved_qualified_arg_no_ts2589 -- --exact --nocapture`
- `cargo test -p tsz-checker --lib ts2589_tests::recursive_conditional_type_alias_definition_emits_ts2589 -- --exact --nocapture`
- `cargo check -p tsz-checker`
