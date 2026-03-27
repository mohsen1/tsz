//! Type checker module for TypeScript AST.
//!
//! This module is organized into several submodules:
//! - `context` - `CheckerContext` for shared state
//! - `expr` - Expression type checking
//! - `statements` - Statement type checking
//! - `declarations` - Declaration type checking
//! - `flow_graph_builder` - Control flow graph builder
//! - `flow_analyzer` - Definite assignment analysis
//! - `control_flow` - Flow analyzer for type narrowing
//! - `error_reporter` - Error reporting utilities
//!
//! Note: The thin checker is the unified checker pipeline; `CheckerState`
//! is an alias to the thin checker.

#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::let_and_return)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::needless_return)]
#![allow(clippy::print_stderr)]
#![allow(clippy::question_mark)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::unnecessary_map_or)]

extern crate self as tsz_checker;

#[allow(clippy::type_complexity)]
pub mod context;
pub mod dispatch;
mod dispatch_helpers;
mod dispatch_yield;
pub mod error_reporter;
pub mod expr;
pub mod module_resolution;
mod query_boundaries;
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
    accessor_checker, call_checker, enum_checker, generic_checker, iterable_checker, jsx,
    parameter_checker, promise_checker, property_checker, reset_stack_overflow_flag,
    signature_builder,
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
    class_type, computation, function_type, interface_type, literal_type, object_type,
    type_checking, type_literal_checker, type_node,
};

pub mod diagnostics {
    pub use tsz_common::diagnostics::{
        Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
        diagnostic_messages, format_message,
    };
}

#[cfg(test)]
pub mod test_utils;

