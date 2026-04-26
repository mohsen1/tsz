# fix(checker): preserve display name for JSDoc generic interface refs

- **Date**: 2026-04-26
- **Branch**: `fix/jsdoc-generic-type-display-name`
- **PR**: TBD
- **Status**: claim
- **Workstream**: Conformance — fingerprint parity

## Intent

When a JSDoc `@type {ClassComponent<any>}` annotation resolves to a generic
interface (or class/type-alias) reference, the JSDoc resolver instantiates
the interface body but never registers a display def for the instantiated
type. As a result, diagnostics format the type as `'ClassComponent'` instead
of `'ClassComponent<any>'`. The typedef path already does this — extend the
same registration to the non-typedef path so `Name<Args>` survives display.

This fixes the fingerprint-only failure in
`subclassThisTypeAssignable01.ts` on the `file1.js` line.

## Files Touched

- `crates/tsz-checker/src/jsdoc/resolution/type_construction.rs` (~30 LOC)
- `crates/tsz-checker/src/tests/dispatch_tests.rs` (regression test)

## Verification

- `cargo nextest run -p tsz-checker --lib` (passes)
- `./scripts/conformance/conformance.sh run --filter "subclassThisTypeAssignable01" --verbose` (fingerprint-mismatch reduced)
