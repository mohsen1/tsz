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
//! - `error_reporter` - Error reporting utilities
//!
//! Note: The thin checker is the unified checker pipeline; `CheckerState`
//! is an alias to the thin checker.

extern crate self as tsz_checker;

pub mod accessor_checker;
pub mod assignability_checker;
pub mod assignment_checker;
pub mod call_checker;
pub mod context;
mod context_constructors;
mod context_def_mapping;
mod context_resolver;
pub mod decorators;
pub mod dispatch;
pub mod enum_checker;
pub mod error_handler;
pub mod error_reporter;
pub mod expr;
pub mod generic_checker;
pub mod iterable_checker;
pub mod jsx_checker;
pub mod judge_integration;
pub mod module_resolution;
pub mod optional_chain;
pub mod parameter_checker;
pub mod promise_checker;
pub mod property_checker;
mod query_boundaries;
pub mod signature_builder;
pub mod statements;
pub mod triple_slash_validator;

#[path = "classes/mod.rs"]
mod classes_domain;
#[path = "declarations/mod.rs"]
mod declarations_domain;
#[path = "flow/mod.rs"]
mod flow_domain;
#[path = "state/mod.rs"]
mod state_domain;
#[path = "symbols/mod.rs"]
mod symbols_domain;
#[path = "types/mod.rs"]
mod types_domain;

pub use classes_domain::{
    class_checker, class_inheritance, constructor_checker, private_checker, super_checker,
};
#[allow(unused_imports)]
pub(crate) use classes_domain::{class_checker_compat, class_implements_checker};

pub use declarations_domain::{declarations, import_checker, module_checker, namespace_checker};
#[allow(unused_imports)]
pub(crate) use declarations_domain::{
    declarations_module, declarations_module_helpers, import_declaration_checker,
};

pub use flow_domain::{
    control_flow, flow_analysis, flow_analyzer, flow_graph_builder, reachability_analyzer,
    reachability_checker,
};
#[allow(unused_imports)]
pub(crate) use flow_domain::{
    control_flow_assignment, control_flow_narrowing, control_flow_type_guards,
    flow_analysis_definite, flow_analysis_usage,
};

pub use state_domain::{
    state, state_checking, state_type_analysis, state_type_environment, state_type_resolution,
};
#[allow(unused_imports)]
pub(crate) use state_domain::{
    state_checking_members, state_class_checking, state_property_checking,
    state_type_analysis_computed, state_type_analysis_computed_helpers,
    state_type_analysis_cross_file, state_type_environment_lazy, state_type_resolution_module,
    state_variable_checking, state_variable_checking_destructuring,
};

pub use symbols_domain::{scope_finder, symbol_resolver};
#[allow(unused_imports)]
pub(crate) use symbols_domain::symbol_resolver_utils;

pub use types_domain::{
    class_type, function_type, interface_type, literal_type, object_type, type_checking,
    type_computation, type_literal_checker, type_node,
};
#[allow(unused_imports)]
pub(crate) use types_domain::{
    property_access_type, type_checking_declarations, type_checking_declarations_utils,
    type_checking_global, type_checking_property_init, type_checking_queries,
    type_checking_queries_binding, type_checking_queries_class, type_checking_queries_lib,
    type_checking_unused, type_checking_utilities, type_checking_utilities_enum,
    type_checking_utilities_jsdoc, type_computation_access, type_computation_call,
    type_computation_call_helpers, type_computation_complex,
};

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
#[path = "../tests/control_flow_type_guard_tests.rs"]
mod control_flow_type_guard_tests;
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
#[path = "../tests/ts1214_let_strict_mode_tests.rs"]
mod ts1214_let_strict_mode_tests;
#[cfg(test)]
#[path = "../tests/ts2300_tests.rs"]
mod ts2300_tests;
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
#[path = "../tests/class_index_signature_compat_tests.rs"]
mod class_index_signature_compat_tests;
#[cfg(test)]
#[path = "../tests/conditional_keyof_test.rs"]
mod conditional_keyof_test;
#[cfg(test)]
#[path = "../tests/enum_nominality_tests.rs"]
mod enum_nominality_tests;
#[cfg(test)]
#[path = "../tests/flow_boundary_contract_tests.rs"]
mod flow_boundary_contract_tests;
#[cfg(test)]
#[path = "../tests/generic_inference_manual.rs"]
mod generic_inference_manual;
#[cfg(test)]
#[path = "../tests/generic_tests.rs"]
mod generic_tests;
#[cfg(test)]
#[path = "../tests/member_access_architecture_boundary_tests.rs"]
mod member_access_architecture_boundary_tests;
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
