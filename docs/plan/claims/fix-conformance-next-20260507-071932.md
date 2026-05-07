# [WIP] fix(checker): suppress deferred React Redux inference extra diagnostic

- **Date**: 2026-05-07
- **Branch**: `fix/conformance-next-20260507-071932`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the quick-picked conformance failure:
`TypeScript/tests/cases/compiler/reactReduxLikeDeferredInferenceAllowsAssignment.ts`.
`tsc` reports `TS2344`; `tsz` currently reports `TS2344` plus an extra
`TS2345`. This slice is scoped to the extra call-argument diagnostic while
preserving the expected type-constraint diagnostic.

## Files Touched

- TBD after diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "reactReduxLikeDeferredInferenceAllowsAssignment" --verbose`
- Planned: focused Rust regression test in the owning crate
- Planned: `./scripts/conformance/conformance.sh snapshot`
