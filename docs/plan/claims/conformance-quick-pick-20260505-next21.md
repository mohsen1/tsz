# [WIP] fix(checker): align intra-expression inference diagnostic surface

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next21`
- **PR**: TBD
- **Status**: claimed
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the quick-picked fingerprint-only mismatch in
`intraExpressionInferences.ts`. The diagnostic code set already matches tsc
(`TS2322`, `TS2339`), but the `map` function assignment diagnostic around
`test.ts:132:5` reports an `error` return property and an optional function
target where tsc reports `any` and the direct function target surface.

## Files Touched

- TBD

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/intraExpressionInferences.ts`.
