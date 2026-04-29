# [WIP] fix(checker): align interface intersection extends fingerprints

- **Date**: 2026-04-29
- **Branch**: `fix/interface-extends-intersection-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only mismatch picked by `scripts/session/quick-pick.sh`
for `interfaceExtendsObjectIntersectionErrors.ts`. The conformance snapshot
shows matching diagnostic codes (`TS2312`, `TS2411`, `TS2413`, `TS2416`,
`TS2430`) but differing fingerprints, so this slice will diagnose whether the
gap is message rendering, type display, or diagnostic anchoring and fix it in
the owning checker/solver boundary.

## Files Touched

- `docs/plan/claims/fix-interface-extends-intersection-fingerprint.md`
- Implementation files TBD after diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "interfaceExtendsObjectIntersectionErrors" --verbose`
- Planned: targeted `cargo nextest run` for touched crate tests.
