# fix(checker): preserve module.exports JS extension in namespace diagnostics

- **Date**: 2026-04-28
- **Branch**: `fix/checker-module-exports-display-js-extension`
- **PR**: #1629
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the fingerprint mismatch for `lateBoundAssignmentDeclarationSupport2.ts`: current-file `module.exports[...]` diagnostics should display `typeof import("<file>.js")`, while required namespace reads continue using the extensionless imported-module display. The root cause was that `module.exports` used the same extension-stripping display path as `exports`, and the property-receiver formatter stripped any namespace display name again at diagnostic time.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/computed_commonjs/exports_detection.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
- `crates/tsz-checker/tests/conformance_issues/features/namespace_construct_signature.rs`

## Verification

- `cargo test -p tsz-checker --test conformance_issues test_js_late_bound -- --nocapture` (3 tests pass)
- `./scripts/conformance/conformance.sh run --filter "lateBoundAssignmentDeclarationSupport2" --verbose` (1/1 pass)
- `cargo fmt --check`
- `cargo clippy -p tsz-checker --test conformance_issues -- -D warnings`
