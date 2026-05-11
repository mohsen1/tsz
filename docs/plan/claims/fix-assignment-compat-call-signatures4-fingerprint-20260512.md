# fix(checker): align assignmentCompatWithCallSignatures4 fingerprints

- **Date**: 2026-05-12
- **Branch**: `fix/assignment-compat-call-signatures4-fingerprint-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Close the fingerprint-only conformance failure for
`TypeScript/tests/cases/conformance/types/typeRelationships/assignmentCompatibility/assignmentCompatWithCallSignatures4.ts`.
The current dashboard reports matching diagnostic codes but TS2322/TS2564
fingerprint drift.

## Files Touched

- TBD after investigation.

## Verification

- Baseline: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter assignmentCompatWithCallSignatures4 --verbose`
- Planned: focused checker regression for the failing diagnostic display/anchor
- Planned: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter assignmentCompatWithCallSignatures4 --verbose`
