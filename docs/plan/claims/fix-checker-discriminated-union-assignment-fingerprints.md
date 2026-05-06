# fix(checker): align discriminated union assignment fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-discriminated-union-assignment-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (conformance)

## Intent

Claiming `TypeScript/tests/cases/conformance/types/typeRelationships/assignmentCompatibility/assignmentCompatWithDiscriminatedUnion.ts`.

Current `origin/main` reports the expected TS2322 code, but the diagnostic
fingerprints differ. The `undefined` assignment displays the alias
`IAxisType` instead of the expected literal union, and an extra tuple-union
assignment diagnostic is emitted for the GH39357 case.

## Verification

- Pending.
