# fix(checker): widen Object.seal mutable object literals

- **Date**: 2026-05-14
- **Branch**: `codex/issue-6863-object-seal-widen-20260514`
- **PR**: #6868
- **Status**: ready
- **Workstream**: conformance / false-positive fixes

## Intent

Add regression coverage for #6863. Current `origin/main` already widens mutable `Object.seal` object literal property values correctly, and this PR locks that behavior while preserving `Object.freeze` readonly/literal behavior.

## Files Touched

- `crates/tsz-checker/tests/ts2322_tests.rs` (regression for `Object.seal` mutability/widening and `Object.freeze` readonly behavior)

## Verification

- `cargo nextest run -p tsz-checker -E 'test(object_seal_widens_mutable_literal_property_values)' --no-fail-fast` (2 tests pass)
