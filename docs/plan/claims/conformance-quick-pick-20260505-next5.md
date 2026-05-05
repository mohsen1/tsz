# [WIP] fix(checker): align JS constructor void property diagnostics

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next5`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance fingerprint mismatch in `assignmentToVoidZero2.ts`.
The picked failure has matching diagnostic codes, but tsz misses the expected
TS2339 fingerprint for `c.p + c.q` where `q` is assigned `void 0` inside a
JavaScript constructor body and should not become a visible instance property.

## Files Touched

- TBD after root-cause inspection.

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/conformance/salsa/assignmentToVoidZero2.ts`.
