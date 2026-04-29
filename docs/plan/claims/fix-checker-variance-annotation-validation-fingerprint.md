# [WIP] fix(checker): align variance annotation validation fingerprints

- **Date**: 2026-04-29
- **Branch**: `fix/checker-variance-annotation-validation-fingerprint`
- **PR**: #1747
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

This PR targets the random conformance pick `TypeScript/tests/cases/compiler/varianceAnnotationValidation.ts`.
The current snapshot reports a fingerprint-only mismatch for TS2322/TS2636 diagnostics, so the slice will diagnose
the display, anchor, or count detail that differs from `tsc` and fix it in the owning checker/solver/printer path.

## Files Touched

- `docs/plan/claims/fix-checker-variance-annotation-validation-fingerprint.md` (claim)
- Implementation files TBD after diagnosis

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "varianceAnnotationValidation" --verbose`
- Planned: targeted crate `cargo nextest run` for any crate changed
