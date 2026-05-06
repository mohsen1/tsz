# fix(checker): align call signature inheritance fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-130614`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/conformance/types/typeRelationships/assignmentCompatibility/callSignatureAssignabilityInInheritance6.ts`.

`tsc` and tsz already agree on the diagnostic codes (`TS2430`, `TS2564`),
but the conformance fingerprints differ. This slice will diagnose whether
the drift is interface-heritage diagnostic anchoring, TS2430 message
rendering, or generic call-signature compatibility reporting, then align the
fingerprints without changing the intended diagnostic set.

## Files Touched

- TBD

## Verification

- TBD
