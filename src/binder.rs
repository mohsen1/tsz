//! Binder shared types for Project Zang.
//!
//! The binder implementation lives in `src/binder/state.rs`.
//! This module provides the shared data structures used across binding, checking,
//! control-flow analysis, and language service features:
//! - `Symbol`, `SymbolId`, `SymbolTable`, `SymbolArena`
//! - `FlowNode`, `FlowNodeId`, `FlowNodeArena`
//! - `Scope`, `ScopeId`, `ContainerKind`, `ScopeContext`

use crate::parser::NodeIndex;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

pub mod state;
mod state_binding;
pub use state::{BinderState, LibContext, ValidationError};

// =============================================================================
// Symbol Flags
// =============================================================================

/// Flags that describe the kind and properties of a symbol.
/// Matches TypeScript's SymbolFlags enum in src/compiler/types.ts
pub mod symbol_flags {
    pub const NONE: u32 = 0;
    pub const FUNCTION_SCOPED_VARIABLE: u32 = 1 << 0; // Variable (var) or parameter
    pub const BLOCK_SCOPED_VARIABLE: u32 = 1 << 1; // Block-scoped variable (let or const)
    pub const PROPERTY: u32 = 1 << 2; // Property or enum member
    pub const ENUM_MEMBER: u32 = 1 << 3; // Enum member
    pub const FUNCTION: u32 = 1 << 4; // Function
    pub const CLASS: u32 = 1 << 5; // Class
    pub const INTERFACE: u32 = 1 << 6; // Interface
    pub const CONST_ENUM: u32 = 1 << 7; // Const enum
    pub const REGULAR_ENUM: u32 = 1 << 8; // Enum
    pub const VALUE_MODULE: u32 = 1 << 9; // Instantiated module
    pub const NAMESPACE_MODULE: u32 = 1 << 10; // Uninstantiated module
    pub const TYPE_LITERAL: u32 = 1 << 11; // Type Literal or mapped type
    pub const OBJECT_LITERAL: u32 = 1 << 12; // Object Literal
    pub const METHOD: u32 = 1 << 13; // Method
    pub const CONSTRUCTOR: u32 = 1 << 14; // Constructor
    pub const GET_ACCESSOR: u32 = 1 << 15; // Get accessor
    pub const SET_ACCESSOR: u32 = 1 << 16; // Set accessor
    pub const SIGNATURE: u32 = 1 << 17; // Call, construct, or index signature
    pub const TYPE_PARAMETER: u32 = 1 << 18; // Type parameter
    pub const TYPE_ALIAS: u32 = 1 << 19; // Type alias
    pub const EXPORT_VALUE: u32 = 1 << 20; // Exported value marker
    pub const ALIAS: u32 = 1 << 21; // Alias for another symbol
    pub const PROTOTYPE: u32 = 1 << 22; // Prototype property
    pub const EXPORT_STAR: u32 = 1 << 23; // Export * declaration
    pub const OPTIONAL: u32 = 1 << 24; // Optional property
    pub const TRANSIENT: u32 = 1 << 25; // Transient symbol
    pub const ASSIGNMENT: u32 = 1 << 26; // Assignment treated as declaration
    pub const MODULE_EXPORTS: u32 = 1 << 27; // CommonJS module.exports
    pub const PRIVATE: u32 = 1 << 28; // Private member
    pub const PROTECTED: u32 = 1 << 29; // Protected member
    pub const ABSTRACT: u32 = 1 << 30; // Abstract member
    pub const STATIC: u32 = 1 << 31; // Static member

