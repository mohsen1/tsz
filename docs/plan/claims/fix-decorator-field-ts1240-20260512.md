# fix(checker): emit TS1240 for ES field decorator value mismatches

- **Date**: 2026-05-12
- **Branch**: `fix/decorator-field-ts1240-20260512`
- **PR**: #5816
- **Status**: ready
- **Workstream**: conformance diagnostics

## Intent

Fix issue #5798 by validating ES field decorator call signatures with the runtime `undefined` value argument. Decorators that require the field value itself now fail signature resolution with TS1240, while decorators that accept `undefined` still pass.

## Files Touched

- `crates/tsz-checker/src/state/state_checking_members/member_declaration_checks.rs` - invoke ES property decorator signature validation.
- `crates/tsz-checker/src/state/state_checking_members/decorator_signature_checks.rs` - add decorator signature helpers.
- `crates/tsz-checker/src/state/state_checking_members/mod.rs` - register the helper module.
- `crates/tsz-checker/tests/ts1240_tests.rs` - cover the missing diagnostic and a compatible field decorator.
- `crates/tsz-checker/Cargo.toml` - register the explicit integration test target.

## Verification

- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --test ts1240_tests`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --test ts1238_tests --test ts1238_generic_decorator_tests`
- `cargo fmt --all --check`
- `git diff --check`
- Pre-commit direct suite is blocked by inherited latest-main failure:
  `js_constructor_property_tests::checked_js_prototype_plain_parent_method_call_reports_ts2531`
  also fails on clean `origin/main` at `a7811bab83`.
