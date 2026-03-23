//! TypeScript name binder for the tsz compiler.
//!
//! This crate provides the shared data structures used across binding, checking,
//! control-flow analysis, and language service features:
//! - `Symbol`, `SymbolId`, `SymbolTable`, `SymbolArena`
//! - `FlowNode`, `FlowNodeId`, `FlowNodeArena`
//! - `Scope`, `ScopeId`, `ContainerKind`, `ScopeContext`
//! - `BinderState` - Name resolution and symbol table construction
//! - `LibFile` - Lib file loading for built-in type definitions

mod binding;
pub mod flow;
pub mod lib_loader;
mod modules;
mod nodes;
pub mod scopes;
pub mod state;
pub mod symbols;

// Re-export core data types at crate root for convenience.
pub use flow::{FlowNode, FlowNodeArena, FlowNodeId, flow_flags};
pub use scopes::{ContainerKind, Scope, ScopeContext, ScopeId};
pub use state::export_surface::{ExportSurface, ExportedSymbol, NamedReexport, WildcardReexport};
pub use state::{
    BinderOptions, BinderState, DeclarationArenaMap, FileFeatures, GlobalAugmentation, LibContext,
    ModuleAugmentation, SemanticDefEntry, SemanticDefKind, ValidationError,
};
pub use symbols::{Symbol, SymbolArena, SymbolId, SymbolTable, symbol_flags};
