# fix(checker): use 'typeof X' source display for JSDoc @constructor identifier args

- **Date**: 2026-04-29
- **Branch**: `fix/checker-jsdoc-constructor-typeof-source-display`
- **PR**: TBD
- **Status**: claim
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

- `cargo nextest run -p tsz-checker --lib` for the new regression test
- `./scripts/conformance/conformance.sh run --filter "jsdocFunctionType" --verbose`
- Quick regression: `./scripts/conformance/conformance.sh run --max 200`
