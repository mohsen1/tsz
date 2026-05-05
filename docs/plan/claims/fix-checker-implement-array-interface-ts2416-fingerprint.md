# fix(checker): align Array interface implementation TS2416 fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/checker-implement-array-interface-ts2416-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only conformance failure in
`TypeScript/tests/cases/compiler/implementArrayInterface.ts`.
Both tsc and tsz emit TS2416, but the diagnostic tuple differs. The planned
scope is to root-cause the mismatch in interface implementation diagnostics,
most likely around method/property compatibility display or anchoring for a
class implementing `Array<T>`.

## Files Touched

- `docs/plan/claims/fix-checker-implement-array-interface-ts2416-fingerprint.md`

## Verification

- Pending
