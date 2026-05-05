# [WIP] fix(checker): align instantiation expression fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-instantiation-expressions-fingerprint`
- **PR**: #2814
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/conformance/types/typeParameters/typeArgumentLists/instantiationExpressions.ts`.
The picker reports matching diagnostic codes (`TS1099`, `TS2344`, `TS2635`),
so this PR will root-cause the remaining message, display, count, or anchor
mismatch around instantiation expression diagnostics.

## Files Touched

- `docs/plan/claims/fix-checker-instantiation-expressions-fingerprint.md`
  (claim)
- Compiler files TBD after root-cause analysis.
- Owning-crate regression test TBD after root-cause analysis.

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- Owning-crate regression test once root-cause is isolated.
- `./scripts/conformance/conformance.sh run --filter "instantiationExpressions" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
