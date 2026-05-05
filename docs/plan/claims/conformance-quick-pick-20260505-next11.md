# [WIP] fix(checker): preserve TS union display order for constructor guard errors

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next11`
- **PR**: #2911
- **Status**: claimed
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance fingerprint mismatch in
`typeGuardConstructorClassAndNumber.ts`. The picked failure has matching
diagnostic codes and locations, but tsz prints the union as `C1 | number` where
tsc prints `number | C1` for TS2339 property-access errors in negative
constructor guard branches.

## Files Touched

- TBD

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/compiler/typeGuardConstructorClassAndNumber.ts`.
