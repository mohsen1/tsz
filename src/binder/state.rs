//! Binder - Binder implementation using NodeArena.
//!
//! This is a clean implementation of the binder that works directly with
//! Node and NodeArena, avoiding the old Node enum pattern matching.

// Allow dead code for binder infrastructure methods that will be used in future phases

#![allow(clippy::print_stderr)]

use crate::binder::{
    ContainerKind, FlowNodeArena, FlowNodeId, Scope, ScopeContext, ScopeId, SymbolArena, SymbolId,
    SymbolTable, flow_flags, symbol_flags,
};
use crate::common::ScriptTarget;
use crate::lib_loader;
use crate::module_resolution_debug::ModuleResolutionDebugger;
use crate::parser::node::{Node, NodeArena};
use crate::parser::node_flags;
use crate::parser::{NodeIndex, NodeList, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tracing::{Level, debug, span};

const MAX_SCOPE_WALK_ITERATIONS: usize = 10_000;

/// Configuration options for the binder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinderOptions {
    /// ECMAScript target version.
    /// This affects language-specific behaviors like block-scoped function hoisting.
    pub target: ScriptTarget,
}

impl Default for BinderOptions {
    fn default() -> Self {
        BinderOptions {
            target: ScriptTarget::default(),
        }
    }
}

/// Lib file context for global type resolution.
/// This mirrors the definition in checker::context to avoid circular dependencies.
#[derive(Clone)]
pub struct LibContext {
    /// The AST arena for this lib file.
    pub arena: Arc<NodeArena>,
    /// The binder state with symbols from this lib file.
    pub binder: Arc<BinderState>,
}

/// Represents a module augmentation with arena context.
///
/// This structure ensures that NodeIndex values remain valid across files by
/// storing the source arena along with the augmentation declaration.
///
/// # Arena Context
///
/// NodeIndex is only valid within its specific NodeArena. When augmentations from
/// multiple files are merged, we need to preserve which arena each NodeIndex belongs to.
///
/// # Example
///
/// ```ignore
/// // File A: observable.d.ts
/// declare module "observable" {
///     interface Observable<T> {
///         filter(pred: (e:T) => boolean): Observable<T>;
///     }
/// }
///
/// // File B: map.ts
/// declare module "observable" {
///     interface Observable<T> {
///         map<U>(proj: (e:T) => U): Observable<U>;
///     }
/// }
/// ```
///
/// The augmentation for "Observable" should include both `filter` from File A's arena
/// and `map` from File B's arena.
#[derive(Clone, Debug)]
pub struct ModuleAugmentation {
    /// Name of the augmented interface/type member (e.g., "map", "filter")
    pub name: String,
    /// Declaration node for this augmentation
    pub node: NodeIndex,
    /// The arena containing this declaration (None during binding, populated during merge)
    pub arena: Option<Arc<NodeArena>>,
}

impl ModuleAugmentation {
    /// Create a new module augmentation without arena context (during binding).
    pub fn new(name: String, node: NodeIndex) -> Self {
        Self {
            name,
            node,
            arena: None,
        }
    }

    /// Create a new module augmentation with arena context (during merge).
    pub fn with_arena(name: String, node: NodeIndex, arena: Arc<NodeArena>) -> Self {
        Self {
            name,
            node,
            arena: Some(arena),
        }
    }
}

/// Binder state using NodeArena.
pub struct BinderState {
    /// Binder options (ES target, etc.)
    pub options: BinderOptions,
    /// Arena for symbol storage
    pub symbols: SymbolArena,
    /// Current symbol table (local scope)
    pub current_scope: SymbolTable,
    /// Stack of parent scopes
    pub(crate) scope_stack: Vec<SymbolTable>,
    /// File-level locals (for module resolution)
    pub file_locals: SymbolTable,
    /// Ambient module declarations by specifier (e.g. "pkg", "./types")
    pub declared_modules: FxHashSet<String>,
    /// Whether the current source file is an external module (has top-level import/export).
    pub(crate) is_external_module: bool,
    /// Flow nodes for control flow analysis
    pub flow_nodes: FlowNodeArena,
    /// Current flow node
    pub(crate) current_flow: FlowNodeId,
    /// Unreachable flow node
    pub(crate) unreachable_flow: FlowNodeId,
    /// Scope chain - stack of scope contexts (legacy, for hoisting)
    pub(crate) scope_chain: Vec<ScopeContext>,
    /// Current scope index in scope_chain
    pub(crate) current_scope_idx: usize,
    /// Node-to-symbol mapping
    pub node_symbols: FxHashMap<u32, SymbolId>,
    /// Symbol-to-arena mapping for cross-file declaration lookup (legacy, stores last arena)
    pub symbol_arenas: FxHashMap<SymbolId, Arc<NodeArena>>,
    /// Declaration-to-arena mapping for precise cross-file declaration lookup
    /// Key: (SymbolId, NodeIndex of declaration) -> Arena containing that declaration
    /// This is needed when a symbol (like Array) is declared across multiple lib files
    pub declaration_arenas: FxHashMap<(SymbolId, NodeIndex), Arc<NodeArena>>,
    /// Node-to-flow mapping: tracks which flow node was active at each AST node
    /// Used by the checker for control flow analysis (type narrowing)
    pub node_flow: FxHashMap<u32, FlowNodeId>,
    /// Flow node after each top-level statement (for incremental binding).
    pub(crate) top_level_flow: FxHashMap<u32, FlowNodeId>,
    /// Map case/default clause nodes to their containing switch statement.
    pub(crate) switch_clause_to_switch: FxHashMap<u32, NodeIndex>,
    /// Hoisted var declarations
    pub(crate) hoisted_vars: Vec<(String, NodeIndex)>,
    /// Hoisted function declarations
    pub(crate) hoisted_functions: Vec<NodeIndex>,

    // ===== Persistent Scope System (for stateless checking) =====
    /// Persistent scopes - enables querying scope information without traversal order
    pub scopes: Vec<Scope>,
    /// Map from AST node (that creates a scope) to its ScopeId
    pub node_scope_ids: FxHashMap<u32, ScopeId>,
    /// Current active ScopeId during binding
    pub(crate) current_scope_id: ScopeId,

    // ===== Module Resolution Debugging =====
    /// Debugger for tracking symbol table operations and scope lookups
    pub debugger: ModuleResolutionDebugger,

    // ===== Global Augmentations =====
    /// Tracks interface/type declarations inside `declare global` blocks that should
    /// merge with lib.d.ts symbols. Maps interface name to declaration NodeIndex values.
    pub global_augmentations: FxHashMap<String, Vec<NodeIndex>>,

    /// Flag indicating we're currently binding inside a `declare global` block
    pub(crate) in_global_augmentation: bool,

    // ===== Module Augmentations (Rule #44) =====
    /// Tracks interface/type declarations inside `declare module 'x'` blocks that should
    /// merge with the target module's symbols. Maps module specifier to augmentations.
    pub module_augmentations: FxHashMap<String, Vec<ModuleAugmentation>>,

    /// Flag indicating we're currently binding inside a module augmentation block
    pub(crate) in_module_augmentation: bool,

    /// The module specifier being augmented (set when in_module_augmentation is true)
    pub(crate) current_augmented_module: Option<String>,

    /// Lib binders for automatic lib symbol resolution.
    /// When get_symbol() doesn't find a symbol locally, it checks these lib binders.
    pub lib_binders: Vec<Arc<BinderState>>,

    /// Symbol IDs that originated from lib files.
    /// Used by get_symbol() to check lib_binders first for these IDs,
    /// avoiding collision with local symbols at the same index.
    pub lib_symbol_ids: FxHashSet<SymbolId>,

    /// Module exports: maps file names to their exported symbols for cross-file module resolution
    /// This enables resolving imports like `import { X } from './file'` where './file' is another file
    pub module_exports: FxHashMap<String, SymbolTable>,

    /// Re-exports: tracks `export { x } from 'module'` declarations
    /// Maps (current_file, exported_name) -> (source_module, original_name)
    /// Example: ("./a.ts", "foo", "./b.ts") means a.ts re-exports "foo" from b.ts
    pub reexports: FxHashMap<String, FxHashMap<String, (String, Option<String>)>>,

