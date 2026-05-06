# fix(checker): align discriminated union assignment fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/checker-discriminated-union-assignment-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Reduce the fingerprint-only conformance failure in
`TypeScript/tests/cases/conformance/types/typeRelationships/assignmentCompatibility/assignmentCompatWithDiscriminatedUnion.ts`.
Both tsc and tsz emit `TS2322`, but diagnostic fingerprints differ for
assignment compatibility involving discriminated unions.

## Files Touched

- `docs/plan/claims/fix-checker-discriminated-union-assignment-fingerprint.md`

## Verification

- Pending.
