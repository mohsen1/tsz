# [WIP] fix(checker): align generic call signature assignment fingerprint

- **Date**: 2026-05-06
- **Branch**: `fix/assignment-compat-generic-call-signatures4-20260506`
- **PR**: #3705
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the quick-picked fingerprint-only target
`TypeScript/tests/cases/conformance/types/typeRelationships/assignmentCompatibility/assignmentCompatWithGenericCallSignatures4.ts`.
The expected and actual code sets both contain `TS2322`, so this slice is
scoped to root-causing the diagnostic message or location drift in generic
call signature assignment compatibility and landing the fix in the owning
checker/solver path with focused Rust coverage.

## Files Touched

- `docs/plan/claims/fix-assignment-compat-generic-call-signatures4-20260506.md`

## Verification

- Pending
