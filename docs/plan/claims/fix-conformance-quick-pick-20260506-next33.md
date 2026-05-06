# [WIP] fix(checker): suppress contextual generator return false positive

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-quick-pick-20260506-next33`
- **PR**: #3910
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Claiming `TypeScript/tests/cases/compiler/contextualParamTypeVsNestedReturnTypeInference4.ts`.

Current `origin/main` emits one extra `TS2322` where tsc accepts nested contextual return inference through `effectGen` / `effectFn` generator callbacks.

This slice will suppress or correctly type the extra assignment diagnostic without changing the expected no-error surface.

## Files Touched

- TBD after root-cause investigation.

## Verification

- Baseline: `./scripts/conformance/conformance.sh run --filter "contextualParamTypeVsNestedReturnTypeInference4" --verbose` (extra TS2322)
