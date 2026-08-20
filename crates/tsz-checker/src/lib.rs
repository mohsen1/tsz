//! Type checker module for TypeScript AST.
//!
//! This module is organized into several submodules:
//! - `context` - `CheckerContext` for shared state
//! - `dispatch` - Expression type computation dispatcher (with companion submodules)
//! - `statements` - Statement type checking
//! - `declarations` - Declaration type checking
//! - `flow_graph_builder` - Control flow graph builder
//! - `flow_analyzer` - Definite assignment analysis
//! - `control_flow` - Flow analyzer for type narrowing
//! - `error_reporter` - Error reporting utilities
//!
//! Note: The thin checker is the unified checker pipeline; `CheckerState`
//! is an alias to the thin checker.

// XL: crate-wide `dead_code` is suppressed because lifting it surfaces ~360
// item-level warnings across the crate (counted via an `--all-targets` pass for
// #13440/#16794). Each needs case-by-case judgment (delete vs. local expect vs.
// intended API) and several candidates are only reachable through tests or
// macros an item-level pass cannot see, so a safe audit is its own campaign.
// Scoping per-module would reproduce the blanket across most of the tree. The
// solver half of #13440 (false `InferenceError` "reserved" label + dead
// `ConstraintSet` methods) is already resolved by #13534; this crate-wide allow
// is the remaining XL item tracked by #13440. The per-symbol `#[expect(dead_code)]`
// annotations inside this crate still self-police: an item-level `expect` overrides
// this blanket, so a stale one (its symbol became used) fails the build even here.
#![allow(dead_code)]

extern crate self as tsz_checker;

pub mod context;
pub mod dispatch;
pub mod error_reporter;
pub mod module_resolution;
mod query_boundaries;
pub mod recovery;
pub mod statements;
pub mod triple_slash_validator;

#[path = "assignability/mod.rs"]
mod assignability_domain;
#[path = "checkers/mod.rs"]
mod checkers_domain;
#[path = "classes/mod.rs"]
mod classes_domain;
#[path = "declarations/mod.rs"]
mod declarations_domain;
#[path = "flow/mod.rs"]
mod flow_domain;
mod jsdoc;
#[path = "state/mod.rs"]
mod state_domain;
#[path = "symbols/mod.rs"]
mod symbols_domain;
#[path = "types/mod.rs"]
mod types_domain;

pub use checkers_domain::{
    accessor_checker, call_checker, clear_all_thread_local_state, enum_checker, generic_checker,
    iterable_checker, jsx, parameter_checker, promise_checker, property_checker,
    reset_per_file_resolution_guards, reset_stack_overflow_flag, signature_builder,
};

pub use assignability_domain::{
    assignability_checker, assignment_checker, subtype_identity_checker,
};

pub use classes_domain::{
    class_checker, class_inheritance, constructor_checker, private_checker, super_checker,
};

pub use declarations_domain::{declarations, import, module_checker, namespace_checker};

pub use flow_domain::{
    control_flow, flow_analysis, flow_analyzer, flow_graph_builder, reachability_checker,
};

pub use state_domain::type_analysis as state_type_analysis;
pub use state_domain::type_resolution::core as state_type_resolution;
pub use state_domain::{state, state_checking, type_environment as state_type_environment};

pub use symbols_domain::{scope_finder, symbol_resolver};

pub use types_domain::{
    class_type, computation, function_type, interface_type, object_type, type_checking,
    type_literal_checker, type_node,
};

pub mod diagnostics {
    pub use crate::jsdoc::diagnostics_typedef_name::jsdoc_typedef_missing_name_anchors;
    pub use tsz_common::diagnostics::{
        Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, RelatedInformationKind,
        diagnostic_codes, diagnostic_messages, format_message, internal_elaboration_messages,
        is_js_grammar_diagnostic, is_parser_grammar_diagnostic,
    };
}

#[doc(hidden)]
pub mod test_utils;

