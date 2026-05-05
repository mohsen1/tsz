# fix(checker): accept JS expando symbol property returns

- **Date**: 2026-05-05
- **Branch**: `fix/checker-js-expando-symbol-property-return`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the false-positive conformance failure in
`TypeScript/tests/cases/compiler/expandoFunctionSymbolPropertyJs.ts`. TypeScript
accepts returning a JS function whose computed `Symbol()` expando property
satisfies a callable interface with a readonly computed symbol member, but tsz
currently emits extra `TS2322` and `TS2741` diagnostics.

## Files Touched

- `docs/plan/claims/fix-checker-js-expando-symbol-property-return.md`

## Verification

- Pending
