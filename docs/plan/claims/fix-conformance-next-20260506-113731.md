# fix(checker): align function call arity fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-113731`
- **PR**: #4023
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/compiler/functionCall11.ts`.

`tsc` and tsz already agree on the diagnostic codes (`TS2345`, `TS2554`) for
this function-call fixture, but the conformance fingerprints differ. This
slice will identify whether the mismatch is diagnostic anchoring, message
formatting, or arity reporting, then align the fingerprints while preserving
the existing code set.

## Files Touched

- TBD

## Verification

- TBD
