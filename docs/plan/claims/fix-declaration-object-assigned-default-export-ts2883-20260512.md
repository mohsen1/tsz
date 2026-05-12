# fix(checker): restore TS2883 for object-assigned default export

- **Date**: 2026-05-12
- **Branch**: `fix/declaration-object-assigned-default-export-ts2883-20260512`
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Restore the missing TS2883 diagnostic in `TypeScript/tests/cases/compiler/declarationEmitObjectAssignedDefaultExport.ts`, which full conformance reports as a PASS -> FAIL regression on latest `main` at `e401ed706f`.

## Files Touched

- TBD after investigation

## Verification

- Full conformance on latest main/mixin branch reported `12580/12582`, with this test as the only real regression and `mixinAccessModifiers.ts` as the remaining known XFAIL.
