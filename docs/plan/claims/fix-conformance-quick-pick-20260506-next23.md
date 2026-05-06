# [WIP] fix(checker): align styled-components TS2344 fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-quick-pick-20260506-next23`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Claiming `TypeScript/tests/cases/compiler/styledComponentsInstantiaionLimitNotReached.ts`.

Current `origin/main` emits `TS2344`, but the fingerprints differ from tsc:

- Missing `TS2344` at `test.ts:172:39` for `WithC`.
- Missing `TS2344` at `test.ts:195:65` for `AnyStyledComponent & C`.
- Extra `TS2344` at `test.ts:91:21` for the conditional `C extends ... ? C : never`.

This slice will align the generic constraint diagnostics without broadening
TS2344 suppression for ordinary type argument constraint failures.

## Files Touched

- TBD after root-cause investigation.

## Verification

- Pending.
