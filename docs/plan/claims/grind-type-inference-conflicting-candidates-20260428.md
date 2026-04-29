# fix(solver): preserve conflicting generic inference candidates

- **Date**: 2026-04-28
- **Branch**: `grind/type-inference-conflicting-candidates-20260428`
- **PR**: #1635
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the TS2345 miss in `typeInferenceConflictingCandidates.ts`, where conflicting direct candidates for a generic type parameter are incorrectly merged/deferred instead of preserving TypeScript's first-candidate mismatch behavior. The solver now fails a later bare-`T` argument when its primitive family conflicts with the first direct candidate, before callback inference can merge the candidates away.

## Files Touched

- `crates/tsz-solver/src/operations/generic_call/resolve.rs`
- `crates/tsz-solver/src/operations/generic_call/inference_helpers.rs`
- `crates/tsz-checker/tests/generic_tests.rs`

## Verification

- CI only per automation instruction.