// Tests that don't depend on root crate's test_fixtures
// The 432 `src/tests/**` registrations live in their own file: the block ran
// ~1300 lines and kept `lib.rs` within a few lines of the 2000-line
// architecture cap, so every checker PR adding a test module raced the
// ceiling. The `../tests/**` registrations below stay here: those modules
// use `use super::*`, which must keep resolving to the crate root.
#[cfg(test)]
#[path = "../tests/assertion_overlap_keyof_primitive_tests.rs"]
mod assertion_overlap_keyof_primitive_tests;
#[cfg(test)]
#[path = "../tests/assertion_overlap_object_primitive_tests.rs"]
mod assertion_overlap_object_primitive_tests;
#[cfg(test)]
#[path = "../tests/assertion_overlap_template_literal_tests.rs"]
mod assertion_overlap_template_literal_tests;
#[cfg(test)]
#[path = "../tests/async_generator_yield_awaited_type_tests.rs"]
mod async_generator_yield_awaited_type_tests;
#[cfg(test)]
#[path = "../tests/async_imported_promise_tests.rs"]
mod async_imported_promise_tests;
#[cfg(test)]
#[path = "../tests/circular_accessor_annotation_tests.rs"]
mod circular_accessor_annotation_tests;
#[cfg(test)]
#[path = "../tests/class_member_closure_tests.rs"]
mod class_member_closure_tests;
#[cfg(test)]
#[path = "../tests/class_member_self_type_circularity_tests.rs"]
mod class_member_self_type_circularity_tests;
#[cfg(test)]
#[path = "../tests/class_property_constructor_flow_inference_tests.rs"]
mod class_property_constructor_flow_inference_tests;
#[cfg(test)]
#[path = "../tests/class_property_typed_const_initializer_tests.rs"]
mod class_property_typed_const_initializer_tests;
#[cfg(test)]
#[path = "../tests/commonjs_circular_alias_ts2303_tests.rs"]
mod commonjs_circular_alias_ts2303_tests;
#[cfg(test)]
#[path = "../tests/commonjs_export_assignment_reference_tests.rs"]
mod commonjs_export_assignment_reference_tests;
#[cfg(test)]
#[path = "../tests/comparability_indexed_access_reduce_tests.rs"]
mod comparability_indexed_access_reduce_tests;
#[cfg(test)]
#[path = "../tests/constructor_accessibility.rs"]
mod constructor_accessibility;
#[cfg(test)]
#[path = "../tests/constructor_overload_excess_property_tests.rs"]
mod constructor_overload_excess_property_tests;
#[cfg(test)]
#[path = "../tests/control_flow_tests.rs"]
mod control_flow_tests;
#[cfg(test)]
#[path = "../tests/control_flow_type_guard_tests.rs"]
mod control_flow_type_guard_tests;
#[cfg(test)]
#[path = "../tests/definite_assignment_tests.rs"]
mod definite_assignment_tests;
#[cfg(test)]
#[path = "../tests/duplicate_parameter_names_function_expression_forms_tests.rs"]
mod duplicate_parameter_names_function_expression_forms_tests;
#[cfg(test)]
#[path = "../tests/dynamic_import_defer_tests.rs"]
mod dynamic_import_defer_tests;
#[cfg(test)]
#[path = "../tests/enum_member_cache_tests.rs"]
mod enum_member_cache_tests;
#[cfg(test)]
#[path = "../tests/enum_merge_tests.rs"]
mod enum_merge_tests;
#[cfg(test)]
#[path = "../tests/enum_recursion_tests.rs"]
mod enum_recursion_tests;
#[cfg(test)]
#[path = "../tests/environment_capabilities_tests.rs"]
mod environment_capabilities_tests;
#[cfg(test)]
#[path = "../tests/fresh_literal_boundary_tests.rs"]
mod fresh_literal_boundary_tests;
#[cfg(test)]
#[path = "../tests/generator_union_return_type_tests.rs"]
mod generator_union_return_type_tests;
#[cfg(test)]
#[path = "../tests/generator_yield_star_next_type_tests.rs"]
mod generator_yield_star_next_type_tests;
#[cfg(test)]
#[path = "../tests/heritage_type_only_tests.rs"]
mod heritage_type_only_tests;
#[cfg(test)]
#[path = "../tests/import_equals_referenced_alias_resolution_tests.rs"]
mod import_equals_referenced_alias_resolution_tests;
#[cfg(test)]
#[path = "../tests/import_type_qualifier_namespace_meaning_tests.rs"]
mod import_type_qualifier_namespace_meaning_tests;
#[cfg(test)]
#[path = "../tests/imported_generator_iterable_tests.rs"]
mod imported_generator_iterable_tests;
#[cfg(test)]
#[path = "../tests/indexed_access_alias_application_relation_tests.rs"]
mod indexed_access_alias_application_relation_tests;
#[cfg(test)]
#[path = "../tests/inferred_return_error_and_self_call_tests.rs"]
mod inferred_return_error_and_self_call_tests;
#[cfg(test)]
#[path = "../tests/isolated_declarations_unannotated_param_tests.rs"]
mod isolated_declarations_unannotated_param_tests;
#[cfg(test)]
#[path = "../tests/js_property_write_self_declaration_tests.rs"]
mod js_property_write_self_declaration_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_postfix_nullable_type_tests.rs"]
mod jsdoc_postfix_nullable_type_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_prototype_assignment_literal_display.rs"]
mod jsdoc_prototype_assignment_literal_display;
#[cfg(test)]
#[path = "../tests/jsdoc_prototype_assignment_target_display.rs"]
mod jsdoc_prototype_assignment_target_display;
#[cfg(test)]
#[path = "../tests/jsdoc_this_arrow_tests.rs"]
mod jsdoc_this_arrow_tests;
#[cfg(test)]
#[path = "../tests/jsx_component_attribute_tests.rs"]
mod jsx_component_attribute_tests;
#[cfg(test)]
#[path = "../tests/literal_application_alias_display_tests.rs"]
mod literal_application_alias_display_tests;
#[cfg(test)]
#[path = "../tests/merged_symbol_tests.rs"]
mod merged_symbol_tests;
#[cfg(test)]
#[path = "../tests/missing_name_pass_constrained_type_param_tests.rs"]
mod missing_name_pass_constrained_type_param_tests;
#[cfg(test)]
#[path = "../tests/name_resolution_boundary_tests.rs"]
mod name_resolution_boundary_tests;
#[cfg(test)]
#[path = "../tests/no_filename_based_behavior_tests.rs"]
mod no_filename_based_behavior_tests;
#[cfg(test)]
#[path = "../tests/noinfer_comparability_overlap_tests.rs"]
mod noinfer_comparability_overlap_tests;
#[cfg(test)]
#[path = "../tests/optional_param_display_tests.rs"]
mod optional_param_display_tests;
#[cfg(test)]
#[path = "../tests/optional_property_subtype_compatibility_tests.rs"]
mod optional_property_subtype_compatibility_tests;
#[cfg(test)]
#[path = "../tests/optional_property_target_undefined_display_tests.rs"]
mod optional_property_target_undefined_display_tests;
#[cfg(test)]
#[path = "../tests/overload_modifier_tests.rs"]
mod overload_modifier_tests;
#[cfg(test)]
#[path = "../tests/override_intersection_display_tests.rs"]
mod override_intersection_display_tests;
#[cfg(test)]
#[path = "../tests/quick_type_nullish_callee_companion_tests.rs"]
mod quick_type_nullish_callee_companion_tests;
#[cfg(test)]
#[path = "../tests/relation_boundary_tests.rs"]
mod relation_boundary_tests;
#[cfg(test)]
#[path = "../tests/rest_parameter_tests.rs"]
mod rest_parameter_tests;
#[cfg(test)]
#[path = "../tests/rest_tuple_contextual_typing_tests.rs"]
mod rest_tuple_contextual_typing_tests;
#[cfg(test)]
#[path = "../tests/spread_rest_diagnostics_tests.rs"]
mod spread_rest_diagnostics_tests;
#[cfg(test)]
#[path = "../tests/spread_rest_tests.rs"]
mod spread_rest_tests;
#[cfg(test)]
#[path = "../tests/stability_validation_tests.rs"]
mod stability_validation_tests;
#[cfg(test)]
#[path = "../tests/string_literal_arithmetic_tests.rs"]
mod string_literal_arithmetic_tests;
#[cfg(test)]
#[path = "../tests/symbol_resolution_tests.rs"]
mod symbol_resolution_tests;
#[cfg(test)]
#[path = "../tests/symbol_resolver_stability_tests.rs"]
mod symbol_resolver_stability_tests;
#[cfg(test)]
mod test_module_registry;
#[cfg(test)]
#[path = "../tests/this_type_tests.rs"]
mod this_type_tests;
#[cfg(test)]
#[path = "../tests/ts1214_let_strict_mode_tests.rs"]
mod ts1214_let_strict_mode_tests;
#[cfg(test)]
#[path = "../tests/ts1323_tests.rs"]
mod ts1323_tests;
#[cfg(test)]
#[path = "../tests/ts1338_tests.rs"]
mod ts1338_tests;
#[cfg(test)]
#[path = "../tests/ts1501_tests.rs"]
mod ts1501_tests;
#[cfg(test)]
#[path = "../tests/ts2300_tests.rs"]
mod ts2300_tests;
#[cfg(test)]
#[path = "../tests/ts2303_tests.rs"]
mod ts2303_tests;
#[cfg(test)]
#[path = "../tests/ts2304_tests.rs"]
mod ts2304_tests;
#[cfg(test)]
#[path = "../tests/ts2305_tests.rs"]
mod ts2305_tests;
#[cfg(test)]
#[path = "../tests/ts2306_tests.rs"]
mod ts2306_tests;
#[cfg(test)]
#[path = "../tests/ts2320_mapped_type_ancestor_tests.rs"]
mod ts2320_mapped_type_ancestor_tests;
#[cfg(test)]
#[path = "../tests/ts2320_tests.rs"]
mod ts2320_tests;
#[cfg(test)]
#[path = "../tests/ts2322_destructuring_obj_literal_tests.rs"]
mod ts2322_destructuring_obj_literal_tests;
#[cfg(test)]
#[path = "../tests/ts2322_indexed_access_type_param_tests.rs"]
mod ts2322_indexed_access_type_param_tests;
#[cfg(test)]
#[path = "../tests/ts2322_mode_routing_matrix.rs"]
mod ts2322_mode_routing_matrix;
#[cfg(test)]
#[path = "../tests/ts2322_tests.rs"]
mod ts2322_tests;
#[cfg(test)]
#[path = "../tests/ts2323_tests.rs"]
mod ts2323_tests;
#[cfg(test)]
#[path = "../tests/ts2347_tests.rs"]
mod ts2347_tests;
#[cfg(test)]
#[path = "../tests/ts2352_both_callable_overlap_repro_tests.rs"]
mod ts2352_both_callable_overlap_repro_tests;
#[cfg(test)]
#[path = "../tests/ts2352_constrained_type_param_target_tests.rs"]
mod ts2352_constrained_type_param_target_tests;
#[cfg(test)]
#[path = "../tests/ts2352_disjoint_literal_property_tests.rs"]
mod ts2352_disjoint_literal_property_tests;
#[cfg(test)]
#[path = "../tests/ts2352_intersection_assertion_tests.rs"]
mod ts2352_intersection_assertion_tests;
#[cfg(test)]
#[path = "../tests/ts2352_void_undefined_assertion_tests.rs"]
mod ts2352_void_undefined_assertion_tests;
#[cfg(test)]
#[path = "../tests/ts2353_tests.rs"]
mod ts2353_tests;
#[cfg(test)]
#[path = "../tests/ts2375_exact_optional_property_display_tests.rs"]
mod ts2375_exact_optional_property_display_tests;
#[cfg(test)]
#[path = "../tests/ts2385_overload_modifier_tests.rs"]
mod ts2385_overload_modifier_tests;
#[cfg(test)]
#[path = "../tests/ts2397_tests.rs"]
mod ts2397_tests;
#[cfg(test)]
#[path = "../tests/ts2411_tests.rs"]
mod ts2411_tests;
#[cfg(test)]
#[path = "../tests/ts2418_computed_property_value_widening_tests.rs"]
mod ts2418_computed_property_value_widening_tests;
#[cfg(test)]
#[path = "../tests/ts2418_wellknown_symbol_declared_member_tests.rs"]
mod ts2418_wellknown_symbol_declared_member_tests;
#[cfg(test)]
#[path = "../tests/ts2428_tests.rs"]
mod ts2428_tests;
#[cfg(test)]
#[path = "../tests/ts2430_tests.rs"]
mod ts2430_tests;
#[cfg(test)]
#[path = "../tests/ts2440_tests.rs"]
mod ts2440_tests;
#[cfg(test)]
#[path = "../tests/ts2450_const_enum_tests.rs"]
mod ts2450_const_enum_tests;
#[cfg(test)]
#[path = "../tests/ts2469_symbol_operator_tests.rs"]
mod ts2469_symbol_operator_tests;
#[cfg(test)]
#[path = "../tests/ts2498_export_star_export_equals_tests.rs"]
mod ts2498_export_star_export_equals_tests;
#[cfg(test)]
#[path = "../tests/ts2498_tests.rs"]
mod ts2498_tests;
#[cfg(test)]
#[path = "../tests/ts2540_readonly_tests.rs"]
mod ts2540_readonly_tests;
#[cfg(test)]
#[path = "../tests/ts2542_readonly_index_coemission_tests.rs"]
mod ts2542_readonly_index_coemission_tests;
#[cfg(test)]
#[path = "../tests/ts2558_new_type_args_tests.rs"]
mod ts2558_new_type_args_tests;
#[cfg(test)]
#[path = "../tests/ts2589_mapped_type_tests.rs"]
mod ts2589_mapped_type_tests;
#[cfg(test)]
#[path = "../tests/ts2589_tests.rs"]
mod ts2589_tests;
#[cfg(test)]
#[path = "../tests/ts2683_tests.rs"]
mod ts2683_tests;
#[cfg(test)]
#[path = "../tests/ts2702_qualifier_namespace_meaning_tests.rs"]
mod ts2702_qualifier_namespace_meaning_tests;
#[cfg(test)]
#[path = "../tests/ts2774_tests.rs"]
mod ts2774_tests;
#[cfg(test)]
#[path = "../tests/ts2838_tests.rs"]
mod ts2838_tests;
#[cfg(test)]
#[path = "../tests/ts2839_tests.rs"]
mod ts2839_tests;
#[cfg(test)]
#[path = "../tests/ts6133_private_name_tests.rs"]
mod ts6133_private_name_tests;
#[cfg(test)]
#[path = "../tests/ts6133_unused_type_params_tests.rs"]
mod ts6133_unused_type_params_tests;
#[cfg(test)]
#[path = "../tests/ts7006_broad_jsdoc_type_cast.rs"]
mod ts7006_broad_jsdoc_type_cast;
#[cfg(test)]
#[path = "../tests/ts7006_iife_arg_implicit_any.rs"]
mod ts7006_iife_arg_implicit_any;
#[cfg(test)]
#[path = "../tests/ts7030_bare_return_tests.rs"]
mod ts7030_bare_return_tests;
#[cfg(test)]
#[path = "../tests/ts7030_undefined_union_return_tests.rs"]
mod ts7030_undefined_union_return_tests;
#[cfg(test)]
#[path = "../tests/ts7036_tests.rs"]
mod ts7036_tests;
#[cfg(test)]
#[path = "../tests/ts7041_tests.rs"]
mod ts7041_tests;
#[cfg(test)]
#[path = "../tests/ts7057_yield_implicit_any.rs"]
mod ts7057_yield_implicit_any;
#[cfg(test)]
#[path = "../tests/tuple_index_access_tests.rs"]
mod tuple_index_access_tests;
#[cfg(test)]
#[path = "../tests/typeof_operator_result_union_tests.rs"]
mod typeof_operator_result_union_tests;
#[cfg(test)]
#[path = "../tests/typeof_unique_symbol_source_display_tests.rs"]
mod typeof_unique_symbol_source_display_tests;
#[cfg(test)]
#[path = "../tests/using_binding_pattern_diagnostics_tests.rs"]
mod using_binding_pattern_diagnostics_tests;
#[cfg(test)]
#[path = "../tests/value_usage_tests.rs"]
mod value_usage_tests;
#[cfg(test)]
#[path = "../tests/yield_star_return_type_tests.rs"]
mod yield_star_return_type_tests;
// Tests kept in root test harness where shared fixtures live.
#[cfg(test)]
#[path = "../tests/architecture_contract_tests.rs"]
mod architecture_contract_tests;
#[cfg(test)]
#[path = "../tests/array_isarray_mutual_subtype_narrowing_tests.rs"]
mod array_isarray_mutual_subtype_narrowing_tests;
#[cfg(test)]
#[path = "../tests/bigint_exponentiation_target_tests.rs"]
mod bigint_exponentiation_target_tests;
#[cfg(test)]
#[path = "../tests/bigint_target_ts2737_tests.rs"]
mod bigint_target_ts2737_tests;
#[cfg(test)]
#[path = "../tests/class_index_signature_compat_tests.rs"]
mod class_index_signature_compat_tests;
#[cfg(test)]
#[path = "../tests/conditional_alias_unreduced_keeps_alias_display_tests.rs"]
mod conditional_alias_unreduced_keeps_alias_display_tests;
#[cfg(test)]
#[path = "../tests/conditional_keyof_test.rs"]
mod conditional_keyof_test;
#[cfg(test)]
#[path = "../tests/conditional_rest_arity_erasure_tests.rs"]
mod conditional_rest_arity_erasure_tests;
#[cfg(test)]
#[path = "../tests/contextual_typing_tests.rs"]
mod contextual_typing_tests;
#[cfg(test)]
#[path = "../tests/cross_file_class_merge_tests.rs"]
mod cross_file_class_merge_tests;
#[cfg(test)]
#[path = "../tests/cross_file_interface_merge_ts2717_tests.rs"]
mod cross_file_interface_merge_ts2717_tests;
#[cfg(test)]
#[path = "../tests/cross_file_type_params_cache_tests.rs"]
mod cross_file_type_params_cache_tests;
#[cfg(test)]
#[path = "../tests/dynamic_import_ts2307_per_callsite_tests.rs"]
mod dynamic_import_ts2307_per_callsite_tests;
#[cfg(test)]
#[path = "../tests/enum_indexed_access_tests.rs"]
mod enum_indexed_access_tests;
#[cfg(test)]
#[path = "../tests/enum_nominality_tests.rs"]
mod enum_nominality_tests;
#[cfg(test)]
#[path = "../tests/file_session_switch_to_file_tests.rs"]
mod file_session_switch_to_file_tests;
#[cfg(test)]
#[path = "../tests/flow_boundary_contract_tests.rs"]
mod flow_boundary_contract_tests;
#[cfg(test)]
#[path = "../tests/for_in_intersection_operand_tests.rs"]
mod for_in_intersection_operand_tests;
#[cfg(test)]
#[path = "../tests/for_in_narrowing_tests.rs"]
mod for_in_narrowing_tests;
#[cfg(test)]
#[path = "../tests/for_in_operand_type_display_tests.rs"]
mod for_in_operand_type_display_tests;
#[cfg(test)]
#[path = "../tests/for_in_optional_chain_ts2405_vs_ts2780_tests.rs"]
mod for_in_optional_chain_ts2405_vs_ts2780_tests;
#[cfg(test)]
#[path = "../tests/for_in_self_reference_and_nullable_operand_tests.rs"]
mod for_in_self_reference_and_nullable_operand_tests;
#[cfg(test)]
#[path = "../tests/for_in_union_operand_tests.rs"]
mod for_in_union_operand_tests;
#[cfg(test)]
#[path = "../tests/for_of_self_reference_operand_spelling_tests.rs"]
mod for_of_self_reference_operand_spelling_tests;
#[cfg(test)]
#[path = "../tests/fresh_object_literal_union_array_member_drill_in_tests.rs"]
mod fresh_object_literal_union_array_member_drill_in_tests;
#[cfg(test)]
#[path = "../tests/function_source_apparent_function_surface_tests.rs"]
mod function_source_apparent_function_surface_tests;
#[cfg(test)]
#[path = "../tests/function_source_numeric_index_target_tests.rs"]
mod function_source_numeric_index_target_tests;
#[cfg(test)]
#[path = "../tests/generic_inference_manual.rs"]
mod generic_inference_manual;
#[cfg(test)]
#[path = "../tests/generic_tests.rs"]
mod generic_tests;
#[cfg(test)]
#[path = "../tests/increment_assignment_target_suppression_tests.rs"]
mod increment_assignment_target_suppression_tests;
#[cfg(test)]
#[path = "../tests/interface_extends_array_json_tests.rs"]
mod interface_extends_array_json_tests;
#[cfg(test)]
#[path = "../tests/intersection_signatures.rs"]
mod intersection_signatures;
#[cfg(test)]
#[path = "../tests/js_constructor_property_tests.rs"]
mod js_constructor_property_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_accessibility_tests.rs"]
mod jsdoc_accessibility_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_callback_rest_tests.rs"]
mod jsdoc_callback_rest_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_cross_file_typedef_tests.rs"]
mod jsdoc_cross_file_typedef_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_enum_circular_tests.rs"]
mod jsdoc_enum_circular_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_function_return_type_anchor_tests.rs"]
mod jsdoc_function_return_type_anchor_tests;
#[cfg(test)]
#[path = "../tests/position_invalid_export_specifier_resolution_tests.rs"]
mod position_invalid_export_specifier_resolution_tests;
#[cfg(test)]
#[path = "../tests/position_invalid_module_element_module_axis_tests.rs"]
mod position_invalid_module_element_module_axis_tests;
#[cfg(test)]
#[path = "../tests/ts2448_binding_pattern_initializer_tdz_tests.rs"]
mod ts2448_binding_pattern_initializer_tdz_tests;