    // Composite flags
    pub const ENUM: u32 = REGULAR_ENUM | CONST_ENUM;
    pub const VARIABLE: u32 = FUNCTION_SCOPED_VARIABLE | BLOCK_SCOPED_VARIABLE;
    pub const VALUE: u32 = VARIABLE
        | PROPERTY
        | ENUM_MEMBER
        | OBJECT_LITERAL
        | FUNCTION
        | CLASS
        | ENUM
        | VALUE_MODULE
        | METHOD
        | GET_ACCESSOR
        | SET_ACCESSOR;
    pub const TYPE: u32 =
        CLASS | INTERFACE | ENUM | ENUM_MEMBER | TYPE_LITERAL | TYPE_PARAMETER | TYPE_ALIAS;
    pub const NAMESPACE: u32 = VALUE_MODULE | NAMESPACE_MODULE | ENUM;
    pub const MODULE: u32 = VALUE_MODULE | NAMESPACE_MODULE;
    pub const ACCESSOR: u32 = GET_ACCESSOR | SET_ACCESSOR;

    // Exclusion flags for redeclaration checks
    // Note: Operator precedence in Rust has & binding tighter than |, so we need parentheses
    // to match TypeScript's semantics for declaration merging rules.
    pub const FUNCTION_SCOPED_VARIABLE_EXCLUDES: u32 = VALUE & !FUNCTION_SCOPED_VARIABLE;
    pub const BLOCK_SCOPED_VARIABLE_EXCLUDES: u32 = VALUE;
    pub const PARAMETER_EXCLUDES: u32 = VALUE;
    pub const PROPERTY_EXCLUDES: u32 = NONE;
    pub const ENUM_MEMBER_EXCLUDES: u32 = VALUE | TYPE;
    // Function can merge with: namespace/module (VALUE_MODULE)
    pub const FUNCTION_EXCLUDES: u32 = VALUE & !FUNCTION & !VALUE_MODULE;
    // Class can merge with: interface, function, and namespace/module
    pub const CLASS_EXCLUDES: u32 = (VALUE | TYPE) & !VALUE_MODULE & !INTERFACE & !FUNCTION;
    // Interface can merge with: interface, class
    pub const INTERFACE_EXCLUDES: u32 = TYPE & !INTERFACE & !CLASS;
    // Enum can merge with: namespace/module and same-kind enum
    pub const REGULAR_ENUM_EXCLUDES: u32 = (VALUE | TYPE) & !REGULAR_ENUM & !VALUE_MODULE;
    pub const CONST_ENUM_EXCLUDES: u32 = (VALUE | TYPE) & !CONST_ENUM & !VALUE_MODULE;
    // Value module (namespace with values) can merge with: function, class, enum, and other value modules
    pub const VALUE_MODULE_EXCLUDES: u32 =
        VALUE & !FUNCTION & !CLASS & !REGULAR_ENUM & !VALUE_MODULE;
    // Pure namespace module can merge with anything
    pub const NAMESPACE_MODULE_EXCLUDES: u32 = NONE;
    pub const METHOD_EXCLUDES: u32 = VALUE & !METHOD;
    pub const GET_ACCESSOR_EXCLUDES: u32 = VALUE & !SET_ACCESSOR;
    pub const SET_ACCESSOR_EXCLUDES: u32 = VALUE & !GET_ACCESSOR;
    pub const TYPE_PARAMETER_EXCLUDES: u32 = TYPE & !TYPE_PARAMETER;
    pub const TYPE_ALIAS_EXCLUDES: u32 = TYPE;
    pub const ALIAS_EXCLUDES: u32 = ALIAS;
}

// =============================================================================
// Symbol
// =============================================================================

/// Unique identifier for a symbol in the symbol table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SymbolId(pub u32);

impl SymbolId {
    pub const NONE: SymbolId = SymbolId(u32::MAX);

    pub fn is_none(&self) -> bool {
        self.0 == u32::MAX
    }
}

