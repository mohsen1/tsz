# [WIP] fix(checker): align constrained type argument inference fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-type-argument-inference-constraints-fingerprint`
- **PR**: #3484
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the picked `typeArgumentInferenceWithConstraints.ts` fingerprint-only
conformance mismatch. The expected and actual diagnostic code sets already
match TypeScript (`TS2322`, `TS2344`, `TS2345`, `TS2349`, `TS2403`), so this
slice is scoped to diagnostic message/source display parity for generic call
inference with constrained type arguments.

## Files Touched

- TBD

## Verification

- Pending