#[cfg(test)]
#[path = "../tests/jsdoc_readonly_tests.rs"]
mod jsdoc_readonly_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_recursive_generic_typedef_tests.rs"]
mod jsdoc_recursive_generic_typedef_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_reference_kernel_tests.rs"]
mod jsdoc_reference_kernel_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_returns_typedef_import_type_anchor_tests.rs"]
mod jsdoc_returns_typedef_import_type_anchor_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_satisfies_tests.rs"]
mod jsdoc_satisfies_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_template_class_tests.rs"]
mod jsdoc_template_class_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_type_expression_tests.rs"]
mod jsdoc_type_expression_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_type_tag_tests.rs"]
mod jsdoc_type_tag_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_typedef_module_export_tests.rs"]
mod jsdoc_typedef_module_export_tests;
#[cfg(test)]
#[path = "../tests/jsx_react_hoc_spread_props_tests.rs"]
mod jsx_react_hoc_spread_props_tests;
#[cfg(test)]
#[path = "../tests/keyof_function_type_is_never_tests.rs"]
mod keyof_function_type_is_never_tests;
#[cfg(test)]
#[path = "../tests/keyof_mapped_as_clause_tests.rs"]
mod keyof_mapped_as_clause_tests;
#[cfg(test)]
#[path = "../tests/keyof_mapped_constraint_key_space_tests.rs"]
mod keyof_mapped_constraint_key_space_tests;
#[cfg(test)]
#[path = "../tests/logical_assignment_narrowing_tests.rs"]
mod logical_assignment_narrowing_tests;
#[cfg(test)]
#[path = "../tests/logical_operator_literal_preservation_tests.rs"]
mod logical_operator_literal_preservation_tests;
#[cfg(test)]
#[path = "../tests/mapped_indexed_access_diagnostic_tests.rs"]
mod mapped_indexed_access_diagnostic_tests;
#[cfg(test)]
#[path = "../tests/mapped_intersection_indexed_access_tests.rs"]
mod mapped_intersection_indexed_access_tests;
#[cfg(test)]
#[path = "../tests/member_access_architecture_boundary_tests.rs"]
mod member_access_architecture_boundary_tests;
#[cfg(test)]
#[path = "../tests/module_resolution_guard_tests.rs"]
mod module_resolution_guard_tests;
#[cfg(test)]
#[path = "../tests/never_absorption_call_spread_tests.rs"]
mod never_absorption_call_spread_tests;
#[cfg(test)]
#[path = "../tests/never_initializer_falls_through_tests.rs"]
mod never_initializer_falls_through_tests;
#[cfg(test)]
#[path = "../tests/never_returning_narrowing_tests.rs"]
mod never_returning_narrowing_tests;
#[cfg(test)]
#[path = "../tests/new_expression_source_display_tests.rs"]
mod new_expression_source_display_tests;
#[cfg(test)]
#[path = "../tests/new_typeof_property_tests.rs"]
mod new_typeof_property_tests;
#[cfg(test)]
#[path = "../tests/nullish_coalescing_discriminated_union_tests.rs"]
mod nullish_coalescing_discriminated_union_tests;
#[cfg(test)]
#[path = "../tests/nullish_coalescing_unknown_result_tests.rs"]
mod nullish_coalescing_unknown_result_tests;
#[cfg(test)]
#[path = "../tests/nullish_operand_checknonnull_parity_tests.rs"]
mod nullish_operand_checknonnull_parity_tests;
#[cfg(test)]
#[path = "../tests/private_brands.rs"]
mod private_brands;
#[cfg(test)]
#[path = "../tests/recursive_alias_application_target_display_tests.rs"]
mod recursive_alias_application_target_display_tests;
#[cfg(test)]
#[path = "../tests/repro_parserreal.rs"]
mod repro_parserreal;
#[cfg(test)]
#[path = "../tests/reverse_mapped_inference_tests.rs"]
mod reverse_mapped_inference_tests;

