# [WIP] fix(checker): report cross-module JSDoc typedef collisions

- **Date**: 2026-05-03
- **Branch**: `fix/jsdoc-typedef-cross-module-05031713`
- **PR**: #2587
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigate and fix `jsdoc/typedefCrossModule5.ts`, where `tsc` reports
TS2300 and TS2451 for a checked-JS cross-file collision between a JSDoc
`@typedef`, a class, and a const, while `tsz` currently emits no diagnostics.

## Files Touched

- TBD

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/conformance/jsdoc/typedefCrossModule5.ts`
  with expected `[TS2300, TS2451]` and actual `[]`.
