# fix(checker): spread readonly const tuples into generic calls

- **Owner**: Codex
- **Issue**: #6828
- **Workstream**: Conformance - call argument spread and generic inference
- **Branch**: `codex/issue-6828-const-tuple-spread-generic-20260514`
- **Status**: Ready for review

## Scope

Investigate and fix the TS2554 false positive where spreading a readonly const tuple into a fixed-arity generic function call is not recognized as individual arguments.

## Validation

- `cargo test -p tsz-checker --test generic_call_inference_tests readonly_const_tuple_spread_into_fixed_arity_generic_call_no_ts2554 -- --nocapture`
- `cargo test -p tsz-checker --test generic_call_inference_tests variadic_tuple_spread_without_assertion_widens_to_primitives -- --nocapture`
- `rg -n "DEBUG|eprintln!" crates/tsz-checker/src/checkers/call_checker/candidate_collection.rs crates/tsz-checker/src/checkers/call_checker/applicability.rs crates/tsz-checker/src/types/computation/call_inference.rs crates/tsz-checker/src/types/computation/call_result.rs`
