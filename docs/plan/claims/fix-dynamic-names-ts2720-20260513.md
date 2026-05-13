# fix(checker): suppress false TS2720 for dynamic-name class implements

- **Date**: 2026-05-13
- **Branch**: `fix-dynamic-names-ts2720-20260513`
- **PR**: #6123
- **Status**: ready
- **Workstream**: conformance

## Intent

Restore the `dynamicNames` conformance case on current `main`, which currently
emits an extra TS2720 for a class implementing another class through computed
property names. The fix should preserve real class-implements-class diagnostics
while avoiding a false-positive suggestion when the apparent mismatch is driven
by computed member identity.

## Files Touched

- `crates/tsz-checker/Cargo.toml`
- `crates/tsz-checker/src/classes/class_implements_checker/core.rs`
- `crates/tsz-checker/tests/dynamic_names_ts2720_tests.rs`

## Verification

- `scripts/conformance/conformance.sh run --filter dynamicNames --workers 1`
  (`3/3 passed`)
- `cargo test -p tsz-checker --test dynamic_names_ts2720_tests -- --nocapture`
  (`1 passed`)
- `cargo test -p tsz-checker --test conformance_issues test_class_extends_and_implements_same_generic_class_emits_ts2720 -- --nocapture`
  (`1 passed`)
- `cargo fmt --all -- --check`
- `git diff --check`