/// A symbol represents a named entity in the program.
/// Symbols are created during binding and used during type checking.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Symbol {
    /// Symbol flags describing kind and properties
    pub flags: u32,
    /// Escaped name of the symbol
    pub escaped_name: String,
    /// Declarations associated with this symbol
    pub declarations: Vec<NodeIndex>,
    /// First value declaration of the symbol
    pub value_declaration: NodeIndex,
    /// Parent symbol (for nested symbols)
    pub parent: SymbolId,
    /// Unique ID for this symbol
    pub id: SymbolId,
    /// Exported members for modules/namespaces
    pub exports: Option<Box<SymbolTable>>,
    /// Members for classes/interfaces
    pub members: Option<Box<SymbolTable>>,
    /// Whether this symbol is exported from its container (namespace/module)
    pub is_exported: bool,
    /// Whether this symbol is type-only (e.g., `import type`).
    pub is_type_only: bool,
    /// File index for cross-file resolution (set during multi-file merge)
    /// This indicates which file's arena contains this symbol's declarations.
    /// Value of u32::MAX means single-file mode (use current arena).
    pub decl_file_idx: u32,
    /// Import module specifier for ES6 imports (e.g., './file' for `import { X } from './file'`)
    /// This enables resolving imported symbols to their actual exports from other files.
    pub import_module: Option<String>,
    /// Original export name for imports with renamed imports (e.g., 'foo' for `import { foo as bar }`)
    /// If None, the import name matches the escaped_name.
    pub import_name: Option<String>,
}

impl Symbol {
    /// Create a new symbol with the given flags and name.
    pub fn new(id: SymbolId, flags: u32, name: String) -> Self {
        Symbol {
            flags,
            escaped_name: name,
            declarations: Vec::new(),
            value_declaration: NodeIndex::NONE,
            parent: SymbolId::NONE,
            id,
            exports: None,
            members: None,
            is_exported: false,
            is_type_only: false,
            decl_file_idx: u32::MAX,
            import_module: None,
            import_name: None,
        }
    }

    /// Check if symbol has all specified flags.
    pub fn has_flags(&self, flags: u32) -> bool {
        (self.flags & flags) == flags
    }

    /// Check if symbol has any of specified flags.
    pub fn has_any_flags(&self, flags: u32) -> bool {
        (self.flags & flags) != 0
    }
}

// =============================================================================
// Symbol Table
// =============================================================================

/// A symbol table maps names to symbols.
/// Used for scope management and name resolution.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SymbolTable {
    /// Symbols indexed by their escaped name (using FxHashMap for faster hashing)
    symbols: FxHashMap<String, SymbolId>,
}

impl SymbolTable {
    pub fn new() -> Self {
        SymbolTable {
            symbols: FxHashMap::default(),
        }
    }

    /// Get a symbol by name.
    pub fn get(&self, name: &str) -> Option<SymbolId> {
        self.symbols.get(name).copied()
    }

    /// Set a symbol by name.
    pub fn set(&mut self, name: String, symbol: SymbolId) {
        self.symbols.insert(name, symbol);
    }

    /// Remove a symbol by name.
    pub fn remove(&mut self, name: &str) -> Option<SymbolId> {
        self.symbols.remove(name)
    }

    /// Check if a name exists in the table.
    pub fn has(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }

    /// Get number of symbols.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Clear all symbols while keeping the allocated capacity.
    pub fn clear(&mut self) {
        self.symbols.clear();
    }

    /// Iterate over symbols.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &SymbolId)> {
        self.symbols.iter()
    }
}

// =============================================================================
// Symbol Arena
// =============================================================================

/// Arena allocator for symbols.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SymbolArena {
    symbols: Vec<Symbol>,
    /// Base offset for symbol IDs (0 for binder, high value for checker-local symbols)
    base_offset: u32,
}

impl SymbolArena {
    /// Base offset for checker-local symbols to avoid ID collisions.
    pub const CHECKER_SYMBOL_BASE: u32 = 0x10000000;
    /// Maximum pre-allocation to avoid capacity overflow.
    const MAX_SYMBOL_PREALLOC: usize = 1_000_000;

    pub fn new() -> Self {
        SymbolArena {
            symbols: Vec::new(),
            base_offset: 0,
        }
    }

