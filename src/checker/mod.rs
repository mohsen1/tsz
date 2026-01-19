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
//!
//! Note: CheckerState has been replaced by ThinCheckerState in thin_checker.rs
//! The types module is still used by both ThinChecker and Solver.

pub mod arena;
pub mod context;
pub mod control_flow;
pub mod declarations;
pub mod decorators;
pub mod expr;
pub mod flow_analyzer;
pub mod flow_graph_builder;
pub mod jsx;
pub mod nullish;
pub mod optional_chain;
pub mod reachability_analyzer;
pub mod statements;
pub mod types;

#[cfg(test)]
mod control_flow_tests;

// Re-export key types
pub use arena::TypeArena;
pub use context::{CheckerContext, EnclosingClassInfo, TypeCache};
pub use control_flow::{FlowAnalyzer, FlowGraph as ControlFlowGraph};
pub use declarations::DeclarationChecker;
pub use expr::ExpressionChecker;
pub use flow_analyzer::{
    AssignmentState, AssignmentStateMap, DefiniteAssignmentAnalyzer, DefiniteAssignmentResult,
    merge_assignment_states,
};
pub use flow_graph_builder::{FlowGraph, FlowGraphBuilder};
pub use reachability_analyzer::ReachabilityAnalyzer;
pub use statements::StatementChecker;
pub use types::{
    ArrayTypeInfo, ConditionalType, EnumTypeInfo, FunctionType, IndexInfo, IndexType,
    IndexedAccessType, IntersectionType, IntrinsicType, LiteralType, LiteralValue, MappedType,
    ObjectType, Signature, TemplateLiteralType, TupleTypeInfo, Type, TypeId, TypeParameter,
    TypeReference, UnionType, diagnostic_codes, object_flags, signature_flags, type_flags,
};