// Tests that don't depend on root crate's test_fixtures
#[cfg(test)]
#[path = "../tests/circular_accessor_annotation_tests.rs"]
mod circular_accessor_annotation_tests;
#[cfg(test)]
#[path = "../tests/class_member_closure_tests.rs"]
mod class_member_closure_tests;
#[cfg(test)]
#[path = "../tests/conformance_issues.rs"]
mod conformance_issues;
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
#[path = "../tests/generator_union_return_type_tests.rs"]
mod generator_union_return_type_tests;
#[cfg(test)]
#[path = "../tests/heritage_type_only_tests.rs"]
mod heritage_type_only_tests;
#[cfg(test)]
#[path = "../tests/jsx_component_attribute_tests.rs"]
mod jsx_component_attribute_tests;
#[cfg(test)]
#[path = "../tests/merged_symbol_tests.rs"]
mod merged_symbol_tests;
#[cfg(test)]
#[path = "../tests/name_resolution_boundary_tests.rs"]
mod name_resolution_boundary_tests;
#[cfg(test)]
#[path = "../tests/no_filename_based_behavior_tests.rs"]
mod no_filename_based_behavior_tests;
#[cfg(test)]
#[path = "../tests/overload_modifier_tests.rs"]
mod overload_modifier_tests;
#[cfg(test)]
#[path = "../tests/override_intersection_display_tests.rs"]
mod override_intersection_display_tests;
#[cfg(test)]
#[path = "../tests/relation_boundary_tests.rs"]
mod relation_boundary_tests;
#[cfg(test)]
#[path = "../tests/rest_parameter_tests.rs"]
mod rest_parameter_tests;
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
#[path = "../tests/symbol_resolver_stability_tests.rs"]
mod symbol_resolver_stability_tests;
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
#[path = "../tests/ts2320_tests.rs"]
mod ts2320_tests;
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
#[path = "../tests/ts2353_tests.rs"]
mod ts2353_tests;
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
#[path = "../tests/ts2540_readonly_tests.rs"]
mod ts2540_readonly_tests;
#[cfg(test)]
#[path = "../tests/ts2558_new_type_args_tests.rs"]
mod ts2558_new_type_args_tests;
#[cfg(test)]
#[path = "../tests/ts2589_tests.rs"]
mod ts2589_tests;
#[cfg(test)]
#[path = "../tests/ts2683_tests.rs"]
mod ts2683_tests;
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
#[path = "tests/architecture_contract_tests.rs"]
mod architecture_contract_tests_src;
#[cfg(test)]
#[path = "tests/call_architecture_tests.rs"]
mod call_architecture_tests;
#[cfg(test)]
#[path = "../tests/class_index_signature_compat_tests.rs"]
mod class_index_signature_compat_tests;
#[cfg(test)]
#[path = "../tests/conditional_infer_tests.rs"]
mod conditional_infer_tests;
#[cfg(test)]
#[path = "../tests/conditional_keyof_test.rs"]
mod conditional_keyof_test;
#[cfg(test)]
#[path = "tests/contextual_return_wrapper_tests.rs"]
mod contextual_return_wrapper_tests;
#[cfg(test)]
#[path = "../tests/contextual_typing_tests.rs"]
mod contextual_typing_tests;
#[cfg(test)]
#[path = "tests/dispatch_tests.rs"]
mod dispatch_tests;
#[cfg(test)]
#[path = "../tests/enum_nominality_tests.rs"]
mod enum_nominality_tests;
#[cfg(test)]
#[path = "../tests/flow_boundary_contract_tests.rs"]
mod flow_boundary_contract_tests;
#[cfg(test)]
#[path = "../tests/for_in_narrowing_tests.rs"]
mod for_in_narrowing_tests;
#[cfg(test)]
#[path = "../tests/generic_inference_manual.rs"]
mod generic_inference_manual;
#[cfg(test)]
#[path = "../tests/generic_tests.rs"]
mod generic_tests;
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
#[path = "../tests/jsdoc_readonly_tests.rs"]
mod jsdoc_readonly_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_reference_kernel_tests.rs"]
mod jsdoc_reference_kernel_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_satisfies_tests.rs"]
mod jsdoc_satisfies_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_template_class_tests.rs"]
mod jsdoc_template_class_tests;
#[cfg(test)]
#[path = "../tests/jsdoc_type_tag_tests.rs"]
mod jsdoc_type_tag_tests;
#[cfg(test)]
#[path = "../tests/keyof_mapped_as_clause_tests.rs"]
mod keyof_mapped_as_clause_tests;
#[cfg(test)]
#[path = "../tests/logical_assignment_narrowing_tests.rs"]
mod logical_assignment_narrowing_tests;
#[cfg(test)]
#[path = "../tests/member_access_architecture_boundary_tests.rs"]
mod member_access_architecture_boundary_tests;
#[cfg(test)]
#[path = "../tests/module_resolution_guard_tests.rs"]
mod module_resolution_guard_tests;
#[cfg(test)]
#[path = "../tests/never_returning_narrowing_tests.rs"]
mod never_returning_narrowing_tests;
#[cfg(test)]
#[path = "../tests/new_typeof_property_tests.rs"]
mod new_typeof_property_tests;
#[cfg(test)]
#[path = "../tests/private_brands.rs"]
mod private_brands;
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
#[path = "../tests/void_param_optionality_tests.rs"]
mod void_param_optionality_tests;

// Re-export key types
pub use context::{CheckerContext, CheckerOptions, EnclosingClassInfo, TypeCache};
pub use control_flow::{FlowAnalyzer, FlowGraph as ControlFlowGraph};
pub use declarations::DeclarationChecker;
pub use dispatch::ExpressionDispatcher;
pub use expr::ExpressionChecker;
pub use flow_analyzer::{
    AssignmentState, AssignmentStateMap, DefiniteAssignmentAnalyzer, DefiniteAssignmentResult,
    merge_assignment_states,
};
pub use flow_graph_builder::{FlowGraph, FlowGraphBuilder};
pub use state::{CheckerState, MAX_CALL_DEPTH, MAX_INSTANTIATION_DEPTH};
pub use statements::{StatementCheckCallbacks, StatementChecker};
pub use tsz_solver::Visibility;
pub use type_node::TypeNodeChecker;