    /// Create a new symbol arena with a base offset for symbol IDs.
    /// Used for checker-local symbols to avoid collisions with binder symbols.
    pub fn new_with_base(base: u32) -> Self {
        SymbolArena {
            symbols: Vec::new(),
            base_offset: base,
        }
    }

    /// Create a new symbol arena with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let safe_capacity = capacity.min(Self::MAX_SYMBOL_PREALLOC);
        SymbolArena {
            symbols: Vec::with_capacity(safe_capacity),
            base_offset: 0,
        }
    }

    /// Allocate a new symbol and return its ID.
    pub fn alloc(&mut self, flags: u32, name: String) -> SymbolId {
        let id = SymbolId(self.base_offset + self.symbols.len() as u32);
        self.symbols.push(Symbol::new(id, flags, name));
        id
    }

    /// Allocate a new symbol by cloning from an existing one, with a new ID.
    /// This copies all symbol data including declarations, exports, members, etc.
    pub fn alloc_from(&mut self, source: &Symbol) -> SymbolId {
        let id = SymbolId(self.base_offset + self.symbols.len() as u32);
        let mut cloned = source.clone();
        cloned.id = id;
        self.symbols.push(cloned);
        id
    }

    /// Get a symbol by ID.
    pub fn get(&self, id: SymbolId) -> Option<&Symbol> {
        if id.is_none() {
            None
        } else if id.0 < self.base_offset {
            // ID is from a different arena (e.g., binder vs checker)
            None
        } else {
            self.symbols.get((id.0 - self.base_offset) as usize)
        }
    }

    /// Get a mutable symbol by ID.
    pub fn get_mut(&mut self, id: SymbolId) -> Option<&mut Symbol> {
        if id.is_none() {
            None
        } else if id.0 < self.base_offset {
            // ID is from a different arena
            None
        } else {
            self.symbols.get_mut((id.0 - self.base_offset) as usize)
        }
    }

    /// Get the number of symbols.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Clear all symbols while keeping the allocated capacity.
    pub fn clear(&mut self) {
        self.symbols.clear();
    }

    /// Find a symbol by name (linear search through all symbols).
    ///
    /// This is a fallback for when scope chain lookup is not available.
    /// Note: This doesn't handle shadowing correctly - it returns the first match.
    /// For proper scoping, use the SymbolTable scope chain instead.
    pub fn find_by_name(&self, name: &str) -> Option<SymbolId> {
        for symbol in &self.symbols {
            if symbol.escaped_name == name {
                return Some(symbol.id);
            }
        }
        None
    }
}

// =============================================================================
// Control Flow Graph
// =============================================================================

/// Flags for flow nodes describing their type and properties.
/// Matches TypeScript's FlowFlags in src/compiler/types.ts
pub mod flow_flags {
    pub const UNREACHABLE: u32 = 1 << 0; // Unreachable code
    pub const START: u32 = 1 << 1; // Start of flow graph
    pub const BRANCH_LABEL: u32 = 1 << 2; // Branch label
    pub const LOOP_LABEL: u32 = 1 << 3; // Loop label
    pub const ASSIGNMENT: u32 = 1 << 4; // Assignment
    pub const TRUE_CONDITION: u32 = 1 << 5; // True condition
    pub const FALSE_CONDITION: u32 = 1 << 6; // False condition
    pub const SWITCH_CLAUSE: u32 = 1 << 7; // Switch clause
    pub const ARRAY_MUTATION: u32 = 1 << 8; // Array mutation
    pub const CALL: u32 = 1 << 9; // Call expression
    pub const REDUCE_LABEL: u32 = 1 << 10; // Reduce label
    pub const REFERENCED: u32 = 1 << 11; // Referenced
    pub const AWAIT_POINT: u32 = 1 << 12; // Await expression (suspension point)
    pub const YIELD_POINT: u32 = 1 << 13; // Yield expression (generator suspension point)

    // Composite flags
    pub const LABEL: u32 = BRANCH_LABEL | LOOP_LABEL;
    pub const CONDITION: u32 = TRUE_CONDITION | FALSE_CONDITION;
}

