# [WIP] fix(checker): align reverse mapped intersection diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/reverse-mapped-intersection-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Fix the fingerprint-only diagnostic mismatch for
`reverseMappedTypeIntersectionConstraint.ts`, where TSZ and tsc agree on the
`TS2322` and `TS2353` codes but disagree on diagnostic details. The fix will
identify whether the mismatch is in assignability display, excess-property
reporting, or reverse-mapped/intersection constraint semantics, then change the
owning layer with a focused Rust regression test.

## Files Touched

- `docs/plan/claims/fix-reverse-mapped-intersection-fingerprint.md` (claim)
- Implementation files TBD after diagnosis.

## Verification

- Pending.
