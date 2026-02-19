//! Type checker module for TypeScript AST.
//!
//! This module is organized into several submodules:
//! - `context` - `CheckerContext` for shared state
//! - `expr` - Expression type checking
//! - `statements` - Statement type checking
//! - `declarations` - Declaration type checking
//! - `flow_graph_builder` - Control flow graph builder
//! - `flow_analyzer` - Definite assignment analysis
//! - `reachability_analyzer` - Unreachable code detection
//! - `control_flow` - Flow analyzer for type narrowing
//! - `accessibility` - Accessibility checking (private/protected)
//! - `error_reporter` - Error reporting utilities
//!
//! Note: The thin checker is the unified checker pipeline; `CheckerState`
//! is an alias to the thin checker.

pub mod accessibility;
pub mod accessor_checker;
pub mod array_type;
pub mod assignability_checker;
pub mod assignment_checker;
pub mod call_checker;
pub mod callable_type;
pub mod class_checker;
mod class_implements_checker;
pub mod class_inheritance;
pub mod class_type;
pub mod conditional_type;
pub mod constructor_checker;
pub mod context;
mod context_constructors;
mod context_resolver;
pub mod control_flow;
mod control_flow_assignment;
mod control_flow_narrowing;
mod control_flow_type_guards;
pub mod declarations;
pub mod decorators;
pub mod dispatch;
pub mod enum_checker;
pub mod error_handler;
pub mod error_reporter;
pub mod expr;
pub mod flow_analysis;
mod flow_analysis_usage;
pub mod flow_analyzer;
pub mod flow_graph_builder;
pub mod flow_narrowing;
pub mod function_type;
pub mod generic_checker;
pub mod import_checker;
mod import_declaration_checker;
pub mod indexed_access_type;
pub mod interface_type;
pub mod intersection_type;
pub mod iterable_checker;
pub mod jsx;
pub mod jsx_checker;
pub mod judge_integration;
pub mod literal_type;
pub mod module_checker;
pub mod module_resolution;
pub mod namespace_checker;
pub mod nullish;
pub mod object_type;
pub mod optional_chain;
pub mod parameter_checker;
pub mod private_checker;
pub mod promise_checker;
mod property_access_type;
pub mod property_checker;
mod query_boundaries;
pub mod reachability_analyzer;
pub mod reachability_checker;
pub mod scope_finder;
pub mod signature_builder;
pub mod sound_checker;
pub mod state;
pub mod state_checking;
mod state_checking_members;
mod state_class_checking;
mod state_property_checking;
pub mod state_type_analysis;
mod state_type_analysis_computed;
pub mod state_type_environment;
pub mod state_type_resolution;
mod state_type_resolution_module;
mod state_variable_checking;
pub mod statements;
pub mod super_checker;
pub mod symbol_resolver;
mod symbol_resolver_utils;
pub mod triple_slash_validator;
pub mod tuple_type;
pub mod type_api;
pub mod type_checking;
mod type_checking_declarations;
mod type_checking_global;
mod type_checking_queries;
mod type_checking_queries_class;
mod type_checking_queries_lib;
mod type_checking_unused;
mod type_checking_utilities;
mod type_checking_utilities_enum;
mod type_checking_utilities_jsdoc;
pub mod type_computation;
mod type_computation_access;
mod type_computation_call;
mod type_computation_complex;
pub mod type_literal_checker;
pub mod type_node;
pub mod type_parameter;
pub mod type_query;
pub mod union_type;
pub mod diagnostics {
    pub use tsz_common::diagnostics::{
        Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
        diagnostic_messages, format_message,
    };
}

// Tests that don't depend on root crate's test_fixtures
#[cfg(test)]
#[path = "../tests/conformance_issues.rs"]
mod conformance_issues;
#[cfg(test)]
#[path = "../tests/control_flow_tests.rs"]
mod control_flow_tests;
#[cfg(test)]
#[path = "../tests/definite_assignment_tests.rs"]
mod definite_assignment_tests;
#[cfg(test)]
#[path = "../tests/enum_member_cache_tests.rs"]
mod enum_member_cache_tests;
#[cfg(test)]
#[path = "../tests/enum_merge_tests.rs"]
mod enum_merge_tests;
#[cfg(test)]
#[path = "../tests/no_filename_based_behavior_tests.rs"]
mod no_filename_based_behavior_tests;
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
#[path = "../tests/ts2322_mode_routing_matrix.rs"]
mod ts2322_mode_routing_matrix;
#[cfg(test)]
#[path = "../tests/ts2322_tests.rs"]
mod ts2322_tests;
#[cfg(test)]
#[path = "../tests/ts2374_duplicate_index_tests.rs"]
mod ts2374_duplicate_index_tests;
#[cfg(test)]
#[path = "../tests/ts2403_tests.rs"]
mod ts2403_tests;
#[cfg(test)]
#[path = "../tests/ts2411_tests.rs"]
mod ts2411_tests;
#[cfg(test)]
#[path = "../tests/ts2540_readonly_tests.rs"]
mod ts2540_readonly_tests;
#[cfg(test)]
#[path = "../tests/ts2558_new_type_args_tests.rs"]
mod ts2558_new_type_args_tests;
#[cfg(test)]
#[path = "../tests/ts6133_unused_type_params_tests.rs"]
mod ts6133_unused_type_params_tests;
#[cfg(test)]
#[path = "../tests/value_usage_tests.rs"]
mod value_usage_tests;
// Tests kept in root test harness where shared fixtures live.
#[cfg(test)]
#[path = "../tests/architecture_contract_tests.rs"]
mod architecture_contract_tests;
#[cfg(test)]
#[path = "tests/architecture_contract_tests.rs"]
mod architecture_contract_tests_src;
#[cfg(test)]
#[path = "../tests/conditional_keyof_test.rs"]
mod conditional_keyof_test;
#[cfg(test)]
#[path = "../tests/enum_nominality_tests.rs"]
mod enum_nominality_tests;
#[cfg(test)]
#[path = "../tests/generic_inference_manual.rs"]
mod generic_inference_manual;
#[cfg(test)]
#[path = "../tests/generic_tests.rs"]
mod generic_tests;
#[cfg(test)]
#[path = "../tests/module_resolution_guard_tests.rs"]
mod module_resolution_guard_tests;
#[cfg(test)]
#[path = "../tests/private_brands.rs"]
mod private_brands;
#[cfg(test)]
#[path = "../tests/repro_parserreal.rs"]
mod repro_parserreal;
#[cfg(test)]
#[path = "../tests/strict_null_manual.rs"]
mod strict_null_manual;

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
pub use reachability_analyzer::ReachabilityAnalyzer;
pub use state::{CheckerState, MAX_CALL_DEPTH, MAX_INSTANTIATION_DEPTH};
pub use statements::{StatementCheckCallbacks, StatementChecker};
pub use tsz_solver::Visibility;
pub use type_node::TypeNodeChecker;
