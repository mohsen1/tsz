# fix(checker): align generic construct signature TS2430 fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-144453`
- **PR**: #4156
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/conformance/types/typeRelationships/subtypesAndSuperTypes/subtypingWithGenericConstructSignaturesWithOptionalParameters.ts`.

`tsc` and tsz agree on diagnostic code `TS2430`, but the fingerprints differ
for generic construct signature inheritance failures. This slice will diagnose
whether the drift is diagnostic anchoring, message shaping, or generic
signature comparison behavior, then align the fingerprints without weakening
the shared inheritance relation path.

## Files Touched

- TBD

## Verification

- TBD