    /// Wildcard re-exports: tracks `export * from 'module'` declarations
    /// Maps current_file -> Vec of source_modules
    /// A file can have multiple wildcard re-exports (e.g., `export * from 'a'; export * from 'b'`)
    pub wildcard_reexports: FxHashMap<String, Vec<String>>,

    /// Cache for resolved exports to avoid repeated lookups through re-export chains.
    /// Key: (module_specifier, export_name) -> resolved SymbolId (or None if not found)
    /// This cache dramatically speeds up barrel file imports where the same export
    /// is looked up multiple times across different files.
    /// Uses RwLock for thread-safety in parallel compilation.
    resolved_export_cache: std::sync::RwLock<FxHashMap<(String, String), Option<SymbolId>>>,
    /// Cache for identifier resolution by AST node.
    /// Key: (arena_pointer, node_index) -> resolved SymbolId (or None if not found).
    /// This avoids repeated scope walks for hot checker paths that ask for the same
    /// identifier symbol many times (e.g. large switch/flow analysis files).
    resolved_identifier_cache: std::sync::RwLock<FxHashMap<(usize, u32), Option<SymbolId>>>,

    /// Shorthand ambient modules: modules declared with just `declare module "xxx"` (no body)
    /// Imports from these modules should resolve to `any` type
    pub shorthand_ambient_modules: FxHashSet<String>,

    /// Flag indicating lib symbols have been merged into this binder's symbol arena.
    /// When true, get_symbol() should prefer local symbols over lib_binders lookups,
    /// since all lib symbols now have unique IDs in the local arena.
    pub(crate) lib_symbols_merged: bool,
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

impl BinderState {
    pub fn new() -> Self {
        Self::with_options(BinderOptions::default())
    }