/// Unique identifier for a flow node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlowNodeId(pub u32);

impl FlowNodeId {
    pub const NONE: FlowNodeId = FlowNodeId(u32::MAX);

    pub fn is_none(&self) -> bool {
        self.0 == u32::MAX
    }
}

/// A node in the control flow graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlowNode {
    /// Flow node flags
    pub flags: u32,
    /// Flow node ID
    pub id: FlowNodeId,
    /// Antecedent flow node(s) - predecessors in the control flow
    pub antecedent: Vec<FlowNodeId>,
    /// Associated AST node (for assignments, conditions, etc.)
    pub node: NodeIndex,
}

impl FlowNode {
    pub fn new(id: FlowNodeId, flags: u32) -> Self {
        FlowNode {
            flags,
            id,
            antecedent: Vec::new(),
            node: NodeIndex::NONE,
        }
    }

    pub fn has_flags(&self, flags: u32) -> bool {
        (self.flags & flags) == flags
    }

    pub fn has_any_flags(&self, flags: u32) -> bool {
        (self.flags & flags) != 0
    }
}

/// Arena for flow nodes.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FlowNodeArena {
    nodes: Vec<FlowNode>,
}

impl FlowNodeArena {
    pub fn new() -> Self {
        FlowNodeArena { nodes: Vec::new() }
    }

    /// Allocate a new flow node.
    pub fn alloc(&mut self, flags: u32) -> FlowNodeId {
        let id = FlowNodeId(self.nodes.len() as u32);
        self.nodes.push(FlowNode::new(id, flags));
        id
    }

    /// Get a flow node by ID.
    pub fn get(&self, id: FlowNodeId) -> Option<&FlowNode> {
        if id.is_none() {
            None
        } else {
            self.nodes.get(id.0 as usize)
        }
    }

    /// Get a mutable flow node by ID.
    pub fn get_mut(&mut self, id: FlowNodeId) -> Option<&mut FlowNode> {
        if id.is_none() {
            None
        } else {
            self.nodes.get_mut(id.0 as usize)
        }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
    }

    /// Find the unreachable flow node in the arena.
    /// This is used when reconstructing a BinderState from serialized flow data.
    pub fn find_unreachable(&self) -> Option<FlowNodeId> {
        for (idx, node) in self.nodes.iter().enumerate() {
            if node.has_any_flags(flow_flags::UNREACHABLE) {
                return Some(FlowNodeId(idx as u32));
            }
        }
        None
    }
}

// =============================================================================
// Persistent Scope System
// =============================================================================

/// Unique identifier for a persistent scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScopeId(pub u32);

impl ScopeId {
    pub const NONE: ScopeId = ScopeId(u32::MAX);

    pub fn is_none(&self) -> bool {
        self.0 == u32::MAX
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
    pub fn new(parent: ScopeId, kind: ContainerKind, node: NodeIndex) -> Self {
        Scope {
            parent,
            table: SymbolTable::new(),
            kind,
            container_node: node,
        }
    }

    /// Check if this scope is a function scope (where var hoisting happens)
    pub fn is_function_scope(&self) -> bool {
        matches!(
            self.kind,
            ContainerKind::SourceFile | ContainerKind::Function | ContainerKind::Module
        )
    }
}

/// Scope context - tracks scope chain and hoisting (used by BinderState).
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
    pub fn new(kind: ContainerKind, node: NodeIndex, parent: Option<usize>) -> Self {
        ScopeContext {
            locals: SymbolTable::new(),
            parent_idx: parent,
            container_kind: kind,
            container_node: node,
            hoisted_vars: Vec::new(),
            hoisted_functions: Vec::new(),
        }
    }

    /// Check if this scope is a function scope (where var hoisting happens)
    pub fn is_function_scope(&self) -> bool {
        matches!(
            self.container_kind,
            ContainerKind::SourceFile | ContainerKind::Function | ContainerKind::Module
        )
    }
}
