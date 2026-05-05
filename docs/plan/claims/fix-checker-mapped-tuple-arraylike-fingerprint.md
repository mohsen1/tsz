# [WIP] fix(checker): align mapped tuple arraylike fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/checker-mapped-tuple-arraylike-fingerprint`
- **PR**: https://github.com/mohsen1/tsz/pull/3143
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only conformance drift in
`TypeScript/tests/cases/compiler/mappedTypeUnionConstrainTupleTreatedAsArrayLike.ts`.
The target already emits the expected `TS2322` diagnostics, but at least one
diagnostic message or anchor differs from TypeScript's baseline.

## Files Touched

- `docs/plan/claims/fix-checker-mapped-tuple-arraylike-fingerprint.md`

## Verification

- Pending implementation.
