//! Binder implementation for TypeScript AST.
//!
//! The binder walks the AST and creates symbols, establishing
//! scope and name resolution.

// Allow dead code for binder infrastructure methods that will be used in future phases
#![allow(dead_code)]

use crate::parser::NodeIndex;
use crate::parser::node_flags;
use crate::parser::syntax_kind_ext;
use crate::parser::thin_node::NodeAccess;
use crate::scanner::SyntaxKind;
use rustc_hash::FxHashMap;
use serde::Serialize; // For syntax kind constants like MODULE_BLOCK, etc.

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
    pub const FUNCTION_SCOPED_VARIABLE_EXCLUDES: u32 = VALUE & !FUNCTION_SCOPED_VARIABLE;
    pub const BLOCK_SCOPED_VARIABLE_EXCLUDES: u32 = VALUE;
    pub const PARAMETER_EXCLUDES: u32 = VALUE;
    pub const PROPERTY_EXCLUDES: u32 = NONE;
    pub const ENUM_MEMBER_EXCLUDES: u32 = VALUE | TYPE;
    pub const FUNCTION_EXCLUDES: u32 = VALUE & !FUNCTION;
    pub const CLASS_EXCLUDES: u32 = VALUE | TYPE & !VALUE_MODULE & !INTERFACE & !FUNCTION;
    pub const INTERFACE_EXCLUDES: u32 = TYPE & !INTERFACE & !CLASS;
    pub const REGULAR_ENUM_EXCLUDES: u32 = VALUE | TYPE & !REGULAR_ENUM;
    pub const CONST_ENUM_EXCLUDES: u32 = VALUE | TYPE & !CONST_ENUM;
    pub const VALUE_MODULE_EXCLUDES: u32 =
        VALUE & !FUNCTION & !CLASS & !REGULAR_ENUM & !VALUE_MODULE;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct SymbolId(pub u32);

impl SymbolId {
    pub const NONE: SymbolId = SymbolId(u32::MAX);

    pub fn is_none(&self) -> bool {
        self.0 == u32::MAX
    }
}

/// A symbol represents a named entity in the program.
/// Symbols are created during binding and used during type checking.
#[derive(Clone, Debug, Serialize)]
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
#[derive(Clone, Debug, Default, Serialize)]
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
#[derive(Clone, Debug, Serialize)]
pub struct SymbolArena {
    symbols: Vec<Symbol>,
    /// Base offset for symbol IDs (0 for binder, high value for checker-local symbols)
    base_offset: u32,
}

/// Node-to-symbol mapping for language service support.
/// Maps AST node indices to their corresponding symbols.
#[derive(Debug, Default, Serialize)]
pub struct NodeSymbolMap {
    /// Maps NodeIndex to SymbolId
    map: FxHashMap<u32, SymbolId>,
}

impl NodeSymbolMap {
    pub fn new() -> Self {
        Self {
            map: FxHashMap::default(),
        }
    }

    /// Associate a node with a symbol.
    pub fn set(&mut self, node: NodeIndex, symbol: SymbolId) {
        if !node.is_none() && !symbol.is_none() {
            self.map.insert(node.0, symbol);
        }
    }

    /// Get the symbol for a node.
    pub fn get(&self, node: NodeIndex) -> Option<SymbolId> {
        if node.is_none() {
            None
        } else {
            self.map.get(&node.0).copied()
        }
    }

    /// Check if a node has a symbol.
    pub fn has(&self, node: NodeIndex) -> bool {
        !node.is_none() && self.map.contains_key(&node.0)
    }

    /// Get the number of mappings.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Iterate over all mappings.
    pub fn iter(&self) -> impl Iterator<Item = (&u32, &SymbolId)> {
        self.map.iter()
    }
}

impl Default for SymbolArena {
    fn default() -> Self {
        SymbolArena {
            symbols: Vec::new(),
            base_offset: 0,
        }
    }
}

impl SymbolArena {
    /// Base offset for checker-local symbols to avoid ID collisions.
    pub const CHECKER_SYMBOL_BASE: u32 = 0x10000000;

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
        SymbolArena {
            symbols: Vec::with_capacity(capacity),
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct FlowNodeId(pub u32);

impl FlowNodeId {
    pub const NONE: FlowNodeId = FlowNodeId(u32::MAX);

    pub fn is_none(&self) -> bool {
        self.0 == u32::MAX
    }
}

/// A node in the control flow graph.
#[derive(Clone, Debug, Serialize)]
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
#[derive(Clone, Debug, Default, Serialize)]
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
}

// =============================================================================
// Persistent Scope System
// =============================================================================

/// Unique identifier for a persistent scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct ScopeId(pub u32);

impl ScopeId {
    pub const NONE: ScopeId = ScopeId(u32::MAX);

    pub fn is_none(&self) -> bool {
        self.0 == u32::MAX
    }
}

/// Container kind - tracks what kind of scope we're in
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
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
#[derive(Clone, Debug, Serialize)]
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

// =============================================================================
// Binder State
// =============================================================================

use crate::parser::{Node, NodeArena};
use wasm_bindgen::prelude::*;

/// Scope context - tracks scope chain and hoisting
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

/// Binder state for walking the AST and creating symbols.
#[wasm_bindgen]
pub struct BinderState {
    /// Arena for allocating symbols
    #[wasm_bindgen(skip)]
    pub symbols: SymbolArena,
    /// Current container's symbol table (locals)
    #[wasm_bindgen(skip)]
    pub current_scope: SymbolTable,
    /// Stack of scopes for nested blocks
    scope_stack: Vec<SymbolTable>,
    /// File-level symbol table
    #[wasm_bindgen(skip)]
    pub file_locals: SymbolTable,
    /// Flow node arena for control flow analysis
    #[wasm_bindgen(skip)]
    pub flow_nodes: FlowNodeArena,
    /// Current flow node
    current_flow: FlowNodeId,
    /// Unreachable flow node (for never-returning code)
    unreachable_flow: FlowNodeId,
    /// Scope chain - stack of scope contexts
    scope_chain: Vec<ScopeContext>,
    /// Current scope index in scope_chain
    current_scope_idx: usize,
    /// Node-to-symbol mapping for language service support
    #[wasm_bindgen(skip)]
    pub node_symbols: NodeSymbolMap,
}

impl BinderState {
    pub fn new() -> Self {
        let mut flow_nodes = FlowNodeArena::new();
        // Create the unreachable flow node
        let unreachable_flow = flow_nodes.alloc(flow_flags::UNREACHABLE);

        BinderState {
            symbols: SymbolArena::new(),
            current_scope: SymbolTable::new(),
            scope_stack: Vec::new(),
            file_locals: SymbolTable::new(),
            flow_nodes,
            current_flow: FlowNodeId::NONE,
            unreachable_flow,
            scope_chain: Vec::new(),
            current_scope_idx: 0,
            node_symbols: NodeSymbolMap::new(),
        }
    }

