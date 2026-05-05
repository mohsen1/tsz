# fix(checker): align this-type relationship fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-this-type-relationships-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/conformance/types/thisType/typeRelationships.ts`.
The picked test already emits the same diagnostic codes as `tsc`
(`TS2322`, `TS2403`, and `TS2739`), so this PR aligns the reported
diagnostic source spans and rendered type fingerprints for this-type
relationship errors.

## Files Touched

- `docs/plan/claims/fix-checker-this-type-relationships-fingerprint.md`

## Verification

- Pending
