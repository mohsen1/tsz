# fix(checker): preserve higher-order retained generic inference

- **Date**: 2026-05-06
- **Branch**: `fix/checker-higher-order-retained-generics-diagnostics`
- **PR**: #3736
- **Status**: ready
- **Workstream**: 1 (TypeScript conformance)

## Intent

This PR investigates and fixes the false-positive diagnostics in `declarationEmitHigherOrderRetainedGenerics.ts`, where tsz currently reports TS2345, TS2769, and TS7031 while upstream `tsc` accepts the file. The goal is to align call inference and contextual typing for this higher-order retained generics pattern without suppressing unrelated call diagnostics.

## Files Touched

- `crates/tsz-checker/src/checkers/call_checker/candidate_collection.rs`
- `crates/tsz-checker/src/checkers/call_checker/diagnostics.rs`
- `crates/tsz-checker/src/checkers/call_checker/overload_resolution.rs`
- `crates/tsz-checker/src/checkers/call_context.rs`
- `crates/tsz-checker/src/types/computation/call_inference.rs`
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`
- `crates/tsz-solver/src/operations/call_args.rs`
- `crates/tsz-solver/src/operations/core/call_resolution.rs`
- `crates/tsz-solver/src/operations/generic_call/inference_helpers.rs`
- `crates/tsz-solver/src/operations/generic_call/normalization.rs`
- `crates/tsz-solver/src/operations/generic_call/resolve.rs`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker overloaded_higher_order_rest_any_constraint_accepts_generic_body overloaded_conditional_alias_first_arg_context_types_binding_pattern_callback conditional_alias_first_arg_context_types_binding_pattern_callback contextual_signature_instantiation_rejects_conflicting_generic_params generic_function_identifier_argument_still_contextually_instantiates`
- `./scripts/conformance/conformance.sh run --filter "declarationEmitHigherOrderRetainedGenerics" --verbose`
