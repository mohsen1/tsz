# fix(checker): align co/contra inference inheritance fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-174200`
- **PR**: #4189
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/compiler/coAndContraVariantInferences4.ts`.

`tsc` and tsz agree on diagnostic codes `TS2344` and `TS2430`, but the
diagnostic fingerprints differ. This slice will diagnose whether the drift is
type display, diagnostic anchoring, or inheritance/generic inference behavior,
then align the fingerprints through the owning checker/solver boundary without
adding a test-specific suppression.

## Files Touched

- TBD

## Verification

- TBD
