# [WIP] fix(checker): honor strict built-in iterator returns

- **Date**: 2026-05-05
- **Branch**: `fix/checker-strict-builtin-iterator-return`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the missing `TS2322` diagnostics in
`TypeScript/tests/cases/compiler/iterableTReturnTNext.ts`. Current tsz on
`origin/main` already emits the expected `TS2416` and `TS5024`, but it misses
assignability failures where `MapIterator<T>.next()` should expose
`IteratorResult<T, undefined>` under `strictBuiltinIteratorReturn`.

## Files Touched

- `docs/plan/claims/fix-checker-strict-builtin-iterator-return.md`

## Verification

- Pending implementation.
