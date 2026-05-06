# [WIP] fix(checker): suppress recursive typeof redeclaration cascade

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-quick-pick-20260506-next22`
- **PR**: #3701
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Claiming `TypeScript/tests/cases/conformance/types/specifyingTypes/typeQueries/recursiveTypesWithTypeof.ts`.

Current `origin/main` emits the expected `TS2454` and `TS2502`, but also emits
an extra `TS2403` for:

```ts
var f: Array<typeof f>;
var f: any;
```

This slice will align the recursive `typeof` redeclaration diagnostics with tsc
without weakening ordinary incompatible variable redeclaration reporting.

## Files Touched

- TBD after root-cause investigation.

## Verification

- Pending.
