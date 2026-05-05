# fix(checker): align strict-mode reserved word diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-strict-mode-reserved-word-diagnostics`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance)

## Intent

Fix the conformance gap in `TypeScript/tests/cases/compiler/strictModeReservedWord.ts`.
`tsz` currently misses duplicate identifier diagnostics for recovered reserved-word
declarations, skips the class expression name diagnostic for `class package`, and
does not report TS2507 when `extends public` resolves to the local `number` variable.

## Files Touched

- TBD

## Verification

- TBD