#[cfg(test)]
#[path = "../tests/strict_null_manual.rs"]
mod strict_null_manual;
#[cfg(test)]
#[path = "../tests/symbol_index_signature_tests.rs"]
mod symbol_index_signature_tests;

#[cfg(test)]
#[path = "../tests/symbol_index_excess_drill_in_tests.rs"]
mod symbol_index_excess_drill_in_tests;

#[cfg(test)]
#[path = "../tests/jsdoc_dotted_typedef_import_type_tests.rs"]
mod jsdoc_dotted_typedef_import_type_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_qualified_chain_namespace_meaning_tests.rs"]
mod jsdoc_qualified_chain_namespace_meaning_tests;
#[cfg(test)]
#[path = "../tests/ts18048_unary_arithmetic_nullish_tests.rs"]
mod ts18048_unary_arithmetic_nullish_tests;
#[cfg(test)]
#[path = "../tests/ts7032_zero_parameter_setter_tests.rs"]
mod ts7032_zero_parameter_setter_tests;
#[cfg(test)]
#[path = "../tests/ts_import_type_commonjs_expando_class_tests.rs"]
mod ts_import_type_commonjs_expando_class_tests;
#[cfg(test)]
#[path = "../tests/variadic_tuple_elaboration_tests.rs"]
mod variadic_tuple_elaboration_tests;
#[cfg(test)]
#[path = "../tests/variadic_tuple_readonly_relation_tests.rs"]
mod variadic_tuple_readonly_relation_tests;
#[cfg(test)]
#[path = "../tests/variadic_tuple_tail_arity_inference_tests.rs"]
mod variadic_tuple_tail_arity_inference_tests;
#[cfg(test)]
#[path = "../tests/void_param_optionality_tests.rs"]
mod void_param_optionality_tests;
#[cfg(test)]
#[path = "../tests/widening_integration_tests.rs"]
mod widening_integration_tests;

