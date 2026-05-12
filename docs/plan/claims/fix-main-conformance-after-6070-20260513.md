# fix(checker): restore conformance after #6070

- **Date**: 2026-05-13
- **Branch**: `codex/fix-main-conformance-after-6070`
- **PR**: TBD
- **Status**: claim
- **Workstream**: CI unblock

## Intent

Restore the seven conformance tests that started failing when #6070 reached
`main`, so performance documentation and follow-up T2.2 PRs can merge through
the normal required CI path.

## Failing Tests

- `TypeScript/tests/cases/compiler/asyncYieldStarContextualType.ts`
- `TypeScript/tests/cases/compiler/coAndContraVariantInferences3.ts`
- `TypeScript/tests/cases/compiler/genericMethodOverspecialization.ts`
- `TypeScript/tests/cases/compiler/noImplicitReturnsExclusions.ts`
- `TypeScript/tests/cases/compiler/yieldStarContextualType.ts`
- `TypeScript/tests/cases/conformance/externalModules/typeOnly/mergedWithLocalValue.ts`
- `TypeScript/tests/cases/conformance/externalModules/valuesMergingAcrossModules.ts`

## Verification Plan

- Run focused conformance filters for each failing test.
- Run targeted checker tests around #6070's TS2852 behavior.
- Run the relevant checker unit suite if the fix touches shared checking paths.
