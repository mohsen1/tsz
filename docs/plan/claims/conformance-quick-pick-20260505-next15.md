# [WIP] fix(checker): align mapped type relationship diagnostics

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next15`
- **PR**: TBD
- **Status**: claimed
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance fingerprint mismatch in
`mappedTypeRelationships.ts`. The picked failure has matching diagnostic codes,
but several TS2322 messages differ from tsc around indexed access and mapped
type relationship displays.

## Files Touched

- TBD

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/conformance/types/mapped/mappedTypeRelationships.ts`.
