# fix(checker): suppress module preserve JS require global diagnostic

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-110532`
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/compiler/modulePreserve2.ts`.

`tsc` reports no diagnostics for this mixed module-preserve fixture. tsz
currently emits an extra `TS2591` for `require` in the checked JavaScript file
that imports from a package with conditional `exports`. This slice will find the
module-preserve JS/environment path and suppress only the false missing-global
diagnostic while preserving real missing `require` diagnostics elsewhere.

## Files Touched

- TBD

## Verification

- TBD
