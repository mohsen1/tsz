# fix(checker): use 'typeof X' source display for JSDoc @constructor identifier args

- **Date**: 2026-04-29
- **Branch**: `fix/checker-jsdoc-constructor-typeof-source-display`
- **PR**: #1786
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance and Fingerprints)

## Intent

`conformance/jsdoc/jsdocFunctionType.ts` is fingerprint-only: tsc renders
the TS2345 source type as `typeof E` for an identifier referring to a
JS-constructor-evidenced (`@constructor`-tagged) function/variable, while
tsz expands the constructor signature
(`new (n: number) => { not_length_on_purpose: number; }`). Add a
source-display branch so that when a call argument is an identifier whose
symbol carries JS-constructor evidence, the diagnostic prints
`typeof <name>` (mirroring tsc's behavior for class-like symbols).

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/display_formatting.rs`
  (add the `typeof X` short-circuit + a regression unit test in the same
  crate's tests)

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-checker`
- `cargo test --package tsz-checker --lib jsdoc_constructor_identifier_argument_uses_typeof_source_display -- --nocapture`
- `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter jsdocFunctionType --verbose` (2/2 passed)