    /// Bind a source file, creating symbols for all declarations.
    pub fn bind_source_file(&mut self, arena: &NodeArena, root: NodeIndex) {
        // Initialize scope chain with source file scope
        self.scope_chain.clear();
        self.scope_chain
            .push(ScopeContext::new(ContainerKind::SourceFile, root, None));
        self.current_scope_idx = 0;
        self.current_scope = SymbolTable::new();

        // Create START flow node for the file
        let start_flow = self.flow_nodes.alloc(flow_flags::START);
        self.current_flow = start_flow;

        if let Some(node) = arena.get(root) {
            if let Node::SourceFile(sf) = node {
                // First pass: collect hoisted declarations
                self.collect_hoisted_declarations(arena, &sf.statements.nodes);

                // Process hoisted function declarations first
                self.process_hoisted_functions(arena);

                // Second pass: bind each statement
                for &stmt_idx in &sf.statements.nodes {
                    self.bind_node(arena, stmt_idx);
                }
            }
        }

        // Store file locals
        self.file_locals = std::mem::take(&mut self.current_scope);
    }

    /// Collect hoisted declarations from statements.
    /// var declarations are hoisted to the containing function scope.
    /// function declarations are hoisted to the top of the containing function scope.
    fn collect_hoisted_declarations(&mut self, arena: &NodeArena, statements: &[NodeIndex]) {
        for &stmt_idx in statements {
            if let Some(node) = arena.get(stmt_idx) {
                match node {
                    // var declarations are hoisted
                    Node::VariableStatement(stmt) => {
                        if let Some(Node::VariableDeclarationList(list)) =
                            arena.get(stmt.declaration_list)
                        {
                            // Check if this is a var declaration (not let/const)
                            // Use proper flags instead of magic numbers
                            let is_var =
                                (list.base.flags & (node_flags::LET | node_flags::CONST)) == 0;
                            if is_var {
                                for &decl_idx in &list.declarations.nodes {
                                    if let Some(Node::VariableDeclaration(decl)) =
                                        arena.get(decl_idx)
                                    {
                                        if let Some(name) =
                                            self.get_identifier_name(arena, decl.name)
                                        {
                                            self.add_hoisted_var(name.to_string(), decl_idx);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // function declarations are hoisted
                    Node::FunctionDeclaration(func) => {
                        if let Some(name) = self.get_identifier_name(arena, func.name) {
                            self.add_hoisted_function(name.to_string(), stmt_idx);
                        }
                    }
                    // Recurse into blocks for var hoisting (but not let/const)
                    Node::Block(block) => {
                        self.collect_hoisted_declarations(arena, &block.statements.nodes);
                    }
                    Node::IfStatement(if_stmt) => {
                        if let Some(Node::Block(block)) = arena.get(if_stmt.then_statement) {
                            self.collect_hoisted_declarations(arena, &block.statements.nodes);
                        }
                        if !if_stmt.else_statement.is_none() {
                            if let Some(Node::Block(block)) = arena.get(if_stmt.else_statement) {
                                self.collect_hoisted_declarations(arena, &block.statements.nodes);
                            }
                        }
                    }
                    Node::WhileStatement(while_stmt) => {
                        if let Some(Node::Block(block)) = arena.get(while_stmt.statement) {
                            self.collect_hoisted_declarations(arena, &block.statements.nodes);
                        }
                    }
                    Node::ForStatement(for_stmt) => {
                        if let Some(Node::Block(block)) = arena.get(for_stmt.statement) {
                            self.collect_hoisted_declarations(arena, &block.statements.nodes);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Add a hoisted var declaration to the current function scope
    fn add_hoisted_var(&mut self, name: String, decl_idx: NodeIndex) {
        if let Some(scope) = self.scope_chain.get_mut(self.current_scope_idx) {
            scope.hoisted_vars.push((name, decl_idx));
        }
    }

    /// Add a hoisted function declaration to the current function scope
    fn add_hoisted_function(&mut self, name: String, decl_idx: NodeIndex) {
        if let Some(scope) = self.scope_chain.get_mut(self.current_scope_idx) {
            scope.hoisted_functions.push((name, decl_idx));
        }
    }

    /// Process hoisted function declarations (declare them before other code)
    fn process_hoisted_functions(&mut self, arena: &NodeArena) {
        // Get hoisted functions from current scope
        let hoisted = if let Some(scope) = self.scope_chain.get(self.current_scope_idx) {
            scope.hoisted_functions.clone()
        } else {
            return;
        };

        // Declare each hoisted function
        for (name, decl_idx) in hoisted {
            if let Some(Node::FunctionDeclaration(_)) = arena.get(decl_idx) {
                self.declare_symbol(name, symbol_flags::FUNCTION, decl_idx);
            }
        }
    }

    /// Enter a new scope (function, block, etc.)
    fn enter_scope(&mut self, kind: ContainerKind, node: NodeIndex) {
        let parent = Some(self.current_scope_idx);
        let new_idx = self.scope_chain.len();
        self.scope_chain.push(ScopeContext::new(kind, node, parent));
        self.current_scope_idx = new_idx;

        // Also push to the legacy scope stack for compatibility
        let old_scope = std::mem::take(&mut self.current_scope);
        self.scope_stack.push(old_scope);
        self.current_scope = SymbolTable::new();
    }

    /// Exit the current scope
    fn exit_scope(&mut self) {
        // Pop from scope chain
        if let Some(scope) = self.scope_chain.get(self.current_scope_idx) {
            if let Some(parent_idx) = scope.parent_idx {
                self.current_scope_idx = parent_idx;
            }
        }

        // Pop from legacy scope stack
        if let Some(parent_scope) = self.scope_stack.pop() {
            self.current_scope = parent_scope;
        }
    }

    /// Find the enclosing function scope for var hoisting
    fn find_function_scope_idx(&self) -> usize {
        let mut idx = self.current_scope_idx;
        while let Some(scope) = self.scope_chain.get(idx) {
            if scope.is_function_scope() {
                return idx;
            }
            if let Some(parent_idx) = scope.parent_idx {
                idx = parent_idx;
            } else {
                break;
            }
        }
        0 // Fall back to source file scope
    }

    /// Look up a symbol in the scope chain
    pub fn lookup_symbol(&self, name: &str) -> Option<SymbolId> {
        // First check current scope
        if let Some(id) = self.current_scope.get(name) {
            return Some(id);
        }

        // Walk up the scope chain
        let mut idx = self.current_scope_idx;
        while let Some(scope) = self.scope_chain.get(idx) {
            if let Some(id) = scope.locals.get(name) {
                return Some(id);
            }
            if let Some(parent_idx) = scope.parent_idx {
                idx = parent_idx;
            } else {
                break;
            }
        }

        // Finally check file locals
        self.file_locals.get(name)
    }

    /// Create a new flow node and set it as current.
    fn create_flow_node(
        &mut self,
        flags: u32,
        antecedent: FlowNodeId,
        node: NodeIndex,
    ) -> FlowNodeId {
        let flow_id = self.flow_nodes.alloc(flags);
        if let Some(flow) = self.flow_nodes.get_mut(flow_id) {
            if !antecedent.is_none() {
                flow.antecedent.push(antecedent);
            }
            flow.node = node;
        }
        flow_id
    }

    /// Create a branch label flow node (for merging control flow).
    fn create_branch_label(&mut self) -> FlowNodeId {
        self.flow_nodes.alloc(flow_flags::BRANCH_LABEL)
    }

    /// Add an antecedent to a branch label.
    fn add_antecedent(&mut self, label: FlowNodeId, antecedent: FlowNodeId) {
        if !antecedent.is_none() && antecedent != self.unreachable_flow {
            if let Some(flow) = self.flow_nodes.get_mut(label) {
                if !flow.antecedent.contains(&antecedent) {
                    flow.antecedent.push(antecedent);
                }
            }
        }
    }

    /// Check if current flow is reachable.
    fn is_reachable(&self) -> bool {
        !self.current_flow.is_none() && self.current_flow != self.unreachable_flow
    }

    /// Bind a single node, creating symbols as needed.
    fn bind_node(&mut self, arena: &NodeArena, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let node = match arena.get(idx) {
            Some(n) => n,
            None => return,
        };

        match node {
            // Variable declarations
            Node::VariableStatement(stmt) => {
                self.bind_node(arena, stmt.declaration_list);
            }
            Node::VariableDeclarationList(list) => {
                for &decl_idx in &list.declarations.nodes {
                    self.bind_node(arena, decl_idx);
                }
            }
            Node::VariableDeclaration(decl) => {
                self.bind_variable_declaration(arena, decl, idx);
            }

            // Function declarations
            Node::FunctionDeclaration(func) => {
                self.bind_function_declaration(arena, func, idx);
            }

            // Class declarations
            Node::ClassDeclaration(class) => {
                self.bind_class_declaration(arena, class, idx);
            }

            // Interface declarations
            Node::InterfaceDeclaration(iface) => {
                self.bind_interface_declaration(arena, iface, idx);
            }

            // Type alias declarations
            Node::TypeAliasDeclaration(alias) => {
                self.bind_type_alias_declaration(arena, alias, idx);
            }

            // Enum declarations
            Node::EnumDeclaration(enum_decl) => {
                self.bind_enum_declaration(arena, enum_decl, idx);
            }

            // Block - creates a new block scope (for let/const)
            Node::Block(block) => {
                self.enter_scope(ContainerKind::Block, idx);
                for &stmt_idx in &block.statements.nodes {
                    self.bind_node(arena, stmt_idx);
                }
                self.exit_scope();
            }

            // Other statements - recurse into children with flow analysis
            Node::IfStatement(if_stmt) => {
                self.bind_if_statement(arena, if_stmt, idx);
            }
            Node::WhileStatement(while_stmt) => {
                self.bind_while_statement(arena, while_stmt, idx);
            }
            Node::ForStatement(for_stmt) => {
                // For statement creates its own block scope for the initializer
                self.enter_scope(ContainerKind::Block, idx);
                self.bind_node(arena, for_stmt.initializer);
                self.bind_node(arena, for_stmt.statement);
                self.exit_scope();
            }
            Node::ForInStatement(for_in) => {
                self.enter_scope(ContainerKind::Block, idx);
                self.bind_node(arena, for_in.initializer);
                self.bind_node(arena, for_in.statement);
                self.exit_scope();
            }
            Node::ForOfStatement(for_of) => {
                self.enter_scope(ContainerKind::Block, idx);
                self.bind_node(arena, for_of.initializer);
                self.bind_node(arena, for_of.statement);
                self.exit_scope();
            }
            Node::SwitchStatement(switch_stmt) => {
                self.bind_switch_statement(arena, switch_stmt, idx);
            }
            Node::TryStatement(try_stmt) => {
                self.bind_try_statement(arena, try_stmt, idx);
            }

            // Import declarations
            Node::ImportDeclaration(import) => {
                self.bind_import_declaration(arena, import, idx);
            }

            // Export declarations
            Node::ExportDeclaration(export) => {
                self.bind_export_declaration(arena, export, idx);
            }

            // Export assignment: export default x or export = x
            Node::ExportAssignment(export_assign) => {
                // Bind the expression (which may contain declarations like arrow functions)
                self.bind_node(arena, export_assign.expression);
            }

            // Module/namespace declarations
            Node::ModuleDeclaration(module) => {
                self.bind_module_declaration(arena, module, idx);
            }
            Node::ModuleBlock(block) => {
                // Module block - the scope is already created by module declaration
                for &stmt_idx in &block.statements.nodes {
                    self.bind_node(arena, stmt_idx);
                }
            }

            // Method declarations (in object literals or classes)
            Node::MethodDeclaration(method) => {
                self.bind_method_declaration(arena, method, idx);
            }

            // Function expressions
            Node::FunctionExpression(func) => {
                self.bind_function_expression(arena, func, idx);
            }

            // Arrow functions
            Node::ArrowFunction(arrow) => {
                self.bind_arrow_function(arena, arrow, idx);
            }

            // Object literals - traverse into properties to find methods
            Node::ObjectLiteralExpression(obj) => {
                for &prop_idx in &obj.properties.nodes {
                    self.bind_node(arena, prop_idx);
                }
            }

            _ => {
                // For other node types, no symbols to create
            }
        }
    }

    /// Push a new scope onto the scope stack.
    fn push_scope(&mut self) {
        let old_scope = std::mem::take(&mut self.current_scope);
        self.scope_stack.push(old_scope);
        self.current_scope = SymbolTable::new();
    }

    /// Pop a scope from the stack.
    fn pop_scope(&mut self) {
        if let Some(parent_scope) = self.scope_stack.pop() {
            self.current_scope = parent_scope;
        }
    }

    /// Declare a symbol in the current scope.
    /// Handles declaration merging for interfaces, namespaces, and functions.
    fn declare_symbol(&mut self, name: String, flags: u32, declaration: NodeIndex) -> SymbolId {
        // Check if symbol already exists
        if let Some(existing_id) = self.current_scope.get(&name) {
            // Get existing flags first to avoid borrow issues
            let existing_flags = self.symbols.get(existing_id).map(|s| s.flags).unwrap_or(0);
            let can_merge = Self::can_merge_flags(existing_flags, flags);

            if let Some(sym) = self.symbols.get_mut(existing_id) {
                if can_merge {
                    // Merge the flags and add the declaration
                    sym.flags |= flags;
                    sym.declarations.push(declaration);

                    // Update value_declaration for merged class/enum/function + namespace symbols
                    // When a class/enum/function merges with a namespace, value_declaration should point to the class/enum/function
                    if (flags & symbol_flags::CLASS) != 0
                        || (flags & symbol_flags::FUNCTION) != 0
                        || (flags & symbol_flags::REGULAR_ENUM) != 0
                    {
                        sym.value_declaration = declaration;
                    } else if sym.value_declaration.is_none() && (flags & symbol_flags::VALUE) != 0
                    {
                        sym.value_declaration = declaration;
                    }
                } else {
                    // Conflicting declaration - still add but could report error
                    sym.declarations.push(declaration);
                }
            }
            // Record node-to-symbol mapping for merged declaration
            self.node_symbols.set(declaration, existing_id);
            return existing_id;
        }

        // Create new symbol
        let id = self.symbols.alloc(flags, name.clone());
        if let Some(sym) = self.symbols.get_mut(id) {
            sym.declarations.push(declaration);
            if sym.value_declaration.is_none() && (flags & symbol_flags::VALUE) != 0 {
                sym.value_declaration = declaration;
            }
        }
        self.current_scope.set(name, id);
        // Record node-to-symbol mapping for the declaration
        self.node_symbols.set(declaration, id);
        id
    }

    /// Check if two symbol flag sets can be merged.
    /// TypeScript allows merging:
    /// - Interface + Interface
    /// - Interface + Class
    /// - Namespace + Namespace
    /// - Namespace + Class/Function/Enum
    /// - Function + Function (overloads)
    /// - Enum + Enum (declaration merging)
    fn can_merge_flags(existing_flags: u32, new_flags: u32) -> bool {
        // Interface can merge with interface
        if (existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }

        if ((existing_flags & symbol_flags::CLASS) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0)
            || ((existing_flags & symbol_flags::INTERFACE) != 0
                && (new_flags & symbol_flags::CLASS) != 0)
        {
            return true;
        }

        // Namespace/module can merge with namespace/module
        if (existing_flags & symbol_flags::MODULE) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
            return true;
        }

        // Namespace can merge with class, function, or enum
        if (existing_flags & symbol_flags::MODULE) != 0 {
            if (new_flags & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
            {
                return true;
            }
        }
        if (new_flags & symbol_flags::MODULE) != 0 {
            if (existing_flags
                & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
            {
                return true;
            }
        }

        // Function overloads
        if (existing_flags & symbol_flags::FUNCTION) != 0
            && (new_flags & symbol_flags::FUNCTION) != 0
        {
            return true;
        }

        // Enum can merge with enum (members are combined)
        if (existing_flags & symbol_flags::ENUM) != 0 && (new_flags & symbol_flags::ENUM) != 0 {
            return true;
        }

        false
    }

    /// Get the name text from an identifier node.
    /// Returns a reference to avoid cloning - callers should clone only when needed.
    fn get_identifier_name<'a>(&self, arena: &'a NodeArena, idx: NodeIndex) -> Option<&'a str> {
        if let Some(Node::Identifier(id)) = arena.get(idx) {
            Some(&id.escaped_text)
        } else {
            None
        }
    }

    /// Get modifier flags (PRIVATE, PROTECTED, ABSTRACT, STATIC) from modifier list.
    fn get_modifier_flags(
        &self,
        arena: &NodeArena,
        modifiers: &Option<crate::parser::NodeList>,
    ) -> u32 {
        let mut flags = 0u32;
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(Node::Token(base)) = arena.get(mod_idx) {
                    if base.kind == SyntaxKind::PrivateKeyword as u16 {
                        flags |= symbol_flags::PRIVATE;
                    } else if base.kind == SyntaxKind::ProtectedKeyword as u16 {
                        flags |= symbol_flags::PROTECTED;
                    } else if base.kind == SyntaxKind::AbstractKeyword as u16 {
                        flags |= symbol_flags::ABSTRACT;
                    } else if base.kind == SyntaxKind::StaticKeyword as u16 {
                        flags |= symbol_flags::STATIC;
                    }
                }
            }
        }
        flags
    }

    // =========================================================================
    // Declaration Binding
    // =========================================================================

    /// Check if this is a var declaration (not let/const) by looking at parent list
    fn is_var_declaration(&self, arena: &NodeArena, decl_idx: NodeIndex) -> bool {
        // We need to look at the parent VariableDeclarationList to determine this
        // For now, use a simple heuristic based on the declaration flags
        // In a full implementation, we'd track this during parsing
        if let Some(Node::VariableDeclaration(decl)) = arena.get(decl_idx) {
            // Check the base node flags - if neither Let nor Const, it's var
            let flags = decl.base.flags;
            (flags & (node_flags::LET | node_flags::CONST)) == 0
        } else {
            false
        }
    }

    fn bind_variable_declaration(
        &mut self,
        arena: &NodeArena,
        decl: &crate::parser::VariableDeclaration,
        decl_idx: NodeIndex,
    ) {
        if let Some(name) = self.get_identifier_name(arena, decl.name) {
            // Determine if this is var (function-scoped) or let/const (block-scoped)
            let is_var = self.is_var_declaration(arena, decl_idx);

            if is_var {
                // var: function-scoped, declares in nearest function scope
                // Note: hoisting already declared it, but we need to handle the
                // actual binding point for flow analysis
                let flags = symbol_flags::FUNCTION_SCOPED_VARIABLE;

                // For var, check if already declared (from hoisting)
                if self.current_scope.has(name) || self.lookup_symbol(name).is_some() {
                    // Already declared via hoisting, just bind the initializer
                    // The symbol was already created during hoisting
                } else {
                    // Declare in function scope
                    self.declare_symbol(name.to_string(), flags, decl_idx);
                }
            } else {
                // let/const: block-scoped, declares in current block
                let flags = symbol_flags::BLOCK_SCOPED_VARIABLE;
                self.declare_symbol(name.to_string(), flags, decl_idx);
            }
        }

        // Traverse into the initializer to bind any function expressions or object literals
        if !decl.initializer.is_none() {
            self.bind_node(arena, decl.initializer);
        }
    }

    fn bind_function_declaration(
        &mut self,
        arena: &NodeArena,
        func: &crate::parser::FunctionDeclaration,
        func_idx: NodeIndex,
    ) {
        // Function declarations are hoisted, so the symbol may already exist
        if let Some(name) = self.get_identifier_name(arena, func.name) {
            // Check if already declared via hoisting
            if !self.current_scope.has(name) {
                self.declare_symbol(name.to_string(), symbol_flags::FUNCTION, func_idx);
            }
        }

        // Bind function body in new function scope
        if !func.body.is_none() {
            self.enter_scope(ContainerKind::Function, func_idx);

            // Collect hoisted declarations within function
            if let Some(Node::Block(block)) = arena.get(func.body) {
                self.collect_hoisted_declarations(arena, &block.statements.nodes);
                self.process_hoisted_functions(arena);
            }

            // Bind parameters first (they're in function scope)
            for &param_idx in &func.parameters.nodes {
                if let Some(Node::ParameterDeclaration(param)) = arena.get(param_idx) {
                    if let Some(name) = self.get_identifier_name(arena, param.name) {
                        self.declare_symbol(
                            name.to_string(),
                            symbol_flags::FUNCTION_SCOPED_VARIABLE,
                            param_idx,
                        );
                    }
                }
            }

            // Bind the function body
            self.bind_node(arena, func.body);
            self.exit_scope();
        }
    }

    fn bind_method_declaration(
        &mut self,
        arena: &NodeArena,
        method: &crate::parser::MethodDeclaration,
        method_idx: NodeIndex,
    ) {
        // Method declarations can appear in classes or object literals
        // For methods in object literals, we don't declare the method name,
        // but we still need to create a function scope and bind parameters

        // Bind method body in new function scope
        if !method.body.is_none() {
            self.enter_scope(ContainerKind::Function, method_idx);

            // Collect hoisted declarations within method
            if let Some(Node::Block(block)) = arena.get(method.body) {
                self.collect_hoisted_declarations(arena, &block.statements.nodes);
                self.process_hoisted_functions(arena);
            }

            // Bind parameters first (they're in function scope)
            for &param_idx in &method.parameters.nodes {
                if let Some(Node::ParameterDeclaration(param)) = arena.get(param_idx) {
                    if let Some(name) = self.get_identifier_name(arena, param.name) {
                        self.declare_symbol(
                            name.to_string(),
                            symbol_flags::FUNCTION_SCOPED_VARIABLE,
                            param_idx,
                        );
                    }
                }
            }

            // Bind the method body
            self.bind_node(arena, method.body);
            self.exit_scope();
        }
    }

    fn bind_function_expression(
        &mut self,
        arena: &NodeArena,
        func: &crate::parser::FunctionExpression,
        func_idx: NodeIndex,
    ) {
        // Function expressions can be named or anonymous
        // Named function expressions declare their name in their own scope

        // Bind function body in new function scope
        if !func.body.is_none() {
            self.enter_scope(ContainerKind::Function, func_idx);

            // If this is a named function expression, declare the name in the function scope
            if let Some(name) = self.get_identifier_name(arena, func.name) {
                self.declare_symbol(name.to_string(), symbol_flags::FUNCTION, func_idx);
            }

            // Collect hoisted declarations within function
            if let Some(Node::Block(block)) = arena.get(func.body) {
                self.collect_hoisted_declarations(arena, &block.statements.nodes);
                self.process_hoisted_functions(arena);
            }

            // Bind parameters first (they're in function scope)
            for &param_idx in &func.parameters.nodes {
                if let Some(Node::ParameterDeclaration(param)) = arena.get(param_idx) {
                    if let Some(name) = self.get_identifier_name(arena, param.name) {
                        self.declare_symbol(
                            name.to_string(),
                            symbol_flags::FUNCTION_SCOPED_VARIABLE,
                            param_idx,
                        );
                    }
                }
            }

            // Bind the function body
            self.bind_node(arena, func.body);
            self.exit_scope();
        }
    }

    fn bind_arrow_function(
        &mut self,
        arena: &NodeArena,
        arrow: &crate::parser::ArrowFunction,
        arrow_idx: NodeIndex,
    ) {
        // Arrow functions are always anonymous and create a function scope

        // Bind arrow body in new function scope
        if !arrow.body.is_none() {
            self.enter_scope(ContainerKind::Function, arrow_idx);

            // Collect hoisted declarations if body is a block
            if let Some(Node::Block(block)) = arena.get(arrow.body) {
                self.collect_hoisted_declarations(arena, &block.statements.nodes);
                self.process_hoisted_functions(arena);
            }

            // Bind parameters first (they're in function scope)
            for &param_idx in &arrow.parameters.nodes {
                if let Some(Node::ParameterDeclaration(param)) = arena.get(param_idx) {
                    if let Some(name) = self.get_identifier_name(arena, param.name) {
                        self.declare_symbol(
                            name.to_string(),
                            symbol_flags::FUNCTION_SCOPED_VARIABLE,
                            param_idx,
                        );
                    }
                }
            }

            // Bind the arrow body
            self.bind_node(arena, arrow.body);
            self.exit_scope();
        }
    }

    fn bind_class_declaration(
        &mut self,
        arena: &NodeArena,
        class: &crate::parser::ClassDeclaration,
        class_idx: NodeIndex,
    ) {
        if let Some(name) = self.get_identifier_name(arena, class.name) {
            self.declare_symbol(name.to_string(), symbol_flags::CLASS, class_idx);
        }

        // Bind class members in a new scope
        self.push_scope();
        for &member_idx in &class.members.nodes {
            self.bind_class_member(arena, member_idx);
        }
        self.pop_scope();
    }

    fn bind_class_member(&mut self, arena: &NodeArena, idx: NodeIndex) {
        if let Some(node) = arena.get(idx) {
            match node {
                Node::MethodDeclaration(method) => {
                    if let Some(name) = self.get_identifier_name(arena, method.name) {
                        let visibility = self.get_modifier_flags(arena, &method.modifiers);
                        self.declare_symbol(
                            name.to_string(),
                            symbol_flags::METHOD | visibility,
                            idx,
                        );
                    }
                }
                Node::PropertyDeclaration(prop) => {
                    if let Some(name) = self.get_identifier_name(arena, prop.name) {
                        let visibility = self.get_modifier_flags(arena, &prop.modifiers);
                        self.declare_symbol(
                            name.to_string(),
                            symbol_flags::PROPERTY | visibility,
                            idx,
                        );
                    }
                }
                Node::ConstructorDeclaration(_) => {
                    self.declare_symbol("constructor".to_string(), symbol_flags::CONSTRUCTOR, idx);
                }
                _ => {}
            }
        }
    }

    /// Bind an if statement with flow analysis.
    fn bind_if_statement(
        &mut self,
        arena: &NodeArena,
        if_stmt: &crate::parser::IfStatement,
        _node_idx: NodeIndex,
    ) {
        // Save the current flow before the condition
        let pre_condition_flow = self.current_flow;

        // Create flow node for the true branch (condition is true)
        let true_flow = self.create_flow_node(
            flow_flags::TRUE_CONDITION,
            pre_condition_flow,
            if_stmt.expression,
        );

        // Bind the then statement with true flow
        self.current_flow = true_flow;
        self.bind_node(arena, if_stmt.then_statement);
        let post_then_flow = self.current_flow;

        // Create a branch label for merging after the if statement
        let merge_label = self.create_branch_label();

        if !if_stmt.else_statement.is_none() {
            // Create flow node for the false branch (condition is false)
            let false_flow = self.create_flow_node(
                flow_flags::FALSE_CONDITION,
                pre_condition_flow,
                if_stmt.expression,
            );

            // Bind the else statement with false flow
            self.current_flow = false_flow;
            self.bind_node(arena, if_stmt.else_statement);
            let post_else_flow = self.current_flow;

            // Add both branches to the merge label
            self.add_antecedent(merge_label, post_then_flow);
            self.add_antecedent(merge_label, post_else_flow);
        } else {
            // No else branch: false path goes directly to merge
            let false_flow = self.create_flow_node(
                flow_flags::FALSE_CONDITION,
                pre_condition_flow,
                if_stmt.expression,
            );

            self.add_antecedent(merge_label, post_then_flow);
            self.add_antecedent(merge_label, false_flow);
        }

        // Set current flow to the merge label
        self.current_flow = merge_label;
    }

    /// Bind a while statement with flow analysis.
    fn bind_while_statement(
        &mut self,
        arena: &NodeArena,
        while_stmt: &crate::parser::WhileStatement,
        _node_idx: NodeIndex,
    ) {
        // Create a loop label for the loop entry
        let loop_label = self.flow_nodes.alloc(flow_flags::LOOP_LABEL);
        if let Some(flow) = self.flow_nodes.get_mut(loop_label) {
            if !self.current_flow.is_none() {
                flow.antecedent.push(self.current_flow);
            }
        }

        self.current_flow = loop_label;

        // Create flow node for the true condition (entering loop body)
        let true_flow = self.create_flow_node(
            flow_flags::TRUE_CONDITION,
            loop_label,
            while_stmt.expression,
        );

        // Bind the loop body
        self.current_flow = true_flow;
        self.bind_node(arena, while_stmt.statement);

        // Loop back to the loop label
        self.add_antecedent(loop_label, self.current_flow);

        // Create flow node for the false condition (exiting loop)
        let false_flow = self.create_flow_node(
            flow_flags::FALSE_CONDITION,
            loop_label,
            while_stmt.expression,
        );

        self.current_flow = false_flow;
    }

    fn bind_interface_declaration(
        &mut self,
        arena: &NodeArena,
        iface: &crate::parser::InterfaceDeclaration,
        iface_idx: NodeIndex,
    ) {
        if let Some(name) = self.get_identifier_name(arena, iface.name) {
            self.declare_symbol(name.to_string(), symbol_flags::INTERFACE, iface_idx);
        }
    }

    fn bind_type_alias_declaration(
        &mut self,
        arena: &NodeArena,
        alias: &crate::parser::TypeAliasDeclaration,
        alias_idx: NodeIndex,
    ) {
        if let Some(name) = self.get_identifier_name(arena, alias.name) {
            self.declare_symbol(name.to_string(), symbol_flags::TYPE_ALIAS, alias_idx);
        }
    }

    fn bind_enum_declaration(
        &mut self,
        arena: &NodeArena,
        enum_decl: &crate::parser::EnumDeclaration,
        enum_idx: NodeIndex,
    ) {
        if let Some(name) = self.get_identifier_name(arena, enum_decl.name) {
            self.declare_symbol(name.to_string(), symbol_flags::REGULAR_ENUM, enum_idx);
        }

        // Bind enum members
        for &member_idx in &enum_decl.members.nodes {
            if let Some(Node::EnumMember(member)) = arena.get(member_idx) {
                if let Some(name) = self.get_identifier_name(arena, member.name) {
                    self.declare_symbol(name.to_string(), symbol_flags::ENUM_MEMBER, member_idx);
                }
            }
        }
    }

    fn bind_import_declaration(
        &mut self,
        arena: &NodeArena,
        import: &crate::parser::ImportDeclaration,
        _import_idx: NodeIndex,
    ) {
        if let Some(Node::ImportClause(clause)) = arena.get(import.import_clause) {
            let clause_type_only = clause.is_type_only;

            // Default import: import Foo from './module'
            if !clause.name.is_none() {
                if let Some(name) = self.get_identifier_name(arena, clause.name) {
                    let sym_id =
                        self.declare_symbol(name.to_string(), symbol_flags::ALIAS, clause.name);
                    // Mark as type-only if import clause is type-only
                    if let Some(sym) = self.symbols.get_mut(sym_id) {
                        sym.is_type_only = clause_type_only;
                    }
                }
            }

            // Named imports: import { Foo, Bar as Baz } from './module'
            if let Some(Node::NamedImports(named)) = arena.get(clause.named_bindings) {
                for &spec_idx in &named.elements.nodes {
                    if let Some(Node::ImportSpecifier(spec)) = arena.get(spec_idx) {
                        // Individual specifier can be type-only: import { type Foo, bar } from 'mod'
                        let spec_type_only = clause_type_only || spec.is_type_only;
                        if let Some(name) = self.get_identifier_name(arena, spec.name) {
                            let sym_id = self.declare_symbol(
                                name.to_string(),
                                symbol_flags::ALIAS,
                                spec_idx,
                            );
                            if let Some(sym) = self.symbols.get_mut(sym_id) {
                                sym.is_type_only = spec_type_only;
                            }
                        }
                    }
                }
            }

            // Namespace import: import * as ns from './module'
            if let Some(Node::NamespaceImport(ns_import)) = arena.get(clause.named_bindings) {
                if let Some(name) = self.get_identifier_name(arena, ns_import.name) {
                    let sym_id = self.declare_symbol(
                        name.to_string(),
                        symbol_flags::ALIAS,
                        clause.named_bindings,
                    );
                    if let Some(sym) = self.symbols.get_mut(sym_id) {
                        sym.is_type_only = clause_type_only;
                    }
                }
            }
        }
    }

    fn bind_export_declaration(
        &mut self,
        arena: &NodeArena,
        export: &crate::parser::ExportDeclaration,
        _export_idx: NodeIndex,
    ) {
        // Export clause can be:
        // - NamedExports: export { foo, bar }
        // - NamespaceExport: export * as ns from 'mod'
        // - Empty for: export * from 'mod' (re-export all)

        if !export.export_clause.is_none() {
            // Handle named exports: export { foo, bar } or export { foo as bar }
            if let Some(Node::NamedExports(named)) = arena.get(export.export_clause) {
                for &spec_idx in &named.elements.nodes {
                    if let Some(Node::ExportSpecifier(spec)) = arena.get(spec_idx) {
                        // For export { foo }, property_name is NONE, name is "foo"
                        // For export { foo as bar }, property_name is "foo", name is "bar"
                        let exported_name = if !spec.name.is_none() {
                            self.get_identifier_name(arena, spec.name)
                        } else {
                            self.get_identifier_name(arena, spec.property_name)
                        };

                        if let Some(name) = exported_name {
                            // Create export symbol marking it as exported
                            // Type-only export: export type { Foo } or export { type Foo }
                            let is_type_only = export.is_type_only || spec.is_type_only;
                            let sym_id = self.declare_symbol(
                                name.to_string(),
                                symbol_flags::EXPORT_VALUE,
                                spec_idx,
                            );
                            if let Some(sym) = self.symbols.get_mut(sym_id) {
                                sym.is_exported = true;
                                sym.is_type_only = is_type_only;
                            }
                        }
                    }
                }
            }
            // Handle namespace export: export * as ns from 'mod'
            else if let Some(Node::NamespaceExport(ns_export)) = arena.get(export.export_clause) {
                if let Some(name) = self.get_identifier_name(arena, ns_export.name) {
                    let sym_id = self.declare_symbol(
                        name.to_string(),
                        symbol_flags::ALIAS,
                        export.export_clause,
                    );
                    if let Some(sym) = self.symbols.get_mut(sym_id) {
                        sym.is_exported = true;
                        sym.is_type_only = export.is_type_only;
                    }
                }
            }
        }
        // export * from 'mod' - re-exports don't create new symbols,
        // they just reference symbols from the target module
    }

    fn bind_module_declaration(
        &mut self,
        arena: &NodeArena,
        module: &crate::parser::ModuleDeclaration,
        module_idx: NodeIndex,
    ) {
        // Get module name (identifier or string literal for external modules)
        let name = self
            .get_identifier_name(arena, module.name)
            .map(|n| n.to_string())
            .or_else(|| arena.get_literal_text(module.name).map(str::to_string));

        let mut module_symbol_id = SymbolId::NONE;
        if let Some(name) = name {
            // Determine if this is a namespace (value) or module (ambient)
            // For simplicity, treat as namespace module (can contain values)
            let flags = symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE;
            module_symbol_id = self.declare_symbol(name, flags, module_idx);
        }

        // Bind module body in new scope
        if !module.body.is_none() {
            self.enter_scope(ContainerKind::Module, module_idx);
            self.bind_node(arena, module.body);

            // Populate exports for the module symbol
            if !module_symbol_id.is_none() {
                self.populate_module_exports(arena, module.body, module_symbol_id);
            }

            self.exit_scope();
        }
    }

    /// Check if a modifier list contains the export keyword.
    fn has_export_modifier(
        &self,
        arena: &NodeArena,
        modifiers: &Option<crate::parser::NodeList>,
    ) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(Node::Token(base)) = arena.get(mod_idx) {
                    if base.kind == SyntaxKind::ExportKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Populate the exports table of a module/namespace symbol based on exported declarations in its body.
    fn populate_module_exports(
        &mut self,
        arena: &NodeArena,
        body_idx: NodeIndex,
        module_symbol_id: SymbolId,
    ) {
        let Some(node) = arena.get(body_idx) else {
            return;
        };

        // Body can be a Block (ModuleBlock) or another ModuleDeclaration (nested namespace)
        let statements = if node.kind() == syntax_kind_ext::MODULE_BLOCK {
            if let Some(Node::ModuleBlock(block)) = arena.get(body_idx) {
                &block.statements.nodes
            } else {
                return;
            }
        } else if let Some(Node::ModuleBlock(block)) = arena.get(body_idx) {
            &block.statements.nodes
        } else {
            return;
        };

        for &stmt_idx in statements {
            if let Some(stmt_node) = arena.get(stmt_idx) {
                // Check for export modifier
                let is_exported = match stmt_node {
                    Node::VariableStatement(v) => self.has_export_modifier(arena, &v.modifiers),
                    Node::FunctionDeclaration(f) => self.has_export_modifier(arena, &f.modifiers),
                    Node::ClassDeclaration(c) => self.has_export_modifier(arena, &c.modifiers),
                    Node::InterfaceDeclaration(i) => self.has_export_modifier(arena, &i.modifiers),
                    Node::TypeAliasDeclaration(t) => self.has_export_modifier(arena, &t.modifiers),
                    Node::EnumDeclaration(e) => self.has_export_modifier(arena, &e.modifiers),
                    Node::ModuleDeclaration(m) => self.has_export_modifier(arena, &m.modifiers),
                    Node::ExportDeclaration(_) => true, // export { x }
                    _ => false,
                };

                if is_exported {
                    // Collect the exported names first
                    let mut exported_names = Vec::new();

                    match stmt_node {
                        Node::VariableStatement(stmt) => {
                            if let Some(Node::VariableDeclarationList(list)) =
                                arena.get(stmt.declaration_list)
                            {
                                for &decl_idx in &list.declarations.nodes {
                                    if let Some(Node::VariableDeclaration(decl)) =
                                        arena.get(decl_idx)
                                    {
                                        if let Some(name) =
                                            self.get_identifier_name(arena, decl.name)
                                        {
                                            exported_names.push(name.to_string());
                                        }
                                    }
                                }
                            }
                        }
                        Node::FunctionDeclaration(func) => {
                            if let Some(name) = self.get_identifier_name(arena, func.name) {
                                exported_names.push(name.to_string());
                            }
                        }
                        Node::ClassDeclaration(class) => {
                            if let Some(name) = self.get_identifier_name(arena, class.name) {
                                exported_names.push(name.to_string());
                            }
                        }
                        Node::EnumDeclaration(enm) => {
                            if let Some(name) = self.get_identifier_name(arena, enm.name) {
                                exported_names.push(name.to_string());
                            }
                        }
                        Node::InterfaceDeclaration(iface) => {
                            if let Some(name) = self.get_identifier_name(arena, iface.name) {
                                exported_names.push(name.to_string());
                            }
                        }
                        Node::TypeAliasDeclaration(alias) => {
                            if let Some(name) = self.get_identifier_name(arena, alias.name) {
                                exported_names.push(name.to_string());
                            }
                        }
                        Node::ModuleDeclaration(module) => {
                            let name = self
                                .get_identifier_name(arena, module.name)
                                .map(|n| n.to_string())
                                .or_else(|| {
                                    arena.get_literal_text(module.name).map(str::to_string)
                                });
                            if let Some(name) = name {
                                exported_names.push(name);
                            }
                        }
                        _ => {}
                    }

                    // Now add them to exports
                    for name in &exported_names {
                        if let Some(sym_id) = self.current_scope.get(name) {
                            if let Some(module_sym) = self.symbols.get_mut(module_symbol_id) {
                                let exports = module_sym
                                    .exports
                                    .get_or_insert_with(|| Box::new(SymbolTable::new()));
                                exports.set(name.clone(), sym_id);
                            }
                            // Mark the child symbol as exported
                            if let Some(child_sym) = self.symbols.get_mut(sym_id) {
                                child_sym.is_exported = true;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Bind a switch statement.
    fn bind_switch_statement(
        &mut self,
        arena: &NodeArena,
        switch_stmt: &crate::parser::SwitchStatement,
        node_idx: NodeIndex,
    ) {
        // Save flow before switch
        let pre_switch_flow = self.current_flow;

        // Create a branch label for the end of switch (for break statements)
        let end_label = self.create_branch_label();

        // Bind the case block
        if let Some(Node::CaseBlock(case_block)) = arena.get(switch_stmt.case_block) {
            for &clause_idx in &case_block.clauses.nodes {
                if let Some(node) = arena.get(clause_idx) {
                    match node {
                        Node::CaseClause(clause) => {
                            // Create switch clause flow node
                            let clause_flow = self.create_flow_node(
                                flow_flags::SWITCH_CLAUSE,
                                pre_switch_flow,
                                clause.expression,
                            );
                            self.current_flow = clause_flow;

                            // Bind statements in the clause (new block scope for let/const)
                            self.enter_scope(ContainerKind::Block, clause_idx);
                            for &stmt_idx in &clause.statements.nodes {
                                self.bind_node(arena, stmt_idx);
                            }
                            self.exit_scope();

                            // Add to end label
                            self.add_antecedent(end_label, self.current_flow);
                        }
                        Node::DefaultClause(clause) => {
                            // Default clause is always reachable
                            let clause_flow = self.create_flow_node(
                                flow_flags::SWITCH_CLAUSE,
                                pre_switch_flow,
                                node_idx,
                            );
                            self.current_flow = clause_flow;

                            self.enter_scope(ContainerKind::Block, clause_idx);
                            for &stmt_idx in &clause.statements.nodes {
                                self.bind_node(arena, stmt_idx);
                            }
                            self.exit_scope();

                            self.add_antecedent(end_label, self.current_flow);
                        }
                        _ => {}
                    }
                }
            }
        }

        self.current_flow = end_label;
    }

    /// Bind a try statement.
    fn bind_try_statement(
        &mut self,
        arena: &NodeArena,
        try_stmt: &crate::parser::TryStatement,
        _node_idx: NodeIndex,
    ) {
        let pre_try_flow = self.current_flow;

        // Create merge label for after try/catch/finally
        let end_label = self.create_branch_label();

        // Bind try block
        self.bind_node(arena, try_stmt.try_block);
        let post_try_flow = self.current_flow;

        // Bind catch clause if present
        if !try_stmt.catch_clause.is_none() {
            if let Some(Node::CatchClause(catch)) = arena.get(try_stmt.catch_clause) {
                // Catch clause has its own scope
                self.enter_scope(ContainerKind::Block, try_stmt.catch_clause);

                // Bind catch variable if present
                if !catch.variable_declaration.is_none() {
                    if let Some(Node::VariableDeclaration(decl)) =
                        arena.get(catch.variable_declaration)
                    {
                        if let Some(name) = self.get_identifier_name(arena, decl.name) {
                            self.declare_symbol(
                                name.to_string(),
                                symbol_flags::BLOCK_SCOPED_VARIABLE,
                                catch.variable_declaration,
                            );
                        }
                    }
                }

                // Reset flow - catch can be entered from any point in try
                self.current_flow = pre_try_flow;
                self.bind_node(arena, catch.block);
                self.add_antecedent(end_label, self.current_flow);

                self.exit_scope();
            }
        }

        // Add post-try flow to end label
        self.add_antecedent(end_label, post_try_flow);

        // Bind finally block if present
        if !try_stmt.finally_block.is_none() {
            // Finally is always executed
            self.current_flow = end_label;
            self.bind_node(arena, try_stmt.finally_block);
        } else {
            self.current_flow = end_label;
        }
    }
}

impl Default for BinderState {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// WASM Methods
// =============================================================================

#[wasm_bindgen]
impl BinderState {
    /// Create a new binder state.
    #[wasm_bindgen(constructor)]
    pub fn create() -> Self {
        Self::new()
    }

    /// Get the number of symbols created.
    #[wasm_bindgen(js_name = getSymbolCount)]
    pub fn get_symbol_count(&self) -> u32 {
        self.symbols.len() as u32
    }

    /// Get file locals as JSON string.
    #[wasm_bindgen(js_name = getFileLocalsJson)]
    pub fn get_file_locals_json(&self) -> String {
        serde_json::to_string(&self.file_locals).unwrap_or_else(|_| "{}".to_string())
    }

    /// Get all symbols as JSON string.
    #[wasm_bindgen(js_name = getSymbolsJson)]
    pub fn get_symbols_json(&self) -> String {
        serde_json::to_string(&self.symbols).unwrap_or_else(|_| "[]".to_string())
    }

    /// Get a symbol by name from file locals.
    #[wasm_bindgen(js_name = getSymbolByName)]
    pub fn get_symbol_by_name(&self, name: &str) -> Option<String> {
        if let Some(id) = self.file_locals.get(name) {
            if let Some(sym) = self.symbols.get(id) {
                return serde_json::to_string(sym).ok();
            }
        }
        None
    }

    /// Check if a name exists in file locals.
    #[wasm_bindgen(js_name = hasSymbol)]
    pub fn has_symbol(&self, name: &str) -> bool {
        self.file_locals.has(name)
    }
}
