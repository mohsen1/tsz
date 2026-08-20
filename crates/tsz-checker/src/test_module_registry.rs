//! Registration of the crate's in-`src` test modules.
//!
//! `#[cfg(test)] #[path = "..."] mod ...;` triples are three lines each, and at
//! 432 of them they dominated `lib.rs` — which sits against the 2000-line
//! architecture cap, so unrelated checker PRs collided here and each new test
//! module pushed the file closer to failing `arch-size`. Keeping them in their
//! own file makes the cost of adding a test module one line in a file that has
//! room for it.
//!
//! Paths stay exactly as they were in `lib.rs`: `#[path]` on a non-inline
//! module resolves against the directory holding the file that declares it, and
//! this file sits in `src/` alongside `lib.rs`, so `tests/x.rs` still means
//! `src/tests/x.rs`. The whole module is already `#[cfg(test)]` at its
//! declaration in `lib.rs`, so the per-entry `#[cfg(test)]` is not repeated.
//!
//! Modules under `crates/tsz-checker/tests/` are deliberately NOT registered
//! here — several of them use `use super::*` and must stay children of the
//! crate root for that to resolve.

#[path = "tests/accessor_inherited_completeness_tests.rs"]
mod accessor_inherited_completeness_tests;
#[path = "tests/accessor_this_parameter_pairing_tests.rs"]
mod accessor_this_parameter_pairing_tests;
#[path = "tests/alias_application_display_retention_tests.rs"]
mod alias_application_display_retention_tests;
#[path = "tests/ambient_class_override_type_compat_tests.rs"]
mod ambient_class_override_type_compat_tests;
#[path = "tests/ambient_declare_async_modifier_tests.rs"]
mod ambient_declare_async_modifier_tests;
#[path = "tests/ambient_declare_async_static_modifier_tests.rs"]
mod ambient_declare_async_static_modifier_tests;
#[path = "tests/ambient_declare_override_modifier_tests.rs"]
mod ambient_declare_override_modifier_tests;
#[path = "tests/ambient_top_level_initializer_ts1039_tests.rs"]
mod ambient_top_level_initializer_ts1039_tests;
#[path = "tests/any_parameter_never_opposite_tests.rs"]
mod any_parameter_never_opposite_tests;
#[path = "tests/application_arg_concrete_index_access_display_tests.rs"]
mod application_arg_concrete_index_access_display_tests;
#[path = "tests/application_target_any_arg_assignability_tests.rs"]
mod application_target_any_arg_assignability_tests;
#[path = "tests/application_unknown_args_assignability_tests.rs"]
mod application_unknown_args_assignability_tests;
#[path = "tests/architecture_contract_tests.rs"]
mod architecture_contract_tests_src;
#[path = "tests/array_destructuring_assignment_element_ts2322_tests.rs"]
mod array_destructuring_assignment_element_ts2322_tests;
#[path = "tests/array_elaboration_relation_routing_arch_tests.rs"]
mod array_elaboration_relation_routing_arch_tests;
#[path = "tests/array_like_constraint_relation_routing_arch_tests.rs"]
mod array_like_constraint_relation_routing_arch_tests;
#[path = "tests/array_literal_relation_routing_arch_tests.rs"]
mod array_literal_relation_routing_arch_tests;
#[path = "tests/array_literal_spread_inference_widening_tests.rs"]
mod array_literal_spread_inference_widening_tests;
#[path = "tests/array_source_literal_element_display_tests.rs"]
mod array_source_literal_element_display_tests;
#[path = "tests/as_const_nested_literal_display_tests.rs"]
mod as_const_nested_literal_display_tests;
#[path = "tests/assertion_thenable_comparability_tests.rs"]
mod assertion_thenable_comparability_tests;
#[path = "tests/assertion_type_predicate_diagnostics_tests.rs"]
mod assertion_type_predicate_diagnostics_tests;
#[path = "tests/assign_to_import_shadowing_local_tests.rs"]
mod assign_to_import_shadowing_local_tests;
#[path = "tests/assignability_diagnostics_relation_routing_arch_tests.rs"]
mod assignability_diagnostics_relation_routing_arch_tests;
#[path = "tests/assignability_display_relation_routing_arch_tests.rs"]
mod assignability_display_relation_routing_arch_tests;
#[path = "tests/assignability_eval_memo_tests.rs"]
mod assignability_eval_memo_tests;
#[path = "tests/assignability_failure_memo_tests.rs"]
mod assignability_failure_memo_tests;
#[path = "tests/assignability_index_access_normalization_boundary_arch_tests.rs"]
mod assignability_index_access_normalization_boundary_arch_tests;
#[path = "tests/assignability_reporter_relation_routing_arch_tests.rs"]
mod assignability_reporter_relation_routing_arch_tests;
#[path = "tests/assignability_type_comparability_relation_routing_arch_tests.rs"]
mod assignability_type_comparability_relation_routing_arch_tests;
#[path = "tests/assignment_ops_relation_routing_arch_tests.rs"]
mod assignment_ops_relation_routing_arch_tests;
#[path = "tests/async_export_modifier_order_ts1042_dedup_tests.rs"]
mod async_export_modifier_order_ts1042_dedup_tests;
#[path = "tests/async_generator_yieldstar_contribution_tests.rs"]
mod async_generator_yieldstar_contribution_tests;
#[path = "tests/async_generator_yieldstar_union_delegate_tests.rs"]
mod async_generator_yieldstar_union_delegate_tests;
#[path = "tests/async_return_invalid_thenable_tests.rs"]
mod async_return_invalid_thenable_tests;
#[path = "tests/await_alias_union_distribution_tests.rs"]
mod await_alias_union_distribution_tests;
#[path = "tests/await_concise_arrow_body_grammar_tests.rs"]
mod await_concise_arrow_body_grammar_tests;
#[path = "tests/await_grammar_computed_property_name_tests.rs"]
mod await_grammar_computed_property_name_tests;
#[path = "tests/await_grammar_expression_position_tests.rs"]
mod await_grammar_expression_position_tests;
#[path = "tests/await_grammar_statement_position_tests.rs"]
mod await_grammar_statement_position_tests;
#[path = "tests/await_static_block_grammar_tests.rs"]
mod await_static_block_grammar_tests;
#[path = "tests/await_structural_thenable_tests.rs"]
mod await_structural_thenable_tests;
#[path = "tests/base_type_param_default_inheritance_tests.rs"]
mod base_type_param_default_inheritance_tests;
#[path = "tests/bind_overloaded_receiver_preserves_signatures_tests.rs"]
mod bind_overloaded_receiver_preserves_signatures_tests;
#[path = "tests/boolean_literal_union_narrowing_tests.rs"]
mod boolean_literal_union_narrowing_tests;
#[path = "tests/boxed_global_env_authority_tests.rs"]
mod boxed_global_env_authority_tests;
#[path = "tests/builtin_iterator_implements_tests.rs"]
mod builtin_iterator_implements_tests;
#[path = "tests/call_architecture_tests.rs"]
mod call_architecture_tests;
#[path = "tests/call_argument_type_parameter_target_elaboration_tests.rs"]
mod call_argument_type_parameter_target_elaboration_tests;
#[path = "tests/call_checker_diagnostic_relation_routing_arch_tests.rs"]
mod call_checker_diagnostic_relation_routing_arch_tests;
#[path = "tests/call_context_relation_routing_arch_tests.rs"]
mod call_context_relation_routing_arch_tests;
#[path = "tests/call_display_format_relation_routing_arch_tests.rs"]
mod call_display_format_relation_routing_arch_tests;
#[path = "tests/call_elaboration_mutual_relation_routing_arch_tests.rs"]
mod call_elaboration_mutual_relation_routing_arch_tests;
#[path = "tests/call_error_anchor_relation_routing_arch_tests.rs"]
mod call_error_anchor_relation_routing_arch_tests;
#[path = "tests/call_error_elaboration_relation_routing_arch_tests.rs"]
mod call_error_elaboration_relation_routing_arch_tests;
#[path = "tests/call_result_constraint_violation_gateway_arch_tests.rs"]
mod call_result_constraint_violation_gateway_arch_tests;
#[path = "tests/call_result_relation_routing_arch_tests.rs"]
mod call_result_relation_routing_arch_tests;
#[path = "tests/call_spread_constructor_parameters_tests.rs"]
mod call_spread_constructor_parameters_tests;
#[path = "tests/callable_constraint_function_identity_tests.rs"]
mod callable_constraint_function_identity_tests;
#[path = "tests/callable_interface_assignment_tests.rs"]
mod callable_interface_assignment_tests;
#[path = "tests/callable_union_relation_routing_arch_tests.rs"]
mod callable_union_relation_routing_arch_tests;
#[path = "tests/circular_export_star_const_value_tests.rs"]
mod circular_export_star_const_value_tests;
#[path = "tests/circular_initializer_deferred_generic_tests.rs"]
mod circular_initializer_deferred_generic_tests;
#[path = "tests/class_boundary_fallback_relation_routing_arch_tests.rs"]
mod class_boundary_fallback_relation_routing_arch_tests;
#[path = "tests/class_constructor_private_static_recovery_tests.rs"]
mod class_constructor_private_static_recovery_tests;
#[path = "tests/class_duplicate_extends_skip_resolution_tests.rs"]
mod class_duplicate_extends_skip_resolution_tests;
#[path = "tests/class_extends_generic_override_variance_tests.rs"]
mod class_extends_generic_override_variance_tests;
#[path = "tests/class_extends_index_relation_routing_arch_tests.rs"]
mod class_extends_index_relation_routing_arch_tests;
#[path = "tests/class_feature_target_gates_tests.rs"]
mod class_feature_target_gates_tests;
#[path = "tests/class_generic_method_self_ref_return_constraint_tests.rs"]
mod class_generic_method_self_ref_return_constraint_tests;
#[path = "tests/class_implements_abstract_member_compat_tests.rs"]
mod class_implements_abstract_member_compat_tests;
#[path = "tests/class_implements_call_construct_signature_tests.rs"]
mod class_implements_call_construct_signature_tests;
#[path = "tests/class_implements_generic_override_variance_tests.rs"]
mod class_implements_generic_override_variance_tests;
#[path = "tests/class_implements_index_relation_routing_arch_tests.rs"]
mod class_implements_index_relation_routing_arch_tests;
#[path = "tests/class_implements_jsdoc_heritage_relation_routing_arch_tests.rs"]
mod class_implements_jsdoc_heritage_relation_routing_arch_tests;
#[path = "tests/class_implements_whole_type_relation_routing_arch_tests.rs"]
mod class_implements_whole_type_relation_routing_arch_tests;
#[path = "tests/class_member_circular_return_tests.rs"]
mod class_member_circular_return_tests;
#[path = "tests/class_member_modifier_grammar_first_error_wins_tests.rs"]
mod class_member_modifier_grammar_first_error_wins_tests;
#[path = "tests/class_namespace_merge_static_overload_duplicate_ts2300_tests.rs"]
mod class_namespace_merge_static_overload_duplicate_ts2300_tests;
#[path = "tests/class_namespace_static_relation_routing_arch_tests.rs"]
mod class_namespace_static_relation_routing_arch_tests;
#[path = "tests/class_static_init_self_new_tests.rs"]
mod class_static_init_self_new_tests;
#[path = "tests/class_static_side_relation_routing_arch_tests.rs"]
mod class_static_side_relation_routing_arch_tests;
#[path = "tests/class_static_wide_symbol_member_index_tests.rs"]
mod class_static_wide_symbol_member_index_tests;
#[path = "tests/class_wide_symbol_member_index_tests.rs"]
mod class_wide_symbol_member_index_tests;
#[path = "tests/closure_destructuring_top_level_diagnostics_tests.rs"]
mod closure_destructuring_top_level_diagnostics_tests;
#[path = "tests/comlink_row_regression_tests.rs"]
mod comlink_row_regression_tests;
#[path = "tests/commonjs_export_assignment_chain_tests.rs"]
mod commonjs_export_assignment_chain_tests;
#[path = "tests/commonjs_export_declaration_level_type_tests.rs"]
mod commonjs_export_declaration_level_type_tests;
#[path = "tests/commonjs_module_exports_jsdoc_type_declared_tests.rs"]
mod commonjs_module_exports_jsdoc_type_declared_tests;
#[path = "tests/commonjs_reentrant_surface_tests.rs"]
mod commonjs_reentrant_surface_tests;
#[path = "tests/commonjs_require_binding_type_meaning_tests.rs"]
mod commonjs_require_binding_type_meaning_tests;
#[path = "tests/commonjs_require_destructure_ts2305_tests.rs"]
mod commonjs_require_destructure_ts2305_tests;
#[path = "tests/compound_nullish_widening_implicit_any_ts7005_tests.rs"]
mod compound_nullish_widening_implicit_any_ts7005_tests;
#[path = "tests/compound_nullish_widening_implicit_any_ts7010_tests.rs"]
mod compound_nullish_widening_implicit_any_ts7010_tests;
#[path = "tests/computed_alias_source_display_tests.rs"]
mod computed_alias_source_display_tests;
#[path = "tests/computed_index_member_source_display_tests.rs"]
mod computed_index_member_source_display_tests;
#[path = "tests/computed_key_missing_property_primary_code_tests.rs"]
mod computed_key_missing_property_primary_code_tests;
#[path = "tests/computed_key_nested_excess_property_tests.rs"]
mod computed_key_nested_excess_property_tests;
#[path = "tests/computed_member_name_diagnostic_display_tests.rs"]
mod computed_member_name_diagnostic_display_tests;
#[path = "tests/computed_symbol_name_unification_tests.rs"]
mod computed_symbol_name_unification_tests;
#[path = "tests/concise_body_return_excess_property_tests.rs"]
mod concise_body_return_excess_property_tests;
#[path = "tests/conditional_break_narrowing_tests.rs"]
mod conditional_break_narrowing_tests;
#[path = "tests/conditional_constraint_relation_routing_arch_tests.rs"]
mod conditional_constraint_relation_routing_arch_tests;
#[path = "tests/conditional_flow_substitution_ts2344_tests.rs"]
mod conditional_flow_substitution_ts2344_tests;
#[path = "tests/conditional_narrowed_index_through_generic_alias_tests.rs"]
mod conditional_narrowed_index_through_generic_alias_tests;
#[path = "tests/conditional_never_param_inference_tests.rs"]
mod conditional_never_param_inference_tests;
#[path = "tests/const_asserted_return_type_tests.rs"]
mod const_asserted_return_type_tests;
#[path = "tests/constraint_position_nullable_access_tests.rs"]
mod constraint_position_nullable_access_tests;
#[path = "tests/constraint_validation_relation_routing_arch_tests.rs"]
mod constraint_validation_relation_routing_arch_tests;
#[path = "tests/constructor_argument_type_parameter_target_elaboration_tests.rs"]
mod constructor_argument_type_parameter_target_elaboration_tests;
#[path = "tests/contextual_callback_shadowed_type_param_tests.rs"]
mod contextual_callback_shadowed_type_param_tests;
#[path = "tests/contextual_new_relation_routing_arch_tests.rs"]
mod contextual_new_relation_routing_arch_tests;
#[path = "tests/contextual_return_wrapper_tests.rs"]
mod contextual_return_wrapper_tests;
#[path = "tests/cross_file_class_instance_publication_tests.rs"]
mod cross_file_class_instance_publication_tests;
#[path = "tests/cross_file_generic_alias_union_implements_tests.rs"]
mod cross_file_generic_alias_union_implements_tests;
#[path = "tests/cross_file_generic_implements_type_param_arena_tests.rs"]
mod cross_file_generic_implements_type_param_arena_tests;
#[path = "tests/cross_file_in_operator_indexed_element_narrowing_tests.rs"]
mod cross_file_in_operator_indexed_element_narrowing_tests;
#[path = "tests/cross_file_interface_property_access_tests.rs"]
mod cross_file_interface_property_access_tests;
#[path = "tests/cross_file_merged_symbol_value_computed_key_tests.rs"]
mod cross_file_merged_symbol_value_computed_key_tests;
#[path = "tests/cross_file_type_param_decl_identity_tests.rs"]
mod cross_file_type_param_decl_identity_tests;
#[path = "tests/cross_file_unresolved_alias_union_simplification_tests.rs"]
mod cross_file_unresolved_alias_union_simplification_tests;
#[path = "tests/cross_module_class_self_member_tests.rs"]
mod cross_module_class_self_member_tests;
#[path = "tests/cross_module_generic_interface_heritage_tests.rs"]
mod cross_module_generic_interface_heritage_tests;
#[path = "tests/declaration_extract_key_path_tests.rs"]
mod declaration_extract_key_path_tests;
#[path = "tests/declare_export_default_expression_ambient_ts2714_tests.rs"]
mod declare_export_default_expression_ambient_ts2714_tests;
#[path = "tests/declare_export_default_modifier_order_ts1319_dedup_tests.rs"]
mod declare_export_default_modifier_order_ts1319_dedup_tests;
#[path = "tests/declare_export_equals_ambient_ts2714_tests.rs"]
mod declare_export_equals_ambient_ts2714_tests;
#[path = "tests/declare_private_member_implicit_any_tests.rs"]
mod declare_private_member_implicit_any_tests;
#[path = "tests/declared_signature_return_literal_display_tests.rs"]
mod declared_signature_return_literal_display_tests;
#[path = "tests/decorator_return_relation_routing_arch_tests.rs"]
mod decorator_return_relation_routing_arch_tests;
#[path = "tests/defaulted_param_deferred_undefined_strip_tests.rs"]
mod defaulted_param_deferred_undefined_strip_tests;
#[path = "tests/definite_assignment_logical_compound_tests.rs"]
mod definite_assignment_logical_compound_tests;
#[path = "tests/destructured_binding_narrowed_property_tests.rs"]
mod destructured_binding_narrowed_property_tests;
#[path = "tests/destructured_discriminant_source_narrowing_tests.rs"]
mod destructured_discriminant_source_narrowing_tests;
#[path = "tests/destructuring_computed_key_any_dynamic_index_tests.rs"]
mod destructuring_computed_key_any_dynamic_index_tests;
#[path = "tests/destructuring_computed_key_error_source_ts2538_tests.rs"]
mod destructuring_computed_key_error_source_ts2538_tests;
#[path = "tests/destructuring_computed_key_index_type_ts2538_tests.rs"]
mod destructuring_computed_key_index_type_ts2538_tests;
#[path = "tests/destructuring_relation_routing_arch_tests.rs"]
mod destructuring_relation_routing_arch_tests;
#[path = "tests/diagnostic_sink_message_surgery_arch_tests.rs"]
mod diagnostic_sink_message_surgery_arch_tests;
#[path = "tests/diagnostic_source_relation_routing_arch_tests.rs"]
mod diagnostic_source_relation_routing_arch_tests;
#[path = "tests/did_you_mean_async_related_tests.rs"]
mod did_you_mean_async_related_tests;
#[path = "tests/direct_generic_return_tests.rs"]
mod direct_generic_return_tests;
#[path = "tests/dispatch_tests.rs"]
mod dispatch_tests;
#[path = "tests/display_keyof_alias_budget_tests.rs"]
mod display_keyof_alias_budget_tests;
#[path = "tests/display_normalization_budget_tests.rs"]
mod display_normalization_budget_tests;
#[path = "tests/do_while_exit_narrowing_tests.rs"]
mod do_while_exit_narrowing_tests;
#[path = "tests/dom_fuel_exhaustion_ts2322_tests.rs"]
mod dom_fuel_exhaustion_ts2322_tests;
#[path = "tests/duplicate_identifier_relation_routing_arch_tests.rs"]
mod duplicate_identifier_relation_routing_arch_tests;
#[path = "tests/duplicate_member_computed_name_ts2300_tests.rs"]
mod duplicate_member_computed_name_ts2300_tests;
#[path = "tests/dynamic_import_relation_routing_arch_tests.rs"]
mod dynamic_import_relation_routing_arch_tests;
#[path = "tests/enclosing_type_param_default_scope_tests.rs"]
mod enclosing_type_param_default_scope_tests;
#[path = "tests/enum_computed_relation_routing_arch_tests.rs"]
mod enum_computed_relation_routing_arch_tests;
#[path = "tests/enum_residual_narrowing_tests.rs"]
mod enum_residual_narrowing_tests;
#[path = "tests/error_reporter_assignability_display_boundary_arch_tests.rs"]
mod error_reporter_assignability_display_boundary_arch_tests;
#[path = "tests/excess_prop_object_union_display_tests.rs"]
mod excess_prop_object_union_display_tests;
#[path = "tests/expando_annotated_receiver_tests.rs"]
mod expando_annotated_receiver_tests;
#[path = "tests/expando_binding_form_eligibility_tests.rs"]
mod expando_binding_form_eligibility_tests;
#[path = "tests/expected_type_from_property_tests.rs"]
mod expected_type_from_property_tests;
#[path = "tests/expected_type_from_return_tests.rs"]
mod expected_type_from_return_tests;
#[path = "tests/explicit_alias_constraint_relation_routing_arch_tests.rs"]
mod explicit_alias_constraint_relation_routing_arch_tests;
#[path = "tests/explicit_type_arg_overload_pruning_tests.rs"]
mod explicit_type_arg_overload_pruning_tests;
#[path = "tests/export_assignment_default_namespace_parse_error_gate_tests.rs"]
mod export_assignment_default_namespace_parse_error_gate_tests;
#[path = "tests/export_declaration_module_element_context_tests.rs"]
mod export_declaration_module_element_context_tests;
#[path = "tests/export_star_default_exclusion_tests.rs"]
mod export_star_default_exclusion_tests;
#[path = "tests/flow_assignment_relation_routing_arch_tests.rs"]
mod flow_assignment_relation_routing_arch_tests;
#[path = "tests/flow_cache_policy_arch_tests.rs"]
mod flow_cache_policy_arch_tests;
#[path = "tests/flow_for_of_destructure_closure_assignment_tests.rs"]
mod flow_for_of_destructure_closure_assignment_tests;
#[path = "tests/flow_guard_boundary_tests.rs"]
mod flow_guard_boundary_tests;
#[path = "tests/flow_inferred_predicate_boundary_tests.rs"]
mod flow_inferred_predicate_boundary_tests;
#[path = "tests/flow_promise_identity_tests.rs"]
mod flow_promise_identity_tests;
#[path = "tests/flow_truthy_proves_assignment_tests.rs"]
mod flow_truthy_proves_assignment_tests;
#[path = "tests/flow_usage_relation_routing_arch_tests.rs"]
mod flow_usage_relation_routing_arch_tests;
#[path = "tests/for_await_non_async_function_body_tests.rs"]
mod for_await_non_async_function_body_tests;
#[path = "tests/for_in_lhs_relation_routing_arch_tests.rs"]
mod for_in_lhs_relation_routing_arch_tests;
#[path = "tests/fresh_const_array_mutable_assignment_tests.rs"]
mod fresh_const_array_mutable_assignment_tests;
#[path = "tests/fresh_object_literal_array_like_union_drill_gate_tests.rs"]
mod fresh_object_literal_array_like_union_drill_gate_tests;
#[path = "tests/fresh_object_literal_union_literal_kind_display_tests.rs"]
mod fresh_object_literal_union_literal_kind_display_tests;
#[path = "tests/fresh_union_property_target_cross_arm_tests.rs"]
mod fresh_union_property_target_cross_arm_tests;
#[path = "tests/function_callee_spread_ts2556_tests.rs"]
mod function_callee_spread_ts2556_tests;
#[path = "tests/function_namespace_merge_property_write_ts2322_tests.rs"]
mod function_namespace_merge_property_write_ts2322_tests;
#[path = "tests/function_parameter_mismatch_elaboration_tests.rs"]
mod function_parameter_mismatch_elaboration_tests;
#[path = "tests/function_type_parameter_grammar_tests.rs"]
mod function_type_parameter_grammar_tests;
#[path = "tests/function_type_relation_routing_arch_tests.rs"]
mod function_type_relation_routing_arch_tests;
#[path = "tests/function_type_return_node_tests.rs"]
mod function_type_return_node_tests;
#[path = "tests/generator_declaration_yield_star_inference_tests.rs"]
mod generator_declaration_yield_star_inference_tests;
#[path = "tests/generator_default_type_argument_relation_tests.rs"]
mod generator_default_type_argument_relation_tests;
#[path = "tests/generator_inferred_yield_star_array_generic_call_tests.rs"]
mod generator_inferred_yield_star_array_generic_call_tests;
#[path = "tests/generator_nested_class_diagnostics_tests.rs"]
mod generator_nested_class_diagnostics_tests;
#[path = "tests/generator_yield_identity_tests.rs"]
mod generator_yield_identity_tests;
#[path = "tests/generator_yield_invalid_thenable_tests.rs"]
mod generator_yield_invalid_thenable_tests;
#[path = "tests/generator_yield_literal_widening_tests.rs"]
mod generator_yield_literal_widening_tests;
#[path = "tests/generator_yield_self_similar_nesting_tests.rs"]
mod generator_yield_self_similar_nesting_tests;
#[path = "tests/generator_yield_star_next_type_tests.rs"]
mod generator_yield_star_next_type_tests;
#[path = "tests/generator_yieldstar_symbol_iterator_contribution_tests.rs"]
mod generator_yieldstar_symbol_iterator_contribution_tests;
#[path = "tests/generator_yieldstar_union_delegate_contribution_tests.rs"]
mod generator_yieldstar_union_delegate_contribution_tests;
#[path = "tests/generic_alias_application_display_tests.rs"]
mod generic_alias_application_display_tests;
#[path = "tests/generic_argument_suppression_relation_routing_arch_tests.rs"]
mod generic_argument_suppression_relation_routing_arch_tests;
#[path = "tests/generic_call_bivariant_callback_nonstrict_tests.rs"]
mod generic_call_bivariant_callback_nonstrict_tests;
#[path = "tests/generic_call_const_asserted_property_widening_tests.rs"]
mod generic_call_const_asserted_property_widening_tests;
#[path = "tests/generic_call_enclosing_type_param_return_tests.rs"]
mod generic_call_enclosing_type_param_return_tests;
#[path = "tests/generic_call_non_fresh_object_widening_tests.rs"]
mod generic_call_non_fresh_object_widening_tests;
#[path = "tests/generic_call_unknown_return_contextual_assignment_tests.rs"]
mod generic_call_unknown_return_contextual_assignment_tests;
#[path = "tests/generic_callable_outer_type_param_mismatch_tests.rs"]
mod generic_callable_outer_type_param_mismatch_tests;
#[path = "tests/generic_callback_outer_context_tests.rs"]
mod generic_callback_outer_context_tests;
#[path = "tests/generic_callback_return_outer_annotation_leak_tests.rs"]
mod generic_callback_return_outer_annotation_leak_tests;
#[path = "tests/generic_callback_sibling_arg_inference_tests.rs"]
mod generic_callback_sibling_arg_inference_tests;
#[path = "tests/generic_callback_union_return_void_body_tests.rs"]
mod generic_callback_union_return_void_body_tests;
#[path = "tests/generic_checker_mod_relation_routing_arch_tests.rs"]
mod generic_checker_mod_relation_routing_arch_tests;
#[path = "tests/generic_class_constructor_literal_preservation_tests.rs"]
mod generic_class_constructor_literal_preservation_tests;
#[path = "tests/generic_class_self_ref_method_param_tests.rs"]
mod generic_class_self_ref_method_param_tests;
#[path = "tests/generic_default_application_arg_preservation_tests.rs"]
mod generic_default_application_arg_preservation_tests;
#[path = "tests/generic_function_param_name_collision_assignability_tests.rs"]
mod generic_function_param_name_collision_assignability_tests;
#[path = "tests/generic_instantiation_boundary_cache_tests.rs"]
mod generic_instantiation_boundary_cache_tests;
#[path = "tests/generic_method_member_variance_assignability_tests.rs"]
mod generic_method_member_variance_assignability_tests;
#[path = "tests/generic_method_override_variance_tests.rs"]
mod generic_method_override_variance_tests;
#[path = "tests/generic_mixed_inheritance_chain_tests.rs"]
mod generic_mixed_inheritance_chain_tests;
#[path = "tests/generic_rest_satisfies_anchor_tests.rs"]
mod generic_rest_satisfies_anchor_tests;
#[path = "tests/generic_rest_tuple_contextual_return_tests.rs"]
mod generic_rest_tuple_contextual_return_tests;
#[path = "tests/generic_signature_context_instantiation_tests.rs"]
mod generic_signature_context_instantiation_tests;
#[path = "tests/generic_spread_iterability_tests.rs"]
mod generic_spread_iterability_tests;
#[path = "tests/generic_unknown_type_arg_tests.rs"]
mod generic_unknown_type_arg_tests;
#[path = "tests/generics_relation_routing_arch_tests.rs"]
mod generics_relation_routing_arch_tests;
#[path = "tests/global_augmentation_computed_key_tests.rs"]
mod global_augmentation_computed_key_tests;
#[path = "tests/global_augmentation_structural_lookup_arch_tests.rs"]
mod global_augmentation_structural_lookup_arch_tests;
#[path = "tests/global_this_typeof_surface_tests.rs"]
mod global_this_typeof_surface_tests;
#[path = "tests/heritage_constraint_structural_name_lookup_arch_tests.rs"]
mod heritage_constraint_structural_name_lookup_arch_tests;
#[path = "tests/heritage_flow_narrowed_base_tests.rs"]
mod heritage_flow_narrowed_base_tests;
#[path = "tests/higher_order_regeneralization_tests.rs"]
mod higher_order_regeneralization_tests;
#[path = "tests/homomorphic_mapped_member_override_tests.rs"]
mod homomorphic_mapped_member_override_tests;
#[path = "tests/identifier_relation_routing_arch_tests.rs"]
mod identifier_relation_routing_arch_tests;
#[path = "tests/import_attributes_relation_routing_arch_tests.rs"]
mod import_attributes_relation_routing_arch_tests;
#[path = "tests/import_shadows_global_ctor_tests.rs"]
mod import_shadows_global_ctor_tests;
#[path = "tests/import_specifier_string_literal_export_name_tests.rs"]
mod import_specifier_string_literal_export_name_tests;
#[path = "tests/imported_predicate_false_branch_tests.rs"]
mod imported_predicate_false_branch_tests;
#[path = "tests/imported_type_reference_raw_symbol_collision_tests.rs"]
mod imported_type_reference_raw_symbol_collision_tests;
#[path = "tests/in_narrow_aliased_union_tests.rs"]
mod in_narrow_aliased_union_tests;
#[path = "tests/in_narrow_apparent_member_tests.rs"]
mod in_narrow_apparent_member_tests;
#[path = "tests/in_narrow_bare_type_param_chained_tests.rs"]
mod in_narrow_bare_type_param_chained_tests;
#[path = "tests/in_operator_relation_routing_arch_tests.rs"]
mod in_operator_relation_routing_arch_tests;
#[path = "tests/index_sig_param_intersection_validity_tests.rs"]
mod index_sig_param_intersection_validity_tests;
#[path = "tests/index_sig_param_resolved_key_type_tests.rs"]
mod index_sig_param_resolved_key_type_tests;
#[path = "tests/index_signature_check_relation_routing_arch_tests.rs"]
mod index_signature_check_relation_routing_arch_tests;
#[path = "tests/index_signature_named_property_relation_tests.rs"]
mod index_signature_named_property_relation_tests;
#[path = "tests/index_signature_nested_object_literal_elaboration_tests.rs"]
mod index_signature_nested_object_literal_elaboration_tests;
#[path = "tests/index_signature_property_relation_routing_arch_tests.rs"]
mod index_signature_property_relation_routing_arch_tests;
#[path = "tests/index_signature_symbol_keyspace_tests.rs"]
mod index_signature_symbol_keyspace_tests;
#[path = "tests/index_signature_value_relation_routing_arch_tests.rs"]
mod index_signature_value_relation_routing_arch_tests;
#[path = "tests/indexed_access_callable_member_constraint_ts2344_tests.rs"]
mod indexed_access_callable_member_constraint_ts2344_tests;
#[path = "tests/indexed_access_constraint_relation_routing_arch_tests.rs"]
mod indexed_access_constraint_relation_routing_arch_tests;
#[path = "tests/infer_conditional_relation_routing_arch_tests.rs"]
mod infer_conditional_relation_routing_arch_tests;
#[path = "tests/inferred_return_object_array_widening_tests.rs"]
mod inferred_return_object_array_widening_tests;
#[path = "tests/initializer_relation_routing_arch_tests.rs"]
mod initializer_relation_routing_arch_tests;
#[path = "tests/instanceof_indexed_access_lhs_tests.rs"]
mod instanceof_indexed_access_lhs_tests;
#[path = "tests/instantiation_expression_inline_utility_modifier_tests.rs"]
mod instantiation_expression_inline_utility_modifier_tests;
#[path = "tests/instantiation_expression_lib_display_tests.rs"]
mod instantiation_expression_lib_display_tests;
#[path = "tests/interface_extends_generic_override_variance_tests.rs"]
mod interface_extends_generic_override_variance_tests;
#[path = "tests/interface_heritage_alias_arg_substitution_tests.rs"]
mod interface_heritage_alias_arg_substitution_tests;
#[path = "tests/interface_heritage_index_relation_routing_arch_tests.rs"]
mod interface_heritage_index_relation_routing_arch_tests;
#[path = "tests/interface_heritage_merge_depth_tests.rs"]
mod interface_heritage_merge_depth_tests;
#[path = "tests/interface_heritage_property_index_relation_routing_arch_tests.rs"]
mod interface_heritage_property_index_relation_routing_arch_tests;
#[path = "tests/interface_index_conflict_relation_routing_arch_tests.rs"]
mod interface_index_conflict_relation_routing_arch_tests;
#[path = "tests/intersection_callable_constraint_ts2344_tests.rs"]
mod intersection_callable_constraint_ts2344_tests;
#[path = "tests/intersection_source_literal_member_display_tests.rs"]
mod intersection_source_literal_member_display_tests;
#[path = "tests/intersection_target_elaboration_tests.rs"]
mod intersection_target_elaboration_tests;
#[path = "tests/invalid_thenable_no_fulfillment_payload_tests.rs"]
mod invalid_thenable_no_fulfillment_payload_tests;
#[path = "tests/invocation_signature_detail_tests.rs"]
mod invocation_signature_detail_tests;
#[path = "tests/issue_9762_literal_init_callback_inference.rs"]
mod issue_9762_literal_init_callback_inference;
#[path = "tests/iterable_next_relation_routing_arch_tests.rs"]
mod iterable_next_relation_routing_arch_tests;
#[path = "tests/iterator_override_widened_value_tests.rs"]
mod iterator_override_widened_value_tests;
#[path = "tests/js_commonjs_default_reexport_ts2305_tests.rs"]
mod js_commonjs_default_reexport_ts2305_tests;
#[path = "tests/js_cross_file_expando_declaration_tests.rs"]
mod js_cross_file_expando_declaration_tests;
#[path = "tests/js_expando_class_expression_prototype_write_tests.rs"]
mod js_expando_class_expression_prototype_write_tests;
#[path = "tests/js_expando_nested_open_host_write_tests.rs"]
mod js_expando_nested_open_host_write_tests;
#[path = "tests/js_expando_nested_prototype_write_callable_tests.rs"]
mod js_expando_nested_prototype_write_callable_tests;
#[path = "tests/js_expando_nested_write_absent_member_tests.rs"]
mod js_expando_nested_write_absent_member_tests;
#[path = "tests/js_expando_order_sensitivity_tests.rs"]
mod js_expando_order_sensitivity_tests;
#[path = "tests/js_file_function_parameters_as_optional_tests.rs"]
mod js_file_function_parameters_as_optional_tests;
#[path = "tests/js_object_literal_private_identifier_ts18016_tests.rs"]
mod js_object_literal_private_identifier_ts18016_tests;
#[path = "tests/js_open_object_property_access_tests.rs"]
mod js_open_object_property_access_tests;
#[path = "tests/js_param_display_required_tests.rs"]
mod js_param_display_required_tests;
#[path = "tests/jsdoc_bare_import_type_tests.rs"]
mod jsdoc_bare_import_type_tests;
#[path = "tests/jsdoc_bare_type_tag_non_prototype_expando_tests.rs"]
mod jsdoc_bare_type_tag_non_prototype_expando_tests;
#[path = "tests/jsdoc_cast_and_define_property_widening_tests.rs"]
mod jsdoc_cast_and_define_property_widening_tests;
#[path = "tests/jsdoc_closure_function_type_tests.rs"]
mod jsdoc_closure_function_type_tests;
#[path = "tests/jsdoc_commonjs_globals_as_type_tests.rs"]
mod jsdoc_commonjs_globals_as_type_tests;
#[path = "tests/jsdoc_empty_augments_class_chain_tests.rs"]
mod jsdoc_empty_augments_class_chain_tests;
#[path = "tests/jsdoc_import_type_constraints_relation_routing_arch_tests.rs"]
mod jsdoc_import_type_constraints_relation_routing_arch_tests;
#[path = "tests/jsdoc_lookup_constraints_relation_routing_arch_tests.rs"]
mod jsdoc_lookup_constraints_relation_routing_arch_tests;
#[path = "tests/jsdoc_nested_type_tag_validation_tests.rs"]
mod jsdoc_nested_type_tag_validation_tests;
#[path = "tests/jsdoc_overload_call_resolution_tests.rs"]
mod jsdoc_overload_call_resolution_tests;
#[path = "tests/jsdoc_retired_tag_diagnostics_tests.rs"]
mod jsdoc_retired_tag_diagnostics_tests;
#[path = "tests/jsdoc_satisfies_duplicate_tag_tests.rs"]
mod jsdoc_satisfies_duplicate_tag_tests;
#[path = "tests/jsdoc_shadowed_type_param_identity_tests.rs"]
mod jsdoc_shadowed_type_param_identity_tests;
#[path = "tests/jsdoc_template_reference_scope_tests.rs"]
mod jsdoc_template_reference_scope_tests;
#[path = "tests/jsdoc_typedef_bare_import_tests.rs"]
mod jsdoc_typedef_bare_import_tests;
#[path = "tests/jsdoc_typedef_commonjs_indirect_export_root_ts2300_tests.rs"]
mod jsdoc_typedef_commonjs_indirect_export_root_ts2300_tests;
#[path = "tests/jsdoc_typedef_distinct_alias_names_tests.rs"]
mod jsdoc_typedef_distinct_alias_names_tests;
#[path = "tests/jsx_children_relation_routing_arch_tests.rs"]
mod jsx_children_relation_routing_arch_tests;
#[path = "tests/jsx_component_props_relation_routing_arch_tests.rs"]
mod jsx_component_props_relation_routing_arch_tests;
#[path = "tests/jsx_element_type_constraint_tests.rs"]
mod jsx_element_type_constraint_tests;
#[path = "tests/jsx_excess_attr_with_spread_display_tests.rs"]
mod jsx_excess_attr_with_spread_display_tests;
#[path = "tests/jsx_generic_spread_relation_routing_arch_tests.rs"]
mod jsx_generic_spread_relation_routing_arch_tests;
#[path = "tests/jsx_overload_relation_routing_arch_tests.rs"]
mod jsx_overload_relation_routing_arch_tests;
#[path = "tests/jsx_props_resolution_relation_routing_arch_tests.rs"]
mod jsx_props_resolution_relation_routing_arch_tests;
#[path = "tests/jsx_props_validation_relation_routing_arch_tests.rs"]
mod jsx_props_validation_relation_routing_arch_tests;
#[path = "tests/jsx_react_alias_relation_routing_arch_tests.rs"]
mod jsx_react_alias_relation_routing_arch_tests;
#[path = "tests/jsx_render_fallback_relation_routing_arch_tests.rs"]
mod jsx_render_fallback_relation_routing_arch_tests;
#[path = "tests/jsx_return_relation_routing_arch_tests.rs"]
mod jsx_return_relation_routing_arch_tests;
#[path = "tests/jsx_single_child_precise_relation_routing_arch_tests.rs"]
mod jsx_single_child_precise_relation_routing_arch_tests;
#[path = "tests/jsx_spread_assignability_relation_routing_arch_tests.rs"]
mod jsx_spread_assignability_relation_routing_arch_tests;
#[path = "tests/jsx_text_child_relation_routing_arch_tests.rs"]
mod jsx_text_child_relation_routing_arch_tests;
#[path = "tests/jsx_type_arg_arity_suppresses_ts2604_tests.rs"]
mod jsx_type_arg_arity_suppresses_ts2604_tests;
#[path = "tests/jsx_union_props_relation_routing_arch_tests.rs"]
mod jsx_union_props_relation_routing_arch_tests;
#[path = "tests/jump_statement_class_member_boundary_tests.rs"]
mod jump_statement_class_member_boundary_tests;
#[path = "tests/jump_statement_return_path_analysis_tests.rs"]
mod jump_statement_return_path_analysis_tests;
#[path = "tests/keyof_alias_composite_display_tests.rs"]
mod keyof_alias_composite_display_tests;
#[path = "tests/keyof_suppression_relation_routing_arch_tests.rs"]
mod keyof_suppression_relation_routing_arch_tests;
#[path = "tests/keyof_type_parameter_deferred_heritage_tests.rs"]
mod keyof_type_parameter_deferred_heritage_tests;
#[path = "tests/lazy_lib_fuel_determinism_tests.rs"]
mod lazy_lib_fuel_determinism_tests;
#[path = "tests/lazy_lib_heritage_guard_tests.rs"]
mod lazy_lib_heritage_guard_tests;
#[path = "tests/lazy_lib_member_access_tests.rs"]
mod lazy_lib_member_access_tests;
#[path = "tests/lib_abstract_member_ts2515_tests.rs"]
mod lib_abstract_member_ts2515_tests;
#[path = "tests/libtype_structural_name_lookup_arch_tests.rs"]
mod libtype_structural_name_lookup_arch_tests;
#[path = "tests/literal_spelled_computed_key_index_signature_code_tests.rs"]
mod literal_spelled_computed_key_index_signature_code_tests;
#[path = "tests/local_type_alias_shadowing_tests.rs"]
mod local_type_alias_shadowing_tests;
#[path = "tests/local_type_vs_type_parameter_declaration_space_tests.rs"]
mod local_type_vs_type_parameter_declaration_space_tests;
#[path = "tests/logical_assignment_member_narrowing_tests.rs"]
mod logical_assignment_member_narrowing_tests;
#[path = "tests/loop_self_referential_property_read_tests.rs"]
mod loop_self_referential_property_read_tests;
#[path = "tests/mapped_conditional_infer_false_branch_canonical_tests.rs"]
mod mapped_conditional_infer_false_branch_canonical_tests;
#[path = "tests/mapped_infer_with_substitution_tests.rs"]
mod mapped_infer_with_substitution_tests;
#[path = "tests/mapped_intersection_excess_property_tests.rs"]
mod mapped_intersection_excess_property_tests;
#[path = "tests/mapped_keyof_remap_excess_property_tests.rs"]
mod mapped_keyof_remap_excess_property_tests;
#[path = "tests/mapped_optional_target_excess_property_tests.rs"]
mod mapped_optional_target_excess_property_tests;
#[path = "tests/mapped_true_base_constraint_relation_routing_arch_tests.rs"]
mod mapped_true_base_constraint_relation_routing_arch_tests;
#[path = "tests/member_assignment_narrowing_join_tests.rs"]
mod member_assignment_narrowing_join_tests;
#[path = "tests/member_modifier_placement_grammar_tests.rs"]
mod member_modifier_placement_grammar_tests;
#[path = "tests/member_name_source_quote_fidelity_tests.rs"]
mod member_name_source_quote_fidelity_tests;
#[path = "tests/merged_interface_constraint_relation_routing_arch_tests.rs"]
mod merged_interface_constraint_relation_routing_arch_tests;
#[path = "tests/merged_interface_construct_order_tests.rs"]
mod merged_interface_construct_order_tests;
#[path = "tests/merged_interface_overload_order_tests.rs"]
mod merged_interface_overload_order_tests;
#[path = "tests/merged_interface_reference_flat_missing_tests.rs"]
mod merged_interface_reference_flat_missing_tests;
#[path = "tests/method_return_type_elaboration_tests.rs"]
mod method_return_type_elaboration_tests;
#[path = "tests/missing_property_base_class_head_tests.rs"]
mod missing_property_base_class_head_tests;
#[path = "tests/missing_property_declared_here_tests.rs"]
mod missing_property_declared_here_tests;
#[path = "tests/missing_property_symbol_members_object_fallback_tests.rs"]
mod missing_property_symbol_members_object_fallback_tests;
#[path = "tests/module_scoped_var_shadows_lib_global_ts2300_tests.rs"]
mod module_scoped_var_shadows_lib_global_ts2300_tests;
#[path = "tests/multi_overload_infer_capture_tests.rs"]
mod multi_overload_infer_capture_tests;
#[path = "tests/mutable_binding_widening_from_const_literal_tests.rs"]
mod mutable_binding_widening_from_const_literal_tests;
#[path = "tests/namespace_body_reachability_tests.rs"]
mod namespace_body_reachability_tests;
#[path = "tests/namespace_property_mismatch_boundary_arch_tests.rs"]
mod namespace_property_mismatch_boundary_arch_tests;
#[path = "tests/narrowed_union_source_display_tests.rs"]
mod narrowed_union_source_display_tests;
#[path = "tests/narrowing_union_source_display_tests.rs"]
mod narrowing_union_source_display_tests;
#[path = "tests/nested_function_async_context_scope_tests.rs"]
mod nested_function_async_context_scope_tests;
#[path = "tests/nested_generic_call_return_context_baked_tests.rs"]
mod nested_generic_call_return_context_baked_tests;
#[path = "tests/nested_tuple_literal_source_display_tests.rs"]
mod nested_tuple_literal_source_display_tests;
#[path = "tests/nested_type_parameter_target_elaboration_tests.rs"]
mod nested_type_parameter_target_elaboration_tests;
#[path = "tests/never_indexed_access_reduction_tests.rs"]
mod never_indexed_access_reduction_tests;
#[path = "tests/never_return_import_alias_tests.rs"]
mod never_return_import_alias_tests;
#[path = "tests/no_implicit_override_ambient_context_tests.rs"]
mod no_implicit_override_ambient_context_tests;
#[path = "tests/no_index_element_implicit_any_tests.rs"]
mod no_index_element_implicit_any_tests;
#[path = "tests/nolib_user_global_array_member_tests.rs"]
mod nolib_user_global_array_member_tests;
#[path = "tests/non_generic_spread_tuple_alias_display_tests.rs"]
mod non_generic_spread_tuple_alias_display_tests;
#[path = "tests/non_strict_non_null_check_narrows_tests.rs"]
mod non_strict_non_null_check_narrows_tests;
#[path = "tests/non_strict_nullish_return_widening_tests.rs"]
mod non_strict_nullish_return_widening_tests;
#[path = "tests/nonstrict_nullish_union_reduction_tests.rs"]
mod nonstrict_nullish_union_reduction_tests;
#[path = "tests/nonstrict_nullish_widening_generic_call_tests.rs"]
mod nonstrict_nullish_widening_generic_call_tests;
#[path = "tests/nonstrict_nullish_widening_mutable_binding_tests.rs"]
mod nonstrict_nullish_widening_mutable_binding_tests;
#[path = "tests/nonstrict_nullish_widening_nested_leaf_tests.rs"]
mod nonstrict_nullish_widening_nested_leaf_tests;
#[path = "tests/nonstrict_return_union_nullish_reduction_tests.rs"]
mod nonstrict_return_union_nullish_reduction_tests;
#[path = "tests/nonstrict_union_nullish_scalar_reduction_tests.rs"]
mod nonstrict_union_nullish_scalar_reduction_tests;
#[path = "tests/nonunique_symbol_property_access_tests.rs"]
mod nonunique_symbol_property_access_tests;
#[path = "tests/noUIA_any_index_emits_ts2322_tests.rs"]
mod nuia_any_index_emits_ts2322_tests;
#[path = "tests/noUIA_write_index_signature_emits_ts2322_tests.rs"]
mod nuia_write_index_signature_emits_ts2322_tests;
#[path = "tests/nullable_union_callback_variance_tests.rs"]
mod nullable_union_callback_variance_tests;
#[path = "tests/nullish_target_relation_routing_arch_tests.rs"]
mod nullish_target_relation_routing_arch_tests;
#[path = "tests/nullish_union_indexed_access_missing_property_tests.rs"]
mod nullish_union_indexed_access_missing_property_tests;
#[path = "tests/object_define_property_identity_tests.rs"]
mod object_define_property_identity_tests;
#[path = "tests/object_global_identity_helper_tests.rs"]
mod object_global_identity_helper_tests;
#[path = "tests/object_literal_computed_symbol_member_tests.rs"]
mod object_literal_computed_symbol_member_tests;
#[path = "tests/object_literal_enclosing_this_type_marker_tests.rs"]
mod object_literal_enclosing_this_type_marker_tests;
#[path = "tests/object_literal_forward_method_return_type_tests.rs"]
mod object_literal_forward_method_return_type_tests;
#[path = "tests/object_literal_method_body_check_tests.rs"]
mod object_literal_method_body_check_tests;
#[path = "tests/object_literal_method_this_parameter_contextual_tests.rs"]
mod object_literal_method_this_parameter_contextual_tests;
#[path = "tests/object_literal_relation_architecture_tests.rs"]
mod object_literal_relation_architecture_tests;
#[path = "tests/object_literal_this_member_order_tests.rs"]
mod object_literal_this_member_order_tests;
#[path = "tests/object_property_arrow_param_annotation_elaboration_tests.rs"]
mod object_property_arrow_param_annotation_elaboration_tests;
#[path = "tests/object_shorthand_literal_preservation_tests.rs"]
mod object_shorthand_literal_preservation_tests;
#[path = "tests/object_spread_discriminant_narrowing_tests.rs"]
mod object_spread_discriminant_narrowing_tests;
#[path = "tests/object_spread_optional_merge_tests.rs"]
mod object_spread_optional_merge_tests;
#[path = "tests/operator_chain_overload_resolution_tests.rs"]
mod operator_chain_overload_resolution_tests;
#[path = "tests/optional_chain_inherent_nullish_tests.rs"]
mod optional_chain_inherent_nullish_tests;
#[path = "tests/optional_chain_parenthesized_target_tests.rs"]
mod optional_chain_parenthesized_target_tests;
#[path = "tests/optional_chain_read_before_write_tests.rs"]
mod optional_chain_read_before_write_tests;
#[path = "tests/optional_chain_root_nullish_strict_only_tests.rs"]
mod optional_chain_root_nullish_strict_only_tests;
#[path = "tests/optional_chain_write_target_nullish_tests.rs"]
mod optional_chain_write_target_nullish_tests;
#[path = "tests/optional_key_extraction_tests.rs"]
mod optional_key_extraction_tests;
#[path = "tests/optional_private_field_undefined_tests.rs"]
mod optional_private_field_undefined_tests;
#[path = "tests/optional_property_union_source_elaboration_tests.rs"]
mod optional_property_union_source_elaboration_tests;
#[path = "tests/overlap_relation_helper_routing_arch_tests.rs"]
mod overlap_relation_helper_routing_arch_tests;
#[path = "tests/overload_anchor_at_argument_tests.rs"]
mod overload_anchor_at_argument_tests;
#[path = "tests/overload_argument_reason_chain_tests.rs"]
mod overload_argument_reason_chain_tests;
#[path = "tests/overload_arity_expanded_spread_count_tests.rs"]
mod overload_arity_expanded_spread_count_tests;
#[path = "tests/overload_elaboration_tests.rs"]
mod overload_elaboration_tests;
#[path = "tests/overload_generic_wrapper_compat_tests.rs"]
mod overload_generic_wrapper_compat_tests;
#[path = "tests/overload_last_candidate_elaborated_anchor_tests.rs"]
mod overload_last_candidate_elaborated_anchor_tests;
#[path = "tests/overload_literal_source_generalization_tests.rs"]
mod overload_literal_source_generalization_tests;
#[path = "tests/overload_param_relation_routing_arch_tests.rs"]
mod overload_param_relation_routing_arch_tests;
#[path = "tests/overload_two_pass_any_source_tests.rs"]
mod overload_two_pass_any_source_tests;
#[path = "tests/overload_union_context_callback_tests.rs"]
mod overload_union_context_callback_tests;
#[path = "tests/overloaded_callable_param_no_implicit_any_tests.rs"]
mod overloaded_callable_param_no_implicit_any_tests;
#[path = "tests/overloaded_contextual_rest_tuple_tests.rs"]
mod overloaded_contextual_rest_tuple_tests;
#[path = "tests/override_incompatibility_elaboration_tests.rs"]
mod override_incompatibility_elaboration_tests;
#[path = "tests/parameter_checker_tests.rs"]
mod parameter_checker_tests;
#[path = "tests/partial_pick_indexed_access_write_tests.rs"]
mod partial_pick_indexed_access_write_tests;
#[path = "tests/polymorphic_this_relation_routing_arch_tests.rs"]
mod polymorphic_this_relation_routing_arch_tests;
#[path = "tests/position_invalid_default_export_expression_tests.rs"]
mod position_invalid_default_export_expression_tests;
#[path = "tests/predicate_narrowed_lib_union_access_tests.rs"]
mod predicate_narrowed_lib_union_access_tests;
#[path = "tests/predicate_narrowed_top_type_source_display_tests.rs"]
mod predicate_narrowed_top_type_source_display_tests;
#[path = "tests/predicate_narrowed_unknown_any_source_display_tests.rs"]
mod predicate_narrowed_unknown_any_source_display_tests;
#[path = "tests/private_field_no_spelling_suggestion_tests.rs"]
mod private_field_no_spelling_suggestion_tests;
#[path = "tests/private_member_relation_routing_arch_tests.rs"]
mod private_member_relation_routing_arch_tests;
#[path = "tests/private_name_modifier_grammar_order_tests.rs"]
mod private_name_modifier_grammar_order_tests;
#[path = "tests/private_optional_field_undefined_tests.rs"]
mod private_optional_field_undefined_tests;
#[path = "tests/promise_like_infer_tests.rs"]
mod promise_like_infer_tests;
#[path = "tests/promise_this_relation_routing_arch_tests.rs"]
mod promise_this_relation_routing_arch_tests;
#[path = "tests/property_alias_display_tests.rs"]
mod property_alias_display_tests;
#[path = "tests/property_index_key_relation_routing_arch_tests.rs"]
mod property_index_key_relation_routing_arch_tests;
#[path = "tests/property_receiver_display_recursion_overflow_tests.rs"]
mod property_receiver_display_recursion_overflow_tests;
#[path = "tests/property_receiver_relation_routing_arch_tests.rs"]
mod property_receiver_relation_routing_arch_tests;
#[path = "tests/reachability_if_no_else_const_condition_tests.rs"]
mod reachability_if_no_else_const_condition_tests;
#[path = "tests/reachability_labeled_break_completion_tests.rs"]
mod reachability_labeled_break_completion_tests;
#[path = "tests/readonly_assignment_no_flow_narrow_tests.rs"]
mod readonly_assignment_no_flow_narrow_tests;
#[path = "tests/readonly_property_assignment_narrowing_tests.rs"]
mod readonly_property_assignment_narrowing_tests;
#[path = "tests/recursive_accumulator_depth_tests.rs"]
mod recursive_accumulator_depth_tests;
#[path = "tests/recursive_callable_infer_cycle_tests.rs"]
mod recursive_callable_infer_cycle_tests;
#[path = "tests/recursive_conditional_infer_termination_tests.rs"]
mod recursive_conditional_infer_termination_tests;
#[path = "tests/recursive_conditional_tuple_spread_display_tests.rs"]
mod recursive_conditional_tuple_spread_display_tests;
#[path = "tests/recursive_generic_arrow_tests.rs"]
mod recursive_generic_arrow_tests;
#[path = "tests/recursive_mapped_intersection_nested_excess_property_tests.rs"]
mod recursive_mapped_intersection_nested_excess_property_tests;
#[path = "tests/recursive_path_default_type_param_tests.rs"]
mod recursive_path_default_type_param_tests;
#[path = "tests/recursive_tuple_alias_diagnostic_display_tests.rs"]
mod recursive_tuple_alias_diagnostic_display_tests;
#[path = "tests/recursive_tuple_rest_cycle_tests.rs"]
mod recursive_tuple_rest_cycle_tests;
#[path = "tests/reexport_default_esmoduleinterop_commonjs_tests.rs"]
mod reexport_default_esmoduleinterop_commonjs_tests;
#[path = "tests/reexport_resolution_cache_tests.rs"]
mod reexport_resolution_cache_tests;
#[path = "tests/reexported_generic_interface_property_tests.rs"]
mod reexported_generic_interface_property_tests;
#[path = "tests/ref_type_params_cache_tests.rs"]
mod ref_type_params_cache_tests;
#[path = "tests/relation_flags_boundary_contract_tests.rs"]
mod relation_flags_boundary_contract_tests;
#[path = "tests/remapped_missing_property_relation_routing_arch_tests.rs"]
mod remapped_missing_property_relation_routing_arch_tests;
#[path = "tests/rest_parameter_relation_routing_arch_tests.rs"]
mod rest_parameter_relation_routing_arch_tests;
#[path = "tests/return_alias_unknown_eval_assignability_tests.rs"]
mod return_alias_unknown_eval_assignability_tests;
#[path = "tests/return_context_promise_identity_tests.rs"]
mod return_context_promise_identity_tests;
#[path = "tests/return_context_type_param_shadowing_tests.rs"]
mod return_context_type_param_shadowing_tests;
#[path = "tests/return_relation_routing_arch_tests.rs"]
mod return_relation_routing_arch_tests;
#[path = "tests/satisfies_callback_return_widening_tests.rs"]
mod satisfies_callback_return_widening_tests;
#[path = "tests/satisfies_relation_routing_arch_tests.rs"]
mod satisfies_relation_routing_arch_tests;
#[path = "tests/self_referential_arrow_property_soundness_tests.rs"]
mod self_referential_arrow_property_soundness_tests;
#[path = "tests/self_referential_conditional_infer_soundness_tests.rs"]
mod self_referential_conditional_infer_soundness_tests;
#[path = "tests/semantic_def_body_read_arch_tests.rs"]
mod semantic_def_body_read_arch_tests;
#[path = "tests/shadowed_type_param_identity_tests.rs"]
mod shadowed_type_param_identity_tests;
#[path = "tests/split_accessor_variance_tests.rs"]
mod split_accessor_variance_tests;
#[path = "tests/spread_array_rest_param_inference_tests.rs"]
mod spread_array_rest_param_inference_tests;
#[path = "tests/spurious_suggestion_suppression_tests.rs"]
mod spurious_suggestion_suppression_tests;
#[path = "tests/state_type_environment_relation_routing_arch_tests.rs"]
mod state_type_environment_relation_routing_arch_tests;
#[path = "tests/strict_callback_param_method_tests.rs"]
mod strict_callback_param_method_tests;
#[path = "tests/strict_mode_class_context_name_tests.rs"]
mod strict_mode_class_context_name_tests;
#[path = "tests/string_literal_union_display_order_tests.rs"]
mod string_literal_union_display_order_tests;
#[path = "tests/suggestion_scan_discarded_tests.rs"]
mod suggestion_scan_discarded_tests;
#[path = "tests/super_call_ts2376_ts17009_priority_tests.rs"]
mod super_call_ts2376_ts17009_priority_tests;
#[path = "tests/super_order_intra_statement_ts17009_tests.rs"]
mod super_order_intra_statement_ts17009_tests;
#[path = "tests/switch_distinct_literal_memo_narrowing_tests.rs"]
mod switch_distinct_literal_memo_narrowing_tests;
#[path = "tests/symbol_env_registration_arch_tests.rs"]
mod symbol_env_registration_arch_tests;
#[path = "tests/symbol_for_identity_helper_tests.rs"]
mod symbol_for_identity_helper_tests;
#[path = "tests/syntax_constraint_relation_routing_arch_tests.rs"]
mod syntax_constraint_relation_routing_arch_tests;
#[path = "tests/synthetic_default_ts_source_gate_tests.rs"]
mod synthetic_default_ts_source_gate_tests;
#[path = "tests/synthetic_unique_atom_union_display_tests.rs"]
mod synthetic_unique_atom_union_display_tests;
#[path = "tests/this_context_self_type_tests.rs"]
mod this_context_self_type_tests;
#[path = "tests/this_lexical_global_type_tests.rs"]
mod this_lexical_global_type_tests;
#[path = "tests/this_parameter_placement_tests.rs"]
mod this_parameter_placement_tests;
#[path = "tests/this_prop_nullish_operand_code_tests.rs"]
mod this_prop_nullish_operand_code_tests;
#[path = "tests/this_source_inference_tests.rs"]
mod this_source_inference_tests;
#[path = "tests/this_void_method_call_tests.rs"]
mod this_void_method_call_tests;
#[path = "tests/top_level_await_boundary_tests.rs"]
mod top_level_await_boundary_tests;
#[path = "tests/truthiness_promise_coercion_tests.rs"]
mod truthiness_promise_coercion_tests;
#[path = "tests/ts1064_awaited_suggestion_tests.rs"]
mod ts1064_awaited_suggestion_tests;
#[path = "tests/ts1101_with_in_strict_mode_tests.rs"]
mod ts1101_with_in_strict_mode_tests;
#[path = "tests/ts1165_ambient_class_method_computed_name_tests.rs"]
mod ts1165_ambient_class_method_computed_name_tests;
#[path = "tests/ts1168_method_overload_computed_name_tests.rs"]
mod ts1168_method_overload_computed_name_tests;
#[path = "tests/ts1170_computed_property_syntactic_form_tests.rs"]
mod ts1170_computed_property_syntactic_form_tests;
#[path = "tests/ts1250_ts1251_ts1252_strict_function_in_block_tests.rs"]
mod ts1250_ts1251_ts1252_strict_function_in_block_tests;
#[path = "tests/ts1293_preserve_isolated_modules_esm_syntax_in_cjs_tests.rs"]
mod ts1293_preserve_isolated_modules_esm_syntax_in_cjs_tests;
#[path = "tests/ts1295_empty_named_clause_cjs_exempt_tests.rs"]
mod ts1295_empty_named_clause_cjs_exempt_tests;
#[path = "tests/ts1309_top_level_await_commonjs_file_tests.rs"]
mod ts1309_top_level_await_commonjs_file_tests;
#[path = "tests/ts1318_abstract_accessor_implementation_tests.rs"]
mod ts1318_abstract_accessor_implementation_tests;
#[path = "tests/ts1361_ambient_computed_property_name_tests.rs"]
mod ts1361_ambient_computed_property_name_tests;
#[path = "tests/ts1539_bigint_literal_property_name_tests.rs"]
mod ts1539_bigint_literal_property_name_tests;
#[path = "tests/ts18010_jsdoc_tag_anchor_tests.rs"]
mod ts18010_jsdoc_tag_anchor_tests;
#[path = "tests/ts18017_ts18018_private_identifier_shadow_related_info_tests.rs"]
mod ts18017_ts18018_private_identifier_shadow_related_info_tests;
#[path = "tests/ts18031_intersection_conflicting_property_tests.rs"]
mod ts18031_intersection_conflicting_property_tests;
#[path = "tests/ts18032_intersection_private_brand_conflict_tests.rs"]
mod ts18032_intersection_private_brand_conflict_tests;
#[path = "tests/ts18050_nullish_keyword_without_strict_null_checks_tests.rs"]
mod ts18050_nullish_keyword_without_strict_null_checks_tests;
#[path = "tests/ts2300_class_static_overload_namespace_export_merge_tests.rs"]
mod ts2300_class_static_overload_namespace_export_merge_tests;
#[path = "tests/ts2322_private_field_narrowing_write_tests.rs"]
mod ts2322_private_field_narrowing_write_tests;
#[path = "tests/ts2322_readonly_array_element_elaboration_tests.rs"]
mod ts2322_readonly_array_element_elaboration_tests;
#[path = "tests/ts2322_same_generic_type_argument_elaboration_tests.rs"]
mod ts2322_same_generic_type_argument_elaboration_tests;
#[path = "tests/ts2323_block_scoped_conflict_message_tests.rs"]
mod ts2323_block_scoped_conflict_message_tests;
#[path = "tests/ts2323_default_export_duplicate_implementation_tests.rs"]
mod ts2323_default_export_duplicate_implementation_tests;
#[path = "tests/ts2323_export_var_namespace_merge_tests.rs"]
mod ts2323_export_var_namespace_merge_tests;
#[path = "tests/ts2323_mixed_exportedness_two_table_tests.rs"]
mod ts2323_mixed_exportedness_two_table_tests;
#[path = "tests/ts2323_variable_redeclaration_two_pass_tests.rs"]
mod ts2323_variable_redeclaration_two_pass_tests;
#[path = "tests/ts2339_js_this_function_name_display_tests.rs"]
mod ts2339_js_this_function_name_display_tests;
#[path = "tests/ts2339_private_access_via_constructor_type_tests.rs"]
mod ts2339_private_access_via_constructor_type_tests;
#[path = "tests/ts2341_private_access_via_type_param_constraint_tests.rs"]
mod ts2341_private_access_via_type_param_constraint_tests;
#[path = "tests/ts2345_fresh_literal_union_argument_head_display_tests.rs"]
mod ts2345_fresh_literal_union_argument_head_display_tests;
#[path = "tests/ts2345_generic_call_concrete_alias_parameter_display_tests.rs"]
mod ts2345_generic_call_concrete_alias_parameter_display_tests;
#[path = "tests/ts2345_private_brand_argument_elaboration_tests.rs"]
mod ts2345_private_brand_argument_elaboration_tests;
#[path = "tests/ts2345_unknown_argument_assignability_tests.rs"]
mod ts2345_unknown_argument_assignability_tests;
#[path = "tests/ts2353_generic_constraint_tests.rs"]
mod ts2353_generic_constraint_tests;
#[path = "tests/ts2383_overload_flag_agreement_tests.rs"]
mod ts2383_overload_flag_agreement_tests;
#[path = "tests/ts2391_default_export_overload_group_tests.rs"]
mod ts2391_default_export_overload_group_tests;
#[path = "tests/ts2393_namespace_reopened_duplicate_implementation_tests.rs"]
mod ts2393_namespace_reopened_duplicate_implementation_tests;
#[path = "tests/ts2445_protected_access_via_subclass_this_tests.rs"]
mod ts2445_protected_access_via_subclass_this_tests;
#[path = "tests/ts2507_extends_non_constructor_value_base_tests.rs"]
mod ts2507_extends_non_constructor_value_base_tests;
#[path = "tests/ts2515_ambient_class_abstract_member_tests.rs"]
mod ts2515_ambient_class_abstract_member_tests;
#[path = "tests/ts2528_default_export_function_overload_tests.rs"]
mod ts2528_default_export_function_overload_tests;
#[path = "tests/ts2528_default_export_interface_merge_tests.rs"]
mod ts2528_default_export_interface_merge_tests;
#[path = "tests/ts2536_deferred_conditional_indexed_access_tests.rs"]
mod ts2536_deferred_conditional_indexed_access_tests;
#[path = "tests/ts2536_error_type_contagion_tests.rs"]
mod ts2536_error_type_contagion_tests;
#[path = "tests/ts2536_nested_generic_indexed_access_constraint_tests.rs"]
mod ts2536_nested_generic_indexed_access_constraint_tests;
#[path = "tests/ts2564_constructor_throw_guard_tests.rs"]
mod ts2564_constructor_throw_guard_tests;
#[path = "tests/ts2565_jsdoc_prototype_type_decl_tests.rs"]
mod ts2565_jsdoc_prototype_type_decl_tests;
#[path = "tests/ts2565_object_literal_nullish_spread_tests.rs"]
mod ts2565_object_literal_nullish_spread_tests;
#[path = "tests/ts2574_rest_tuple_element_type_tests.rs"]
mod ts2574_rest_tuple_element_type_tests;
#[path = "tests/ts2590_array_literal_identity_skip_tests.rs"]
mod ts2590_array_literal_identity_skip_tests;
#[path = "tests/ts2591_node_global_type_position_tests.rs"]
mod ts2591_node_global_type_position_tests;
#[path = "tests/ts2693_signature_binding_shadowed_primitive_tests.rs"]
mod ts2693_signature_binding_shadowed_primitive_tests;
#[path = "tests/ts2739_alias_unfold_display_tests.rs"]
mod ts2739_alias_unfold_display_tests;
#[path = "tests/ts7031_destructuring_nullish_array_literal_tests.rs"]
mod ts7031_destructuring_nullish_array_literal_tests;
#[path = "tests/ts7053_apparent_receiver_display_tests.rs"]
mod ts7053_apparent_receiver_display_tests;
#[path = "tests/ts7053_index_reason_chain_tests.rs"]
mod ts7053_index_reason_chain_tests;
#[path = "tests/ts7053_js_constructor_element_access_literal_like_tests.rs"]
mod ts7053_js_constructor_element_access_literal_like_tests;
#[path = "tests/ts8030_jsdoc_type_tag_message_tests.rs"]
mod ts8030_jsdoc_type_tag_message_tests;
#[path = "tests/tuple_spread_flattening_tests.rs"]
mod tuple_spread_flattening_tests;
#[path = "tests/type_alias_computed_display_tests.rs"]
mod type_alias_computed_display_tests;
#[path = "tests/type_alias_primitive_display_tests.rs"]
mod type_alias_primitive_display_tests;
#[path = "tests/type_analysis_env_merge_arch_tests.rs"]
mod type_analysis_env_merge_arch_tests;
#[path = "tests/type_param_default_relation_routing_arch_tests.rs"]
mod type_param_default_relation_routing_arch_tests;
#[path = "tests/type_parameter_default_identity_tests.rs"]
mod type_parameter_default_identity_tests;
#[path = "tests/type_position_resolution_cache_tests.rs"]
mod type_position_resolution_cache_tests;
#[path = "tests/type_predicate_alias_relation_tests.rs"]
mod type_predicate_alias_relation_tests;
#[path = "tests/type_predicate_assignability_elaboration_tests.rs"]
mod type_predicate_assignability_elaboration_tests;
#[path = "tests/type_predicate_relation_routing_arch_tests.rs"]
mod type_predicate_relation_routing_arch_tests;
#[path = "tests/typeof_class_name_structural_lookup_arch_tests.rs"]
mod typeof_class_name_structural_lookup_arch_tests;
#[path = "tests/typeof_const_spread_index_access_tests.rs"]
mod typeof_const_spread_index_access_tests;
#[path = "tests/typeof_unknown_logical_narrowing_tests.rs"]
mod typeof_unknown_logical_narrowing_tests;
#[path = "tests/under_applied_generic_constructor_fill_tests.rs"]
mod under_applied_generic_constructor_fill_tests;
#[path = "tests/union_call_resolution_tests.rs"]
mod union_call_resolution_tests;
#[path = "tests/union_constraint_relation_routing_arch_tests.rs"]
mod union_constraint_relation_routing_arch_tests;
#[path = "tests/union_display_longhand_primitive_repaint_tests.rs"]
mod union_display_longhand_primitive_repaint_tests;
#[path = "tests/union_excess_property_relation_routing_arch_tests.rs"]
mod union_excess_property_relation_routing_arch_tests;
#[path = "tests/union_index_signature_kind_subtype_reduction_tests.rs"]
mod union_index_signature_kind_subtype_reduction_tests;
#[path = "tests/union_index_signature_relation_routing_arch_tests.rs"]
mod union_index_signature_relation_routing_arch_tests;
#[path = "tests/union_multi_overload_unified_sig_tests.rs"]
mod union_multi_overload_unified_sig_tests;
#[path = "tests/union_source_literal_target_display_tests.rs"]
mod union_source_literal_target_display_tests;
#[path = "tests/union_to_tuple_never_base_depth_tests.rs"]
mod union_to_tuple_never_base_depth_tests;
#[path = "tests/unique_symbol_assignment_ts2322_tests.rs"]
mod unique_symbol_assignment_ts2322_tests;
#[path = "tests/unique_symbol_member_lookup_family_tests.rs"]
mod unique_symbol_member_lookup_family_tests;
#[path = "tests/unresolved_def_eval_cache_backstop_tests.rs"]
mod unresolved_def_eval_cache_backstop_tests;
#[path = "tests/unused_typeof_type_query_reference_tests.rs"]
mod unused_typeof_type_query_reference_tests;
#[path = "tests/using_declaration_implicit_any_tests.rs"]
mod using_declaration_implicit_any_tests;
#[path = "tests/variadic_tuple_alias_display_tests.rs"]
mod variadic_tuple_alias_display_tests;
#[path = "tests/variadic_tuple_constraint_literal_preservation_tests.rs"]
mod variadic_tuple_constraint_literal_preservation_tests;
#[path = "tests/variadic_tuple_spread_element_inference_tests.rs"]
mod variadic_tuple_spread_element_inference_tests;
#[path = "tests/variance_property_function_bivariance_tests.rs"]
mod variance_property_function_bivariance_tests;
#[path = "tests/verbatim_module_syntax_export_default_alias_ts1284_tests.rs"]
mod verbatim_module_syntax_export_default_alias_ts1284_tests;
#[path = "tests/verbatim_module_syntax_export_default_type_only_import_ts1284_tests.rs"]
mod verbatim_module_syntax_export_default_type_only_import_ts1284_tests;
#[path = "tests/verbatim_module_syntax_export_default_type_only_reexport_ts1290_tests.rs"]
mod verbatim_module_syntax_export_default_type_only_reexport_ts1290_tests;
#[path = "tests/verbatim_module_syntax_export_equals_require_ts1282_ts1283_tests.rs"]
mod verbatim_module_syntax_export_equals_require_ts1282_ts1283_tests;
#[path = "tests/verbatim_module_syntax_export_equals_ts1291_tests.rs"]
mod verbatim_module_syntax_export_equals_ts1291_tests;
#[path = "tests/verbatim_module_syntax_reexport_chain_ts1484_ts1485_tests.rs"]
mod verbatim_module_syntax_reexport_chain_ts1484_ts1485_tests;
#[path = "tests/void_undefined_discriminant_narrowing_tests.rs"]
mod void_undefined_discriminant_narrowing_tests;
#[path = "tests/well_known_symbol_alias_member_tests.rs"]
mod well_known_symbol_alias_member_tests;
#[path = "tests/window_self_globalthis_resolution_tests.rs"]
mod window_self_globalthis_resolution_tests;
#[path = "tests/yield_grammar_computed_property_name_tests.rs"]
mod yield_grammar_computed_property_name_tests;
#[path = "tests/yieldstar_async_iterable_invalid_thenable_element_tests.rs"]
mod yieldstar_async_iterable_invalid_thenable_element_tests;
#[path = "tests/zod_type_query_regression_tests.rs"]
mod zod_type_query_regression_tests;
