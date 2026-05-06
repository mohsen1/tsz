# [WIP] fix(checker): align common type intersection fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/common-type-intersection-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)
- **Claimed**: 2026-05-06 07:06:14 UTC

## Intent

Fix the picked fingerprint-only conformance mismatch in
`TypeScript/tests/cases/conformance/types/intersection/commonTypeIntersection.ts`.
The snapshot reports matching diagnostic code `TS2322`; this slice will
root-cause the remaining fingerprint divergence in message rendering, source
type display, or diagnostic anchoring and fix it in the owning checker/solver
layer.

## Files Touched

- `docs/plan/claims/fix-common-type-intersection-fingerprint.md`
- implementation files TBD after diagnosis
- owning-crate Rust regression test

## Verification

- targeted owning-crate regression test
- targeted conformance rerun for `commonTypeIntersection`
