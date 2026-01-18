//! ThinBinder - Binder implementation using ThinNodeArena.
//!
//! This is a clean implementation of the binder that works directly with
//! ThinNode and ThinNodeArena, avoiding the old Node enum pattern matching.

// Allow dead code: This module contains binder infrastructure for symbol table construction,
// scope management, and control flow graph building. Some internal helpers (scope chain
// manipulation, hoisting utilities, flow node factories) are infrastructure for complete
// TypeScript semantics and may not be fully exercised by current tests. The binder is
// actively used throughout the codebase (33+ files) via ThinBinderState for type checking,
// LSP features, and code transforms.
#![allow(dead_code)]
// Allow collapsible if statements - this is a style choice that doesn't affect correctness
// Many nested if-let patterns are clearer when not collapsed for readability
#![allow(clippy::collapsible_if)]

use crate::binder::{
    ContainerKind, FlowNodeArena, FlowNodeId, Scope, ScopeContext, ScopeId, Symbol, SymbolArena,
    SymbolId, SymbolTable, flow_flags, symbol_flags,
};
use crate::lib_loader;
use crate::module_resolution_debug::ModuleResolutionDebugger;
use crate::parser::node_flags;
use crate::parser::thin_node::{ThinNode, ThinNodeArena};
use crate::parser::{NodeIndex, NodeList, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

/// Lib file context for global type resolution.
/// This mirrors the definition in checker::context to avoid circular dependencies.
#[derive(Clone)]
pub struct LibContext {
    /// The AST arena for this lib file.
    pub arena: Arc<ThinNodeArena>,
    /// The binder state with symbols from this lib file.
    pub binder: Arc<ThinBinderState>,
}

/// Binder state using ThinNodeArena.
pub struct ThinBinderState {
    /// Arena for symbol storage
    pub symbols: SymbolArena,
    /// Current symbol table (local scope)
    pub current_scope: SymbolTable,
    /// Stack of parent scopes
    scope_stack: Vec<SymbolTable>,
    /// File-level locals (for module resolution)
    pub file_locals: SymbolTable,
    /// Ambient module declarations by specifier (e.g. "pkg", "./types")
    pub declared_modules: FxHashSet<String>,
    /// Whether the current source file is an external module (has top-level import/export).
    is_external_module: bool,
    /// Flow nodes for control flow analysis
    pub flow_nodes: FlowNodeArena,
    /// Current flow node
    current_flow: FlowNodeId,
    /// Unreachable flow node
    unreachable_flow: FlowNodeId,
    /// Scope chain - stack of scope contexts (legacy, for hoisting)
    scope_chain: Vec<ScopeContext>,
    /// Current scope index in scope_chain
    current_scope_idx: usize,
    /// Node-to-symbol mapping
    pub node_symbols: FxHashMap<u32, SymbolId>,
    /// Symbol-to-arena mapping for cross-file declaration lookup
    pub symbol_arenas: FxHashMap<SymbolId, Arc<ThinNodeArena>>,
    /// Node-to-flow mapping: tracks which flow node was active at each AST node
    /// Used by the checker for control flow analysis (type narrowing)
    pub node_flow: FxHashMap<u32, FlowNodeId>,
    /// Flow node after each top-level statement (for incremental binding).
    top_level_flow: FxHashMap<u32, FlowNodeId>,
    /// Map case/default clause nodes to their containing switch statement.
    switch_clause_to_switch: FxHashMap<u32, NodeIndex>,
    /// Hoisted var declarations
    hoisted_vars: Vec<(String, NodeIndex)>,
    /// Hoisted function declarations
    hoisted_functions: Vec<NodeIndex>,

    // ===== Persistent Scope System (for stateless checking) =====
    /// Persistent scopes - enables querying scope information without traversal order
    pub scopes: Vec<Scope>,
    /// Map from AST node (that creates a scope) to its ScopeId
    pub node_scope_ids: FxHashMap<u32, ScopeId>,
    /// Current active ScopeId during binding
    current_scope_id: ScopeId,

    // ===== Module Resolution Debugging =====
    /// Debugger for tracking symbol table operations and scope lookups
    pub debugger: ModuleResolutionDebugger,

    // ===== Global Augmentations =====
    /// Tracks interface/type declarations inside `declare global` blocks that should
    /// merge with lib.d.ts symbols. Maps interface name to declaration NodeIndex values.
    pub global_augmentations: FxHashMap<String, Vec<NodeIndex>>,

    /// Flag indicating we're currently binding inside a `declare global` block
    in_global_augmentation: bool,

    /// Lib binders for automatic lib symbol resolution.
    /// When get_symbol() doesn't find a symbol locally, it checks these lib binders.
    lib_binders: Vec<Arc<ThinBinderState>>,

    /// Module exports: maps file names to their exported symbols for cross-file module resolution
    /// This enables resolving imports like `import { X } from './file'` where './file' is another file
    pub module_exports: FxHashMap<String, SymbolTable>,

    /// Re-exports: tracks `export * from 'module'` and `export { x } from 'module'` declarations
    /// Maps (current_file, exported_name) -> (source_module, original_name)
    /// For wildcard re-exports, exported_name is "*" to indicate all exports
    /// Example: ("./a.ts", "*", "./b.ts") means a.ts re-exports everything from b.ts
    /// Example: ("./a.ts", "foo", "./b.ts") means a.ts re-exports "foo" from b.ts
    pub reexports: FxHashMap<String, FxHashMap<String, (String, Option<String>)>>,
}

/// Validation result describing issues found in the symbol table
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    /// A node->symbol mapping points to a non-existent symbol
    BrokenSymbolLink { node_index: u32, symbol_id: u32 },
    /// A symbol exists but has no declarations (orphaned)
    OrphanedSymbol { symbol_id: u32, name: String },
    /// A symbol's value_declaration points to a non-existent node
    InvalidValueDeclaration { symbol_id: u32, name: String },
}

/// Statistics about symbol resolution attempts and successes.
#[derive(Debug, Clone, Default)]
pub struct ResolutionStats {
    /// Total number of resolution attempts
    pub attempts: u64,
    /// Number of successful resolutions in scopes
    pub scope_hits: u64,
    /// Number of successful resolutions in file_locals
    pub file_local_hits: u64,
    /// Number of successful resolutions in lib_binders
    pub lib_binder_hits: u64,
    /// Number of failed resolutions
    pub failures: u64,
}

impl ThinBinderState {
    pub fn new() -> Self {
        let mut flow_nodes = FlowNodeArena::new();
        let unreachable_flow = flow_nodes.alloc(flow_flags::UNREACHABLE);

        ThinBinderState {
            symbols: SymbolArena::new(),
            current_scope: SymbolTable::new(),
            scope_stack: Vec::new(),
            file_locals: SymbolTable::new(),
            declared_modules: FxHashSet::default(),
            is_external_module: false,
            flow_nodes,
            current_flow: FlowNodeId::NONE,
            unreachable_flow,
            scope_chain: Vec::new(),
            current_scope_idx: 0,
            node_symbols: FxHashMap::default(),
            symbol_arenas: FxHashMap::default(),
            node_flow: FxHashMap::default(),
            top_level_flow: FxHashMap::default(),
            switch_clause_to_switch: FxHashMap::default(),
            hoisted_vars: Vec::new(),
            hoisted_functions: Vec::new(),
            scopes: Vec::new(),
            node_scope_ids: FxHashMap::default(),
            current_scope_id: ScopeId::NONE,
            debugger: ModuleResolutionDebugger::new(),
            global_augmentations: FxHashMap::default(),
            in_global_augmentation: false,
            lib_binders: Vec::new(),
            module_exports: FxHashMap::default(),
            reexports: FxHashMap::default(),
        }
    }

    pub fn reset(&mut self) {
        self.symbols.clear();
        self.current_scope.clear();
        self.scope_stack.clear();
        self.file_locals.clear();
        self.declared_modules.clear();
        self.is_external_module = false;
        self.flow_nodes.clear();
        self.unreachable_flow = self.flow_nodes.alloc(flow_flags::UNREACHABLE);
        self.current_flow = FlowNodeId::NONE;
        self.scope_chain.clear();
        self.current_scope_idx = 0;
        self.node_symbols.clear();
        self.symbol_arenas.clear();
        self.node_flow.clear();
        self.top_level_flow.clear();
        self.switch_clause_to_switch.clear();
        self.hoisted_vars.clear();
        self.hoisted_functions.clear();
        self.scopes.clear();
        self.node_scope_ids.clear();
        self.current_scope_id = ScopeId::NONE;
        self.debugger.clear();
        self.global_augmentations.clear();
        self.in_global_augmentation = false;
        self.lib_binders.clear();
        self.module_exports.clear();
        self.reexports.clear();
    }

    /// Set the current file name for debugging purposes.
    /// This should be called before binding a source file.
    pub fn set_debug_file(&mut self, file_name: &str) {
        self.debugger.set_current_file(file_name);
    }

    /// Get the module resolution debug summary.
    /// Returns a human-readable summary of all recorded debug events.
    pub fn get_debug_summary(&self) -> String {
        self.debugger.get_summary()
    }

    /// Create a ThinBinderState from existing bound state.
    ///
    /// This is used for type checking after parallel binding and symbol merging.
    /// The symbols and node_symbols come from the merged program state.
    pub fn from_bound_state(
        symbols: SymbolArena,
        file_locals: SymbolTable,
        node_symbols: FxHashMap<u32, SymbolId>,
    ) -> Self {
        let mut flow_nodes = FlowNodeArena::new();
        let unreachable_flow = flow_nodes.alloc(flow_flags::UNREACHABLE);

        ThinBinderState {
            symbols,
            current_scope: SymbolTable::new(),
            scope_stack: Vec::new(),
            file_locals,
            declared_modules: FxHashSet::default(),
            is_external_module: false,
            flow_nodes,
            current_flow: FlowNodeId::NONE,
            unreachable_flow,
            scope_chain: Vec::new(),
            current_scope_idx: 0,
            node_symbols,
            symbol_arenas: FxHashMap::default(),
            node_flow: FxHashMap::default(),
            top_level_flow: FxHashMap::default(),
            switch_clause_to_switch: FxHashMap::default(),
            hoisted_vars: Vec::new(),
            hoisted_functions: Vec::new(),
            scopes: Vec::new(),
            node_scope_ids: FxHashMap::default(),
            current_scope_id: ScopeId::NONE,
            debugger: ModuleResolutionDebugger::new(),
            global_augmentations: FxHashMap::default(),
            in_global_augmentation: false,
            lib_binders: Vec::new(),
            module_exports: FxHashMap::default(),
            reexports: FxHashMap::default(),
        }
    }

    /// Create a ThinBinderState from existing bound state, preserving scopes.
    pub fn from_bound_state_with_scopes(
        symbols: SymbolArena,
        file_locals: SymbolTable,
        node_symbols: FxHashMap<u32, SymbolId>,
        scopes: Vec<Scope>,
        node_scope_ids: FxHashMap<u32, ScopeId>,
    ) -> Self {
        Self::from_bound_state_with_scopes_and_augmentations(
            symbols,
            file_locals,
            node_symbols,
            scopes,
            node_scope_ids,
            FxHashMap::default(),
            FxHashMap::default(),
            FxHashMap::default(),
        )
    }

    /// Create a ThinBinderState from existing bound state, preserving scopes and global augmentations.
    ///
    /// This is used for type checking after parallel binding and symbol merging.
    /// Global augmentations are interface/type declarations inside `declare global` blocks
    /// that should merge with lib.d.ts symbols during type resolution.
    #[allow(clippy::too_many_arguments)]
    pub fn from_bound_state_with_scopes_and_augmentations(
        symbols: SymbolArena,
        file_locals: SymbolTable,
        node_symbols: FxHashMap<u32, SymbolId>,
        scopes: Vec<Scope>,
        node_scope_ids: FxHashMap<u32, ScopeId>,
        global_augmentations: FxHashMap<String, Vec<crate::parser::NodeIndex>>,
        module_exports: FxHashMap<String, SymbolTable>,
        reexports: FxHashMap<String, FxHashMap<String, (String, Option<String>)>>,
    ) -> Self {
        let mut flow_nodes = FlowNodeArena::new();
        let unreachable_flow = flow_nodes.alloc(flow_flags::UNREACHABLE);

        ThinBinderState {
            symbols,
            current_scope: SymbolTable::new(),
            scope_stack: Vec::new(),
            file_locals,
            declared_modules: FxHashSet::default(),
            is_external_module: false,
            flow_nodes,
            current_flow: FlowNodeId::NONE,
            unreachable_flow,
            scope_chain: Vec::new(),
            current_scope_idx: 0,
            node_symbols,
            symbol_arenas: FxHashMap::default(),
            node_flow: FxHashMap::default(),
            top_level_flow: FxHashMap::default(),
            switch_clause_to_switch: FxHashMap::default(),
            hoisted_vars: Vec::new(),
            hoisted_functions: Vec::new(),
            scopes,
            node_scope_ids,
            current_scope_id: ScopeId::NONE,
            debugger: ModuleResolutionDebugger::new(),
            global_augmentations,
            in_global_augmentation: false,
            lib_binders: Vec::new(),
            module_exports,
            reexports,
        }
    }

    /// Resolve an identifier to a symbol by walking up the persistent scope tree.
    /// This method enables stateless checking - the checker can query scope information
    /// without maintaining a traversal-order-dependent stack.
    ///
    /// Returns the SymbolId for the identifier, or None if not found.
    ///
    /// Debug logging (P1 Task):
    /// When debug mode is enabled, logs:
    /// - Scope chain traversal
    /// - Falls through to file_locals
    /// - Falls through to lib_binders
    /// - Resolution failures
    pub fn resolve_identifier(
        &self,
        arena: &ThinNodeArena,
        node_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let node = arena.get(node_idx)?;

        // Get the identifier text
        let name = if let Some(ident) = arena.get_identifier(node) {
            &ident.escaped_text
        } else {
            return None;
        };

        let debug_enabled = crate::module_resolution_debug::is_debug_enabled();

        if let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) {
            // Walk up the scope chain
            let mut scope_depth = 0;
            while !scope_id.is_none() {
                if let Some(scope) = self.scopes.get(scope_id.0 as usize) {
                    if let Some(sym_id) = scope.table.get(name) {
                        if debug_enabled {
                            eprintln!("[RESOLVE] '{}' FOUND in scope at depth {} (id={})",
                                name, scope_depth, sym_id.0);
                        }
                        // Resolve import if this symbol is imported from another module
                        if let Some(resolved) = self.resolve_import_if_needed(sym_id) {
                            return Some(resolved);
                        }
                        return Some(sym_id);
                    }
                    scope_id = scope.parent;
                    scope_depth += 1;
                } else {
                    break;
                }
            }
        }

        // Fallback for bound-state binders without persistent scopes.
        if let Some(sym_id) = self.resolve_parameter_fallback(arena, node_idx, name) {
            if debug_enabled {
                eprintln!("[RESOLVE] '{}' FOUND via parameter fallback (id={})",
                    name, sym_id.0);
            }
            // Resolve import if this symbol is imported from another module
            if let Some(resolved) = self.resolve_import_if_needed(sym_id) {
                return Some(resolved);
            }
            return Some(sym_id);
        }

        // Finally check file locals / globals
        if let Some(sym_id) = self.file_locals.get(name) {
            if debug_enabled {
                eprintln!("[RESOLVE] '{}' FOUND in file_locals (id={})",
                    name, sym_id.0);
            }
            // Resolve import if this symbol is imported from another module
            if let Some(resolved) = self.resolve_import_if_needed(sym_id) {
                return Some(resolved);
            }
            return Some(sym_id);
        }

