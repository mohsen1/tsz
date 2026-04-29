# [WIP] fix(checker): suppress contextual nested return inference TS2345

- **Date**: 2026-04-29
- **Branch**: `fix/checker-contextual-nested-return-inference`
- **PR**: #1707
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the random conformance pick `contextualParamTypeVsNestedReturnTypeInference4.ts`, where TSZ currently emits an extra TS2345 that `tsc` does not report. The expected scope is a checker/solver contextual typing or inference boundary correction, with a focused Rust unit test locking the invariant and targeted conformance verification.

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`
- `crates/tsz-checker/src/types/computation/call_display.rs`
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`

## Verification

- `cargo check --package tsz-checker` (passes)
- `cargo check --package tsz-solver` (passes)
- `cargo nextest run --package tsz-checker --test generic_call_inference_tests contextual_nested_generator_return_inference_drops_stale_ts2345` (passes)
- `./scripts/conformance/conformance.sh run --filter "contextualParamTypeVsNestedReturnTypeInference4" --verbose` (1/1 passes)
- `./scripts/conformance/conformance.sh run --max 200` reports existing `aliasDoesNotDuplicateSignatures.ts` TS2708 drift in the sampled prefix; the targeted conformance pick remains fixed.
- `cargo nextest run --package tsz-checker --lib` currently fails pre-existing architecture/enum checks unrelated to this slice (`types/function_type.rs` LOC guard, `types/computation/call/inner.rs` LOC guard, and `enum_nominality_tests::test_number_literal_to_numeric_enum_type`).
