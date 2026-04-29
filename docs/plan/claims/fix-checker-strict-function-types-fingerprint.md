# [WIP] fix(checker): align strict function type fingerprints

- **Date**: 2026-04-29
- **Branch**: `fix/checker-strict-function-types-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance / fingerprint-only TS2322 TS2328)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/compiler/strictFunctionTypesErrors.ts`, selected by
`scripts/session/quick-pick.sh`. The initial target has matching diagnostic
codes (`TS2322`, `TS2328`) but mismatched fingerprints, so the investigation
will focus on message rendering, elaboration, and/or diagnostic anchoring
while keeping assignability routed through the shared checker/solver boundary.

## Files Touched

- `docs/plan/claims/fix-checker-strict-function-types-fingerprint.md`
- Implementation files TBD after verbose conformance inspection.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "strictFunctionTypesErrors" --verbose`
- Planned: targeted owning-crate unit tests via `cargo nextest run`