// Re-export key types
pub use context::{CheckerContext, CheckerOptions, EnclosingClassInfo, TypeCache};
pub use control_flow::{FlowAnalyzer, FlowGraph as ControlFlowGraph};
pub use declarations::DeclarationChecker;
pub use dispatch::ExpressionDispatcher;
pub use flow_analyzer::{
    AssignmentState, AssignmentStateMap, DefiniteAssignmentAnalyzer, DefiniteAssignmentResult,
    merge_assignment_states,
};
pub use flow_graph_builder::{FlowGraph, FlowGraphBuilder};
pub use recovery::RecoveryReason;
pub use state::{CheckerState, MAX_CALL_DEPTH, MAX_INSTANTIATION_DEPTH};
pub use statements::{StatementCheckCallbacks, StatementChecker};
pub use tsz_solver::Visibility;
pub use type_node::TypeNodeChecker;

/// Run the JS-only `TS8xxx` grammar pass on a parsed source file and return any
/// diagnostics it emits. The pass is normally invoked as part of the regular
/// `check_source_file` walk; this entry point lets callers (notably the CLI's
/// `--noCheck` parse-only path) surface those grammar diagnostics without
/// running the full type-checking pipeline. Returns an empty vector for non-JS
/// files (the underlying pass no-ops via `is_js_file`).
#[must_use]
pub fn run_js_grammar_pass(
    arena: &tsz_parser::NodeArena,
    binder: &tsz_binder::BinderState,
    source_file: tsz_parser::NodeIndex,
    file_name: String,
    options: context::CheckerOptions,
) -> Vec<diagnostics::Diagnostic> {
    let Some(source) = arena.get_source_file_at(source_file) else {
        return Vec::new();
    };
    let statements = source.statements.nodes.as_slice();
    if statements.is_empty() {
        return Vec::new();
    }
    let interner = tsz_solver::construction::TypeInterner::new();
    let mut checker = CheckerState::new(arena, binder, &interner, file_name, options);
    checker.check_js_grammar_statements(statements);
    checker.ctx.diagnostics
}

