# fix(checker): align generic call signature optional parameter fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-121407`
- **PR**: #4053
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/conformance/types/typeRelationships/subtypesAndSuperTypes/subtypingWithGenericCallSignaturesWithOptionalParameters.ts`.

`tsc` and tsz already agree on the diagnostic code (`TS2430`), but the
fingerprints differ. This slice will diagnose whether the drift is diagnostic
anchoring, message formatting, or call-signature compatibility rendering, then
align the fingerprint without changing the intended diagnostic set.

## Files Touched

- TBD

## Verification

- TBD
