//! Persistent scope system for the binder.
//!
//! Provides `Scope`, `ScopeId`, `ScopeContext`, and `ContainerKind`.

use serde::{Deserialize, Serialize};
use tsz_parser::NodeIndex;

use crate::symbols::SymbolTable;

// =============================================================================
// Persistent Scope System
// =============================================================================

/// Unique identifier for a persistent scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScopeId(pub u32);

impl ScopeId {
    pub const NONE: Self = Self(u32::MAX);

    #[must_use]
    pub const fn is_none(&self) -> bool {
        self.0 == u32::MAX
    }

    #[must_use]
    pub const fn is_some(&self) -> bool {
        self.0 != u32::MAX
    }
}

/// Container kind - tracks what kind of scope we're in
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerKind {
    /// Source file (global scope)
    SourceFile,
    /// Function/method body (creates function scope)
    Function,
    /// Module/namespace body
    Module,
    /// Class body
    Class,
    /// Block (if, while, for, etc.) - only creates block scope
    Block,
}

/// A persistent scope containing symbols and a link to its parent.
/// This enables stateless checking by allowing the checker to query
/// scope information without maintaining a traversal-order-dependent stack.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scope {
    /// Parent scope ID (for scope chain lookup)
    pub parent: ScopeId,
    /// Symbols defined in this scope
    pub table: SymbolTable,
    /// The kind of container this scope represents
    pub kind: ContainerKind,
    /// The AST node that created this scope
    pub container_node: NodeIndex,
}

impl Scope {
    #[must_use]
    pub fn new(parent: ScopeId, kind: ContainerKind, node: NodeIndex) -> Self {
        Self {
            parent,
            table: SymbolTable::new(),
            kind,
            container_node: node,
        }
    }

    /// Create a scope with pre-allocated capacity for its symbol table.
    /// Useful for class scopes where the member count is known from the AST.
    #[must_use]
    pub fn with_capacity(
        parent: ScopeId,
        kind: ContainerKind,
        node: NodeIndex,
        capacity: usize,
    ) -> Self {
        Self {
            parent,
            table: SymbolTable::with_capacity(capacity),
            kind,
            container_node: node,
        }
    }

    /// Check if this scope is a function scope (where var hoisting happens)
    #[must_use]
    pub const fn is_function_scope(&self) -> bool {
        matches!(
            self.kind,
            ContainerKind::SourceFile | ContainerKind::Function | ContainerKind::Module
        )
    }
}

/// Scope context - tracks scope chain and hoisting (used by `BinderState`).
#[derive(Clone, Debug)]
pub struct ScopeContext {
    /// The symbol table for this scope
    pub locals: SymbolTable,
    /// Parent scope (for scope chain lookup)
    pub parent_idx: Option<usize>,
    /// The kind of container this scope belongs to
    pub container_kind: ContainerKind,
    /// Node index of the container
    pub container_node: NodeIndex,
    /// Hoisted var declarations (for function scope)
    pub hoisted_vars: Vec<(String, NodeIndex)>,
    /// Hoisted function declarations (for function scope)
    pub hoisted_functions: Vec<(String, NodeIndex)>,
}

impl ScopeContext {
    #[must_use]
    pub fn new(kind: ContainerKind, node: NodeIndex, parent: Option<usize>) -> Self {
        Self {
            locals: SymbolTable::new(),
            parent_idx: parent,
            container_kind: kind,
            container_node: node,
            hoisted_vars: Vec::new(),
            hoisted_functions: Vec::new(),
        }
    }

    /// Check if this scope is a function scope (where var hoisting happens)
    #[must_use]
    pub const fn is_function_scope(&self) -> bool {
        matches!(
            self.container_kind,
            ContainerKind::SourceFile | ContainerKind::Function | ContainerKind::Module
        )
    }
}
