# [WIP] fix(checker): suppress contextual nested return inference TS2345

- **Date**: 2026-04-29
- **Branch**: `fix/checker-contextual-nested-return-inference`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the random conformance pick `contextualParamTypeVsNestedReturnTypeInference4.ts`, where TSZ currently emits an extra TS2345 that `tsc` does not report. The expected scope is a checker/solver contextual typing or inference boundary correction, with a focused Rust unit test locking the invariant and targeted conformance verification.

## Files Touched

- `docs/plan/claims/fix-checker-contextual-nested-return-inference.md`
- Implementation files TBD after diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "contextualParamTypeVsNestedReturnTypeInference4" --verbose`
- Planned: unit tests for touched crate(s) with `cargo nextest run`