    pub fn with_options(options: BinderOptions) -> Self {
        let mut flow_nodes = FlowNodeArena::new();
        let unreachable_flow = flow_nodes.alloc(flow_flags::UNREACHABLE);

        BinderState {
            options,
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
            declaration_arenas: FxHashMap::default(),
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
            module_augmentations: FxHashMap::default(),
            in_module_augmentation: false,
            current_augmented_module: None,
            lib_binders: Vec::new(),
            lib_symbol_ids: FxHashSet::default(),
            module_exports: FxHashMap::default(),
            reexports: FxHashMap::default(),
            wildcard_reexports: FxHashMap::default(),
            resolved_export_cache: std::sync::RwLock::new(FxHashMap::default()),
            resolved_identifier_cache: std::sync::RwLock::new(FxHashMap::default()),
            shorthand_ambient_modules: FxHashSet::default(),
            lib_symbols_merged: false,
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
        self.declaration_arenas.clear();
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
        self.module_augmentations.clear();
        self.in_module_augmentation = false;
        self.current_augmented_module = None;
        self.lib_binders.clear();
        self.lib_symbol_ids.clear();
        self.module_exports.clear();
        self.reexports.clear();
        self.wildcard_reexports.clear();
        self.resolved_export_cache.write().unwrap().clear();
        self.resolved_identifier_cache.write().unwrap().clear();
        self.shorthand_ambient_modules.clear();
        self.lib_symbols_merged = false;
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

    /// Get the arena for a specific declaration of a symbol.
    ///
    /// For symbols that are declared across multiple lib files (e.g., `Array` which is
    /// declared in es5.d.ts, es2015.core.d.ts, etc.), each declaration may be in a
    /// different arena. This method returns the correct arena for a specific declaration.
    ///
    /// Falls back to `symbol_arenas` (which stores the last arena for the symbol) if
    /// no specific declaration arena is found.
    ///
    /// Returns `None` if no arena is found for this symbol/declaration.
    pub fn get_arena_for_declaration(
        &self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
    ) -> Option<&Arc<NodeArena>> {
        // First try the precise declaration-to-arena mapping
        if let Some(arena) = self.declaration_arenas.get(&(sym_id, decl_idx)) {
            return Some(arena);
        }
        // Fall back to symbol-level arena (for backwards compatibility and non-merged symbols)
        self.symbol_arenas.get(&sym_id)
    }

    /// Create a BinderState from pre-parsed lib data.
    ///
    /// This is used for loading pre-parsed lib files where we only have
    /// symbols and file_locals (no node_symbols or other binding state).
    pub fn from_preparsed(symbols: SymbolArena, file_locals: SymbolTable) -> Self {
        Self::from_bound_state(symbols, file_locals, FxHashMap::default())
    }

    /// Create a BinderState from existing bound state.
    ///
    /// This is used for type checking after parallel binding and symbol merging.
    /// The symbols and node_symbols come from the merged program state.
    pub fn from_bound_state(
        symbols: SymbolArena,
        file_locals: SymbolTable,
        node_symbols: FxHashMap<u32, SymbolId>,
    ) -> Self {
        Self::from_bound_state_with_options(
            BinderOptions::default(),
            symbols,
            file_locals,
            node_symbols,
        )
    }

    /// Create a BinderState from existing bound state with options.
    pub fn from_bound_state_with_options(
        options: BinderOptions,
        symbols: SymbolArena,
        file_locals: SymbolTable,
        node_symbols: FxHashMap<u32, SymbolId>,
    ) -> Self {
        let mut flow_nodes = FlowNodeArena::new();
        let unreachable_flow = flow_nodes.alloc(flow_flags::UNREACHABLE);

        BinderState {
            options,
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
            declaration_arenas: FxHashMap::default(),
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
            module_augmentations: FxHashMap::default(),
            in_module_augmentation: false,
            current_augmented_module: None,
            lib_binders: Vec::new(),
            lib_symbol_ids: FxHashSet::default(),
            module_exports: FxHashMap::default(),
            reexports: FxHashMap::default(),
            wildcard_reexports: FxHashMap::default(),
            resolved_export_cache: std::sync::RwLock::new(FxHashMap::default()),
            resolved_identifier_cache: std::sync::RwLock::new(FxHashMap::default()),
            shorthand_ambient_modules: FxHashSet::default(),
            lib_symbols_merged: false,
        }
    }

    /// Create a BinderState from existing bound state, preserving scopes.
    pub fn from_bound_state_with_scopes(
        symbols: SymbolArena,
        file_locals: SymbolTable,
        node_symbols: FxHashMap<u32, SymbolId>,
        scopes: Vec<Scope>,
        node_scope_ids: FxHashMap<u32, ScopeId>,
    ) -> Self {
        Self::from_bound_state_with_scopes_and_augmentations(
            BinderOptions::default(),
            symbols,
            file_locals,
            node_symbols,
            scopes,
            node_scope_ids,
            FxHashMap::default(), // global_augmentations
            FxHashMap::default(), // module_augmentations
            FxHashMap::default(), // module_exports
            FxHashMap::default(), // reexports
            FxHashMap::default(), // wildcard_reexports
            FxHashMap::default(), // symbol_arenas
            FxHashMap::default(), // declaration_arenas
            FxHashSet::default(), // shorthand_ambient_modules
            FlowNodeArena::new(),
            FxHashMap::default(), // node_flow
            FxHashMap::default(), // switch_clause_to_switch
        )
    }

    /// Create a BinderState from existing bound state, preserving scopes and global augmentations.
    ///
    /// This is used for type checking after parallel binding and symbol merging.
    /// Global augmentations are interface/type declarations inside `declare global` blocks
    /// that should merge with lib.d.ts symbols during type resolution.
    /// Module augmentations are interface/type declarations inside `declare module 'x'` blocks
    /// that should merge with the target module's symbols.
    pub fn from_bound_state_with_scopes_and_augmentations(
        options: BinderOptions,
        symbols: SymbolArena,
        file_locals: SymbolTable,
        node_symbols: FxHashMap<u32, SymbolId>,
        scopes: Vec<Scope>,
        node_scope_ids: FxHashMap<u32, ScopeId>,
        global_augmentations: FxHashMap<String, Vec<crate::parser::NodeIndex>>,
        module_augmentations: FxHashMap<String, Vec<ModuleAugmentation>>,
        module_exports: FxHashMap<String, SymbolTable>,
        reexports: FxHashMap<String, FxHashMap<String, (String, Option<String>)>>,
        wildcard_reexports: FxHashMap<String, Vec<String>>,
        symbol_arenas: FxHashMap<SymbolId, Arc<NodeArena>>,
        declaration_arenas: FxHashMap<(SymbolId, NodeIndex), Arc<NodeArena>>,
        shorthand_ambient_modules: FxHashSet<String>,
        flow_nodes: FlowNodeArena,
        node_flow: FxHashMap<u32, FlowNodeId>,
        switch_clause_to_switch: FxHashMap<u32, NodeIndex>,
    ) -> Self {
        // Find the unreachable flow node in the existing flow_nodes, or create a new one
        let unreachable_flow = flow_nodes.find_unreachable().unwrap_or_else(|| {
            // This shouldn't happen in practice since the binder always creates an unreachable flow
            FlowNodeId::NONE
        });

        BinderState {
            options,
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
            symbol_arenas,
            declaration_arenas,
            node_flow,
            top_level_flow: FxHashMap::default(),
            switch_clause_to_switch,
            hoisted_vars: Vec::new(),
            hoisted_functions: Vec::new(),
            scopes,
            node_scope_ids,
            current_scope_id: ScopeId::NONE,
            debugger: ModuleResolutionDebugger::new(),
            global_augmentations,
            in_global_augmentation: false,
            module_augmentations,
            in_module_augmentation: false,
            current_augmented_module: None,
            lib_binders: Vec::new(),
            lib_symbol_ids: FxHashSet::default(),
            module_exports,
            reexports,
            wildcard_reexports,
            resolved_export_cache: std::sync::RwLock::new(FxHashMap::default()),
            resolved_identifier_cache: std::sync::RwLock::new(FxHashMap::default()),
            shorthand_ambient_modules,
            lib_symbols_merged: false,
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
    pub fn resolve_identifier(&self, arena: &NodeArena, node_idx: NodeIndex) -> Option<SymbolId> {
        // Fast path: identifier resolution is pure for a fixed binder + arena.
        // Cache both hits and misses to avoid repeated scope walks in checker hot paths.
        let cache_key = (arena as *const NodeArena as usize, node_idx.0);
        if let Some(&cached) = self
            .resolved_identifier_cache
            .read()
            .unwrap()
            .get(&cache_key)
        {
            return cached;
        }

        let _span = span!(Level::DEBUG, "resolve_identifier", node_idx = node_idx.0).entered();

        let result = 'resolve: {
            let Some(node) = arena.get(node_idx) else {
                break 'resolve None;
            };

            // Get the identifier text
            let name = if let Some(ident) = arena.get_identifier(node) {
                &ident.escaped_text
            } else {
                break 'resolve None;
            };

            debug!("[RESOLVE] Looking up identifier '{}'", name);

            if let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) {
                // Walk up the scope chain
                let mut scope_depth = 0;
                while !scope_id.is_none() {
                    if let Some(scope) = self.scopes.get(scope_id.0 as usize) {
                        if let Some(sym_id) = scope.table.get(name) {
                            debug!(
                                "[RESOLVE] '{}' FOUND in scope at depth {} (id={})",
                                name, scope_depth, sym_id.0
                            );
                            // Resolve import if this symbol is imported from another module
                            if let Some(resolved) = self.resolve_import_if_needed(sym_id) {
                                break 'resolve Some(resolved);
                            }
                            break 'resolve Some(sym_id);
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
                debug!(
                    "[RESOLVE] '{}' FOUND via parameter fallback (id={})",
                    name, sym_id.0
                );
                // Resolve import if this symbol is imported from another module
                if let Some(resolved) = self.resolve_import_if_needed(sym_id) {
                    break 'resolve Some(resolved);
                }
                break 'resolve Some(sym_id);
            }

            // Finally check file locals / globals
            if let Some(sym_id) = self.file_locals.get(name) {
                debug!(
                    "[RESOLVE] '{}' FOUND in file_locals (id={})",
                    name, sym_id.0
                );
                // Resolve import if this symbol is imported from another module
                if let Some(resolved) = self.resolve_import_if_needed(sym_id) {
                    break 'resolve Some(resolved);
                }
                break 'resolve Some(sym_id);
            }

            // Chained lookup: check lib binders for global symbols
            // This enables resolving console, Array, Object, etc. from lib.d.ts
            for (i, lib_binder) in self.lib_binders.iter().enumerate() {
                if let Some(sym_id) = lib_binder.file_locals.get(name) {
                    debug!(
                        "[RESOLVE] '{}' FOUND in lib_binder[{}] (id={}) - LIB SYMBOL",
                        name, i, sym_id.0
                    );
                    // Note: lib symbols are not imports, so no need to resolve
                    break 'resolve Some(sym_id);
                }
            }

            // Symbol not found - log the failure
            debug!(
                "[RESOLVE] '{}' NOT FOUND - searched scopes, file_locals, and {} lib binders",
                name,
                self.lib_binders.len()
            );

            None
        };

        self.resolved_identifier_cache
            .write()
            .unwrap()
            .insert(cache_key, result);

        result
    }

    /// Resolve an identifier by walking scopes and invoking a filter callback on candidates.
    ///
    /// This keeps scope traversal in the binder while allowing callers (checker) to
    /// apply contextual filtering (e.g., value-only vs type-only, class member filtering).
    pub fn resolve_identifier_with_filter<F>(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        lib_binders: &[Arc<BinderState>],
        mut accept: F,
    ) -> Option<SymbolId>
    where
        F: FnMut(SymbolId) -> bool,
    {
        let node = arena.get(node_idx)?;
        let name = if let Some(ident) = arena.get_identifier(node) {
            ident.escaped_text.as_str()
        } else {
            return None;
        };

        let mut consider = |sym_id: SymbolId| -> Option<SymbolId> {
            if accept(sym_id) { Some(sym_id) } else { None }
        };

        if let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) {
            let mut iterations = 0;
            while !scope_id.is_none() {
                iterations += 1;
                if iterations > MAX_SCOPE_WALK_ITERATIONS {
                    break;
                }
                let Some(scope) = self.scopes.get(scope_id.0 as usize) else {
                    break;
                };

                if let Some(sym_id) = scope.table.get(name) {
                    if let Some(found) = consider(sym_id) {
                        return Some(found);
                    }
                }

                if scope.kind == ContainerKind::Module {
                    if let Some(container_sym_id) = self.get_node_symbol(scope.container_node) {
                        if let Some(container_symbol) =
                            self.get_symbol_with_libs(container_sym_id, lib_binders)
                        {
                            if let Some(exports) = container_symbol.exports.as_ref() {
                                if let Some(member_id) = exports.get(name) {
                                    if let Some(found) = consider(member_id) {
                                        return Some(found);
                                    }
                                }
                            }
                        }
                    }
                }

                scope_id = scope.parent;
            }
        }

        if let Some(sym_id) = self.file_locals.get(name) {
            if let Some(found) = consider(sym_id) {
                return Some(found);
            }
        }

        if !self.lib_symbols_merged {
            for lib_binder in lib_binders {
                if let Some(sym_id) = lib_binder.file_locals.get(name) {
                    if let Some(found) = consider(sym_id) {
                        return Some(found);
                    }
                }
            }
        }

        None
    }

    /// Collect visible symbol names for diagnostics and suggestions.
    pub fn collect_visible_symbol_names(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
    ) -> Vec<String> {
        let mut names = FxHashSet::default();

        if let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) {
            let mut iterations = 0;
            while !scope_id.is_none() {
                iterations += 1;
                if iterations > MAX_SCOPE_WALK_ITERATIONS {
                    break;
                }
                let Some(scope) = self.scopes.get(scope_id.0 as usize) else {
                    break;
                };
                for (symbol_name, _) in scope.table.iter() {
                    names.insert(symbol_name.clone());
                }
                scope_id = scope.parent;
            }
        }

        for (symbol_name, _) in self.file_locals.iter() {
            names.insert(symbol_name.clone());
        }

        names.into_iter().collect()
    }

    /// Resolve private identifiers (#foo) across class scopes.
    ///
    /// Returns (symbols_found, saw_class_scope).
    pub fn resolve_private_identifier_symbols(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
    ) -> (Vec<SymbolId>, bool) {
        let node = match arena.get(node_idx) {
            Some(node) => node,
            None => return (Vec::new(), false),
        };
        let name = match arena.get_identifier(node) {
            Some(ident) => ident.escaped_text.as_str(),
            None => return (Vec::new(), false),
        };

        let mut symbols = Vec::new();
        let mut saw_class_scope = false;
        let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) else {
            return (symbols, saw_class_scope);
        };

        let mut iterations = 0;
        while !scope_id.is_none() {
            iterations += 1;
            if iterations > MAX_SCOPE_WALK_ITERATIONS {
                break;
            }
            let Some(scope) = self.scopes.get(scope_id.0 as usize) else {
                break;
            };
            if scope.kind == ContainerKind::Class {
                saw_class_scope = true;
            }
            if let Some(sym_id) = scope.table.get(name) {
                symbols.push(sym_id);
            }
            scope_id = scope.parent;
        }

        (symbols, saw_class_scope)
    }

    pub(crate) fn resolve_parameter_fallback(
        &self,
        arena: &NodeArena,
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
    pub(crate) fn resolve_import_if_needed(&self, sym_id: SymbolId) -> Option<SymbolId> {
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
    ///
    /// Results are cached to speed up repeated lookups (common with barrel files).
    pub(crate) fn resolve_import_with_reexports(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<SymbolId> {
        // Check cache first for fast path
        let cache_key = (module_specifier.to_string(), export_name.to_string());
        if let Some(&cached) = self.resolved_export_cache.read().unwrap().get(&cache_key) {
            return cached;
        }

        let mut visited = rustc_hash::FxHashSet::default();
        let result =
            self.resolve_import_with_reexports_inner(module_specifier, export_name, &mut visited);

        // Cache the result (including None for not found)
        self.resolved_export_cache
            .write()
            .unwrap()
            .insert(cache_key, result);
        result
    }

    /// Inner implementation with cycle detection for module re-exports.
    pub(crate) fn resolve_import_with_reexports_inner(
        &self,
        module_specifier: &str,
        export_name: &str,
        visited: &mut rustc_hash::FxHashSet<(String, String)>,
    ) -> Option<SymbolId> {
        let _span =
            span!(Level::DEBUG, "resolve_import_with_reexports", %module_specifier, %export_name)
                .entered();

        // Cycle detection: check if we've already visited this (module, export) pair
        let key = (module_specifier.to_string(), export_name.to_string());
        if visited.contains(&key) {
            return None;
        }
        visited.insert(key);

        // First, check if it's a direct export from this module
        if let Some(module_table) = self.module_exports.get(module_specifier)
            && let Some(sym_id) = module_table.get(export_name)
        {
            debug!(
                "[RESOLVE_IMPORT] '{}' from module '{}' -> direct export symbol id={}",
                export_name, module_specifier, sym_id.0
            );
            return Some(sym_id);
        }

        // Not found in direct exports, check for named re-exports
        if let Some(file_reexports) = self.reexports.get(module_specifier) {
            // Check for named re-export: `export { foo } from 'bar'`
            if let Some((source_module, original_name)) = file_reexports.get(export_name) {
                let name_to_lookup = original_name.as_deref().unwrap_or(export_name);
                debug!(
                    "[RESOLVE_IMPORT] '{}' from module '{}' -> following named re-export from '{}', original name='{}'",
                    export_name, module_specifier, source_module, name_to_lookup
                );
                return self.resolve_import_with_reexports_inner(
                    source_module,
                    name_to_lookup,
                    visited,
                );
            }
        }

        // Check for wildcard re-exports: `export * from 'bar'`
        // A module can have multiple wildcard re-exports, check all of them
        if let Some(source_modules) = self.wildcard_reexports.get(module_specifier) {
            for source_module in source_modules {
                debug!(
                    "[RESOLVE_IMPORT] '{}' from module '{}' -> trying wildcard re-export from '{}'",
                    export_name, module_specifier, source_module
                );
                if let Some(result) =
                    self.resolve_import_with_reexports_inner(source_module, export_name, visited)
                {
                    return Some(result);
                }
            }
        }

        // Export not found
        debug!(
            "[RESOLVE_IMPORT] '{}' from module '{}' -> NOT FOUND",
            export_name, module_specifier
        );
        None
    }

    /// Public method for testing import resolution with reexports.
    /// This allows tests to verify that wildcard and named re-exports are properly resolved.
    #[cfg(test)]
    pub fn resolve_import_if_needed_public(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<SymbolId> {
        self.resolve_import_with_reexports(module_specifier, export_name)
    }

    /// Resolve an import symbol to its target, following re-export chains.
    ///
    /// This is used by the checker to resolve imported symbols to their actual declarations,
    /// following both named re-exports (`export { foo } from 'bar'`) and wildcard re-exports
    /// (`export * from 'bar'`).
    ///
    /// Returns the resolved SymbolId if found, None otherwise.
    pub fn resolve_import_symbol(&self, sym_id: SymbolId) -> Option<SymbolId> {
        self.resolve_import_if_needed(sym_id)
    }

    /// Find the enclosing scope for a given node by walking up the AST.
    /// Returns the ScopeId of the nearest scope-creating ancestor node.
    pub(crate) fn find_enclosing_scope(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
    ) -> Option<ScopeId> {
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
    pub(crate) fn enter_persistent_scope(&mut self, kind: ContainerKind, node: NodeIndex) {
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
    pub(crate) fn exit_persistent_scope(&mut self) {
        if !self.current_scope_id.is_none()
            && let Some(scope) = self.scopes.get(self.current_scope_id.0 as usize)
        {
            self.current_scope_id = scope.parent;
        }
    }

    /// Declare a symbol in the current persistent scope.
    /// This adds the symbol to the persistent scope table for later querying.
    pub(crate) fn declare_in_persistent_scope(&mut self, name: String, sym_id: SymbolId) {
        if !self.current_scope_id.is_none()
            && let Some(scope) = self.scopes.get_mut(self.current_scope_id.0 as usize)
        {
            scope.table.set(name, sym_id);
        }
    }

    pub(crate) fn sync_current_scope_to_persistent(&mut self) {
        if self.current_scope_id.is_none() {
            return;
        }
        if let Some(persistent_scope) = self.scopes.get_mut(self.current_scope_id.0 as usize) {
            for (name, &sym_id) in self.current_scope.iter() {
                persistent_scope.table.set(name.clone(), sym_id);
            }
        }
    }

    pub(crate) fn source_file_is_external_module(
        &self,
        arena: &NodeArena,
        root: NodeIndex,
    ) -> bool {
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

    // =========================================================================
    // Lib Symbol Merging (SymbolId collision fix)
    // =========================================================================

    /// Check if two symbols can be merged across different lib files or files.
    ///
    /// TypeScript allows merging:
    /// - Interface + Interface (declaration merging)
    /// - Namespace + Namespace (declaration merging)
    /// - Class + Interface (merging for class declarations)
    /// - Namespace + Class/Function/Enum (augmentation)
    /// - Enum + Enum (declaration merging)
    pub(crate) fn can_merge_symbols(existing_flags: u32, new_flags: u32) -> bool {
        // Interface can merge with interface
        if (existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }

        // Class can merge with interface
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
        if (existing_flags & symbol_flags::MODULE) != 0
            && (new_flags & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::MODULE) != 0
            && (existing_flags
                & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
        {
            return true;
        }

        // Enum can merge with enum
        if (existing_flags & symbol_flags::ENUM) != 0 && (new_flags & symbol_flags::ENUM) != 0 {
            return true;
        }

        // Interface can merge with VALUE symbols (e.g., `interface Promise<T>` + `declare var Promise`)
        // This enables global types like Object, Array, Promise to be used as both types and constructors
        if (existing_flags & symbol_flags::INTERFACE) != 0 && (new_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::INTERFACE) != 0 && (existing_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }

        false
    }

    /// Merge lib contexts into this binder's symbol arena with remapped IDs.
    ///
    /// This is the core fix for SymbolId collisions across lib binders. Instead of
    /// storing raw lib SymbolIds (which collide), we:
    /// 1. Clone each lib symbol into our local symbol arena with a new unique ID
    /// 2. Remap internal references (parent, exports, members) to use new IDs
    /// 3. Update file_locals to use the new IDs
    /// 4. Track which arena each symbol's declarations belong to
    ///
    /// After this method, all symbol lookups can use our local arena directly,
    /// avoiding cross-binder ID collisions.
    pub fn merge_lib_contexts_into_binder(&mut self, lib_contexts: &[LibContext]) {
        // Visible globals can change after merge; invalidate identifier resolutions.
        self.resolved_identifier_cache.write().unwrap().clear();

        if lib_contexts.is_empty() {
            return;
        }

        // Phase 1: Clone all lib symbols into local arena, building remap maps
        // Maps: (lib_binder_ptr, old_id) -> new_id
        let mut lib_symbol_remap: FxHashMap<(usize, SymbolId), SymbolId> = FxHashMap::default();
        // Maps: symbol name -> new_id (for merging same-name symbols)
        let mut merged_by_name: FxHashMap<String, SymbolId> = FxHashMap::default();

        for lib_ctx in lib_contexts {
            let lib_binder_ptr = Arc::as_ptr(&lib_ctx.binder) as usize;

            // Process all symbols in this lib binder
            for i in 0..lib_ctx.binder.symbols.len() {
                let local_id = SymbolId(i as u32);
                let Some(lib_sym) = lib_ctx.binder.symbols.get(local_id) else {
                    continue;
                };

                // Check if a symbol with this name already exists (cross-lib merging)
                let new_id = if let Some(&existing_id) = merged_by_name.get(&lib_sym.escaped_name) {
                    // Symbol already exists - check if we can merge
                    if let Some(existing_sym) = self.symbols.get(existing_id) {
                        if Self::can_merge_symbols(existing_sym.flags, lib_sym.flags) {
                            // Merge: reuse existing symbol ID, merge declarations
                            if let Some(existing_mut) = self.symbols.get_mut(existing_id) {
                                existing_mut.flags |= lib_sym.flags;
                                for &decl in &lib_sym.declarations {
                                    if !existing_mut.declarations.contains(&decl) {
                                        existing_mut.declarations.push(decl);
                                        // Track which arena this specific declaration belongs to
                                        self.declaration_arenas.insert(
                                            (existing_id, decl),
                                            Arc::clone(&lib_ctx.arena),
                                        );
                                    }
                                }
                                // Update value_declaration if not set
                                if existing_mut.value_declaration.is_none()
                                    && !lib_sym.value_declaration.is_none()
                                {
                                    existing_mut.value_declaration = lib_sym.value_declaration;
                                }
                            }
                            existing_id
                        } else {
                            // Cannot merge - allocate new (shadowing)
                            let new_id = self.symbols.alloc_from(lib_sym);
                            merged_by_name.insert(lib_sym.escaped_name.clone(), new_id);
                            // Track declaration arenas for new symbol
                            for &decl in &lib_sym.declarations {
                                self.declaration_arenas
                                    .insert((new_id, decl), Arc::clone(&lib_ctx.arena));
                            }
                            new_id
                        }
                    } else {
                        // Shouldn't happen - allocate new
                        let new_id = self.symbols.alloc_from(lib_sym);
                        merged_by_name.insert(lib_sym.escaped_name.clone(), new_id);
                        // Track declaration arenas for new symbol
                        for &decl in &lib_sym.declarations {
                            self.declaration_arenas
                                .insert((new_id, decl), Arc::clone(&lib_ctx.arena));
                        }
                        new_id
                    }
                } else {
                    // New symbol - allocate in local arena
                    let new_id = self.symbols.alloc_from(lib_sym);
                    merged_by_name.insert(lib_sym.escaped_name.clone(), new_id);
                    // Track declaration arenas for new symbol
                    for &decl in &lib_sym.declarations {
                        self.declaration_arenas
                            .insert((new_id, decl), Arc::clone(&lib_ctx.arena));
                    }
                    new_id
                };

                // Store the remapping
                lib_symbol_remap.insert((lib_binder_ptr, local_id), new_id);

                // Track which arena contains this symbol's declarations (legacy - stores last arena)
                self.symbol_arenas
                    .insert(new_id, Arc::clone(&lib_ctx.arena));
            }
        }

        // Phase 2: Remap internal references (parent, exports, members)
        // We need a second pass because parents/exports/members may reference symbols
        // that were processed later in the first pass.
        for lib_ctx in lib_contexts {
            let lib_binder_ptr = Arc::as_ptr(&lib_ctx.binder) as usize;

            for i in 0..lib_ctx.binder.symbols.len() {
                let local_id = SymbolId(i as u32);
                let Some(&new_id) = lib_symbol_remap.get(&(lib_binder_ptr, local_id)) else {
                    continue;
                };
                let Some(lib_sym) = lib_ctx.binder.symbols.get(local_id) else {
                    continue;
                };

                // Remap parent
                if !lib_sym.parent.is_none() {
                    if let Some(&new_parent) =
                        lib_symbol_remap.get(&(lib_binder_ptr, lib_sym.parent))
                    {
                        if let Some(sym) = self.symbols.get_mut(new_id) {
                            sym.parent = new_parent;
                        }
                    }
                }

                // Remap exports
                if let Some(exports) = &lib_sym.exports {
                    let mut remapped_exports = SymbolTable::new();
                    for (name, &export_id) in exports.iter() {
                        if let Some(&new_export_id) =
                            lib_symbol_remap.get(&(lib_binder_ptr, export_id))
                        {
                            remapped_exports.set(name.clone(), new_export_id);
                        }
                    }
                    if !remapped_exports.is_empty() {
                        if let Some(sym) = self.symbols.get_mut(new_id) {
                            if sym.exports.is_none() {
                                sym.exports = Some(Box::new(remapped_exports));
                            } else if let Some(existing) = sym.exports.as_mut() {
                                for (name, id) in remapped_exports.iter() {
                                    if !existing.has(name) {
                                        existing.set(name.clone(), *id);
                                    }
                                }
                            }
                        }
                    }
                }

                // Remap members
                if let Some(members) = &lib_sym.members {
                    let mut remapped_members = SymbolTable::new();
                    for (name, &member_id) in members.iter() {
                        if let Some(&new_member_id) =
                            lib_symbol_remap.get(&(lib_binder_ptr, member_id))
                        {
                            remapped_members.set(name.clone(), new_member_id);
                        }
                    }
                    if !remapped_members.is_empty() {
                        if let Some(sym) = self.symbols.get_mut(new_id) {
                            if sym.members.is_none() {
                                sym.members = Some(Box::new(remapped_members));
                            } else if let Some(existing) = sym.members.as_mut() {
                                for (name, id) in remapped_members.iter() {
                                    if !existing.has(name) {
                                        existing.set(name.clone(), *id);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Phase 3: Update file_locals with remapped IDs
        for lib_ctx in lib_contexts {
            let lib_binder_ptr = Arc::as_ptr(&lib_ctx.binder) as usize;

            for (name, &local_id) in lib_ctx.binder.file_locals.iter() {
                if let Some(&new_id) = lib_symbol_remap.get(&(lib_binder_ptr, local_id)) {
                    // Only add if not already present (user symbols take precedence)
                    if !self.file_locals.has(name) {
                        self.file_locals.set(name.clone(), new_id);
                    }
                }
            }
        }

        // Mark that lib symbols have been merged
        self.lib_symbols_merged = true;
    }

    #[cfg(test)]
    pub(crate) fn resolved_identifier_cache_len(&self) -> usize {
        self.resolved_identifier_cache.read().unwrap().len()
    }

    /// Inject lib file symbols into file_locals for global symbol resolution.
    ///
    /// This method now delegates to `merge_lib_contexts_into_binder` which properly
    /// remaps SymbolIds to avoid collisions across lib binders.
    ///
    /// # Arguments
    /// * `lib_contexts` - Vector of lib file contexts (arena + binder pairs)
    pub fn inject_lib_symbols(&mut self, lib_contexts: &[LibContext]) {
        self.merge_lib_contexts_into_binder(lib_contexts);
    }

    /// Bind a source file using NodeArena.
    pub fn bind_source_file(&mut self, arena: &NodeArena, root: NodeIndex) {
        // Binding mutates scope/symbol tables, so stale identifier resolution entries
        // from prior passes must be dropped.
        self.resolved_identifier_cache.write().unwrap().clear();

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

            // Process hoisted function declarations first (for hoisting)
            self.process_hoisted_functions(arena);

            // Process hoisted var declarations (for hoisting)
            self.process_hoisted_vars(arena);

            // Second pass: bind each statement
            for &stmt_idx in &sf.statements.nodes {
                self.bind_node(arena, stmt_idx);
                self.top_level_flow.insert(stmt_idx.0, self.current_flow);
            }

            // Populate module_exports for cross-file import resolution
            // This enables type-only import elision and proper import validation
            let file_name = sf.file_name.clone();
            self.populate_module_exports_from_file_symbols(arena, &file_name);
        }

        self.sync_current_scope_to_persistent();

        // Store file locals from the ROOT scope only, not nested namespaces/modules.
        // This prevents namespace-local symbols from being accessible globally.
        // User symbols take precedence - only add lib symbols if no user symbol exists.
        let existing_file_locals = std::mem::take(&mut self.file_locals);

        // Only collect symbols from the root SourceFile scope, not nested namespaces/modules
        let root_scope_symbols = if let Some(root_scope) = self.scopes.first() {
            // The first scope is always the SourceFile scope
            root_scope.table.clone()
        } else {
            // Fallback: empty scope if no scopes exist (shouldn't happen)
            SymbolTable::new()
        };

        // Debug: log what's going into file_locals
        if std::env::var("BIND_DEBUG").is_ok() {
            eprintln!(
                "[FILE_LOCALS] Root scope has {} symbols",
                root_scope_symbols.len()
            );
            for (name, _) in root_scope_symbols.iter() {
                eprintln!("[FILE_LOCALS]   - {}", name);
            }
        }

        self.file_locals = root_scope_symbols;

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

    /// Populate module_exports from file-level module symbols.
    ///
    /// This enables cross-file import resolution and type-only import elision.
    /// After binding a source file, we collect all module-level exports and
    /// add them to the module_exports table keyed by the file name.
    ///
    /// # Arguments
    /// * `arena` - The NodeArena containing the AST
    /// * `file_name` - The name of the file being bound (used as the key in module_exports)
    fn populate_module_exports_from_file_symbols(&mut self, _arena: &NodeArena, file_name: &str) {
        use crate::binder::symbol_flags;

        // Collect all exports from all module-level symbols in this file
        let mut file_exports = SymbolTable::new();

        // Iterate through file_locals to find modules and their exports
        for (_name, &sym_id) in self.file_locals.iter() {
            if let Some(symbol) = self.symbols.get(sym_id) {
                // Check if this is a module/namespace symbol
                if (symbol.flags & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE))
                    != 0
                {
                    // If the module has an exports table, merge it into file_exports
                    if let Some(module_exports) = symbol.exports.as_ref() {
                        for (export_name, &export_sym_id) in module_exports.iter() {
                            if !file_exports.has(export_name) {
                                file_exports.set(export_name.clone(), export_sym_id);
                            }
                        }
                    }
                }
            }
        }

        // Add to module_exports if we found any exports
        if !file_exports.is_empty() {
            self.module_exports
                .insert(file_name.to_string(), file_exports);
        }
    }

    /// Merge lib file symbols into the current scope.
    ///
    /// This is called during binder initialization to ensure global symbols
    /// from lib.d.ts (like `Object`, `Function`, `console`, etc.) are available
    /// during type checking.
    ///
    /// This method now uses `merge_lib_contexts_into_binder` which properly
    /// remaps SymbolIds to avoid collisions across lib binders.
    ///
    /// # Parameters
    /// - `lib_files`: Slice of Arc<LibFile> containing parsed and bound lib files
    ///
    /// # Example
    /// ```ignore
    /// let mut binder = BinderState::new();
    /// binder.bind_source_file(arena, root);
    /// binder.merge_lib_symbols(&lib_files);
    /// ```
    pub fn merge_lib_symbols(&mut self, lib_files: &[Arc<lib_loader::LibFile>]) {
        // Merging lib globals changes visible symbols, so invalidate identifier cache.
        self.resolved_identifier_cache.write().unwrap().clear();

        // Convert LibFiles to LibContexts
        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();

        // Use the new merge helper that properly remaps SymbolIds
        self.merge_lib_contexts_into_binder(&lib_contexts);

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

        // Note: We no longer need to track lib_binders separately since
        // all lib symbols are now in our local symbol arena with unique IDs.
        // However, we keep lib_binders populated for backward compatibility
        // with any code that still iterates through them.
        for lib in lib_files {
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
    /// - `arena`: The NodeArena containing the AST
    /// - `root`: The root node index of the source file
    /// - `lib_files`: Optional slice of Arc<LibFile> containing lib files
    pub fn bind_source_file_with_libs(
        &mut self,
        arena: &NodeArena,
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
        arena: &NodeArena,
        root: NodeIndex,
        prefix_statements: &[NodeIndex],
        old_suffix_statements: &[NodeIndex],
        new_suffix_statements: &[NodeIndex],
        reparse_start: u32,
    ) -> bool {
        // Incremental binding mutates scopes; clear stale identifier resolutions.
        self.resolved_identifier_cache.write().unwrap().clear();

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
        self.process_hoisted_vars(arena);

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

    pub(crate) fn prune_incremental_maps(&mut self, arena: &NodeArena, reparse_start: u32) {
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
    pub(crate) fn collect_hoisted_declarations(
        &mut self,
        arena: &NodeArena,
        statements: &NodeList,
    ) {
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
                        // In ES5 and earlier, function declarations inside blocks are hoisted
                        // to the containing function scope. In ES6+, they remain block-scoped.
                        // Only descend into blocks to collect functions if we're in ES5 mode.
                        if self.options.target.is_es5() {
                            if let Some(block) = arena.get_block(node) {
                                self.collect_hoisted_declarations(arena, &block.statements);
                            }
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
                        || k == syntax_kind_ext::DO_STATEMENT =>
                    {
                        if let Some(loop_data) = arena.get_loop(node) {
                            self.collect_hoisted_from_node(arena, loop_data.statement);
                        }
                    }
                    k if k == syntax_kind_ext::FOR_STATEMENT => {
                        if let Some(loop_data) = arena.get_loop(node) {
                            // Hoist var declarations from initializer (e.g., `for (var i = 0; ...)`)
                            let init = loop_data.initializer;
                            if !init.is_none() {
                                if let Some(init_node) = arena.get(init) {
                                    if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                                    {
                                        self.collect_hoisted_var_decl(arena, init);
                                    }
                                }
                            }
                            // Hoist from the loop body
                            self.collect_hoisted_from_node(arena, loop_data.statement);
                        }
                    }
                    k if k == syntax_kind_ext::FOR_IN_STATEMENT
                        || k == syntax_kind_ext::FOR_OF_STATEMENT =>
                    {
                        if let Some(for_data) = arena.get_for_in_of(node) {
                            // Hoist var declarations from the initializer (e.g., `for (var x in obj)`)
                            let init = for_data.initializer;
                            if let Some(init_node) = arena.get(init) {
                                if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                                    self.collect_hoisted_var_decl(arena, init);
                                }
                            }
                            // Hoist from the loop body
                            self.collect_hoisted_from_node(arena, for_data.statement);
                        }
                    }
                    k if k == syntax_kind_ext::TRY_STATEMENT => {
                        if let Some(try_data) = arena.get_try(node) {
                            // Hoist from try block
                            self.collect_hoisted_from_node(arena, try_data.try_block);
                            // Hoist from catch clause's block
                            if !try_data.catch_clause.is_none() {
                                if let Some(catch_node) = arena.get(try_data.catch_clause) {
                                    if let Some(catch_data) = arena.get_catch_clause(catch_node) {
                                        self.collect_hoisted_from_node(arena, catch_data.block);
                                    }
                                }
                            }
                            // Hoist from finally block
                            if !try_data.finally_block.is_none() {
                                self.collect_hoisted_from_node(arena, try_data.finally_block);
                            }
                        }
                    }
                    k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                        if let Some(switch_data) = arena.get_switch(node) {
                            // The case_block is treated as a block - get its children (case/default clauses)
                            if let Some(case_block_node) = arena.get(switch_data.case_block) {
                                if let Some(block_data) = arena.get_block(case_block_node) {
                                    // Each child is a case/default clause with statements
                                    for &clause_idx in &block_data.statements.nodes {
                                        if let Some(clause_node) = arena.get(clause_idx) {
                                            if let Some(clause_data) =
                                                arena.get_case_clause(clause_node)
                                            {
                                                self.collect_hoisted_declarations(
                                                    arena,
                                                    &clause_data.statements,
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    k if k == syntax_kind_ext::LABELED_STATEMENT => {
                        if let Some(label_data) = arena.get_labeled_statement(node) {
                            self.collect_hoisted_from_node(arena, label_data.statement);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn collect_hoisted_var_decl(&mut self, arena: &NodeArena, decl_list_idx: NodeIndex) {
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
                                if let Some(name) = self.get_identifier_name(arena, ident_idx) {
                                    self.hoisted_vars.push((name.to_string(), ident_idx));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn collect_hoisted_from_node(&mut self, arena: &NodeArena, idx: NodeIndex) {
        if let Some(node) = arena.get(idx) {
            if node.kind == syntax_kind_ext::BLOCK {
                // In ES5 and earlier, function declarations inside blocks are hoisted
                // to the containing function scope. In ES6+, they remain block-scoped.
                // Only descend into blocks to collect functions if we're in ES5 mode.
                if self.options.target.is_es5() {
                    if let Some(block) = arena.get_block(node) {
                        self.collect_hoisted_declarations(arena, &block.statements);
                    }
                }
            } else {
                // Handle single statement (not wrapped in a block)
                // e.g., `if (x) var y = 1;` or `while (x) var i = 0;`
                let mut stmts = crate::parser::NodeList::new();
                stmts.nodes.push(idx);
                self.collect_hoisted_declarations(arena, &stmts);
            }
        }
    }

    /// Process hoisted function declarations.
    pub(crate) fn process_hoisted_functions(&mut self, arena: &NodeArena) {
        let functions = std::mem::take(&mut self.hoisted_functions);
        for func_idx in functions {
            if let Some(node) = arena.get(func_idx)
                && let Some(func) = arena.get_function(node)
                && let Some(name) = self.get_identifier_name(arena, func.name)
            {
                let is_exported = self.has_export_modifier(arena, &func.modifiers);
                let sym_id =
                    self.declare_symbol(name, symbol_flags::FUNCTION, func_idx, is_exported);

                // Also add to persistent scope
                self.declare_in_persistent_scope(name.to_string(), sym_id);
            }
        }
    }

    /// Process hoisted var declarations.
    /// Var declarations are hoisted to the top of their function/global scope.
    pub(crate) fn process_hoisted_vars(&mut self, arena: &NodeArena) {
        let hoisted_vars = std::mem::take(&mut self.hoisted_vars);
        for (name, decl_idx) in hoisted_vars {
            // Declare the var symbol with FUNCTION_SCOPED_VARIABLE flag
            // This makes it accessible before its actual declaration point
            let is_exported = self.is_node_exported(arena, decl_idx);
            let sym_id = self.declare_symbol(
                &name,
                symbol_flags::FUNCTION_SCOPED_VARIABLE,
                decl_idx,
                is_exported,
            );

            // Also add to persistent scope
            self.declare_in_persistent_scope(name, sym_id);
        }
    }

    /// Bind a node and its children.
    pub(crate) fn bind_node(&mut self, arena: &NodeArena, idx: NodeIndex) {
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
                    use tracing::trace;

                    // Bind the condition expression (record identifiers in it)
                    self.bind_expression(arena, if_stmt.expression);

                    // Save the pre-condition flow
                    let pre_condition_flow = self.current_flow;
                    trace!(
                        pre_condition_flow = pre_condition_flow.0,
                        "if statement: pre_condition_flow",
                    );

                    // Create TRUE_CONDITION flow for the then branch
                    let true_flow = self.create_flow_condition(
                        flow_flags::TRUE_CONDITION,
                        pre_condition_flow,
                        if_stmt.expression,
                    );
                    trace!(
                        true_flow = true_flow.0,
                        "if statement: created TRUE_CONDITION flow",
                    );

                    // Bind the then branch with narrowed flow
                    self.current_flow = true_flow;
                    trace!("if statement: binding then branch with TRUE_CONDITION flow");
                    self.bind_node(arena, if_stmt.then_statement);
                    let after_then_flow = self.current_flow;
                    trace!(
                        after_then_flow = after_then_flow.0,
                        "if statement: after_then_flow",
                    );

                    // Handle else branch if present
                    let after_else_flow = if !if_stmt.else_statement.is_none() {
                        // Create FALSE_CONDITION flow for the else branch
                        let false_flow = self.create_flow_condition(
                            flow_flags::FALSE_CONDITION,
                            pre_condition_flow,
                            if_stmt.expression,
                        );
                        trace!(
                            false_flow = false_flow.0,
                            "if statement: created FALSE_CONDITION flow",
                        );

                        // Bind the else branch with narrowed flow
                        self.current_flow = false_flow;
                        trace!("if statement: binding else branch with FALSE_CONDITION flow");
                        self.bind_node(arena, if_stmt.else_statement);
                        let result = self.current_flow;
                        trace!(result = result.0, "if statement: after_else_flow",);
                        result
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
                    trace!(
                        merge_label = merge_label.0,
                        "if statement: created merge label",
                    );
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
                    // export = expr; exports all members of expr as module exports
                    // For example: export = Utils; makes all Utils exports available
                    self.bind_node(arena, assign.expression);

                    // If the expression is an identifier, resolve it and copy its exports
                    if let Some(name) = self.get_identifier_name(arena, assign.expression) {
                        if let Some(sym_id) = self
                            .current_scope
                            .get(name)
                            .or_else(|| self.file_locals.get(name))
                        {
                            // Copy the symbol's exports to the current module's exports
                            // This makes export = Namespace; work correctly
                            if let Some(symbol) = self.symbols.get(sym_id) {
                                if let Some(ref exports) = symbol.exports {
                                    // Add all exports from the target symbol to file_locals
                                    for (export_name, &export_sym_id) in exports.iter() {
                                        self.file_locals.set(export_name.clone(), export_sym_id);
                                    }
                                }
                            }
                        }
                    }
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
                    // Use bind_expression instead of bind_node to properly record flow
                    // for identifiers within property access expressions etc.
                    self.bind_expression(arena, expr_stmt.expression);
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

            // Non-null expression - just bind the inner expression
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if node.has_data()
                    && let Some(unary) = arena.unary_exprs_ex.get(node.data_index as usize)
                {
                    self.bind_node(arena, unary.expression);
                }
            }

            // Await expression - create flow node for async suspension point
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => {
                if let Some(unary) = arena.get_unary_expr_ex(node) {
                    self.bind_node(arena, unary.expression);
                }
                let flow = self.create_flow_await_point(idx);
                self.current_flow = flow;
            }

            // Yield expression - create flow node for generator suspension point
            k if k == syntax_kind_ext::YIELD_EXPRESSION => {
                if let Some(unary) = arena.get_unary_expr_ex(node) {
                    self.bind_node(arena, unary.expression);
                }
                let flow = self.create_flow_yield_point(idx);
                self.current_flow = flow;
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

            // Typeof, void expressions - record flow and traverse into operand
            k if k == syntax_kind_ext::TYPE_OF_EXPRESSION
                || k == syntax_kind_ext::VOID_EXPRESSION =>
            {
                self.record_flow(idx);
                if let Some(unary) = arena.get_unary_expr(node) {
                    self.bind_node(arena, unary.operand);
                }
            }

            // Await, yield expressions - record flow and traverse into expression
            // Note: These use unary_exprs_ex storage with `expression` field, not unary_exprs
            k if k == syntax_kind_ext::AWAIT_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION =>
            {
                self.record_flow(idx);
                if let Some(unary) = arena.get_unary_expr_ex(node) {
                    self.bind_node(arena, unary.expression);
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
    pub(crate) fn get_identifier_name<'a>(
        &self,
        arena: &'a NodeArena,
        idx: NodeIndex,
    ) -> Option<&'a str> {
        if let Some(node) = arena.get(idx)
            && let Some(id) = arena.get_identifier(node)
        {
            return Some(&id.escaped_text);
        }
        None
    }

    pub(crate) fn collect_binding_identifiers(
        &self,
        arena: &NodeArena,
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

    pub(crate) fn collect_file_scope_names_for_statements(
        &self,
        arena: &NodeArena,
        statements: &[NodeIndex],
        out: &mut FxHashSet<String>,
    ) {
        for &stmt_idx in statements {
            self.collect_file_scope_names_for_statement(arena, stmt_idx, out);
        }
    }

    pub(crate) fn collect_file_scope_names_for_statement(
        &self,
        arena: &NodeArena,
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
                    } else if clause_node.kind == SyntaxKind::Identifier as u16
                        && let Some(name) = self.get_identifier_name(arena, export.export_clause)
                    {
                        out.insert(name.to_string());
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

    pub(crate) fn collect_hoisted_file_scope_names(
        &self,
        arena: &NodeArena,
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

    pub(crate) fn collect_hoisted_file_scope_from_node(
        &self,
        arena: &NodeArena,
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

    pub(crate) fn collect_variable_decl_names(
        &self,
        arena: &NodeArena,
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

    pub(crate) fn collect_import_names(
        &self,
        arena: &NodeArena,
        node: &Node,
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
                    if let Some(name) = self.get_identifier_name(arena, clause.named_bindings) {
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
                            if let Some(name) = self.get_identifier_name(arena, local_ident) {
                                out.insert(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn collect_statement_symbol_nodes(
        &self,
        arena: &NodeArena,
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

    pub(crate) fn collect_variable_decl_symbol_nodes(
        &self,
        arena: &NodeArena,
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
                if let Some(_name) = self.get_identifier_name(arena, decl.name) {
                    out.push(decl.name);
                } else {
                    let mut names = Vec::new();
                    self.collect_binding_identifiers(arena, decl.name, &mut names);
                    out.extend(names);
                }
            }
        }
    }

    pub(crate) fn collect_import_symbol_nodes(
        &self,
        arena: &NodeArena,
        node: &Node,
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

    pub(crate) fn collect_export_symbol_nodes(
        &self,
        arena: &NodeArena,
        node: &Node,
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
    pub(crate) fn has_abstract_modifier(
        &self,
        arena: &NodeArena,
        modifiers: &Option<NodeList>,
    ) -> bool {
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
    pub(crate) fn has_static_modifier(
        &self,
        arena: &NodeArena,
        modifiers: &Option<NodeList>,
    ) -> bool {
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
    pub(crate) fn has_export_modifier(
        &self,
        arena: &NodeArena,
        modifiers: &Option<NodeList>,
    ) -> bool {
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
    pub(crate) fn has_declare_modifier(
        &self,
        arena: &NodeArena,
        modifiers: &Option<NodeList>,
    ) -> bool {
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
    pub(crate) fn has_const_modifier(
        &self,
        arena: &NodeArena,
        modifiers: &Option<NodeList>,
    ) -> bool {
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
    pub(crate) fn is_node_exported(&self, arena: &NodeArena, idx: NodeIndex) -> bool {
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
                        if let Some(stmt_node) = arena.get(stmt_idx)
                            && stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                            && let Some(var_stmt) = arena.get_variable(stmt_node)
                        {
                            return self.has_export_modifier(arena, &var_stmt.modifiers);
                        }
                    }
                }
            }
            _ => {}
        }
        false
    }

    /// Declare a symbol in the current scope, merging when allowed.
    pub(crate) fn declare_symbol(
        &mut self,
        name: &str,
        flags: u32,
        declaration: NodeIndex,
        is_exported: bool,
    ) -> SymbolId {
        if let Some(existing_id) = self.current_scope.get(name) {
            // Check if the existing symbol is in the local symbol table.
            // If not (e.g., it's from a lib binder), we should create a new local symbol
            // to shadow the lib symbol with the local declaration.
            if self.symbols.get(existing_id).is_none() {
                // The existing_id is from a lib binder, not our local binder.
                // Create a new symbol in the local binder to shadow the lib symbol.
                let sym_id = self.symbols.alloc(flags, name.to_string());
                if let Some(sym) = self.symbols.get_mut(sym_id) {
                    sym.declarations.push(declaration);
                    if (flags & symbol_flags::VALUE) != 0 {
                        sym.value_declaration = declaration;
                    }
                    sym.is_exported = is_exported;
                }
                // Update current_scope to point to the local symbol (shadowing)
                self.current_scope.set(name.to_string(), sym_id);
                self.node_symbols.insert(declaration.0, sym_id);
                self.declare_in_persistent_scope(name.to_string(), sym_id);
                return sym_id;
            }

            let existing_flags = self.symbols.get(existing_id).map(|s| s.flags).unwrap_or(0);
            let can_merge = Self::can_merge_flags(existing_flags, flags);

            let combined_flags = if can_merge {
                existing_flags | flags
            } else {
                existing_flags
            };

            // Record merge event for debugging
            self.debugger
                .record_merge(name, existing_id, existing_flags, flags, combined_flags);

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
                self.debugger.record_declaration(
                    name,
                    existing_id,
                    combined_flags,
                    sym.declarations.len(),
                    true,
                );
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
        self.debugger
            .record_declaration(name, sym_id, flags, 1, false);

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

        if (existing_flags & symbol_flags::MODULE) != 0
            && (new_flags & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::MODULE) != 0
            && (existing_flags
                & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
        {
            return true;
        }

        if (existing_flags & symbol_flags::FUNCTION) != 0
            && (new_flags & symbol_flags::FUNCTION) != 0
        {
            return true;
        }

        // Allow method overloads to merge (method signature + method implementation)
        if (existing_flags & symbol_flags::METHOD) != 0 && (new_flags & symbol_flags::METHOD) != 0 {
            return true;
        }

        // Allow INTERFACE to merge with VALUE symbols (e.g., `interface Object` + `declare var Object`)
        // This enables global types like Object, Array, Promise to be used as both types and constructors
        if (existing_flags & symbol_flags::INTERFACE) != 0 && (new_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::INTERFACE) != 0 && (existing_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }

        // Allow TYPE_ALIAS to merge with VALUE symbols
        // In TypeScript, type aliases and values exist in separate namespaces
        // and can share the same name:
        //   type Foo = number;
        //   export const Foo = 1;  // legal: Foo is both a type and a value
        if (existing_flags & symbol_flags::TYPE_ALIAS) != 0
            && (new_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::TYPE_ALIAS) != 0
            && (existing_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }

        // Allow static and instance members to have the same name
        // TypeScript allows a class to have both a static member and an instance member with the same name
        // e.g., class C { static foo; foo; }
        let existing_is_static = (existing_flags & symbol_flags::STATIC) != 0;
        let new_is_static = (new_flags & symbol_flags::STATIC) != 0;
        if existing_is_static != new_is_static {
            // One is static, one is instance - allow merge
            return true;
        }

        false
    }

    // Scope management

    pub(crate) fn enter_scope(&mut self, kind: ContainerKind, node: NodeIndex) {
        // Legacy scope chain management
        let parent = Some(self.current_scope_idx);
        self.scope_chain.push(ScopeContext::new(kind, node, parent));
        self.current_scope_idx = self.scope_chain.len() - 1;
        self.push_scope();

        // Persistent scope management (for stateless checking)
        self.enter_persistent_scope(kind, node);
    }

    pub(crate) fn exit_scope(&mut self, arena: &NodeArena) {
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
        if let Some(ctx) = self.scope_chain.get(self.current_scope_idx)
            && let Some(parent) = ctx.parent_idx
        {
            self.current_scope_idx = parent;
        }

        // Exit persistent scope
        self.exit_persistent_scope();
    }

    pub(crate) fn push_scope(&mut self) {
        let old_scope = std::mem::take(&mut self.current_scope);
        self.scope_stack.push(old_scope);
        self.current_scope = SymbolTable::new();
    }

    pub(crate) fn pop_scope(&mut self) {
        if let Some(parent_scope) = self.scope_stack.pop() {
            self.current_scope = parent_scope;
        }
    }
}