        // Chained lookup: check lib binders for global symbols
        // This enables resolving console, Array, Object, etc. from lib.d.ts
        for (i, lib_binder) in self.lib_binders.iter().enumerate() {
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
                if debug_enabled {
                    eprintln!("[RESOLVE] '{}' FOUND in lib_binder[{}] (id={}) - LIB SYMBOL",
                        name, i, sym_id.0);
                }
                // Note: lib symbols are not imports, so no need to resolve
                return Some(sym_id);
            }
        }

        // Symbol not found - log the failure
        if debug_enabled {
            eprintln!("[RESOLVE] '{}' NOT FOUND - searched scopes, file_locals, and {} lib binders",
                name, self.lib_binders.len());
        }

        None
    }

    fn resolve_parameter_fallback(
        &self,
        arena: &ThinNodeArena,
        node_idx: NodeIndex,
        name: &str,
    ) -> Option<SymbolId> {
        if self.scopes.is_empty() {
            let mut current = node_idx;
            while !current.is_none() {
                let node = arena.get(current)?;
                if let Some(func) = arena.get_function(node) {
                    for &param_idx in &func.parameters.nodes {
                        let param_node = arena.get(param_idx)?;
                        let param = arena.get_parameter(param_node)?;
                        let ident_node = arena.get(param.name)?;
                        let ident = arena.get_identifier(ident_node)?;
                        if ident.escaped_text == name {
                            return self.node_symbols.get(&param.name.0).copied();
                        }
                    }
                }
                let ext = arena.get_extended(current)?;
                current = ext.parent;
            }
        }
        None
    }

    /// Resolve an imported symbol to its actual export from the source module.
    ///
    /// When a symbol is imported (e.g., `import { foo } from './file'`), the binder creates
    /// a local ALIAS symbol with `import_module` set to './file'. This method resolves that
    /// alias to the actual exported symbol from the source module by looking up `module_exports`
    /// and following re-export chains.
    ///
    /// Returns the resolved SymbolId, or the original sym_id if it's not an import or resolution fails.
    fn resolve_import_if_needed(&self, sym_id: SymbolId) -> Option<SymbolId> {
        // Get the symbol to check if it's an import
        let sym = self.symbols.get(sym_id)?;
        let module_specifier = sym.import_module.as_ref()?;

        // Determine the export name:
        // - If import_name is set, use it (for renamed imports like `import { foo as bar }`)
        // - Otherwise use the symbol's escaped_name
        let export_name = sym.import_name.as_ref().unwrap_or(&sym.escaped_name);

        // Try to resolve the import, following re-export chains
        self.resolve_import_with_reexports(module_specifier, export_name)
    }

    /// Resolve an import by name from a module, following re-export chains.
    ///
    /// This function handles:
    /// - Direct exports: `export { foo }` - looks up in module_exports
    /// - Named re-exports: `export { foo } from 'bar'` - follows the re-export mapping
    /// - Wildcard re-exports: `export * from 'bar'` - searches the re-exported module
    fn resolve_import_with_reexports(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<SymbolId> {
        let debug_enabled = crate::module_resolution_debug::is_debug_enabled();

        // First, check if it's a direct export from this module
        if let Some(module_table) = self.module_exports.get(module_specifier)
            && let Some(sym_id) = module_table.get(export_name)
        {
            if debug_enabled {
                eprintln!(
                    "[RESOLVE_IMPORT] '{}' from module '{}' -> direct export symbol id={}",
                    export_name, module_specifier, sym_id.0
                );
            }
            return Some(sym_id);
        }

        // Not found in direct exports, check for re-exports
        if let Some(file_reexports) = self.reexports.get(module_specifier) {
            // Check for named re-export: `export { foo } from 'bar'`
            if let Some((source_module, original_name)) = file_reexports.get(export_name) {
                let name_to_lookup = original_name.as_deref().unwrap_or(export_name);
                if debug_enabled {
                    eprintln!(
                        "[RESOLVE_IMPORT] '{}' from module '{}' -> following named re-export from '{}', original name='{}'",
                        export_name, module_specifier, source_module, name_to_lookup
                    );
                }
                return self.resolve_import_with_reexports(source_module, name_to_lookup);
            }

            // Check for wildcard re-export: `export * from 'bar'`
            if let Some((source_module, _)) = file_reexports.get("*") {
                if debug_enabled {
                    eprintln!(
                        "[RESOLVE_IMPORT] '{}' from module '{}' -> following wildcard re-export from '{}'",
                        export_name, module_specifier, source_module
                    );
                }
                return self.resolve_import_with_reexports(source_module, export_name);
            }
        }

        // Export not found
        if debug_enabled {
            eprintln!(
                "[RESOLVE_IMPORT] '{}' from module '{}' -> NOT FOUND",
                export_name, module_specifier
            );
        }
        None
    }

    /// Find the enclosing scope for a given node by walking up the AST.
    /// Returns the ScopeId of the nearest scope-creating ancestor node.
    fn find_enclosing_scope(&self, arena: &ThinNodeArena, node_idx: NodeIndex) -> Option<ScopeId> {
        let mut current = node_idx;

        // Walk up the AST using parent pointers to find the nearest scope
        while !current.is_none() {
            // Check if this node creates a scope
            if let Some(&scope_id) = self.node_scope_ids.get(&current.0) {
                return Some(scope_id);
            }

            // Move to parent node
            if let Some(_node) = arena.get(current) {
                if let Some(ext) = arena.get_extended(current) {
                    current = ext.parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // If no scope found, return the root scope (index 0) if it exists
        if !self.scopes.is_empty() {
            Some(ScopeId(0))
        } else {
            None
        }
    }

    /// Enter a new persistent scope (in addition to legacy scope chain).
    /// This method is called when binding begins for a scope-creating node.
    fn enter_persistent_scope(&mut self, kind: ContainerKind, node: NodeIndex) {
        // Create new scope linked to current
        let new_scope_id = ScopeId(self.scopes.len() as u32);
        let new_scope = Scope::new(self.current_scope_id, kind, node);
        self.scopes.push(new_scope);

        // Map node to this scope
        if !node.is_none() {
            self.node_scope_ids.insert(node.0, new_scope_id);
        }

        // Update current scope
        self.current_scope_id = new_scope_id;
    }

    /// Exit the current persistent scope.
    fn exit_persistent_scope(&mut self) {
        if !self.current_scope_id.is_none()
            && let Some(scope) = self.scopes.get(self.current_scope_id.0 as usize)
        {
            self.current_scope_id = scope.parent;
        }
    }

    /// Declare a symbol in the current persistent scope.
    /// This adds the symbol to the persistent scope table for later querying.
    fn declare_in_persistent_scope(&mut self, name: String, sym_id: SymbolId) {
        if !self.current_scope_id.is_none()
            && let Some(scope) = self.scopes.get_mut(self.current_scope_id.0 as usize)
        {
            scope.table.set(name, sym_id);
        }
    }

    fn sync_current_scope_to_persistent(&mut self) {
        if self.current_scope_id.is_none() {
            return;
        }
        if let Some(persistent_scope) = self.scopes.get_mut(self.current_scope_id.0 as usize) {
            for (name, &sym_id) in self.current_scope.iter() {
                persistent_scope.table.set(name.clone(), sym_id);
            }
        }
    }

    fn source_file_is_external_module(&self, arena: &ThinNodeArena, root: NodeIndex) -> bool {
        let Some(node) = arena.get(root) else {
            return false;
        };
        let Some(source) = arena.get_source_file(node) else {
            return false;
        };

        for &stmt_idx in &source.statements.nodes {
            if stmt_idx.is_none() {
                continue;
            }
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            match stmt.kind {
                syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    return true;
                }
                _ => {}
            }
            if self.is_node_exported(arena, stmt_idx) {
                return true;
            }
        }

        false
    }

    /// Inject lib file symbols into file_locals for global symbol resolution.
    ///
    /// This should be called during binding to make global symbols like
    /// `console`, `Array`, `Promise`, etc. available in the current file's scope.
    ///
    /// # Arguments
    /// * `lib_contexts` - Vector of lib file contexts (arena + binder pairs)
    pub fn inject_lib_symbols(&mut self, lib_contexts: &[LibContext]) {
        for lib_ctx in lib_contexts {
            // Copy symbol references from lib binder's file_locals into our file_locals
            for (name, &sym_id) in lib_ctx.binder.file_locals.iter() {
                // Store the symbol reference
                self.file_locals.set(name.clone(), sym_id);

                // Track which arena this symbol belongs to for cross-file resolution
                self.symbol_arenas
                    .insert(sym_id, Arc::clone(&lib_ctx.arena));
            }
        }
    }

    /// Bind a source file using ThinNodeArena.
    pub fn bind_source_file(&mut self, arena: &ThinNodeArena, root: NodeIndex) {
        // Preserve lib symbols that were merged before binding (e.g., in parallel.rs)
        // When merge_lib_symbols is called before bind_source_file, lib symbols are stored
        // in file_locals and need to be preserved across the binding process.
        let lib_symbols: FxHashMap<String, SymbolId> = self
            .file_locals
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        let has_lib_symbols = !lib_symbols.is_empty();

        // Initialize scope chain with source file scope (legacy)
        self.scope_chain.clear();
        self.scope_chain
            .push(ScopeContext::new(ContainerKind::SourceFile, root, None));
        self.current_scope_idx = 0;
        self.current_scope = SymbolTable::new();

        // Initialize persistent scope system
        self.scopes.clear();
        self.node_scope_ids.clear();
        self.current_scope_id = ScopeId::NONE;
        self.top_level_flow.clear();

        // Create root persistent scope for the source file
        self.enter_persistent_scope(ContainerKind::SourceFile, root);

        // Pre-populate root persistent scope with lib symbols if they were merged before binding
        if has_lib_symbols {
            if let Some(root_scope) = self.scopes.first_mut() {
                for (name, sym_id) in &lib_symbols {
                    root_scope.table.set(name.clone(), *sym_id);
                }
            }

            // Also merge lib symbols into current_scope for immediate availability
            // This ensures symbols like console, Array, Promise are available during binding
            for (name, sym_id) in &lib_symbols {
                if !self.current_scope.has(name) {
                    self.current_scope.set(name.clone(), *sym_id);
                }
            }
        }

        // Create START flow node for the file
        let start_flow = self.flow_nodes.alloc(flow_flags::START);
        self.current_flow = start_flow;
        self.is_external_module = self.source_file_is_external_module(arena, root);

        if let Some(node) = arena.get(root)
            && let Some(sf) = arena.get_source_file(node)
        {
            // First pass: collect hoisted declarations
            self.collect_hoisted_declarations(arena, &sf.statements);

            // Process hoisted function declarations first
            self.process_hoisted_functions(arena);

            // Process hoisted var declarations
            self.process_hoisted_vars();

            // Second pass: bind each statement
            for &stmt_idx in &sf.statements.nodes {
                self.bind_node(arena, stmt_idx);
                self.top_level_flow.insert(stmt_idx.0, self.current_flow);
            }
        }

        self.sync_current_scope_to_persistent();

        // Store file locals, preserving any existing lib symbols.
        // User symbols take precedence - only add lib symbols if no user symbol exists.
        let existing_file_locals = std::mem::take(&mut self.file_locals);
        self.file_locals = std::mem::take(&mut self.current_scope);

        // Merge back any existing file locals (e.g., lib symbols) that were pre-populated.
        for (name, sym_id) in existing_file_locals.iter() {
            if !self.file_locals.has(name) {
                self.file_locals.set(name.clone(), *sym_id);
            }
        }

        // Restore lib symbols from the saved lib_symbols map (if they were pre-merged).
        if has_lib_symbols {
            for (name, sym_id) in &lib_symbols {
                if !self.file_locals.has(name) {
                    self.file_locals.set(name.clone(), *sym_id);
                }
            }
        }
    }

    /// Merge lib file symbols into the current scope.
    ///
    /// This is called during binder initialization to ensure global symbols
    /// from lib.d.ts (like `Object`, `Function`, `console`, etc.) are available
    /// during type checking.
    ///
    /// # Parameters
    /// - `lib_files`: Slice of Arc<LibFile> containing parsed and bound lib files
    ///
    /// # Example
    /// ```ignore
    /// let mut binder = ThinBinderState::new();
    /// binder.bind_source_file(arena, root);
    /// binder.merge_lib_symbols(&lib_files);
    /// ```
    pub fn merge_lib_symbols(&mut self, lib_files: &[Arc<lib_loader::LibFile>]) {
        // Merge lib symbols into file_locals (global scope)
        lib_loader::merge_lib_symbols(&mut self.file_locals, lib_files);

        // Also merge into the current scope if we're at the root level
        if self.scope_chain.len() <= 1 {
            for (name, sym_id) in self.file_locals.iter() {
                if !self.current_scope.has(name) {
                    self.current_scope.set(name.clone(), *sym_id);
                }
            }
        }

        // Merge into the root persistent scope
        if let Some(root_scope) = self.scopes.first_mut() {
            for (name, sym_id) in self.file_locals.iter() {
                if !root_scope.table.has(name) {
                    root_scope.table.set(name.clone(), *sym_id);
                }
            }
        }

        // Track lib arenas for cross-file symbol resolution
        for lib in lib_files {
            // Store symbol arena mappings for all lib symbols
            for (_name, sym_id) in lib.binder.file_locals.iter() {
                if !self.symbol_arenas.contains_key(sym_id) {
                    self.symbol_arenas.insert(*sym_id, Arc::clone(&lib.arena));
                }
            }
            // Store lib binders for automatic symbol resolution in get_symbol()
            self.lib_binders.push(Arc::clone(&lib.binder));
        }
    }

    /// Bind a source file with lib symbols merged in.
    ///
    /// This is a convenience method that combines `bind_source_file` and `merge_lib_symbols`.
    ///
    /// CRITICAL: Lib symbols MUST be merged BEFORE binding the source file so that
    /// global symbols like `console`, `Array`, `Promise` are available during binding.
    /// If we bind first, the binder will emit TS2304 errors for these symbols.
    ///
    /// # Parameters
    /// - `arena`: The ThinNodeArena containing the AST
    /// - `root`: The root node index of the source file
    /// - `lib_files`: Optional slice of Arc<LibFile> containing lib files
    pub fn bind_source_file_with_libs(
        &mut self,
        arena: &ThinNodeArena,
        root: NodeIndex,
        lib_files: &[Arc<lib_loader::LibFile>],
    ) {
        // IMPORTANT: Merge lib symbols FIRST so they're available during binding
        if !lib_files.is_empty() {
            self.merge_lib_symbols(lib_files);
        }
        self.bind_source_file(arena, root);
    }

    /// Incrementally bind new statements after a prefix without rebinding the entire file.
    pub fn bind_source_file_incremental(
        &mut self,
        arena: &ThinNodeArena,
        root: NodeIndex,
        prefix_statements: &[NodeIndex],
        old_suffix_statements: &[NodeIndex],
        new_suffix_statements: &[NodeIndex],
        reparse_start: u32,
    ) -> bool {
        let last_prefix = match prefix_statements.last() {
            Some(stmt) => *stmt,
            None => return false,
        };
        let start_flow = match self.top_level_flow.get(&last_prefix.0) {
            Some(flow) => *flow,
            None => return false,
        };
        if self.scopes.is_empty() {
            return false;
        }

        self.is_external_module = self.source_file_is_external_module(arena, root);

        self.prune_incremental_maps(arena, reparse_start);

        let mut prefix_names = FxHashSet::default();
        self.collect_file_scope_names_for_statements(arena, prefix_statements, &mut prefix_names);

        let mut old_suffix_names = FxHashSet::default();
        self.collect_file_scope_names_for_statements(
            arena,
            old_suffix_statements,
            &mut old_suffix_names,
        );

        for name in old_suffix_names {
            if prefix_names.contains(&name) {
                continue;
            }
            self.file_locals.remove(&name);
            if let Some(scope) = self.scopes.get_mut(0) {
                scope.table.remove(&name);
            }
        }

        let mut symbol_nodes = Vec::new();
        self.collect_statement_symbol_nodes(arena, old_suffix_statements, &mut symbol_nodes);
        for node in symbol_nodes {
            if let Some(sym_id) = self.node_symbols.remove(&node.0)
                && let Some(sym) = self.symbols.get_mut(sym_id)
            {
                sym.declarations.retain(|decl| *decl != node);
                if sym.value_declaration == node {
                    sym.value_declaration =
                        sym.declarations.first().copied().unwrap_or(NodeIndex::NONE);
                }
            }
        }

        for stmt_idx in old_suffix_statements {
            self.top_level_flow.remove(&stmt_idx.0);
        }

        // Reset transient binding state while keeping existing symbols and scopes.
        self.scope_chain.clear();
        self.scope_chain
            .push(ScopeContext::new(ContainerKind::SourceFile, root, None));
        self.current_scope_idx = 0;
        self.scope_stack.clear();
        self.current_scope = self.file_locals.clone();
        self.hoisted_vars.clear();
        self.hoisted_functions.clear();
        self.current_scope_id = ScopeId(0);
        self.current_flow = start_flow;

        let new_suffix_list = NodeList {
            nodes: new_suffix_statements.to_vec(),
            pos: 0,
            end: 0,
            has_trailing_comma: false,
        };

        self.collect_hoisted_declarations(arena, &new_suffix_list);
        self.process_hoisted_functions(arena);
        self.process_hoisted_vars();

        for &stmt_idx in new_suffix_statements {
            self.bind_node(arena, stmt_idx);
            self.top_level_flow.insert(stmt_idx.0, self.current_flow);
        }

        self.sync_current_scope_to_persistent();

        // Store file locals, preserving any existing lib symbols
        // This ensures symbols from merge_lib_symbols() are not lost
        let existing_file_locals = std::mem::take(&mut self.file_locals);
        self.file_locals = std::mem::take(&mut self.current_scope);
        // Merge back any existing file locals (e.g., lib symbols) that were pre-populated
        for (name, sym_id) in existing_file_locals.iter() {
            if !self.file_locals.has(name) {
                self.file_locals.set(name.clone(), *sym_id);
            }
        }

        true
    }

    fn prune_incremental_maps(&mut self, arena: &ThinNodeArena, reparse_start: u32) {
        if reparse_start == 0 {
            return;
        }

        let keep_node = |node_id: &u32| {
            arena
                .get(NodeIndex(*node_id))
                .is_some_and(|node| node.pos < reparse_start)
        };

        self.node_flow.retain(|node_id, _| keep_node(node_id));
        self.node_scope_ids.retain(|node_id, _| keep_node(node_id));
        self.switch_clause_to_switch
            .retain(|node_id, _| keep_node(node_id));
    }

    /// Collect hoisted declarations from statements.
    fn collect_hoisted_declarations(&mut self, arena: &ThinNodeArena, statements: &NodeList) {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = arena.get(stmt_idx) {
                match node.kind {
                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = arena.get_variable(node) {
                            // VariableStatement stores declaration_list as first element
                            if let Some(&decl_list_idx) = var_stmt.declarations.nodes.first() {
                                self.collect_hoisted_var_decl(arena, decl_list_idx);
                            }
                        }
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        self.hoisted_functions.push(stmt_idx);
                    }
                    k if k == syntax_kind_ext::BLOCK => {
                        if let Some(block) = arena.get_block(node) {
                            self.collect_hoisted_declarations(arena, &block.statements);
                        }
                    }
                    k if k == syntax_kind_ext::IF_STATEMENT => {
                        if let Some(if_stmt) = arena.get_if_statement(node) {
                            self.collect_hoisted_from_node(arena, if_stmt.then_statement);
                            if !if_stmt.else_statement.is_none() {
                                self.collect_hoisted_from_node(arena, if_stmt.else_statement);
                            }
                        }
                    }
                    k if k == syntax_kind_ext::WHILE_STATEMENT
                        || k == syntax_kind_ext::DO_STATEMENT
                        || k == syntax_kind_ext::FOR_STATEMENT =>
                    {
                        if let Some(loop_data) = arena.get_loop(node) {
                            self.collect_hoisted_from_node(arena, loop_data.statement);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn collect_hoisted_var_decl(&mut self, arena: &ThinNodeArena, decl_list_idx: NodeIndex) {
        if let Some(node) = arena.get(decl_list_idx)
            && let Some(list) = arena.get_variable(node)
        {
            // Check if this is a var declaration (not let/const)
            let is_var = (node.flags as u32 & (node_flags::LET | node_flags::CONST)) == 0;
            if is_var {
                for &decl_idx in &list.declarations.nodes {
                    if let Some(decl_node) = arena.get(decl_idx)
                        && let Some(decl) = arena.get_variable_declaration(decl_node)
                    {
                        if let Some(name) = self.get_identifier_name(arena, decl.name) {
                            self.hoisted_vars.push((name.to_string(), decl_idx));
                        } else {
                            let mut names = Vec::new();
                            self.collect_binding_identifiers(arena, decl.name, &mut names);
                            for ident_idx in names {
                                if let Some(name) =
                                    self.get_identifier_name(arena, ident_idx)
                                {
                                    self.hoisted_vars.push((name.to_string(), ident_idx));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn collect_hoisted_from_node(&mut self, arena: &ThinNodeArena, idx: NodeIndex) {
        if let Some(node) = arena.get(idx)
            && node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = arena.get_block(node)
        {
            self.collect_hoisted_declarations(arena, &block.statements);
        }
    }

    /// Process hoisted function declarations.
    fn process_hoisted_functions(&mut self, arena: &ThinNodeArena) {
        let functions = std::mem::take(&mut self.hoisted_functions);
        for func_idx in functions {
            if let Some(node) = arena.get(func_idx)
                && let Some(func) = arena.get_function(node)
                && let Some(name) = self.get_identifier_name(arena, func.name)
            {
                let is_exported = self.has_export_modifier(arena, &func.modifiers);
                let sym_id = self.declare_symbol(
                    name,
                    symbol_flags::FUNCTION,
                    func_idx,
                    is_exported,
                );

                // Also add to persistent scope
                self.declare_in_persistent_scope(name.to_string(), sym_id);
            }
        }
    }

    /// Process hoisted var declarations.
    /// Var declarations are hoisted to the enclosing function or file scope.
    fn process_hoisted_vars(&mut self) {
        let vars = std::mem::take(&mut self.hoisted_vars);
        for (name, decl_idx) in vars {
            // Declare the var in the current scope (function or file level)
            let sym_id = self.declare_symbol(
                &name,
                symbol_flags::FUNCTION_SCOPED_VARIABLE,
                decl_idx,
                false, // hoisted vars are not exported
            );

            // Also add to persistent scope
            self.declare_in_persistent_scope(name, sym_id);
        }
    }

    /// Bind a node and its children.
    fn bind_node(&mut self, arena: &ThinNodeArena, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let node = match arena.get(idx) {
            Some(n) => n,
            None => return,
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                self.record_flow(idx);
            }
            // Variable declarations
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = arena.get_variable(node) {
                    // VariableStatement stores declaration_list as first element
                    if let Some(&decl_list_idx) = var_stmt.declarations.nodes.first() {
                        self.bind_node(arena, decl_list_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
                if let Some(list) = arena.get_variable(node) {
                    for &decl_idx in &list.declarations.nodes {
                        self.bind_node(arena, decl_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                self.bind_variable_declaration(arena, node, idx);
            }

            // Function declarations
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.bind_function_declaration(arena, node, idx);
            }

            // Method declarations (in object literals)
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = arena.get_method_decl(node) {
                    self.bind_callable_body(arena, &method.parameters, method.body, idx);
                }
            }

            // Class declarations
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.record_flow(idx);
                self.bind_class_declaration(arena, node, idx);
            }
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                self.bind_class_expression(arena, node, idx);
            }

            // Interface declarations
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                self.bind_interface_declaration(arena, node, idx);
            }

            // Type alias declarations
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.bind_type_alias_declaration(arena, node, idx);
            }

            // Enum declarations
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.bind_enum_declaration(arena, node, idx);
            }

            // Block - creates a new block scope
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = arena.get_block(node) {
                    self.enter_scope(ContainerKind::Block, idx);
                    for &stmt_idx in &block.statements.nodes {
                        self.bind_node(arena, stmt_idx);
                    }
                    self.exit_scope(arena);
                }
            }

            // If statement - build flow graph for type narrowing
            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.record_flow(idx);
                if let Some(if_stmt) = arena.get_if_statement(node) {
                    // Bind the condition expression (record identifiers in it)
                    self.bind_expression(arena, if_stmt.expression);

                    // Save the pre-condition flow
                    let pre_condition_flow = self.current_flow;

                    // Create TRUE_CONDITION flow for the then branch
                    let true_flow = self.create_flow_condition(
                        flow_flags::TRUE_CONDITION,
                        pre_condition_flow,
                        if_stmt.expression,
                    );

                    // Bind the then branch with narrowed flow
                    self.current_flow = true_flow;
                    self.bind_node(arena, if_stmt.then_statement);
                    let after_then_flow = self.current_flow;

                    // Handle else branch if present
                    let after_else_flow = if !if_stmt.else_statement.is_none() {
                        // Create FALSE_CONDITION flow for the else branch
                        let false_flow = self.create_flow_condition(
                            flow_flags::FALSE_CONDITION,
                            pre_condition_flow,
                            if_stmt.expression,
                        );

                        // Bind the else branch with narrowed flow
                        self.current_flow = false_flow;
                        self.bind_node(arena, if_stmt.else_statement);
                        self.current_flow
                    } else {
                        // No else branch - false condition goes directly to merge
                        self.create_flow_condition(
                            flow_flags::FALSE_CONDITION,
                            pre_condition_flow,
                            if_stmt.expression,
                        )
                    };

                    // Create merge point for branches
                    let merge_label = self.create_branch_label();
                    self.add_antecedent(merge_label, after_then_flow);
                    self.add_antecedent(merge_label, after_else_flow);
                    self.current_flow = merge_label;
                }
            }

            // While/do statement
            k if k == syntax_kind_ext::WHILE_STATEMENT || k == syntax_kind_ext::DO_STATEMENT => {
                if let Some(loop_data) = arena.get_loop(node) {
                    let pre_loop_flow = self.current_flow;
                    let loop_label = self.create_loop_label();
                    if !self.current_flow.is_none() {
                        self.add_antecedent(loop_label, self.current_flow);
                    }
                    self.current_flow = loop_label;

                    if node.kind == syntax_kind_ext::DO_STATEMENT {
                        self.bind_node(arena, loop_data.statement);
                        self.bind_expression(arena, loop_data.condition);

                        let pre_condition_flow = self.current_flow;
                        let true_flow = self.create_flow_condition(
                            flow_flags::TRUE_CONDITION,
                            pre_condition_flow,
                            loop_data.condition,
                        );
                        self.add_antecedent(loop_label, true_flow);

                        let false_flow = self.create_flow_condition(
                            flow_flags::FALSE_CONDITION,
                            pre_condition_flow,
                            loop_data.condition,
                        );
                        let merge_label = self.create_branch_label();
                        self.add_antecedent(merge_label, pre_condition_flow);
                        self.add_antecedent(merge_label, false_flow);
                        self.current_flow = merge_label;
                    } else {
                        self.bind_expression(arena, loop_data.condition);

                        let pre_condition_flow = self.current_flow;
                        let true_flow = self.create_flow_condition(
                            flow_flags::TRUE_CONDITION,
                            pre_condition_flow,
                            loop_data.condition,
                        );
                        self.current_flow = true_flow;
                        self.bind_node(arena, loop_data.statement);
                        self.add_antecedent(loop_label, self.current_flow);

                        let false_flow = self.create_flow_condition(
                            flow_flags::FALSE_CONDITION,
                            pre_condition_flow,
                            loop_data.condition,
                        );
                        let merge_label = self.create_branch_label();
                        self.add_antecedent(merge_label, pre_loop_flow);
                        self.add_antecedent(merge_label, false_flow);
                        self.current_flow = merge_label;
                    }
                }
            }

            // For statement
            k if k == syntax_kind_ext::FOR_STATEMENT => {
                self.record_flow(idx);
                if let Some(loop_data) = arena.get_loop(node) {
                    self.enter_scope(ContainerKind::Block, idx);
                    self.bind_node(arena, loop_data.initializer);

                    let pre_loop_flow = self.current_flow;
                    let loop_label = self.create_loop_label();
                    if !self.current_flow.is_none() {
                        self.add_antecedent(loop_label, self.current_flow);
                    }
                    self.current_flow = loop_label;

                    if !loop_data.condition.is_none() {
                        self.bind_expression(arena, loop_data.condition);
                        let pre_condition_flow = self.current_flow;
                        let true_flow = self.create_flow_condition(
                            flow_flags::TRUE_CONDITION,
                            pre_condition_flow,
                            loop_data.condition,
                        );
                        self.current_flow = true_flow;
                        self.bind_node(arena, loop_data.statement);
                        self.bind_expression(arena, loop_data.incrementor);
                        self.add_antecedent(loop_label, self.current_flow);

                        let false_flow = self.create_flow_condition(
                            flow_flags::FALSE_CONDITION,
                            pre_condition_flow,
                            loop_data.condition,
                        );
                        let merge_label = self.create_branch_label();
                        self.add_antecedent(merge_label, pre_loop_flow);
                        self.add_antecedent(merge_label, false_flow);
                        self.current_flow = merge_label;
                    } else {
                        self.bind_node(arena, loop_data.statement);
                        self.bind_expression(arena, loop_data.incrementor);
                        self.add_antecedent(loop_label, self.current_flow);
                        let merge_label = self.create_branch_label();
                        self.add_antecedent(merge_label, loop_label);
                        self.add_antecedent(merge_label, self.current_flow);
                        self.current_flow = merge_label;
                    }
                    self.exit_scope(arena);
                }
            }

            // For-in/for-of
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                self.record_flow(idx);
                if let Some(for_data) = arena.get_for_in_of(node) {
                    self.enter_scope(ContainerKind::Block, idx);
                    self.bind_node(arena, for_data.initializer);
                    let loop_label = self.create_loop_label();
                    if !self.current_flow.is_none() {
                        self.add_antecedent(loop_label, self.current_flow);
                    }
                    self.current_flow = loop_label;

                    self.bind_expression(arena, for_data.expression);
                    if !for_data.initializer.is_none() {
                        let flow = self.create_flow_assignment(for_data.initializer);
                        self.current_flow = flow;
                    }
                    self.bind_node(arena, for_data.statement);
                    self.add_antecedent(loop_label, self.current_flow);
                    let merge_label = self.create_branch_label();
                    self.add_antecedent(merge_label, loop_label);
                    self.add_antecedent(merge_label, self.current_flow);
                    self.current_flow = merge_label;
                    self.exit_scope(arena);
                }
            }

            // Switch statement
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                self.bind_switch_statement(arena, node, idx);
            }

            // Try statement
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.bind_try_statement(arena, node, idx);
            }

            // Labeled statement
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = arena.get_labeled_statement(node) {
                    self.bind_node(arena, labeled.statement);
                }
            }

            // With statement
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_stmt) = arena.get_with_statement(node) {
                    self.bind_node(arena, with_stmt.expression);
                    self.bind_node(arena, with_stmt.then_statement);
                }
            }

            // Import declarations
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                self.bind_import_declaration(arena, node, idx);
            }

            // Import equals declaration (import x = ns.member)
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.bind_import_equals_declaration(arena, node, idx);
            }

            // Export declarations - bind the exported declaration
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                self.bind_export_declaration(arena, node, idx);
            }
            // Export assignment - bind the assigned expression
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                if let Some(assign) = arena.get_export_assignment(node) {
                    self.bind_node(arena, assign.expression);
                }
            }

            // Module/namespace declarations
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.bind_module_declaration(arena, node, idx);
            }
            k if k == syntax_kind_ext::MODULE_BLOCK => {
                if let Some(block) = arena.get_module_block(node)
                    && let Some(ref statements) = block.statements
                {
                    for &stmt_idx in &statements.nodes {
                        self.bind_node(arena, stmt_idx);
                    }
                }
            }

            // Expression statements - record flow and traverse into the expression
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                self.record_flow(idx);
                if let Some(expr_stmt) = arena.get_expression_statement(node) {
                    self.bind_node(arena, expr_stmt.expression);
                }
            }

            // Return/throw statements - traverse into the expression
            k if k == syntax_kind_ext::RETURN_STATEMENT
                || k == syntax_kind_ext::THROW_STATEMENT =>
            {
                if let Some(ret) = arena.get_return_statement(node)
                    && !ret.expression.is_none()
                {
                    self.bind_node(arena, ret.expression);
                }
            }

            // Binary expressions - traverse into operands
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = arena.get_binary_expr(node)
                    && self.is_assignment_operator(bin.operator_token)
                {
                    self.bind_node(arena, bin.left);
                    self.bind_node(arena, bin.right);
                    let flow = self.create_flow_assignment(idx);
                    self.current_flow = flow;
                    return;
                }
                // Record flow for binary expressions to support flow analysis in closures
                self.bind_binary_expression_iterative(arena, idx);
                self.record_flow(idx);
            }

            // Conditional expressions - traverse into branches
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = arena.get_conditional_expr(node) {
                    self.bind_node(arena, cond.condition);
                    self.bind_node(arena, cond.when_true);
                    self.bind_node(arena, cond.when_false);
                }
            }

            // Property access / element access
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                self.record_flow(idx);
                if let Some(access) = arena.get_access_expr(node) {
                    self.bind_node(arena, access.expression);
                    self.bind_node(arena, access.name_or_argument);
                }
            }

            // Prefix/postfix unary expressions
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                if let Some(unary) = arena.get_unary_expr(node) {
                    self.bind_node(arena, unary.operand);
                    if unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16
                    {
                        let flow = self.create_flow_assignment(idx);
                        self.current_flow = flow;
                    }
                }
            }

            // Await/yield expressions
            k if k == syntax_kind_ext::AWAIT_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION =>
            {
                if node.has_data()
                    && let Some(unary) = arena.unary_exprs_ex.get(node.data_index as usize)
                {
                    self.bind_node(arena, unary.expression);
                }
            }

            // Type assertions / as / satisfies
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                if node.has_data()
                    && let Some(assertion) = arena.type_assertions.get(node.data_index as usize)
                {
                    self.bind_node(arena, assertion.expression);
                }
            }

            // Decorators
            k if k == syntax_kind_ext::DECORATOR => {
                if let Some(decorator) = arena.get_decorator(node) {
                    self.bind_node(arena, decorator.expression);
                }
            }

            // Tagged templates
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                if node.has_data()
                    && let Some(tagged) = arena.tagged_templates.get(node.data_index as usize)
                {
                    self.bind_node(arena, tagged.tag);
                    self.bind_node(arena, tagged.template);
                }
            }

            // Template expressions
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                if let Some(template) = arena.get_template_expr(node) {
                    self.bind_node(arena, template.head);
                    for &span in &template.template_spans.nodes {
                        self.bind_node(arena, span);
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_SPAN => {
                if let Some(span) = arena.get_template_span(node) {
                    self.bind_node(arena, span.expression);
                    self.bind_node(arena, span.literal);
                }
            }

            // Object/array literals
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                if let Some(lit) = arena.get_literal_expr(node) {
                    for &elem in &lit.elements.nodes {
                        self.bind_node(arena, elem);
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = arena.get_property_assignment(node) {
                    self.bind_node(arena, prop.name);
                    self.bind_node(arena, prop.initializer);
                }
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(prop) = arena.get_shorthand_property(node) {
                    self.bind_node(arena, prop.name);
                    if !prop.object_assignment_initializer.is_none() {
                        self.bind_node(arena, prop.object_assignment_initializer);
                    }
                }
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT
                || k == syntax_kind_ext::SPREAD_ASSIGNMENT =>
            {
                if let Some(spread) = arena.get_spread(node) {
                    self.bind_node(arena, spread.expression);
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                if let Some(computed) = arena.get_computed_property(node) {
                    self.bind_node(arena, computed.expression);
                }
            }

            // Call expressions - traverse into callee and arguments
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = arena.get_call_expr(node) {
                    self.bind_node(arena, call.expression);
                    if let Some(args) = &call.arguments {
                        for &arg in &args.nodes {
                            self.bind_node(arena, arg);
                        }
                    }
                    let flow = self.create_flow_call(idx);
                    self.current_flow = flow;
                    if self.is_array_mutation_call(arena, idx) {
                        let flow = self.create_flow_array_mutation(idx);
                        self.current_flow = flow;
                    }
                }
            }

            // New expressions - traverse into expression and arguments
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                if let Some(new_expr) = arena.get_call_expr(node) {
                    self.bind_node(arena, new_expr.expression);
                    if let Some(args) = &new_expr.arguments {
                        for &arg in &args.nodes {
                            self.bind_node(arena, arg);
                        }
                    }
                }
            }

            // Parenthesized expressions - traverse into inner expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = arena.get_parenthesized(node) {
                    self.bind_node(arena, paren.expression);
                }
            }

            // Arrow function expressions - bind body
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                self.bind_arrow_function(arena, node, idx);
            }

            // Function expressions - bind body
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                self.bind_function_expression(arena, node, idx);
            }

            // Typeof, void, await, yield expressions - record flow and traverse into operand
            k if k == syntax_kind_ext::TYPE_OF_EXPRESSION
                || k == syntax_kind_ext::VOID_EXPRESSION
                || k == syntax_kind_ext::AWAIT_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION =>
            {
                self.record_flow(idx);
                if let Some(unary) = arena.get_unary_expr(node) {
                    self.bind_node(arena, unary.operand);
                }
            }

            // Identifier references - record current flow for type narrowing queries
            k if k == SyntaxKind::Identifier as u16 => {
                self.record_flow(idx);
            }

            _ => {
                // For other node types, no symbols to create
            }
        }
    }

    /// Get identifier name from a node index.
    fn get_identifier_name<'a>(&self, arena: &'a ThinNodeArena, idx: NodeIndex) -> Option<&'a str> {
        if let Some(node) = arena.get(idx)
            && let Some(id) = arena.get_identifier(node)
        {
            return Some(&id.escaped_text);
        }
        None
    }

    fn collect_binding_identifiers(
        &self,
        arena: &ThinNodeArena,
        idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        if idx.is_none() {
            return;
        }

        let Some(node) = arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                out.push(idx);
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(binding) = arena.get_binding_element(node) {
                    self.collect_binding_identifiers(arena, binding.name, out);
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                if let Some(pattern) = arena.get_binding_pattern(node) {
                    for &elem in &pattern.elements.nodes {
                        if elem.is_none() {
                            continue;
                        }
                        self.collect_binding_identifiers(arena, elem, out);
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_file_scope_names_for_statements(
        &self,
        arena: &ThinNodeArena,
        statements: &[NodeIndex],
        out: &mut FxHashSet<String>,
    ) {
        for &stmt_idx in statements {
            self.collect_file_scope_names_for_statement(arena, stmt_idx, out);
        }
    }

    fn collect_file_scope_names_for_statement(
        &self,
        arena: &ThinNodeArena,
        idx: NodeIndex,
        out: &mut FxHashSet<String>,
    ) {
        let Some(node) = arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = arena.get_variable(node) {
                    for &decl_list_idx in &var_stmt.declarations.nodes {
                        self.collect_variable_decl_names(arena, decl_list_idx, true, out);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = arena.get_function(node)
                    && let Some(name) = self.get_identifier_name(arena, func.name)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = arena.get_class(node)
                    && let Some(name) = self.get_identifier_name(arena, class.name)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(iface) = arena.get_interface(node)
                    && let Some(name) = self.get_identifier_name(arena, iface.name)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(alias) = arena.get_type_alias(node)
                    && let Some(name) = self.get_identifier_name(arena, alias.name)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = arena.get_enum(node)
                    && let Some(name) = self.get_identifier_name(arena, enum_decl.name)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = arena.get_module(node)
                    && let Some(name) = self.get_identifier_name(arena, module.name)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                self.collect_import_names(arena, node, out);
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                if let Some(import) = arena.get_import_decl(node)
                    && let Some(name) = self.get_identifier_name(arena, import.import_clause)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export) = arena.get_export_decl(node) {
                    if export.export_clause.is_none() {
                        return;
                    }
                    let Some(clause_node) = arena.get(export.export_clause) else {
                        return;
                    };
                    if self.is_declaration(clause_node.kind) {
                        self.collect_file_scope_names_for_statement(
                            arena,
                            export.export_clause,
                            out,
                        );
                    } else if clause_node.kind == SyntaxKind::Identifier as u16 {
                        if let Some(name) = self.get_identifier_name(arena, export.export_clause) {
                            out.insert(name.to_string());
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::BLOCK
                || k == syntax_kind_ext::IF_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT =>
            {
                self.collect_hoisted_file_scope_names(arena, idx, out);
            }
            _ => {}
        }
    }

    fn collect_hoisted_file_scope_names(
        &self,
        arena: &ThinNodeArena,
        idx: NodeIndex,
        out: &mut FxHashSet<String>,
    ) {
        let Some(node) = arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = arena.get_variable(node)
                    && let Some(&decl_list_idx) = var_stmt.declarations.nodes.first()
                {
                    self.collect_variable_decl_names(arena, decl_list_idx, false, out);
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = arena.get_function(node)
                    && let Some(name) = self.get_identifier_name(arena, func.name)
                {
                    out.insert(name.to_string());
                }
            }
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.collect_hoisted_file_scope_names(arena, stmt_idx, out);
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = arena.get_if_statement(node) {
                    self.collect_hoisted_file_scope_from_node(arena, if_stmt.then_statement, out);
                    if !if_stmt.else_statement.is_none() {
                        self.collect_hoisted_file_scope_from_node(
                            arena,
                            if_stmt.else_statement,
                            out,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT =>
            {
                if let Some(loop_data) = arena.get_loop(node) {
                    self.collect_hoisted_file_scope_from_node(arena, loop_data.statement, out);
                }
            }
            _ => {}
        }
    }

    fn collect_hoisted_file_scope_from_node(
        &self,
        arena: &ThinNodeArena,
        idx: NodeIndex,
        out: &mut FxHashSet<String>,
    ) {
        if let Some(node) = arena.get(idx)
            && node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = arena.get_block(node)
        {
            for &stmt_idx in &block.statements.nodes {
                self.collect_hoisted_file_scope_names(arena, stmt_idx, out);
            }
        }
    }

    fn collect_variable_decl_names(
        &self,
        arena: &ThinNodeArena,
        decl_list_idx: NodeIndex,
        include_block_scoped: bool,
        out: &mut FxHashSet<String>,
    ) {
        let Some(node) = arena.get(decl_list_idx) else {
            return;
        };
        let Some(list) = arena.get_variable(node) else {
            return;
        };
        let is_var = (node.flags as u32 & (node_flags::LET | node_flags::CONST)) == 0;
        if !include_block_scoped && !is_var {
            return;
        }

        for &decl_idx in &list.declarations.nodes {
            if let Some(decl_node) = arena.get(decl_idx)
                && let Some(decl) = arena.get_variable_declaration(decl_node)
            {
                if let Some(name) = self.get_identifier_name(arena, decl.name) {
                    out.insert(name.to_string());
                } else {
                    let mut names = Vec::new();
                    self.collect_binding_identifiers(arena, decl.name, &mut names);
                    for ident_idx in names {
                        if let Some(name) = self.get_identifier_name(arena, ident_idx) {
                            out.insert(name.to_string());
                        }
                    }
                }
            }
        }
    }

    fn collect_import_names(
        &self,
        arena: &ThinNodeArena,
        node: &ThinNode,
        out: &mut FxHashSet<String>,
    ) {
        if let Some(import) = arena.get_import_decl(node)
            && let Some(clause_node) = arena.get(import.import_clause)
            && let Some(clause) = arena.get_import_clause(clause_node)
        {
            if !clause.name.is_none()
                && let Some(name) = self.get_identifier_name(arena, clause.name)
            {
                out.insert(name.to_string());
            }
            if !clause.named_bindings.is_none()
                && let Some(bindings_node) = arena.get(clause.named_bindings)
            {
                if bindings_node.kind == SyntaxKind::Identifier as u16 {
                    if let Some(name) =
                        self.get_identifier_name(arena, clause.named_bindings)
                    {
                        out.insert(name.to_string());
                    }
                } else if let Some(named) = arena.get_named_imports(bindings_node) {
                    for &spec_idx in &named.elements.nodes {
                        if let Some(spec_node) = arena.get(spec_idx)
                            && let Some(spec) = arena.get_specifier(spec_node)
                        {
                            let local_ident = if !spec.name.is_none() {
                                spec.name
                            } else {
                                spec.property_name
                            };
                            if let Some(name) =
                                self.get_identifier_name(arena, local_ident)
                            {
                                out.insert(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    fn collect_statement_symbol_nodes(
        &self,
        arena: &ThinNodeArena,
        statements: &[NodeIndex],
        out: &mut Vec<NodeIndex>,
    ) {
        for &stmt_idx in statements {
            let Some(node) = arena.get(stmt_idx) else {
                continue;
            };
            match node.kind {
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    if let Some(var_stmt) = arena.get_variable(node) {
                        for &decl_list_idx in &var_stmt.declarations.nodes {
                            self.collect_variable_decl_symbol_nodes(arena, decl_list_idx, out);
                        }
                    }
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::INTERFACE_DECLARATION
                    || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || k == syntax_kind_ext::ENUM_DECLARATION
                    || k == syntax_kind_ext::MODULE_DECLARATION =>
                {
                    out.push(stmt_idx);
                }
                k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                    self.collect_import_symbol_nodes(arena, node, out);
                }
                k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                    out.push(stmt_idx);
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    self.collect_export_symbol_nodes(arena, node, out);
                }
                _ => {}
            }
        }
    }

    fn collect_variable_decl_symbol_nodes(
        &self,
        arena: &ThinNodeArena,
        decl_list_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(node) = arena.get(decl_list_idx) else {
            return;
        };
        let Some(list) = arena.get_variable(node) else {
            return;
        };

        for &decl_idx in &list.declarations.nodes {
            out.push(decl_idx);
            if let Some(decl_node) = arena.get(decl_idx)
                && let Some(decl) = arena.get_variable_declaration(decl_node)
            {
                if self.get_identifier_name(arena, decl.name).is_some() {
                    out.push(decl.name);
                } else {
                    let mut names = Vec::new();
                    self.collect_binding_identifiers(arena, decl.name, &mut names);
                    out.extend(names);
                }
            }
        }
    }

    fn collect_import_symbol_nodes(
        &self,
        arena: &ThinNodeArena,
        node: &ThinNode,
        out: &mut Vec<NodeIndex>,
    ) {
        if let Some(import) = arena.get_import_decl(node)
            && let Some(clause_node) = arena.get(import.import_clause)
            && let Some(clause) = arena.get_import_clause(clause_node)
        {
            if !clause.name.is_none() {
                out.push(clause.name);
            }
            if !clause.named_bindings.is_none()
                && let Some(bindings_node) = arena.get(clause.named_bindings)
            {
                if bindings_node.kind == SyntaxKind::Identifier as u16 {
                    out.push(clause.named_bindings);
                } else if let Some(named) = arena.get_named_imports(bindings_node) {
                    for &spec_idx in &named.elements.nodes {
                        out.push(spec_idx);
                        if let Some(spec_node) = arena.get(spec_idx)
                            && let Some(spec) = arena.get_specifier(spec_node)
                        {
                            let local_ident = if !spec.name.is_none() {
                                spec.name
                            } else {
                                spec.property_name
                            };
                            if !local_ident.is_none() {
                                out.push(local_ident);
                            }
                        }
                    }
                }
            }
        }
    }

    fn collect_export_symbol_nodes(
        &self,
        arena: &ThinNodeArena,
        node: &ThinNode,
        out: &mut Vec<NodeIndex>,
    ) {
        if let Some(export) = arena.get_export_decl(node) {
            if export.export_clause.is_none() {
                return;
            }
            let Some(clause_node) = arena.get(export.export_clause) else {
                return;
            };
            if let Some(named) = arena.get_named_imports(clause_node) {
                for &spec_idx in &named.elements.nodes {
                    out.push(spec_idx);
                }
            } else if self.is_declaration(clause_node.kind) {
                self.collect_statement_symbol_nodes(arena, &[export.export_clause], out);
            } else if clause_node.kind == SyntaxKind::Identifier as u16 {
                out.push(export.export_clause);
            }
        }
    }

    /// Check if modifiers list contains the 'abstract' keyword.
    fn has_abstract_modifier(&self, arena: &ThinNodeArena, modifiers: &Option<NodeList>) -> bool {
        use crate::scanner::SyntaxKind;

        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::AbstractKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if modifiers list contains the 'static' keyword.
    fn has_static_modifier(&self, arena: &ThinNodeArena, modifiers: &Option<NodeList>) -> bool {
        use crate::scanner::SyntaxKind;

        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::StaticKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if modifiers list contains the 'export' keyword.
    fn has_export_modifier(&self, arena: &ThinNodeArena, modifiers: &Option<NodeList>) -> bool {
        use crate::scanner::SyntaxKind;

        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::ExportKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if modifiers list contains the 'declare' keyword.
    fn has_declare_modifier(&self, arena: &ThinNodeArena, modifiers: &Option<NodeList>) -> bool {
        use crate::scanner::SyntaxKind;

        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::DeclareKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if modifiers list contains the 'const' keyword.
    fn has_const_modifier(&self, arena: &ThinNodeArena, modifiers: &Option<NodeList>) -> bool {
        use crate::scanner::SyntaxKind;

        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::ConstKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a node is exported.
    /// Handles walking up the tree for VariableDeclaration -> VariableStatement.
    fn is_node_exported(&self, arena: &ThinNodeArena, idx: NodeIndex) -> bool {
        let Some(node) = arena.get(idx) else {
            return false;
        };

        // 1. Check direct modifiers (Function, Class, Interface, Enum, Module, TypeAlias)
        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = arena.get_function(node) {
                    return self.has_export_modifier(arena, &func.modifiers);
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = arena.get_class(node) {
                    return self.has_export_modifier(arena, &class.modifiers);
                }
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(iface) = arena.get_interface(node) {
                    return self.has_export_modifier(arena, &iface.modifiers);
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(alias) = arena.get_type_alias(node) {
                    return self.has_export_modifier(arena, &alias.modifiers);
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = arena.get_enum(node) {
                    return self.has_export_modifier(arena, &enum_decl.modifiers);
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = arena.get_module(node) {
                    return self.has_export_modifier(arena, &module.modifiers);
                }
            }
            // 2. Handle VariableDeclaration (walk up to VariableStatement)
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                // Walk up: VariableDeclaration -> VariableDeclarationList -> VariableStatement
                if let Some(ext) = arena.get_extended(idx) {
                    let list_idx = ext.parent;
                    if let Some(list_ext) = arena.get_extended(list_idx) {
                        let stmt_idx = list_ext.parent;
                        if let Some(stmt_node) = arena.get(stmt_idx) {
                            if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                                if let Some(var_stmt) = arena.get_variable(stmt_node) {
                                    return self.has_export_modifier(arena, &var_stmt.modifiers);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        false
    }

    /// Declare a symbol in the current scope, merging when allowed.
    fn declare_symbol(
        &mut self,
        name: &str,
        flags: u32,
        declaration: NodeIndex,
        is_exported: bool,
    ) -> SymbolId {
        if let Some(existing_id) = self.current_scope.get(name) {
            let existing_flags = self.symbols.get(existing_id).map(|s| s.flags).unwrap_or(0);
            let can_merge = Self::can_merge_flags(existing_flags, flags);

            let combined_flags = if can_merge {
                existing_flags | flags
            } else {
                existing_flags
            };

            // Record merge event for debugging
            self.debugger.record_merge(name, existing_id, existing_flags, flags, combined_flags);

            if let Some(sym) = self.symbols.get_mut(existing_id) {
                if can_merge {
                    sym.flags |= flags;
                    if sym.value_declaration.is_none() && (flags & symbol_flags::VALUE) != 0 {
                        sym.value_declaration = declaration;
                    }
                }

                if !sym.declarations.contains(&declaration) {
                    sym.declarations.push(declaration);
                }
                if is_exported {
                    sym.is_exported = true;
                }

                // Record declaration event (merge)
                self.debugger.record_declaration(name, existing_id, combined_flags, sym.declarations.len(), true);
            }

            self.node_symbols.insert(declaration.0, existing_id);
            self.declare_in_persistent_scope(name.to_string(), existing_id);
            return existing_id;
        }

        let sym_id = self.symbols.alloc(flags, name.to_string());
        if let Some(sym) = self.symbols.get_mut(sym_id) {
            sym.declarations.push(declaration);
            if sym.value_declaration.is_none() && (flags & symbol_flags::VALUE) != 0 {
                sym.value_declaration = declaration;
            }
            sym.is_exported = is_exported;
        }
        self.current_scope.set(name.to_string(), sym_id);
        self.node_symbols.insert(declaration.0, sym_id);
        self.declare_in_persistent_scope(name.to_string(), sym_id);

        // Record declaration event (new symbol)
        self.debugger.record_declaration(name, sym_id, flags, 1, false);

        sym_id
    }

    /// Check if two symbol flag sets can be merged.
    /// Made public for use in checker to detect duplicate identifiers (TS2300).
    pub fn can_merge_flags(existing_flags: u32, new_flags: u32) -> bool {
        if (existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }

        if (existing_flags & symbol_flags::CLASS != 0 && (new_flags & symbol_flags::INTERFACE) != 0)
            || (existing_flags & symbol_flags::INTERFACE != 0
                && (new_flags & symbol_flags::CLASS) != 0)
        {
            return true;
        }

        if (existing_flags & symbol_flags::MODULE) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
            return true;
        }

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

        if (existing_flags & symbol_flags::FUNCTION) != 0
            && (new_flags & symbol_flags::FUNCTION) != 0
        {
            return true;
        }

        // Allow INTERFACE to merge with VALUE symbols (e.g., `interface Object` + `declare var Object`)
        // This enables global types like Object, Array, Promise to be used as both types and constructors
        if (existing_flags & symbol_flags::INTERFACE) != 0 && (new_flags & symbol_flags::VALUE) != 0 {
            return true;
        }
        if (new_flags & symbol_flags::INTERFACE) != 0 && (existing_flags & symbol_flags::VALUE) != 0 {
            return true;
        }

        false
    }

    // Scope management

    fn enter_scope(&mut self, kind: ContainerKind, node: NodeIndex) {
        // Legacy scope chain management
        let parent = Some(self.current_scope_idx);
        self.scope_chain.push(ScopeContext::new(kind, node, parent));
        self.current_scope_idx = self.scope_chain.len() - 1;
        self.push_scope();

        // Persistent scope management (for stateless checking)
        self.enter_persistent_scope(kind, node);
    }

    fn exit_scope(&mut self, arena: &ThinNodeArena) {
        // Capture exports before popping if this is a module/namespace
        if let Some(ctx) = self.scope_chain.get(self.current_scope_idx) {
            match ctx.container_kind {
                ContainerKind::Module => {
                    // Find the symbol for this module/namespace
                    if let Some(sym_id) = self.node_symbols.get(&ctx.container_node.0) {
                        let export_all = self
                            .scope_chain
                            .get(self.current_scope_idx)
                            .and_then(|ctx| arena.get(ctx.container_node))
                            .and_then(|node| arena.get_module(node))
                            .map(|module| {
                                let is_external = arena
                                    .get(module.name)
                                    .map(|name_node| {
                                        name_node.kind == SyntaxKind::StringLiteral as u16
                                            || name_node.kind
                                                == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                                    })
                                    .unwrap_or(false);
                                self.has_declare_modifier(arena, &module.modifiers) || is_external
                            })
                            .unwrap_or(false);

                        // Filter exports: only include symbols with is_exported = true or EXPORT_VALUE flag
                        let mut exports = SymbolTable::new();
                        for (name, &child_id) in self.current_scope.iter() {
                            if let Some(child) = self.symbols.get(child_id) {
                                // Check explicit export flag OR if it's an EXPORT_VALUE (from export {})
                                if export_all
                                    || child.is_exported
                                    || (child.flags & symbol_flags::EXPORT_VALUE) != 0
                                {
                                    exports.set(name.clone(), child_id);
                                }
                            }
                        }

                        // Persist filtered exports
                        if let Some(symbol) = self.symbols.get_mut(*sym_id) {
                            if let Some(ref mut existing) = symbol.exports {
                                for (name, &child_id) in exports.iter() {
                                    existing.set(name.clone(), child_id);
                                }
                            } else {
                                symbol.exports = Some(Box::new(exports));
                            }
                        }
                    }
                }
                ContainerKind::Class => {
                    // Find the symbol for this class
                    if let Some(sym_id) = self.node_symbols.get(&ctx.container_node.0) {
                        // Persist the current scope as the class's members
                        if let Some(symbol) = self.symbols.get_mut(*sym_id) {
                            symbol.members = Some(Box::new(self.current_scope.clone()));
                        }
                    }
                }
                _ => {}
            }
        }

        // Copy current scope to persistent scope before popping
        self.sync_current_scope_to_persistent();

        self.pop_scope();
        if let Some(ctx) = self.scope_chain.get(self.current_scope_idx) {
            if let Some(parent) = ctx.parent_idx {
                self.current_scope_idx = parent;
            }
        }

        // Exit persistent scope
        self.exit_persistent_scope();
    }

    fn push_scope(&mut self) {
        let old_scope = std::mem::take(&mut self.current_scope);
        self.scope_stack.push(old_scope);
        self.current_scope = SymbolTable::new();
    }

    fn pop_scope(&mut self) {
        if let Some(parent_scope) = self.scope_stack.pop() {
            self.current_scope = parent_scope;
        }
    }

    // Declaration binding methods

    fn bind_variable_declaration(
        &mut self,
        arena: &ThinNodeArena,
        node: &ThinNode,
        idx: NodeIndex,
    ) {
        if let Some(decl) = arena.get_variable_declaration(node) {
            let mut decl_flags = node.flags as u32;
            if (decl_flags & (node_flags::LET | node_flags::CONST)) == 0 {
                if let Some(ext) = arena.get_extended(idx) {
                    if let Some(parent_node) = arena.get(ext.parent) {
                        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                            decl_flags |= parent_node.flags as u32;
                        }
                    }
                }
            }
            let is_block_scoped = (decl_flags & (node_flags::LET | node_flags::CONST)) != 0;
            if let Some(name) = self.get_identifier_name(arena, decl.name) {
                // Determine if block-scoped (let/const) or function-scoped (var)
                let flags = if is_block_scoped {
                    symbol_flags::BLOCK_SCOPED_VARIABLE
                } else {
                    symbol_flags::FUNCTION_SCOPED_VARIABLE
                };

                // Check if exported BEFORE allocating symbol
                let is_exported = self.is_node_exported(arena, idx);

                let sym_id = self.declare_symbol(name, flags, idx, is_exported);
                self.node_symbols.insert(decl.name.0, sym_id);
            } else {
                let flags = if is_block_scoped {
                    symbol_flags::BLOCK_SCOPED_VARIABLE
                } else {
                    symbol_flags::FUNCTION_SCOPED_VARIABLE
                };
                let is_exported = self.is_node_exported(arena, idx);

                let mut names = Vec::new();
                self.collect_binding_identifiers(arena, decl.name, &mut names);
                for ident_idx in names {
                    if let Some(name) = self.get_identifier_name(arena, ident_idx) {
                        self.declare_symbol(name, flags, ident_idx, is_exported);
                    }
                }
            }

            if !decl.initializer.is_none() {
                self.bind_node(arena, decl.initializer);
                let flow = self.create_flow_assignment(idx);
                self.current_flow = flow;
            }
        }
    }

    fn bind_function_declaration(
        &mut self,
        arena: &ThinNodeArena,
        node: &ThinNode,
        idx: NodeIndex,
    ) {
        if let Some(func) = arena.get_function(node) {
            self.bind_modifiers(arena, &func.modifiers);
            // Function declaration creates a symbol in the current scope
            if let Some(name) = self.get_identifier_name(arena, func.name) {
                let is_exported = self.has_export_modifier(arena, &func.modifiers);
                self.declare_symbol(name, symbol_flags::FUNCTION, idx, is_exported);
            }

            // Enter function scope and bind body
            self.enter_scope(ContainerKind::Function, idx);
            self.declare_arguments_symbol();

            self.with_fresh_flow(|binder| {
                // Bind parameters
                for &param_idx in &func.parameters.nodes {
                    binder.bind_parameter(arena, param_idx);
                }

                // Bind body
                binder.bind_node(arena, func.body);
            });

            self.exit_scope(arena);
        }
    }

    fn bind_parameter(&mut self, arena: &ThinNodeArena, idx: NodeIndex) {
        if let Some(node) = arena.get(idx) {
            if let Some(param) = arena.get_parameter(node) {
                self.bind_modifiers(arena, &param.modifiers);
                if let Some(name) = self.get_identifier_name(arena, param.name) {
                    let sym_id = self.declare_symbol(
                        name,
                        symbol_flags::FUNCTION_SCOPED_VARIABLE,
                        idx,
                        false,
                    );
                    self.node_symbols.insert(param.name.0, sym_id);
                } else {
                    let mut names = Vec::new();
                    self.collect_binding_identifiers(arena, param.name, &mut names);
                    for ident_idx in names {
                        if let Some(name) = self.get_identifier_name(arena, ident_idx) {
                            self.declare_symbol(
                                name,
                                symbol_flags::FUNCTION_SCOPED_VARIABLE,
                                ident_idx,
                                false,
                            );
                        }
                    }
                }

                if !param.initializer.is_none() {
                    self.bind_node(arena, param.initializer);
                }
            }
        }
    }

    /// Bind an arrow function expression - creates a scope and binds the body.
    fn bind_arrow_function(&mut self, arena: &ThinNodeArena, node: &ThinNode, idx: NodeIndex) {
        if let Some(func) = arena.get_function(node) {
            self.bind_modifiers(arena, &func.modifiers);
            // Enter function scope
            self.enter_scope(ContainerKind::Function, idx);

            // Capture enclosing flow for closures (preserves narrowing for const/let variables)
            self.with_fresh_flow_inner(
                |binder| {
                    // Bind parameters
                    for &param_idx in &func.parameters.nodes {
                        binder.bind_parameter(arena, param_idx);
                    }

                    // Bind body (could be a block or an expression)
                    binder.bind_node(arena, func.body);
                },
                true,
            );

            self.exit_scope(arena);
        }
    }

    /// Bind a function expression - creates a scope and binds the body.
    fn bind_function_expression(&mut self, arena: &ThinNodeArena, node: &ThinNode, idx: NodeIndex) {
        if let Some(func) = arena.get_function(node) {
            self.bind_modifiers(arena, &func.modifiers);
            // Enter function scope
            self.enter_scope(ContainerKind::Function, idx);
            self.declare_arguments_symbol();

            // Capture enclosing flow for closures (preserves narrowing for const/let variables)
            self.with_fresh_flow_inner(
                |binder| {
                    // Bind parameters
                    for &param_idx in &func.parameters.nodes {
                        binder.bind_parameter(arena, param_idx);
                    }

                    // Bind body
                    binder.bind_node(arena, func.body);
                },
                true,
            );

            self.exit_scope(arena);
        }
    }

    /// Bind a method declaration - creates a scope and binds the body.
    fn bind_method_declaration(&mut self, arena: &ThinNodeArena, node: &ThinNode, idx: NodeIndex) {
        if let Some(method) = arena.get_method_decl(node) {
            self.bind_modifiers(arena, &method.modifiers);
            // Enter function scope
            self.enter_scope(ContainerKind::Function, idx);
            self.declare_arguments_symbol();

            self.with_fresh_flow(|binder| {
                // Bind parameters
                for &param_idx in &method.parameters.nodes {
                    binder.bind_parameter(arena, param_idx);
                }

                // Bind body
                binder.bind_node(arena, method.body);
            });

            self.exit_scope(arena);
        }
    }

    fn bind_callable_body(
        &mut self,
        arena: &ThinNodeArena,
        parameters: &NodeList,
        body: NodeIndex,
        idx: NodeIndex,
    ) {
        self.enter_scope(ContainerKind::Function, idx);
        self.declare_arguments_symbol();

        self.with_fresh_flow(|binder| {
            for &param_idx in &parameters.nodes {
                binder.bind_parameter(arena, param_idx);
            }

            if !body.is_none() {
                binder.bind_node(arena, body);
            }
        });

        self.exit_scope(arena);
    }

    fn bind_modifiers(&mut self, arena: &ThinNodeArena, modifiers: &Option<NodeList>) {
        if let Some(list) = modifiers {
            for &modifier_idx in &list.nodes {
                self.bind_node(arena, modifier_idx);
            }
        }
    }

    fn declare_arguments_symbol(&mut self) {
        self.declare_symbol(
            "arguments",
            symbol_flags::FUNCTION_SCOPED_VARIABLE,
            NodeIndex::NONE,
            false,
        );
    }

    fn bind_class_declaration(&mut self, arena: &ThinNodeArena, node: &ThinNode, idx: NodeIndex) {
        if let Some(class) = arena.get_class(node) {
            self.bind_modifiers(arena, &class.modifiers);
            if let Some(name) = self.get_identifier_name(arena, class.name) {
                // Start with CLASS flag
                let mut flags = symbol_flags::CLASS;

                // Add ABSTRACT flag if class has 'abstract' modifier
                if self.has_abstract_modifier(arena, &class.modifiers) {
                    flags |= symbol_flags::ABSTRACT;
                }

                // Check if exported BEFORE allocating symbol
                let is_exported = self.has_export_modifier(arena, &class.modifiers);

                self.declare_symbol(name, flags, idx, is_exported);
            }

            // Enter class scope for members
            self.enter_scope(ContainerKind::Class, idx);

            for &member_idx in &class.members.nodes {
                self.bind_class_member(arena, member_idx);
            }

            self.exit_scope(arena);
        }
    }

    fn bind_class_expression(&mut self, arena: &ThinNodeArena, node: &ThinNode, idx: NodeIndex) {
        if let Some(class) = arena.get_class(node) {
            self.bind_modifiers(arena, &class.modifiers);
            self.enter_scope(ContainerKind::Class, idx);

            if let Some(name) = self.get_identifier_name(arena, class.name) {
                let mut flags = symbol_flags::CLASS;
                if self.has_abstract_modifier(arena, &class.modifiers) {
                    flags |= symbol_flags::ABSTRACT;
                }
                let sym_id = self.declare_symbol(name, flags, idx, false);
                self.node_symbols.insert(class.name.0, sym_id);
            }

            for &member_idx in &class.members.nodes {
                self.bind_class_member(arena, member_idx);
            }

            self.exit_scope(arena);
        }
    }

    fn bind_class_member(&mut self, arena: &ThinNodeArena, idx: NodeIndex) {
        if let Some(node) = arena.get(idx) {
            match node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = arena.get_method_decl(node) {
                        self.bind_modifiers(arena, &method.modifiers);
                        if let Some(name_node) = arena.get(method.name) {
                            if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                                self.bind_node(arena, method.name);
                            }
                        }
                        if let Some(name) = self.get_identifier_name(arena, method.name) {
                            let mut flags = symbol_flags::METHOD;
                            if self.has_abstract_modifier(arena, &method.modifiers) {
                                flags |= symbol_flags::ABSTRACT;
                            }
                            if self.has_static_modifier(arena, &method.modifiers) {
                                flags |= symbol_flags::STATIC;
                            }
                            let sym_id = self.declare_symbol(name, flags, idx, false);
                            self.node_symbols.insert(method.name.0, sym_id);
                        }
                        self.bind_callable_body(arena, &method.parameters, method.body, idx);
                    }
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = arena.get_property_decl(node) {
                        self.bind_modifiers(arena, &prop.modifiers);
                        if let Some(name_node) = arena.get(prop.name) {
                            if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                                self.bind_node(arena, prop.name);
                            }
                        }
                        if let Some(name) = self.get_identifier_name(arena, prop.name) {
                            let mut flags = symbol_flags::PROPERTY;
                            if self.has_abstract_modifier(arena, &prop.modifiers) {
                                flags |= symbol_flags::ABSTRACT;
                            }
                            if self.has_static_modifier(arena, &prop.modifiers) {
                                flags |= symbol_flags::STATIC;
                            }
                            let sym_id = self.declare_symbol(name, flags, idx, false);
                            self.node_symbols.insert(prop.name.0, sym_id);
                        }

                        if !prop.initializer.is_none() {
                            self.bind_node(arena, prop.initializer);
                        }
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = arena.get_accessor(node) {
                        self.bind_modifiers(arena, &accessor.modifiers);
                        if let Some(name_node) = arena.get(accessor.name) {
                            if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                                self.bind_node(arena, accessor.name);
                            }
                        }
                        if let Some(name) = self.get_identifier_name(arena, accessor.name) {
                            let mut flags = if node.kind == syntax_kind_ext::GET_ACCESSOR {
                                symbol_flags::GET_ACCESSOR
                            } else {
                                symbol_flags::SET_ACCESSOR
                            };
                            if self.has_abstract_modifier(arena, &accessor.modifiers) {
                                flags |= symbol_flags::ABSTRACT;
                            }
                            if self.has_static_modifier(arena, &accessor.modifiers) {
                                flags |= symbol_flags::STATIC;
                            }
                            let sym_id = self.declare_symbol(name, flags, idx, false);
                            self.node_symbols.insert(accessor.name.0, sym_id);
                        }
                        self.bind_callable_body(arena, &accessor.parameters, accessor.body, idx);
                    }
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    self.declare_symbol("constructor", symbol_flags::CONSTRUCTOR, idx, false);
                    if let Some(ctor) = arena.get_constructor(node) {
                        self.bind_modifiers(arena, &ctor.modifiers);
                        self.bind_callable_body(arena, &ctor.parameters, ctor.body, idx);
                    }
                }
                k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                    if let Some(block) = arena.get_block(node) {
                        self.enter_scope(ContainerKind::Block, idx);
                        for &stmt_idx in &block.statements.nodes {
                            self.bind_node(arena, stmt_idx);
                        }
                        self.exit_scope(arena);
                    }
                }
                _ => {}
            }
        }
    }

    fn bind_interface_declaration(
        &mut self,
        arena: &ThinNodeArena,
        node: &ThinNode,
        idx: NodeIndex,
    ) {
        if let Some(iface) = arena.get_interface(node) {
            if let Some(name) = self.get_identifier_name(arena, iface.name) {
                // Check if exported BEFORE allocating symbol
                let is_exported = self.has_export_modifier(arena, &iface.modifiers);

                // If we're inside a global augmentation block, track this as an augmentation
                // that should merge with lib.d.ts symbols at type resolution time
                if self.in_global_augmentation {
                    self.global_augmentations
                        .entry(name.to_string())
                        .or_default()
                        .push(idx);
                }

                self.declare_symbol(name, symbol_flags::INTERFACE, idx, is_exported);
            }
        }
    }

    fn bind_type_alias_declaration(
        &mut self,
        arena: &ThinNodeArena,
        node: &ThinNode,
        idx: NodeIndex,
    ) {
        if let Some(alias) = arena.get_type_alias(node) {
            if let Some(name) = self.get_identifier_name(arena, alias.name) {
                // Check if exported BEFORE allocating symbol
                let is_exported = self.has_export_modifier(arena, &alias.modifiers);

                self.declare_symbol(name, symbol_flags::TYPE_ALIAS, idx, is_exported);
            }
        }
    }

    fn bind_enum_declaration(&mut self, arena: &ThinNodeArena, node: &ThinNode, idx: NodeIndex) {
        if let Some(enum_decl) = arena.get_enum(node) {
            if let Some(name) = self.get_identifier_name(arena, enum_decl.name) {
                // Check if exported BEFORE allocating symbol
                let is_exported = self.has_export_modifier(arena, &enum_decl.modifiers);
                // Check if this is a const enum
                let is_const = self.has_const_modifier(arena, &enum_decl.modifiers);
                let enum_flags = if is_const {
                    symbol_flags::CONST_ENUM
                } else {
                    symbol_flags::REGULAR_ENUM
                };

                let enum_sym_id = self.declare_symbol(name, enum_flags, idx, is_exported);

                // Get existing exports (for namespace merging)
                let mut exports = SymbolTable::new();
                if let Some(enum_symbol) = self.symbols.get(enum_sym_id) {
                    if let Some(ref existing_exports) = enum_symbol.exports {
                        exports = (**existing_exports).clone();
                    }
                }

                // Bind enum members and add them to exports
                // This allows enum members to be accessed as Enum.MemberName
                // and enables enum + namespace merging
                self.enter_scope(ContainerKind::Block, idx);
                for &member_idx in &enum_decl.members.nodes {
                    if let Some(member_node) = arena.get(member_idx) {
                        if let Some(member) = arena.get_enum_member(member_node) {
                            if let Some(member_name) = self.get_identifier_name(arena, member.name)
                            {
                                let sym_id = self
                                    .symbols
                                    .alloc(symbol_flags::ENUM_MEMBER, member_name.to_string());
                                // Set value_declaration for enum members so the checker can find the parent enum
                                if let Some(sym) = self.symbols.get_mut(sym_id) {
                                    sym.value_declaration = member_idx;
                                    sym.declarations.push(member_idx);
                                }
                                self.current_scope.set(member_name.to_string(), sym_id);
                                self.node_symbols.insert(member_idx.0, sym_id);
                                // Add to exports for namespace merging
                                exports.set(member_name.to_string(), sym_id);
                            }
                        }
                    }
                }
                self.exit_scope(arena);

                // Update the enum's exports with members
                if let Some(enum_symbol) = self.symbols.get_mut(enum_sym_id) {
                    enum_symbol.exports = Some(Box::new(exports));
                }
            }
        }
    }

    fn bind_switch_statement(&mut self, arena: &ThinNodeArena, node: &ThinNode, idx: NodeIndex) {
        self.record_flow(idx);
        if let Some(switch_data) = arena.get_switch(node) {
            self.bind_expression(arena, switch_data.expression);

            let pre_switch_flow = self.current_flow;
            let end_label = self.create_branch_label();
            let mut fallthrough_flow = FlowNodeId::NONE;

            // Case block contains case clauses
            if let Some(case_block_node) = arena.get(switch_data.case_block) {
                if let Some(case_block) = arena.get_block(case_block_node) {
                    for &clause_idx in &case_block.statements.nodes {
                        if let Some(clause_node) = arena.get(clause_idx) {
                            if let Some(clause) = arena.get_case_clause(clause_node) {
                                self.switch_clause_to_switch.insert(clause_idx.0, idx);

                                self.current_flow = pre_switch_flow;
                                if !clause.expression.is_none() {
                                    self.bind_expression(arena, clause.expression);
                                }

                                let clause_flow = self.create_switch_clause_flow(
                                    pre_switch_flow,
                                    fallthrough_flow,
                                    clause_idx,
                                );
                                self.current_flow = clause_flow;

                                for &stmt_idx in &clause.statements.nodes {
                                    self.bind_node(arena, stmt_idx);
                                }

                                self.add_antecedent(end_label, self.current_flow);

                                if self.clause_allows_fallthrough(arena, clause) {
                                    fallthrough_flow = self.current_flow;
                                } else {
                                    fallthrough_flow = FlowNodeId::NONE;
                                }
                            }
                        }
                    }
                }
            }

            self.current_flow = end_label;
        }
    }

    fn clause_allows_fallthrough(
        &self,
        arena: &ThinNodeArena,
        clause: &crate::parser::thin_node::CaseClauseData,
    ) -> bool {
        let Some(&last_stmt_idx) = clause.statements.nodes.last() else {
            return true;
        };

        let Some(stmt_node) = arena.get(last_stmt_idx) else {
            return true;
        };

        let kind = stmt_node.kind;
        !(kind == syntax_kind_ext::BREAK_STATEMENT
            || kind == syntax_kind_ext::RETURN_STATEMENT
            || kind == syntax_kind_ext::THROW_STATEMENT
            || kind == syntax_kind_ext::CONTINUE_STATEMENT)
    }

    fn bind_try_statement(&mut self, arena: &ThinNodeArena, node: &ThinNode, idx: NodeIndex) {
        self.record_flow(idx);
        if let Some(try_data) = arena.get_try(node) {
            let pre_try_flow = self.current_flow;
            let end_label = self.create_branch_label();

            // Bind try block
            self.bind_node(arena, try_data.try_block);
            let post_try_flow = self.current_flow;

            // Bind catch clause
            if !try_data.catch_clause.is_none() {
                if let Some(catch_node) = arena.get(try_data.catch_clause) {
                    if let Some(catch) = arena.get_catch_clause(catch_node) {
                        self.enter_scope(ContainerKind::Block, idx);

                        // Catch can be entered from any point in try.
                        self.current_flow = pre_try_flow;

                        // Bind catch variable and mark it assigned.
                        if !catch.variable_declaration.is_none() {
                            self.bind_node(arena, catch.variable_declaration);
                            let flow = self.create_flow_assignment(catch.variable_declaration);
                            self.current_flow = flow;
                        }

                        // Bind catch block
                        self.bind_node(arena, catch.block);
                        self.add_antecedent(end_label, self.current_flow);

                        self.exit_scope(arena);
                    }
                }
            }

            // Add post-try flow to end label
            self.add_antecedent(end_label, post_try_flow);

            // Bind finally block
            if !try_data.finally_block.is_none() {
                self.current_flow = end_label;
                self.bind_node(arena, try_data.finally_block);
            } else {
                self.current_flow = end_label;
            }
        }
    }

    fn bind_import_declaration(&mut self, arena: &ThinNodeArena, node: &ThinNode, _idx: NodeIndex) {
        if let Some(import) = arena.get_import_decl(node) {
            // Get module specifier for cross-file module resolution
            let module_specifier = if !import.module_specifier.is_none() {
                arena.get(import.module_specifier)
                    .and_then(|spec_node| arena.get_literal(spec_node))
                    .map(|lit| lit.text.clone())
            } else {
                None
            };

            if let Some(clause_node) = arena.get(import.import_clause) {
                if let Some(clause) = arena.get_import_clause(clause_node) {
                    let clause_type_only = clause.is_type_only;
                    // Default import
                    if !clause.name.is_none() {
                        if let Some(name) = self.get_identifier_name(arena, clause.name) {
                            let sym_id = self.symbols.alloc(symbol_flags::ALIAS, name.to_string());
                            if let Some(sym) = self.symbols.get_mut(sym_id) {
                                sym.declarations.push(clause.name);
                                sym.is_type_only = clause_type_only;
                                // Track module for cross-file resolution
                                if let Some(ref specifier) = module_specifier {
                                    sym.import_module = Some(specifier.clone());
                                }
                            }
                            self.current_scope.set(name.to_string(), sym_id);
                            self.node_symbols.insert(clause.name.0, sym_id);
                        }
                    }

                    // Named imports
                    if !clause.named_bindings.is_none() {
                        if let Some(bindings_node) = arena.get(clause.named_bindings) {
                            if bindings_node.kind == SyntaxKind::Identifier as u16 {
                                if let Some(name) =
                                    self.get_identifier_name(arena, clause.named_bindings)
                                {
                                    let sym_id =
                                        self.symbols.alloc(symbol_flags::ALIAS, name.to_string());
                                    if let Some(sym) = self.symbols.get_mut(sym_id) {
                                        sym.declarations.push(clause.named_bindings);
                                        sym.is_type_only = clause_type_only;
                                        // Track module for cross-file resolution
                                        if let Some(ref specifier) = module_specifier {
                                            sym.import_module = Some(specifier.clone());
                                        }
                                    }
                                    self.current_scope.set(name.to_string(), sym_id);
                                    self.node_symbols.insert(clause.named_bindings.0, sym_id);
                                }
                            } else if let Some(named) = arena.get_named_imports(bindings_node) {
                                // Handle namespace import: import * as ns from 'module'
                                if !named.name.is_none() {
                                    if let Some(name) =
                                        self.get_identifier_name(arena, named.name)
                                    {
                                        let sym_id = self
                                            .symbols
                                            .alloc(symbol_flags::ALIAS, name.to_string());
                                        if let Some(sym) = self.symbols.get_mut(sym_id) {
                                            sym.declarations.push(named.name);
                                            sym.is_type_only = clause_type_only;
                                            // Track module for cross-file resolution
                                            if let Some(ref specifier) = module_specifier {
                                                sym.import_module = Some(specifier.clone());
                                            }
                                        }
                                        self.current_scope.set(name.to_string(), sym_id);
                                        self.node_symbols.insert(named.name.0, sym_id);
                                        self.node_symbols
                                            .insert(clause.named_bindings.0, sym_id);
                                    }
                                }
                                // Handle named imports: import { foo, bar } from 'module'
                                for &spec_idx in &named.elements.nodes {
                                    if let Some(spec_node) = arena.get(spec_idx) {
                                        if let Some(spec) = arena.get_specifier(spec_node) {
                                            let spec_type_only =
                                                clause_type_only || spec.is_type_only;
                                            let local_ident = if !spec.name.is_none() {
                                                spec.name
                                            } else {
                                                spec.property_name
                                            };
                                            let local_name =
                                                self.get_identifier_name(arena, local_ident);

                                            if let Some(name) = local_name {
                                                let sym_id = self
                                                    .symbols
                                                    .alloc(symbol_flags::ALIAS, name.to_string());

                                                // Get property name before mutable borrow to avoid borrow checker error
                                                let prop_name = if !spec.name.is_none() && !spec.property_name.is_none() {
                                                    self.get_identifier_name(arena, spec.property_name)
                                                } else {
                                                    None
                                                };

                                                if let Some(sym) = self.symbols.get_mut(sym_id) {
                                                    sym.declarations.push(local_ident);
                                                    sym.is_type_only = spec_type_only;
                                                    // Track module and original name for cross-file resolution
                                                    if let Some(ref specifier) = module_specifier {
                                                        sym.import_module = Some(specifier.clone());
                                                        // For renamed imports (import { foo as bar }), track original name
                                                        if let Some(prop_name) = prop_name {
                                                            sym.import_name = Some(prop_name.to_string());
                                                        }
                                                    }
                                                }
                                                self.current_scope.set(name.to_string(), sym_id);
                                                self.node_symbols.insert(spec_idx.0, sym_id);
                                                self.node_symbols.insert(local_ident.0, sym_id);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Bind import equals declaration: import x = ns.member or import x = require("...")
    fn bind_import_equals_declaration(
        &mut self,
        arena: &ThinNodeArena,
        node: &ThinNode,
        idx: NodeIndex,
    ) {
        if let Some(import) = arena.get_import_decl(node) {
            // import_clause holds the alias name (e.g., 'x' in 'import x = ...')
            if let Some(name) = self.get_identifier_name(arena, import.import_clause) {
                // Check if exported (for export import x = ns.member)
                let is_exported = self.has_export_modifier(arena, &import.modifiers);

                // Create symbol with ALIAS flag
                let sym_id = self.symbols.alloc(symbol_flags::ALIAS, name.to_string());

                if let Some(sym) = self.symbols.get_mut(sym_id) {
                    sym.declarations.push(idx);
                    sym.value_declaration = idx;
                    sym.is_exported = is_exported;
                }

                self.current_scope.set(name.to_string(), sym_id);
                self.node_symbols.insert(idx.0, sym_id);
                // Also add to persistent scope for checker lookup
                self.declare_in_persistent_scope(name.to_string(), sym_id);
            }
        }
    }

    /// Mark symbols associated with a declaration node as exported.
    /// This is required because the parser wraps exported declarations in ExportDeclaration
    /// nodes instead of attaching modifiers to the declaration itself.
    fn mark_exported_symbols(&mut self, arena: &ThinNodeArena, idx: NodeIndex) {
        // 1. Try direct symbol lookup (Function, Class, Enum, Module, Interface, TypeAlias)
        if let Some(sym_id) = self.node_symbols.get(&idx.0) {
            if let Some(sym) = self.symbols.get_mut(*sym_id) {
                sym.is_exported = true;
            }
            return;
        }

        // 2. Handle VariableStatement -> VariableDeclarationList -> VariableDeclaration
        // Variable statements don't have a symbol; their declarations do.
        if let Some(node) = arena.get(idx) {
            if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                if let Some(var) = arena.get_variable(node) {
                    for &list_idx in &var.declarations.nodes {
                        if let Some(list_node) = arena.get(list_idx) {
                            if let Some(list) = arena.get_variable(list_node) {
                                for &decl_idx in &list.declarations.nodes {
                                    if let Some(sym_id) = self.node_symbols.get(&decl_idx.0) {
                                        if let Some(sym) = self.symbols.get_mut(*sym_id) {
                                            sym.is_exported = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn bind_export_declaration(&mut self, arena: &ThinNodeArena, node: &ThinNode, _idx: NodeIndex) {
        if let Some(export) = arena.get_export_decl(node) {
            // Export clause can be:
            // - NamedExports: export { foo, bar }
            // - NamespaceExport: export * as ns from 'mod'
            // - Declaration: export function/class/const/etc
            // - or NONE for: export * from 'mod'

            // Check if the entire export declaration is type-only: export type { ... }
            let export_type_only = export.is_type_only;

            if !export.export_clause.is_none() {
                if let Some(clause_node) = arena.get(export.export_clause) {
                    // Check if it's named exports { foo, bar }
                    if let Some(named) = arena.get_named_imports(clause_node) {
                        // Check if this is a re-export: export { foo } from 'module'
                        if !export.module_specifier.is_none() {
                            // Get the module name from module_specifier
                            let module_name = if export.module_specifier.is_some() {
                                let idx = export.module_specifier;
                                arena.get(idx).and_then(|node| arena.get_literal(node))
                                    .map(|lit| lit.text.clone())
                            } else {
                                None
                            };

                            if let Some(source_module) = module_name {
                                let current_file = self.debugger.current_file.clone();

                                // Collect all the export mappings first (before mutable borrow)
                                let mut export_mappings: Vec<(String, Option<String>)> = Vec::new();
                                for &spec_idx in &named.elements.nodes {
                                    if let Some(spec_node) = arena.get(spec_idx) {
                                        if let Some(spec) = arena.get_specifier(spec_node) {
                                            // Get the original name (property_name) and exported name (name)
                                            let original_name = if spec.property_name.is_some() {
                                                self.get_identifier_name(arena, spec.property_name)
                                            } else {
                                                None
                                            };
                                            let exported_name = if spec.name.is_some() {
                                                self.get_identifier_name(arena, spec.name)
                                            } else {
                                                None
                                            };

                                            if let Some(exported) = exported_name.or(original_name) {
                                                export_mappings.push((
                                                    exported.to_string(),
                                                    original_name.map(|s| s.to_string()),
                                                ));
                                            }
                                        }
                                    }
                                }

                                // Now apply the mutable borrow to insert the mappings
                                let file_reexports = self.reexports.entry(current_file).or_default();
                                for (exported, original) in &export_mappings {
                                    file_reexports.insert(exported.clone(), (source_module.clone(), original.clone()));
                                }

                                // Also create alias symbols for re-exported names in file_locals
                                // This makes them accessible in the current module's scope
                                for &spec_idx in &named.elements.nodes {
                                    if let Some(spec_node) = arena.get(spec_idx) {
                                        if let Some(spec) = arena.get_specifier(spec_node) {
                                            // Get the local name (what this module exports as)
                                            let exported_name = if !spec.name.is_none() {
                                                self.get_identifier_name(arena, spec.name)
                                            } else if !spec.property_name.is_none() {
                                                self.get_identifier_name(arena, spec.property_name)
                                            } else {
                                                None
                                            };

                                            if let Some(name) = exported_name {
                                                let spec_type_only = export_type_only || spec.is_type_only;
                                                let sym_id = self.declare_symbol(
                                                    name,
                                                    symbol_flags::ALIAS,
                                                    spec_idx,
                                                    true, // re-exports are always exported
                                                );
                                                if let Some(sym) = self.symbols.get_mut(sym_id) {
                                                    sym.is_type_only = spec_type_only;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            // Regular export { foo, bar } without 'from' clause
                            // Bind each export specifier as an EXPORT_VALUE
                            for &spec_idx in &named.elements.nodes {
                                if let Some(spec_node) = arena.get(spec_idx) {
                                    if let Some(spec) = arena.get_specifier(spec_node) {
                                        // Determine if this specifier is type-only
                                        // (either from export type { ... } or export { type foo })
                                        let spec_type_only = export_type_only || spec.is_type_only;

                                        // For export { foo }, property_name is NONE, name is "foo"
                                        // For export { foo as bar }, property_name is "foo", name is "bar"
                                        let exported_name = if !spec.name.is_none() {
                                            self.get_identifier_name(arena, spec.name)
                                        } else {
                                            self.get_identifier_name(arena, spec.property_name)
                                        };

                                        if let Some(name) = exported_name {
                                            // Create export symbol (EXPORT_VALUE for value exports)
                                            // This marks the name as exported from this module
                                            let sym_id = self
                                                .symbols
                                                .alloc(symbol_flags::EXPORT_VALUE, name.to_string());
                                            // Set is_type_only and is_exported on the symbol
                                            if let Some(sym) = self.symbols.get_mut(sym_id) {
                                                sym.is_exported = true;
                                                sym.is_type_only = spec_type_only;
                                            }
                                            self.node_symbols.insert(spec_idx.0, sym_id);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Check if it's an exported declaration (function, class, variable, etc.)
                    else if self.is_declaration(clause_node.kind) {
                        // Recursively bind the declaration
                        // This handles: export function foo() {}, export class Bar {}, export const x = 1
                        self.bind_node(arena, export.export_clause);

                        // FIX: Explicitly mark the bound symbol(s) as exported
                        // because the inner declaration node lacks the 'export' modifier
                        self.mark_exported_symbols(arena, export.export_clause);
                    }
                    // Namespace export: export * as ns from 'mod'
                    else if let Some(name) = self.get_identifier_name(arena, export.export_clause)
                    {
                        let sym_id = self.symbols.alloc(symbol_flags::ALIAS, name.to_string());
                        // Set is_type_only and is_exported for namespace exports
                        if let Some(sym) = self.symbols.get_mut(sym_id) {
                            sym.is_exported = true;
                            sym.is_type_only = export_type_only;
                        }
                        self.current_scope.set(name.to_string(), sym_id);
                        self.node_symbols.insert(export.export_clause.0, sym_id);
                    } else if export.is_default_export {
                        // export default <expression> should still bind inner locals.
                        self.bind_node(arena, export.export_clause);
                    }
                }
            }

            // Handle `export * from 'module'` (wildcard re-exports)
            // This is when export_clause is None but module_specifier is not None
            if export.export_clause.is_none() && !export.module_specifier.is_none() {
                let module_name = if export.module_specifier.is_some() {
                    let idx = export.module_specifier;
                    arena.get(idx).and_then(|node| arena.get_literal(node))
                        .map(|lit| lit.text.clone())
                } else {
                    None
                };

                if let Some(source_module) = module_name {
                    let current_file = self.debugger.current_file.clone();
                    let file_reexports = self.reexports.entry(current_file).or_default();

                    // Use "*" to indicate wildcard re-export
                    file_reexports.insert("*".to_string(), (source_module, None));
                }
            }
        }
    }

    /// Check if a node kind is a declaration that should be bound
    fn is_declaration(&self, kind: u16) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::VARIABLE_STATEMENT
            || kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || kind == syntax_kind_ext::ENUM_DECLARATION
            || kind == syntax_kind_ext::MODULE_DECLARATION
    }

    fn bind_module_declaration(&mut self, arena: &ThinNodeArena, node: &ThinNode, idx: NodeIndex) {
        if let Some(module) = arena.get_module(node) {
            let is_global_augmentation = (node.flags as u32) & node_flags::GLOBAL_AUGMENTATION != 0
                || arena
                    .get(module.name)
                    .and_then(|name_node| {
                        if let Some(ident) = arena.get_identifier(name_node) {
                            return Some(ident.escaped_text == "global");
                        }
                        if name_node.kind == SyntaxKind::GlobalKeyword as u16 {
                            return Some(true);
                        }
                        None
                    })
                    .unwrap_or(false);

            if is_global_augmentation {
                if !module.body.is_none() {
                    self.node_scope_ids
                        .insert(module.body.0, self.current_scope_id);
                    // Set flag so interface declarations inside are tracked as augmentations
                    let was_in_global_augmentation = self.in_global_augmentation;
                    self.in_global_augmentation = true;
                    self.bind_node(arena, module.body);
                    self.in_global_augmentation = was_in_global_augmentation;
                }
                return;
            }

            if let Some(name_node) = arena.get(module.name) {
                if name_node.kind == SyntaxKind::StringLiteral as u16
                    || name_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                {
                    // Ambient module declaration with string literal name
                    // These should always be tracked, regardless of whether the file is an external module
                    if let Some(lit) = arena.get_literal(name_node) {
                        if !lit.text.is_empty() {
                            self.declared_modules.insert(lit.text.clone());
                        }
                    }
                }
            }

            let name = self
                .get_identifier_name(arena, module.name)
                .map(str::to_string)
                .or_else(|| {
                    arena
                        .get(module.name)
                        .and_then(|name_node| arena.get_literal(name_node))
                        .map(|lit| lit.text.clone())
                });
            let mut prior_exports: Option<SymbolTable> = None;
            let mut module_symbol_id = SymbolId::NONE;
            if let Some(name) = name {
                let mut is_exported = self.has_export_modifier(arena, &module.modifiers);
                if !is_exported {
                    if let Some(ext) = arena.get_extended(idx) {
                        let parent_idx = ext.parent;
                        if let Some(parent_node) = arena.get(parent_idx) {
                            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                                if let Some(parent_module) = arena.get_module(parent_node) {
                                    if parent_module.body == idx {
                                        is_exported = true;
                                    }
                                }
                            }
                        }
                    }
                }
                let flags = symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE;
                module_symbol_id = self.declare_symbol(&name, flags, idx, is_exported);
                prior_exports = self
                    .symbols
                    .get(module_symbol_id)
                    .and_then(|symbol| symbol.exports.as_ref())
                    .map(|exports| exports.as_ref().clone());
            }

            // Enter module scope
            self.enter_scope(ContainerKind::Module, idx);

            if let Some(exports) = prior_exports {
                for (name, &child_id) in exports.iter() {
                    self.current_scope.set(name.clone(), child_id);
                }
            }

            // Also register the MODULE_BLOCK body node with the same scope
            // so that identifiers inside the namespace can find their enclosing scope
            // when walking up through the parent chain (identifier -> ... -> MODULE_BLOCK -> MODULE_DECLARATION)
            if !module.body.is_none() {
                self.node_scope_ids
                    .insert(module.body.0, self.current_scope_id);
            }

            self.bind_node(arena, module.body);

            // Populate exports for the module symbol
            if !module_symbol_id.is_none() && !module.body.is_none() {
                self.populate_module_exports(arena, module.body, module_symbol_id);
            }

            self.exit_scope(arena);
        }
    }

    /// Populate the exports table of a module/namespace symbol based on exported declarations in its body.
    fn populate_module_exports(
        &mut self,
        arena: &ThinNodeArena,
        body_idx: NodeIndex,
        module_symbol_id: SymbolId,
    ) {
        let Some(node) = arena.get(body_idx) else {
            return;
        };

        // Get the module block statements
        let statements = if let Some(module_block) = arena.get_module_block(node) {
            if let Some(stmts) = &module_block.statements {
                &stmts.nodes
            } else {
                return;
            }
        } else {
            return;
        };

        for &stmt_idx in statements {
            if let Some(stmt_node) = arena.get(stmt_idx) {
                // Check for export modifier
                let is_exported = match stmt_node.kind {
                    syntax_kind_ext::VARIABLE_STATEMENT => arena
                        .get_variable(stmt_node)
                        .and_then(|v| v.modifiers.as_ref())
                        .is_some_and(|mods| self.has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::FUNCTION_DECLARATION => arena
                        .get_function(stmt_node)
                        .and_then(|f| f.modifiers.as_ref())
                        .is_some_and(|mods| self.has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::CLASS_DECLARATION => arena
                        .get_class(stmt_node)
                        .and_then(|c| c.modifiers.as_ref())
                        .is_some_and(|mods| self.has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::INTERFACE_DECLARATION => arena
                        .get_interface(stmt_node)
                        .and_then(|i| i.modifiers.as_ref())
                        .is_some_and(|mods| self.has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::TYPE_ALIAS_DECLARATION => arena
                        .get_type_alias(stmt_node)
                        .and_then(|t| t.modifiers.as_ref())
                        .is_some_and(|mods| self.has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::ENUM_DECLARATION => arena
                        .get_enum(stmt_node)
                        .and_then(|e| e.modifiers.as_ref())
                        .is_some_and(|mods| self.has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::MODULE_DECLARATION => arena
                        .get_module(stmt_node)
                        .and_then(|m| m.modifiers.as_ref())
                        .is_some_and(|mods| self.has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::EXPORT_DECLARATION => true, // export { x }
                    _ => false,
                };

                if is_exported {
                    // Collect the exported names first
                    let mut exported_names = Vec::new();

                    match stmt_node.kind {
                        syntax_kind_ext::VARIABLE_STATEMENT => {
                            if let Some(var_stmt) = arena.get_variable(stmt_node) {
                                for &decl_idx in &var_stmt.declarations.nodes {
                                    if let Some(decl_node) = arena.get(decl_idx) {
                                        if let Some(decl) =
                                            arena.get_variable_declaration(decl_node)
                                        {
                                            if let Some(name_node) = arena.get(decl.name) {
                                                if let Some(ident) = arena.get_identifier(name_node)
                                                {
                                                    exported_names
                                                        .push(ident.escaped_text.to_string());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        syntax_kind_ext::FUNCTION_DECLARATION => {
                            if let Some(func) = arena.get_function(stmt_node) {
                                if let Some(name) = self.get_identifier_name(arena, func.name) {
                                    exported_names.push(name.to_string());
                                }
                            }
                        }
                        syntax_kind_ext::CLASS_DECLARATION => {
                            if let Some(class) = arena.get_class(stmt_node) {
                                if let Some(name) = self.get_identifier_name(arena, class.name) {
                                    exported_names.push(name.to_string());
                                }
                            }
                        }
                        syntax_kind_ext::ENUM_DECLARATION => {
                            if let Some(enm) = arena.get_enum(stmt_node) {
                                if let Some(name) = self.get_identifier_name(arena, enm.name) {
                                    exported_names.push(name.to_string());
                                }
                            }
                        }
                        syntax_kind_ext::INTERFACE_DECLARATION => {
                            if let Some(iface) = arena.get_interface(stmt_node) {
                                if let Some(name) = self.get_identifier_name(arena, iface.name) {
                                    exported_names.push(name.to_string());
                                }
                            }
                        }
                        syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                            if let Some(alias) = arena.get_type_alias(stmt_node) {
                                if let Some(name) = self.get_identifier_name(arena, alias.name) {
                                    exported_names.push(name.to_string());
                                }
                            }
                        }
                        syntax_kind_ext::MODULE_DECLARATION => {
                            if let Some(module) = arena.get_module(stmt_node) {
                                let name = self
                                    .get_identifier_name(arena, module.name)
                                    .map(str::to_string)
                                    .or_else(|| {
                                        arena
                                            .get(module.name)
                                            .and_then(|name_node| arena.get_literal(name_node))
                                            .map(|lit| lit.text.clone())
                                    });
                                if let Some(name) = name {
                                    exported_names.push(name);
                                }
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

    /// Check if any modifier in a NodeList is the export keyword.
    fn has_export_modifier_any(&self, arena: &ThinNodeArena, modifiers: &NodeList) -> bool {
        for &mod_idx in &modifiers.nodes {
            if let Some(mod_node) = arena.get(mod_idx) {
                if mod_node.kind == SyntaxKind::ExportKeyword as u16 {
                    return true;
                }
            }
        }
        false
    }

    // Public accessors

    pub fn get_symbol(&self, id: SymbolId) -> Option<&Symbol> {
        // First try local symbols
        if let Some(sym) = self.symbols.get(id) {
            return Some(sym);
        }
        // Then try lib binders (if any have been merged)
        for lib_binder in &self.lib_binders {
            if let Some(sym) = lib_binder.symbols.get(id) {
                return Some(sym);
            }
        }
        None
    }

    /// Get a symbol, checking lib binders if not found locally.
    /// This is used by the checker to resolve symbols that come from lib.d.ts.
    pub fn get_symbol_with_libs<'a>(
        &'a self,
        id: SymbolId,
        lib_binders: &'a [Arc<ThinBinderState>],
) -> Option<&'a Symbol> {
    // First try local symbols
    if let Some(sym) = self.symbols.get(id) {
        return Some(sym);
    }

    // Then try lib binders
    for lib_binder in lib_binders {
        if let Some(sym) = lib_binder.symbols.get(id) {
            return Some(sym);
        }
    }

    None
    }

    pub fn get_node_symbol(&self, node: NodeIndex) -> Option<SymbolId> {
        self.node_symbols.get(&node.0).copied()
    }

    pub fn get_symbols(&self) -> &SymbolArena {
        &self.symbols
    }

    /// Get the flow node that was active at a given AST node.
    /// Used by the checker for control flow analysis.
    pub fn get_node_flow(&self, node: NodeIndex) -> Option<FlowNodeId> {
        self.node_flow.get(&node.0).copied()
    }

    /// Get the containing switch statement for a case/default clause.
    pub fn get_switch_for_clause(&self, clause: NodeIndex) -> Option<NodeIndex> {
        self.switch_clause_to_switch.get(&clause.0).copied()
    }

    /// Record the current flow node for an AST node.
    /// Called during binding to track flow position for identifiers and other expressions.
    fn record_flow(&mut self, node: NodeIndex) {
        if !self.current_flow.is_none() {
            self.node_flow.insert(node.0, self.current_flow);
        }
    }

    fn with_fresh_flow<F>(&mut self, bind_body: F)
    where
        F: FnOnce(&mut Self),
    {
        self.with_fresh_flow_inner(bind_body, false);
    }

    /// Create a fresh flow for a function body, optionally capturing the enclosing flow for closures.
    /// If capture_enclosing is true, the START node will point to the enclosing flow, allowing
    /// const/let variables to preserve narrowing from the outer scope.
    fn with_fresh_flow_inner<F>(&mut self, bind_body: F, capture_enclosing: bool)
    where
        F: FnOnce(&mut Self),
    {
        let prev_flow = self.current_flow;
        let start_flow = self.flow_nodes.alloc(flow_flags::START);

        // For closures (arrow functions and function expressions), capture the enclosing flow
        // so that const/let variables can preserve narrowing from the outer scope
        if capture_enclosing && !prev_flow.is_none() {
            if let Some(start_node) = self.flow_nodes.get_mut(start_flow) {
                start_node.antecedent.push(prev_flow);
            }
        }

        self.current_flow = start_flow;
        bind_body(self);
        self.current_flow = prev_flow;
    }

    // =========================================================================
    // Flow graph construction helpers
    // =========================================================================

    /// Create a branch label flow node for merging control flow paths.
    fn create_branch_label(&mut self) -> FlowNodeId {
        self.flow_nodes.alloc(flow_flags::BRANCH_LABEL)
    }

    /// Create a loop label flow node for back-edges.
    fn create_loop_label(&mut self) -> FlowNodeId {
        self.flow_nodes.alloc(flow_flags::LOOP_LABEL)
    }

    /// Create a flow condition node for tracking type narrowing.
    fn create_flow_condition(
        &mut self,
        flags: u32,
        antecedent: FlowNodeId,
        condition: NodeIndex,
    ) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flags);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.antecedent.push(antecedent);
            node.node = condition;
        }
        id
    }

    /// Create a flow node for a switch clause with optional fallthrough.
    fn create_switch_clause_flow(
        &mut self,
        pre_switch: FlowNodeId,
        fallthrough: FlowNodeId,
        clause: NodeIndex,
    ) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::SWITCH_CLAUSE);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = clause;
        }
        self.add_antecedent(id, pre_switch);
        self.add_antecedent(id, fallthrough);
        id
    }

    /// Create a flow node for an assignment.
    fn create_flow_assignment(&mut self, assignment: NodeIndex) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::ASSIGNMENT);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = assignment;
            if !self.current_flow.is_none() {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    /// Create a flow node for a call expression.
    fn create_flow_call(&mut self, call: NodeIndex) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::CALL);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = call;
            if !self.current_flow.is_none() {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    /// Create a flow node for array mutation (e.g. push/splice).
    fn create_flow_array_mutation(&mut self, call: NodeIndex) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::ARRAY_MUTATION);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = call;
            if !self.current_flow.is_none() {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    /// Add an antecedent to a flow node (for merging branches).
    fn add_antecedent(&mut self, label: FlowNodeId, antecedent: FlowNodeId) {
        if antecedent.is_none() || antecedent == self.unreachable_flow {
            return;
        }
        if let Some(node) = self.flow_nodes.get_mut(label) {
            if !node.antecedent.contains(&antecedent) {
                node.antecedent.push(antecedent);
            }
        }
    }

    // =========================================================================
    // Expression binding for flow analysis
    // =========================================================================

    fn is_assignment_operator(&self, operator: u16) -> bool {
        matches!(
            operator,
            k if k == SyntaxKind::EqualsToken as u16
                || k == SyntaxKind::PlusEqualsToken as u16
                || k == SyntaxKind::MinusEqualsToken as u16
                || k == SyntaxKind::AsteriskEqualsToken as u16
                || k == SyntaxKind::AsteriskAsteriskEqualsToken as u16
                || k == SyntaxKind::SlashEqualsToken as u16
                || k == SyntaxKind::PercentEqualsToken as u16
                || k == SyntaxKind::LessThanLessThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::AmpersandEqualsToken as u16
                || k == SyntaxKind::BarEqualsToken as u16
                || k == SyntaxKind::BarBarEqualsToken as u16
                || k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || k == SyntaxKind::QuestionQuestionEqualsToken as u16
                || k == SyntaxKind::CaretEqualsToken as u16
        )
    }

    fn is_array_mutation_call(&self, arena: &ThinNodeArena, call_idx: NodeIndex) -> bool {
        let Some(call_node) = arena.get(call_idx) else {
            return false;
        };
        let Some(call) = arena.get_call_expr(call_node) else {
            return false;
        };
        let Some(callee_node) = arena.get(call.expression) else {
            return false;
        };
        let Some(access) = arena.get_access_expr(callee_node) else {
            return false;
        };
        if access.question_dot_token {
            return false;
        }
        let Some(name_node) = arena.get(access.name_or_argument) else {
            return false;
        };
        let name = if let Some(ident) = arena.get_identifier(name_node) {
            ident.escaped_text.as_str()
        } else if let Some(literal) = arena.get_literal(name_node) {
            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                literal.text.as_str()
            } else {
                return false;
            }
        } else {
            return false;
        };

        matches!(
            name,
            "copyWithin"
                | "fill"
                | "pop"
                | "push"
                | "reverse"
                | "shift"
                | "sort"
                | "splice"
                | "unshift"
        )
    }

    // Avoid deep recursion on large left-associative binary expression chains.
    fn bind_binary_expression_iterative(&mut self, arena: &ThinNodeArena, root: NodeIndex) {
        enum WorkItem {
            Visit(NodeIndex),
            PostAssign(NodeIndex),
        }

        let mut stack = vec![WorkItem::Visit(root)];
        while let Some(item) = stack.pop() {
            match item {
                WorkItem::Visit(idx) => {
                    let node = match arena.get(idx) {
                        Some(n) => n,
                        None => continue,
                    };

                    if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                        if let Some(bin) = arena.get_binary_expr(node) {
                            if self.is_assignment_operator(bin.operator_token) {
                                stack.push(WorkItem::PostAssign(idx));
                                if !bin.right.is_none() {
                                    stack.push(WorkItem::Visit(bin.right));
                                }
                                if !bin.left.is_none() {
                                    stack.push(WorkItem::Visit(bin.left));
                                }
                                continue;
                            }
                            if !bin.right.is_none() {
                                stack.push(WorkItem::Visit(bin.right));
                            }
                            if !bin.left.is_none() {
                                stack.push(WorkItem::Visit(bin.left));
                            }
                        }
                        continue;
                    }

                    self.bind_node(arena, idx);
                }
                WorkItem::PostAssign(idx) => {
                    let flow = self.create_flow_assignment(idx);
                    self.current_flow = flow;
                }
            }
        }
    }

    fn bind_binary_expression_flow_iterative(&mut self, arena: &ThinNodeArena, root: NodeIndex) {
        enum WorkItem {
            Visit(NodeIndex),
            PostAssign(NodeIndex),
        }

        let mut stack = vec![WorkItem::Visit(root)];
        while let Some(item) = stack.pop() {
            match item {
                WorkItem::Visit(idx) => {
                    let node = match arena.get(idx) {
                        Some(n) => n,
                        None => continue,
                    };

                    if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                        self.record_flow(idx);
                        if let Some(bin) = arena.get_binary_expr(node) {
                            if self.is_assignment_operator(bin.operator_token) {
                                stack.push(WorkItem::PostAssign(idx));
                                if !bin.right.is_none() {
                                    stack.push(WorkItem::Visit(bin.right));
                                }
                                if !bin.left.is_none() {
                                    stack.push(WorkItem::Visit(bin.left));
                                }
                                continue;
                            }
                            if !bin.right.is_none() {
                                stack.push(WorkItem::Visit(bin.right));
                            }
                            if !bin.left.is_none() {
                                stack.push(WorkItem::Visit(bin.left));
                            }
                        }
                        continue;
                    }

                    self.bind_expression(arena, idx);
                }
                WorkItem::PostAssign(idx) => {
                    let flow = self.create_flow_assignment(idx);
                    self.current_flow = flow;
                }
            }
        }
    }

    /// Bind an expression and record flow positions for identifiers.
    /// This is used for condition expressions in if/while/for statements.
    fn bind_expression(&mut self, arena: &ThinNodeArena, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let node = match arena.get(idx) {
            Some(n) => n,
            None => return,
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(bin) = arena.get_binary_expr(node) {
                if self.is_assignment_operator(bin.operator_token) {
                    self.record_flow(idx);
                    self.bind_expression(arena, bin.left);
                    self.bind_expression(arena, bin.right);
                    let flow = self.create_flow_assignment(idx);
                    self.current_flow = flow;
                    return;
                }
            }
            self.bind_binary_expression_flow_iterative(arena, idx);
            return;
        }

        // Record flow position for this node
        self.record_flow(idx);

        match node.kind {
            // Identifiers - record flow position for type narrowing
            k if k == SyntaxKind::Identifier as u16 => {
                // Already recorded above
                return;
            }

            // Prefix unary (e.g., typeof x, !x)
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = arena.get_unary_expr(node) {
                    self.bind_expression(arena, unary.operand);
                    if unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16
                    {
                        let flow = self.create_flow_assignment(idx);
                        self.current_flow = flow;
                    }
                }
                return;
            }

            // Property access (e.g., x.foo) or element access (e.g., x[0])
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = arena.get_access_expr(node) {
                    self.bind_expression(arena, access.expression);
                    // For element access, also bind the argument
                    if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                        self.bind_expression(arena, access.name_or_argument);
                    }
                }
                return;
            }

            // Call expression (e.g., isString(x))
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = arena.get_call_expr(node) {
                    self.bind_expression(arena, call.expression);
                    if let Some(args) = &call.arguments {
                        for &arg in &args.nodes {
                            self.bind_expression(arena, arg);
                        }
                    }
                    if self.is_array_mutation_call(arena, idx) {
                        let flow = self.create_flow_array_mutation(idx);
                        self.current_flow = flow;
                    }
                }
                return;
            }

            // Parenthesized expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = arena.get_parenthesized(node) {
                    self.bind_expression(arena, paren.expression);
                }
                return;
            }

            // Type assertion (e.g., x as string)
            k if k == syntax_kind_ext::AS_EXPRESSION || k == syntax_kind_ext::TYPE_ASSERTION => {
                if let Some(as_expr) = arena.get_access_expr(node) {
                    self.bind_expression(arena, as_expr.expression);
                }
                return;
            }

            // Conditional expression (ternary)
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = arena.get_conditional_expr(node) {
                    self.bind_expression(arena, cond.condition);
                    self.bind_expression(arena, cond.when_true);
                    self.bind_expression(arena, cond.when_false);
                }
                return;
            }

            _ => {}
        }

        self.bind_node(arena, idx);
    }

    /// Run post-binding validation checks on the symbol table.
    /// Returns a list of validation errors found.
    pub fn validate_symbol_table(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        for (&node_idx, &sym_id) in self.node_symbols.iter() {
            if self.symbols.get(sym_id).is_none() {
                errors.push(ValidationError::BrokenSymbolLink {
                    node_index: node_idx,
                    symbol_id: sym_id.0,
                });
            }
        }

        for i in 0..self.symbols.len() {
            let sym_id = crate::binder::SymbolId(i as u32);
            if let Some(sym) = self.symbols.get(sym_id) {
                if sym.declarations.is_empty() {
                    errors.push(ValidationError::OrphanedSymbol {
                        symbol_id: i as u32,
                        name: sym.escaped_name.clone(),
                    });
                }
            }
        }

        for i in 0..self.symbols.len() {
            let sym_id = crate::binder::SymbolId(i as u32);
            if let Some(sym) = self.symbols.get(sym_id) {
                if !sym.value_declaration.is_none() {
                    let has_node_mapping = self.node_symbols.contains_key(&sym.value_declaration.0);
                    if !has_node_mapping {
                        errors.push(ValidationError::InvalidValueDeclaration {
                            symbol_id: i as u32,
                            name: sym.escaped_name.clone(),
                        });
                    }
                }
            }
        }

        errors
    }

    /// Check if the symbol table has any validation errors.
    pub fn is_symbol_table_valid(&self) -> bool {
        self.validate_symbol_table().is_empty()
    }

    // ========================================================================
    // Lib Symbol Validation (P0 Task - Improve Test Runner Lib Injection)
    // ========================================================================

    /// Expected global symbols that should be available from lib.d.ts.
    /// These are core ECMAScript globals that should always be present.
    const EXPECTED_GLOBAL_SYMBOLS: &'static [&'static str] = &[
        // Core types
        "Object", "Function", "Array", "String", "Number", "Boolean", "Symbol", "BigInt",
        // Error types
        "Error", "EvalError", "RangeError", "ReferenceError", "SyntaxError", "TypeError", "URIError",
        // Collections
        "Map", "Set", "WeakMap", "WeakSet",
        // Promises and async
        "Promise",
        // Object reflection
        "Reflect", "Proxy",
        // Global functions
        "eval", "isNaN", "isFinite", "parseFloat", "parseInt",
        // Global values
        "Infinity", "NaN", "undefined",
        // Console (if DOM lib is loaded)
        "console",
    ];

    /// Validate that expected global symbols are present after binding.
    ///
    /// This method should be called after `bind_source_file_with_libs` to ensure
    /// that lib symbols were properly loaded and merged into the binder.
    ///
    /// Returns a list of missing symbol names. Empty list means all expected symbols are present.
    ///
    /// # Example
    /// ```ignore
    /// binder.bind_source_file_with_libs(arena, root, &lib_files);
    /// let missing = binder.validate_global_symbols();
    /// if !missing.is_empty() {
    ///     eprintln!("WARNING: Missing global symbols: {:?}", missing);
    /// }
    /// ```
    pub fn validate_global_symbols(&self) -> Vec<String> {
        let mut missing = Vec::new();

        for &symbol_name in Self::EXPECTED_GLOBAL_SYMBOLS {
            // Check if the symbol is available via resolve_identifier
            // (which checks both file_locals and lib_binders)
            let is_available = self.file_locals.has(symbol_name) ||
                self.lib_binders.iter().any(|b| b.file_locals.has(symbol_name));

            if !is_available {
                missing.push(symbol_name.to_string());
            }
        }

        missing
    }

    /// Get a detailed report of lib symbol availability.
    ///
    /// Returns a human-readable string showing:
    /// - Which expected symbols are present
    /// - Which expected symbols are missing
    /// - Total symbol count from file_locals and lib_binders
    pub fn get_lib_symbol_report(&self) -> String {
        let mut report = String::new();
        report.push_str("=== Lib Symbol Availability Report ===\n\n");

        // Count total symbols
        let file_local_count = self.file_locals.len();
        let lib_binder_count: usize = self.lib_binders.iter()
            .map(|b| b.file_locals.len())
            .sum();

        report.push_str(&format!("File locals: {} symbols\n", file_local_count));
        report.push_str(&format!("Lib binders: {} symbols ({} binders)\n\n",
            lib_binder_count, self.lib_binders.len()));

        // Check each expected symbol
        let mut present = Vec::new();
        let mut missing = Vec::new();

        for &symbol_name in Self::EXPECTED_GLOBAL_SYMBOLS {
            let is_available = self.file_locals.has(symbol_name) ||
                self.lib_binders.iter().any(|b| b.file_locals.has(symbol_name));

            if is_available {
                present.push(symbol_name);
            } else {
                missing.push(symbol_name);
            }
        }

        report.push_str(&format!("Expected symbols present: {}/{}\n", present.len(), Self::EXPECTED_GLOBAL_SYMBOLS.len()));
        if !missing.is_empty() {
            report.push_str("\nMissing symbols:\n");
            for name in &missing {
                report.push_str(&format!("  - {}\n", name));
            }
        }

        // Show which lib binders contribute symbols
        if !self.lib_binders.is_empty() {
            report.push_str("\nLib binder contributions:\n");
            for (i, lib_binder) in self.lib_binders.iter().enumerate() {
                report.push_str(&format!("  Lib binder {}: {} symbols\n",
                    i, lib_binder.file_locals.len()));
            }
        }

        report
    }

    /// Log missing lib symbols with debug context.
    ///
    /// This should be called at test start to warn about missing lib symbols
    /// that might cause test failures.
    ///
    /// Returns true if any expected symbols are missing.
    pub fn log_missing_lib_symbols(&self) -> bool {
        let missing = self.validate_global_symbols();

        if !missing.is_empty() {
            eprintln!("[LIB_SYMBOL_WARNING] Missing {} expected global symbols: {:?}",
                missing.len(), missing);
            eprintln!("[LIB_SYMBOL_WARNING] This may cause test failures due to unresolved symbols.");
            eprintln!("[LIB_SYMBOL_WARNING] Ensure lib.d.ts is loaded via addLibFile() before binding.");
            true
        } else {
            if crate::module_resolution_debug::is_debug_enabled() {
                eprintln!("[LIB_SYMBOL_INFO] All {} expected global symbols are present.",
                    Self::EXPECTED_GLOBAL_SYMBOLS.len());
            }
            false
        }
    }

    /// Verify that lib symbols from multiple test files are properly merged.
    ///
    /// This method checks that symbols from multiple lib files are all accessible
    /// through the binder's symbol resolution chain.
    ///
    /// # Arguments
    /// * `lib_files` - The lib files that were supposed to be merged
    ///
    /// Returns a list of lib file names whose symbols are not fully accessible.
    pub fn verify_lib_symbol_merge(&self, lib_files: &[Arc<lib_loader::LibFile>]) -> Vec<String> {
        let mut inaccessible = Vec::new();

        for lib_file in lib_files {
            let file_name = lib_file.file_name.clone();

            // Check if symbols from this lib file are accessible
            let mut has_accessible_symbols = false;
            for (name, &_sym_id) in lib_file.binder.file_locals.iter() {
                // Try to resolve the symbol through our binder
                if self.file_locals.get(name).is_some() ||
                   self.lib_binders.iter().any(|b| b.file_locals.get(name).is_some()) {
                    has_accessible_symbols = true;
                    break;
                }
            }

            if !has_accessible_symbols && !lib_file.binder.file_locals.is_empty() {
                inaccessible.push(file_name);
            }
        }

        inaccessible
    }

    // ========================================================================
    // Symbol Resolution Statistics (P1 Task - Debug Logging)
    // ========================================================================

    /// Get a snapshot of current symbol resolution statistics.
    ///
    /// This method scans the binder state to provide statistics about
    /// symbol resolution capability, including:
    /// - Available symbols by source (scopes, file_locals, lib_binders)
    /// - Potential resolution paths
    pub fn get_resolution_stats(&self) -> ResolutionStats {
        // Count symbols in each resolution tier
        let scope_symbols: u64 = self.scopes.iter()
            .map(|s| s.table.len() as u64)
            .sum();

        let file_local_symbols = self.file_locals.len() as u64;

        let lib_binder_symbols: u64 = self.lib_binders.iter()
            .map(|b| b.file_locals.len() as u64)
            .sum();

        ResolutionStats {
            attempts: 0, // Would need runtime tracking
            scope_hits: scope_symbols,
            file_local_hits: file_local_symbols,
            lib_binder_hits: lib_binder_symbols,
            failures: 0, // Would need runtime tracking
        }
    }

    /// Get a human-readable summary of resolution statistics.
    pub fn get_resolution_summary(&self) -> String {
        let stats = self.get_resolution_stats();
        format!(
            "Symbol Resolution Summary:\n\
             - Scope symbols: {}\n\
             - File local symbols: {}\n\
             - Lib binder symbols: {} (from {} binders)\n\
             - Total accessible symbols: {}\n\
             - Expected global symbols: {}",
            stats.scope_hits,
            stats.file_local_hits,
            stats.lib_binder_hits,
            self.lib_binders.len(),
            stats.scope_hits + stats.file_local_hits + stats.lib_binder_hits,
            Self::EXPECTED_GLOBAL_SYMBOLS.len()
        )
    }
}

impl Default for ThinBinderState {
    fn default() -> Self {
        Self::new()
    }
}
