# [WIP] fix(checker): align deferred conditional assignability fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-conditional-deferred-assignability-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/compiler/conditionalTypeAssignabilityWhenDeferred.ts`.
The picked test already emits the same diagnostic codes as `tsc`
(`TS2322`, `TS2345`), so this PR will root-cause the remaining message,
display, count, or anchor mismatch around deferred conditional type
assignability.

## Files Touched

- `docs/plan/claims/fix-checker-conditional-deferred-assignability-fingerprint.md`
  (claim)
- Compiler files TBD after root-cause analysis.
- Owning-crate regression test TBD after root-cause analysis.

## Verification

- `./scripts/conformance/conformance.sh run --filter "conditionalTypeAssignabilityWhenDeferred" --verbose`
- Owning-crate regression test once root-cause is isolated.
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
