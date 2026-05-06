# fix(checker): align expando function symbol property JS diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/checker-expando-function-symbol-property-js`
- **PR**: https://github.com/mohsen1/tsz/pull/3611
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

This PR targets the quick-picked false-positive conformance mismatch in
`TypeScript/tests/cases/compiler/expandoFunctionSymbolPropertyJs.ts`.
TypeScript reports no diagnostics for the case, but tsz currently reports
extra `TS2322` and `TS2741` diagnostics.

## Context

Selected with `scripts/session/quick-pick.sh --seed 3609`.

## Files Touched

- TBD

## Verification

- Pending
