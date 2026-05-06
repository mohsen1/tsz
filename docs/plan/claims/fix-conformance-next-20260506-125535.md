# fix(checker): align rest argument call fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-125535`
- **PR**: #4078
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/compiler/functionCall10.ts`.

`tsc` and tsz already agree on the diagnostic code (`TS2345`) for this
rest-parameter call fixture, but the conformance fingerprints differ. This
slice will diagnose whether the drift is argument diagnostic anchoring,
message rendering, or rest-parameter expected-type display, then align the
fingerprint without changing the intended diagnostic set.

## Files Touched

- TBD

## Verification

- TBD
