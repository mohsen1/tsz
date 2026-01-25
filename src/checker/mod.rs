//! Type checker module for TypeScript AST.
//!
//! This module is organized into several submodules:
//! - `types` - Type definitions (Type enum, flags, diagnostics)
//! - `arena` - TypeArena for type allocation
//! - `context` - CheckerContext for shared state
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
//! is an alias to the thin checker. The types module is shared with the solver.

pub mod accessibility;
pub mod arena;
pub mod array_type;
pub mod callable_type;
pub mod class_type;
pub mod conditional_type;
pub mod constructor_checker;
pub mod context;
pub mod control_flow;
pub mod declarations;
pub mod decorators;
pub mod enum_checker;
pub mod error_handler;
pub mod error_reporter;
pub mod expr;
pub mod flow_analysis;
pub mod flow_analyzer;
pub mod flow_graph_builder;
pub mod flow_narrowing;
pub mod function_type;
pub mod indexed_access_type;
pub mod interface_type;
pub mod intersection_type;
pub mod iterable_checker;
pub mod jsx;
pub mod literal_type;
// pub mod module_validation; // TODO: Fix API mismatches
pub mod nullish;
pub mod object_type;
pub mod optional_chain;
pub mod promise_checker;
pub mod reachability_analyzer;
pub mod state;
pub mod statements;
pub mod symbol_resolver;
pub mod tuple_type;
pub mod type_checking;
pub mod type_computation;
// pub mod type_computing_visitor; // TODO: module not found
pub mod type_parameter;
pub mod type_query;
pub mod types;
pub mod union_type;

#[cfg(test)]
mod contextual_typing_tests;
#[cfg(test)]
mod control_flow_tests;
#[cfg(test)]
mod error_cascade_tests;
#[cfg(test)]
mod global_type_tests;
#[cfg(test)]
mod no_filename_based_behavior_tests;
#[cfg(test)]
mod spread_rest_tests;
#[cfg(test)]
mod stability_validation_tests;
#[cfg(test)]
mod symbol_resolver_stability_tests;
#[cfg(test)]
mod ts2304_tests;
#[cfg(test)]
mod value_usage_tests;

// Re-export key types
pub use arena::TypeArena;
pub use context::{CheckerContext, CheckerOptions, EnclosingClassInfo, TypeCache};
pub use control_flow::{FlowAnalyzer, FlowGraph as ControlFlowGraph};
pub use declarations::DeclarationChecker;
pub use expr::ExpressionChecker;
pub use flow_analyzer::{
    AssignmentState, AssignmentStateMap, DefiniteAssignmentAnalyzer, DefiniteAssignmentResult,
    merge_assignment_states,
};
pub use flow_graph_builder::{FlowGraph, FlowGraphBuilder};
pub use reachability_analyzer::ReachabilityAnalyzer;
pub use state::{CheckerState, MAX_CALL_DEPTH, MAX_INSTANTIATION_DEPTH};
pub use statements::StatementChecker;
pub use types::{
    ArrayTypeInfo, ConditionalType, EnumTypeInfo, FunctionType, IndexInfo, IndexType,
    IndexedAccessType, IntersectionType, IntrinsicType, LiteralType, LiteralValue, MappedType,
    ObjectType, Signature, TemplateLiteralType, TupleTypeInfo, Type, TypeId, TypeParameter,
    TypeReference, UnionType, diagnostic_codes, object_flags, signature_flags, type_flags,
};
