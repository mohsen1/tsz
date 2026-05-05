# [WIP] fix(checker): align JS element access contextual type diagnostic

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next7`
- **PR**: #2825
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance fingerprint mismatch in
`jsElementAccessNoContextualTypeCrash.ts`. The picked failure has matching
diagnostic codes, but tsz reports TS2741 at `self['Common'] || {}` with type
display `Common`; tsc reports the diagnostic at the statement start with
display `typeof Common`.

## Files Touched

- TBD after root-cause inspection.

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/compiler/jsElementAccessNoContextualTypeCrash.ts`.
