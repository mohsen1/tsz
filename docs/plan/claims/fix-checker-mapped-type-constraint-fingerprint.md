# fix(checker): align mapped type constraint fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-mapped-type-constraint-fingerprint`
- **PR**: #3029
- **Status**: ready
- **Workstream**: 1 (Conformance - mapped type constraint diagnostics)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/conformance/types/mapped/mappedTypeConstraints2.ts`,
a fingerprint-only TS2322 failure. The live mismatch shows `tsz` emits the
right diagnostic code but prints anonymous mapped-type bodies and broad
`[string]` indexed-access displays where `tsc` preserves alias/index forms
such as `Mapped2<K>[`get${K}`]`, `Foo<T>[`get${T}`]`, and
`ObjectWithUnderscoredKeys<K>[`_${K}`]`.

This PR will root-cause the indexed-access/mapped-type display path and align
the TS2322 fingerprints with `tsc`, with a focused checker or solver
regression test for the invariant.

## Files Touched

- `crates/tsz-solver/src/type_queries/mapped.rs`
- `crates/tsz-solver/src/evaluation/evaluate.rs`
- `crates/tsz-solver/src/evaluation/evaluate_rules/keyof.rs`
- `crates/tsz-solver/src/evaluation/evaluate_rules/mapped.rs`
- `crates/tsz-solver/src/intern/core/interner.rs`
- `crates/tsz-solver/src/operations/expression_ops.rs`
- `crates/tsz-solver/src/diagnostics/format/mod.rs`
- `crates/tsz-solver/src/type_queries/data/content_predicates.rs`
- `crates/tsz-checker/src/query_boundaries/common.rs`
- `crates/tsz-checker/src/query_boundaries/assignability.rs`
- `crates/tsz-checker/src/types/computation/access.rs`
- `crates/tsz-checker/src/state/type_environment/core.rs`
- `crates/tsz-checker/src/state/variable_checking/core.rs`
- `crates/tsz-checker/src/state/state_checking/property.rs`
- `crates/tsz-checker/src/types/property_access_helpers/access_semantics.rs`
- `crates/tsz-checker/src/assignability/assignability_diagnostics.rs`
- `crates/tsz-checker/src/error_reporter/assignability_helpers.rs`
- `crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs`
- `crates/tsz-checker/src/error_reporter/core_formatting.rs`
- `crates/tsz-checker/src/context/mod.rs`
- `crates/tsz-checker/src/context/constructors.rs`
- `crates/tsz-checker/tests/mapped_indexed_access_diagnostic_tests.rs`

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-solver`
- `cargo check --package tsz-checker`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-checker --lib mapped_indexed_access_diagnostic_tests`
- `cargo nextest run --package tsz-checker --lib literal_application_alias_display_tests::ts2345_alias_resolves_independent_of_iteration_variable_name keyof_mapped_as_clause_tests::mapped_type_as_clause_over_object_union_produces_concrete_type literal_application_alias_display_tests::ts2345_keys_extended_by_alias_resolves_to_literal_in_param_display contextual_typing_tests::test_deferred_mapped_intersection_preserves_contextual_property_types contextual_typing_tests::test_contextual_function_object_property_intersection_sequence homomorphic_remap_missing_property_uses_specialized_source_display mapped_indexed_access_diagnostic_tests`
- `cargo nextest run --package tsz-checker --test conformance_issues features::function_shape::test_generic_filtering_mapped_callbacks_use_widened_round2_context`
- `cargo nextest run --package tsz-solver --lib mapped_key_remap_tests::test_keyof_generic_remapped_mapped_type_keeps_concrete_lower_bound_keys`
- `cargo nextest run --package tsz-checker --lib architecture_contract_tests_src::test_no_push_diagnostic_outside_error_reporter architecture_contract_tests_src::test_solver_imports_go_through_query_boundaries architecture_contract_tests_src::test_no_inline_type_queries_in_cleaned_modules`
- `./scripts/conformance/conformance.sh run --filter "mappedTypeConstraints2" --verbose` -> `FINAL RESULTS: 1/1 passed (100.0%)`
- `./scripts/conformance/conformance.sh run --max 200` -> `FINAL RESULTS: 200/200 passed (100.0%)`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` -> `FINAL RESULTS: 12453/12582 passed (99.0%)`