/// Run only the `--isolatedDeclarations` grammar pass on a parsed source file
/// and return any TS9007/TS9011/TS9012/etc. diagnostics it emits. The CLI's
/// `--noCheck` shortcut otherwise skips the regular checker entirely; tsc
/// still emits these declaration-emit-prerequisite diagnostics in that mode
/// because they gate `.d.ts` emission, not type checking.
///
/// Returns an empty vector when `isolated_declarations` is false in `options`
/// or the file is a `.d.ts` (the underlying pass no-ops in those cases).
#[must_use]
pub fn run_isolated_declarations_pass(
    arena: &tsz_parser::NodeArena,
    binder: &tsz_binder::BinderState,
    source_file: tsz_parser::NodeIndex,
    file_name: String,
    options: context::CheckerOptions,
) -> Vec<diagnostics::Diagnostic> {
    if !options.isolated_declarations {
        return Vec::new();
    }
    let Some(source) = arena.get_source_file_at(source_file) else {
        return Vec::new();
    };
    let statements = source.statements.nodes.as_slice();
    if statements.is_empty() {
        return Vec::new();
    }
    let interner = tsz_solver::construction::TypeInterner::new();
    let mut checker = CheckerState::new(arena, binder, &interner, file_name, options);
    checker.check_isolated_declarations(statements);
    checker.check_isolated_decl_class_expressions(statements);
    checker.check_isolated_decl_augmentations(statements);
    checker.ctx.diagnostics
}
