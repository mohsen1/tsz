//! TypeScript name binder for the tsz compiler.
//!
//! This crate provides the shared data structures used across binding, checking,
//! control-flow analysis, and language service features:
//! - `Symbol`, `SymbolId`, `SymbolTable`, `SymbolArena`
//! - `FlowNode`, `FlowNodeId`, `FlowNodeArena`
//! - `Scope`, `ScopeId`, `ContainerKind`, `ScopeContext`
//! - `BinderState` - Name resolution and symbol table construction
//! - `LibFile` - Lib file loading for built-in type definitions

pub mod flow;
pub mod lib_loader;
pub mod module_resolution_debug;
pub mod scopes;
pub mod state;
mod state_binding;
mod state_binding_validation;
mod state_flow_helpers;
mod state_import_export;
mod state_lib_merge;
mod state_module_binding;
mod state_node_binding;
mod state_node_binding_names;
mod state_resolution;
pub mod symbols;

// Re-export core data types at crate root for convenience.
pub use flow::{FlowNode, FlowNodeArena, FlowNodeId, flow_flags};
pub use scopes::{ContainerKind, Scope, ScopeContext, ScopeId};
pub use state::{
    BinderOptions, BinderState, DeclarationArenaMap, FileFeatures, GlobalAugmentation, LibContext,
    ModuleAugmentation, ValidationError,
};
pub use symbols::{Symbol, SymbolArena, SymbolId, SymbolTable, symbol_flags};
