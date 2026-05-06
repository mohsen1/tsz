# [WIP] fix(checker): align typeArgumentInference fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-quick-pick-20260506-next`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/conformance/expressions/functionCalls/typeArgumentInference.ts`.
The stored snapshot reports a fingerprint-only mismatch with matching
`TS2322`, `TS2345`, and `TS2403` code sets. This PR will root-cause the
generic call inference or diagnostic rendering drift and land the fix in the
owning checker/solver path with a focused Rust regression test.

## Files Touched

- `docs/plan/claims/fix-conformance-quick-pick-20260506-next.md`

## Verification

- Pending
