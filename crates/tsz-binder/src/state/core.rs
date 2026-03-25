//! Core implementation of `BinderState` methods.
//!
//! Extracted from `mod.rs` to follow the thin-mod.rs pattern.

use super::{BinderState, BinderStateScopeInputs, LibContext};
use crate::lib_loader;
use crate::modules::resolution_debug::ModuleResolutionDebugger;
use crate::{
    ContainerKind, FlowNodeArena, FlowNodeId, Scope, ScopeContext, ScopeId, Symbol, SymbolArena,
    SymbolId, SymbolTable, flow_flags, symbol_flags,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

use super::{BinderOptions, FileFeatures};

impl BinderStateScopeInputs {
    pub(super) fn with_scopes(scopes: Vec<Scope>, node_scope_ids: FxHashMap<u32, ScopeId>) -> Self {
        Self {
            scopes,
            node_scope_ids,
            flow_nodes: FlowNodeArena::new(),
            ..Self::default()
        }
    }
}

impl BinderState {
    #[must_use]
    pub fn new() -> Self {
        Self::with_options(BinderOptions::default())
    }

    #[must_use]
    pub fn with_options(options: BinderOptions) -> Self {
        let mut flow_nodes = FlowNodeArena::new();
        let unreachable_flow = flow_nodes.alloc(flow_flags::UNREACHABLE);

        // Pre-size the largest hash maps to avoid resize thrashing.
        // These capacities are tuned for typical source files (500-5000 AST nodes).
        // Oversizing is cheap (a few KB of empty buckets) but undersizing causes
        // O(N) rehash cascades during binding.
        let mut binder = Self {
            options,
            symbols: SymbolArena::new(),
            current_scope: SymbolTable::new(),
            scope_stack: Vec::with_capacity(16),
            file_locals: SymbolTable::new(),
            expando_properties: FxHashMap::default(),
            declared_modules: FxHashSet::default(),
            is_external_module: false,
            is_strict_scope: false,
            flow_nodes,
            current_flow: FlowNodeId::NONE,
            unreachable_flow,
            scope_chain: Vec::with_capacity(32),
            current_scope_idx: 0,
            node_symbols: FxHashMap::with_capacity_and_hasher(256, Default::default()),
            module_declaration_exports_publicly: FxHashMap::default(),
            symbol_arenas: FxHashMap::default(),
            declaration_arenas: FxHashMap::default(),
            cross_file_node_symbols: FxHashMap::default(),
            node_flow: FxHashMap::with_capacity_and_hasher(128, Default::default()),
            top_level_flow: FxHashMap::default(),
            switch_clause_to_switch: FxHashMap::default(),
            hoisted_vars: Vec::new(),
            hoisted_functions: Vec::new(),
            scopes: Vec::with_capacity(32),
            node_scope_ids: FxHashMap::with_capacity_and_hasher(64, Default::default()),
            current_scope_id: ScopeId::NONE,
            debugger: ModuleResolutionDebugger::new(),
            global_augmentations: FxHashMap::default(),
            in_global_augmentation: false,
            module_augmentations: FxHashMap::default(),
            in_module_augmentation: false,
            current_augmented_module: None,
            augmentation_target_modules: FxHashMap::default(),
            lib_binders: Vec::new(),
            lib_symbol_ids: FxHashSet::default(),
            lib_symbol_reverse_remap: FxHashMap::default(),
            module_exports: FxHashMap::default(),
            reexports: FxHashMap::default(),
            wildcard_reexports: FxHashMap::default(),
            wildcard_reexports_type_only: FxHashMap::default(),
            resolved_export_cache: Default::default(),
            resolved_identifier_cache: Default::default(),
            shorthand_ambient_modules: FxHashSet::default(),
            modules_with_export_equals: FxHashSet::default(),
            module_export_equals_non_module: FxHashMap::default(),
            lib_symbols_merged: false,
            break_targets: Vec::new(),
            continue_targets: Vec::new(),
            return_targets: Vec::new(),
            file_features: FileFeatures::NONE,
            alias_partners: FxHashMap::default(),
            semantic_defs: FxHashMap::default(),
            file_import_sources: Vec::new(),
            file_idx: u32::MAX,
        };
        binder.recompute_module_export_equals_non_module();
        binder
    }

    /// Reset binder state to its initial values.
    ///
    /// # Panics
    ///
    /// Panics if the resolved identifier/export caches are poisoned when clearing
    /// their locks.
    pub fn reset(&mut self) {
        self.symbols.clear();
        self.current_scope.clear();
        self.scope_stack.clear();
        self.file_locals.clear();
        self.expando_properties.clear();
        self.declared_modules.clear();
        self.is_external_module = false;
        self.is_strict_scope = false;
        self.flow_nodes.clear();
        self.unreachable_flow = self.flow_nodes.alloc(flow_flags::UNREACHABLE);
        self.current_flow = FlowNodeId::NONE;
        self.scope_chain.clear();
        self.current_scope_idx = 0;
        self.node_symbols.clear();
        self.module_declaration_exports_publicly.clear();
        self.symbol_arenas.clear();
        self.declaration_arenas.clear();
        self.cross_file_node_symbols.clear();
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
        self.wildcard_reexports_type_only.clear();
        self.resolved_export_cache
            .write()
            .expect("RwLock not poisoned")
            .clear();
        self.resolved_identifier_cache
            .write()
            .expect("RwLock not poisoned")
            .clear();
        self.shorthand_ambient_modules.clear();
        self.modules_with_export_equals.clear();
        self.module_export_equals_non_module.clear();
        self.lib_symbols_merged = false;
        self.break_targets.clear();
        self.continue_targets.clear();
        self.return_targets.clear();
        self.semantic_defs.clear();
        self.file_import_sources.clear();
        // Note: file_idx is NOT reset here. It is set by the driver (LSP/CLI)
        // and should persist across re-binds of the same file.
    }

    /// Set the stable file index for per-file identity tracking.
    ///
    /// When set before `bind_source_file`, all symbols and `SemanticDefEntry`
    /// records created during binding will use this index as their `file_id`.
    /// This enables `DefinitionStore::invalidate_file(file_idx)` to clean up
    /// stale definitions when a file is removed or replaced.
    ///
    /// Defaults to `u32::MAX` (unassigned) for backward compatibility with
    /// single-file and CLI modes that don't need per-file invalidation.
    pub const fn set_file_idx(&mut self, file_idx: u32) {
        self.file_idx = file_idx;
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
        if let Some(arena) = self
            .declaration_arenas
            .get(&(sym_id, decl_idx))
            .and_then(|v| v.first())
        {
            return Some(arena);
        }
        // Fall back to symbol-level arena (for backwards compatibility and non-merged symbols)
        self.symbol_arenas.get(&sym_id)
    }

    /// Create a `BinderState` from pre-parsed lib data.
    ///
    /// This is used for loading pre-parsed lib files where we only have
    /// symbols and `file_locals` (no `node_symbols` or other binding state).
    #[must_use]
    pub fn from_preparsed(symbols: SymbolArena, file_locals: SymbolTable) -> Self {
        Self::from_bound_state(symbols, file_locals, FxHashMap::default())
    }

    /// Create a `BinderState` from existing bound state.
    ///
    /// This is used for type checking after parallel binding and symbol merging.
    /// The symbols and `node_symbols` come from the merged program state.
    #[must_use]
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

    /// Create a `BinderState` from existing bound state with options.
    #[must_use]
    pub fn from_bound_state_with_options(
        options: BinderOptions,
        symbols: SymbolArena,
        file_locals: SymbolTable,
        node_symbols: FxHashMap<u32, SymbolId>,
    ) -> Self {
        let mut flow_nodes = FlowNodeArena::new();
        let unreachable_flow = flow_nodes.alloc(flow_flags::UNREACHABLE);

        let mut binder = Self {
            options,
            symbols,
            current_scope: SymbolTable::new(),
            scope_stack: Vec::new(),
            file_locals,
            expando_properties: FxHashMap::default(),
            declared_modules: FxHashSet::default(),
            is_external_module: false,
            is_strict_scope: false,
            flow_nodes,
            current_flow: FlowNodeId::NONE,
            unreachable_flow,
            scope_chain: Vec::new(),
            current_scope_idx: 0,
            node_symbols,
            module_declaration_exports_publicly: FxHashMap::default(),
            symbol_arenas: FxHashMap::default(),
            declaration_arenas: FxHashMap::default(),
            cross_file_node_symbols: FxHashMap::default(),
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
            augmentation_target_modules: FxHashMap::default(),
            lib_binders: Vec::new(),
            lib_symbol_ids: FxHashSet::default(),
            lib_symbol_reverse_remap: FxHashMap::default(),
            module_exports: FxHashMap::default(),
            reexports: FxHashMap::default(),
            wildcard_reexports: FxHashMap::default(),
            wildcard_reexports_type_only: FxHashMap::default(),
            resolved_export_cache: Default::default(),
            resolved_identifier_cache: Default::default(),
            shorthand_ambient_modules: FxHashSet::default(),
            modules_with_export_equals: FxHashSet::default(),
            module_export_equals_non_module: FxHashMap::default(),
            lib_symbols_merged: false,
            break_targets: Vec::new(),
            continue_targets: Vec::new(),
            return_targets: Vec::new(),
            file_features: FileFeatures::NONE,
            alias_partners: FxHashMap::default(),
            semantic_defs: FxHashMap::default(),
            file_import_sources: Vec::new(),
            file_idx: u32::MAX,
        };
        binder.recompute_module_export_equals_non_module();
        binder
    }

    /// Create a `BinderState` from existing bound state, preserving scopes.
    #[must_use]
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
            BinderStateScopeInputs::with_scopes(scopes, node_scope_ids),
        )
    }

    /// Create a `BinderState` from existing bound state, preserving scopes and global augmentations.
    ///
    /// This is used for type checking after parallel binding and symbol merging.
    /// Global augmentations are interface/type declarations inside `declare global` blocks
    /// that should merge with lib.d.ts symbols during type resolution.
    /// Module augmentations are interface/type declarations inside `declare module 'x'` blocks
    /// that should merge with the target module's symbols.
    #[must_use]
    pub fn from_bound_state_with_scopes_and_augmentations(
        options: BinderOptions,
        symbols: SymbolArena,
        file_locals: SymbolTable,
        node_symbols: FxHashMap<u32, SymbolId>,
        inputs: BinderStateScopeInputs,
    ) -> Self {
        let BinderStateScopeInputs {
            scopes,
            node_scope_ids,
            global_augmentations,
            module_augmentations,
            augmentation_target_modules,
            module_exports,
            module_declaration_exports_publicly,
            reexports,
            wildcard_reexports,
            wildcard_reexports_type_only,
            symbol_arenas,
            declaration_arenas,
            cross_file_node_symbols,
            shorthand_ambient_modules,
            modules_with_export_equals,
            flow_nodes,
            node_flow,
            switch_clause_to_switch,
            expando_properties,
            alias_partners,
        } = inputs;

        // Find the unreachable flow node in the existing flow_nodes, or create a new one
        let unreachable_flow = flow_nodes.find_unreachable().unwrap_or(
            // This shouldn't happen in practice since the binder always creates an unreachable flow
            FlowNodeId::NONE,
        );

        let mut binder = Self {
            options,
            symbols,
            current_scope: SymbolTable::new(),
            scope_stack: Vec::new(),
            file_locals,
            expando_properties,
            declared_modules: FxHashSet::default(),
            is_external_module: false,
            is_strict_scope: false,
            flow_nodes,
            current_flow: FlowNodeId::NONE,
            unreachable_flow,
            scope_chain: Vec::new(),
            current_scope_idx: 0,
            node_symbols,
            module_declaration_exports_publicly,
            symbol_arenas,
            declaration_arenas,
            cross_file_node_symbols,
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
            augmentation_target_modules,
            lib_binders: Vec::new(),
            lib_symbol_ids: FxHashSet::default(),
            lib_symbol_reverse_remap: FxHashMap::default(),
            module_exports,
            reexports,
            wildcard_reexports,
            wildcard_reexports_type_only,
            resolved_export_cache: Default::default(),
            resolved_identifier_cache: Default::default(),
            shorthand_ambient_modules,
            modules_with_export_equals,
            module_export_equals_non_module: FxHashMap::default(),
            lib_symbols_merged: false,
            break_targets: Vec::new(),
            continue_targets: Vec::new(),
            return_targets: Vec::new(),
            file_features: FileFeatures::NONE,
            alias_partners,
            semantic_defs: FxHashMap::default(),
            file_import_sources: Vec::new(),
            file_idx: u32::MAX,
        };
        if let Some(root_scope) = binder.scopes.first() {
            binder.current_scope = root_scope.table.clone();
            let mut root_context =
                ScopeContext::new(root_scope.kind, root_scope.container_node, None);
            root_context.locals = root_scope.table.clone();
            binder.scope_chain.push(root_context);
            binder.current_scope_id = ScopeId(0);
            binder.current_scope_idx = 0;
        }
        binder.recompute_module_export_equals_non_module();
        binder
    }

    /// Enter a new persistent scope (in addition to legacy scope chain).
    /// This method is called when binding begins for a scope-creating node.
    pub(crate) fn enter_persistent_scope(&mut self, kind: ContainerKind, node: NodeIndex) {
        // Create new scope linked to current
        let new_scope_id =
            ScopeId(u32::try_from(self.scopes.len()).expect("persistent scope count exceeds u32"));
        let new_scope = Scope::new(self.current_scope_id, kind, node);
        self.scopes.push(new_scope);

        // Map node to this scope
        if node.is_some() {
            self.node_scope_ids.insert(node.0, new_scope_id);
        }

        // Update current scope
        self.current_scope_id = new_scope_id;
    }

    /// Exit the current persistent scope.
    pub(crate) fn exit_persistent_scope(&mut self) {
        if self.current_scope_id.is_some()
            && let Some(scope) = self.scopes.get(self.current_scope_id.0 as usize)
        {
            self.current_scope_id = scope.parent;
        }
    }

    /// Declare a symbol in the current persistent scope.
    /// This adds the symbol to the persistent scope table for later querying.
    /// Skipped during module augmentation to prevent augmented symbols from
    /// leaking into the augmenting file's scope (and subsequently into `file_locals/globals`).
    pub(crate) fn declare_in_persistent_scope(&mut self, name: String, sym_id: SymbolId) {
        if self.in_module_augmentation {
            return;
        }
        if self.current_scope_id.is_some()
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

    pub(crate) fn source_file_is_external_module(arena: &NodeArena, root: NodeIndex) -> bool {
        let Some(source) = arena.get_source_file_at(root) else {
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
                | syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    return true;
                }
                _ => {}
            }
            if Self::is_node_exported(arena, stmt_idx) {
                return true;
            }
        }

        if Self::source_file_contains_import_meta(arena, root) {
            return true;
        }

        // Check for CommonJS module indicator: `module.exports = ...` or `exports.x = ...`
        Self::source_file_has_commonjs_indicator(arena, &source.statements.nodes)
    }

    /// Check if any top-level statement is a CommonJS module.exports or exports.x assignment.
    /// This detects patterns like:
    /// - `module.exports = { ... }`
    /// - `module.exports.x = ...`
    /// - `exports.x = ...`
    fn source_file_has_commonjs_indicator(arena: &NodeArena, stmts: &[NodeIndex]) -> bool {
        for &stmt_idx in stmts {
            if stmt_idx.is_none() {
                continue;
            }
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = arena.get_expression_statement(stmt) else {
                continue;
            };
            let Some(expr_node) = arena.get(expr_stmt.expression) else {
                continue;
            };
            // Check for assignment: `lhs = rhs`
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = arena.get_binary_expr(expr_node) else {
                continue;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }
            // Check left side for `module.exports` or `exports.x` pattern
            if Self::is_commonjs_export_target(arena, binary.left) {
                return true;
            }
        }
        false
    }

    /// Check if a node is a CommonJS export target: `module.exports`, `module.exports.x`, or `exports.x`.
    fn is_commonjs_export_target(arena: &NodeArena, idx: NodeIndex) -> bool {
        let Some(node) = arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = arena.get_access_expr(node) else {
            return false;
        };

        // Check for `module.exports` (name_or_argument is "exports", expression is "module")
        let Some(expr_node) = arena.get(access.expression) else {
            return false;
        };

        if let Some(expr_id) = arena.get_identifier(expr_node) {
            let expr_name = &expr_id.escaped_text;
            if let Some(name_node) = arena.get(access.name_or_argument)
                && let Some(name_id) = arena.get_identifier(name_node)
            {
                // `module.exports` or `module.exports = ...`
                if expr_name == "module" && name_id.escaped_text == "exports" {
                    return true;
                }
                // `exports.x` (any property assignment on `exports`)
                if expr_name == "exports" {
                    return true;
                }
            }
        }

        // Check for `module.exports.x` (expression is `module.exports`)
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(inner_access) = arena.get_access_expr(expr_node)
            && let Some(inner_expr) = arena.get(inner_access.expression)
            && let Some(inner_id) = arena.get_identifier(inner_expr)
            && inner_id.escaped_text == "module"
            && let Some(inner_name) = arena.get(inner_access.name_or_argument)
            && let Some(inner_name_id) = arena.get_identifier(inner_name)
            && inner_name_id.escaped_text == "exports"
        {
            return true;
        }

        false
    }

    pub(crate) fn source_file_contains_import_meta(arena: &NodeArena, root: NodeIndex) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            if idx.is_none() {
                continue;
            }
            let Some(node) = arena.get(idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = arena.get_access_expr(node)
                && let Some(expr_node) = arena.get(access.expression)
                && expr_node.kind == tsz_scanner::SyntaxKind::ImportKeyword as u16
            {
                return true;
            }

            // Add children to stack
            for child in arena.get_children(idx) {
                stack.push(child);
            }
        }

        false
    }

    /// Check if a list of statements starts with a "use strict" prologue directive.
    /// Prologue directives are string literal expression statements at the top of a scope.
    fn has_use_strict_prologue(arena: &NodeArena, stmts: &[NodeIndex]) -> bool {
        for &stmt_idx in stmts {
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                break; // Prologues must be at the top
            }
            let Some(expr_stmt) = arena.get_expression_statement(stmt) else {
                break;
            };
            let Some(expr) = arena.get(expr_stmt.expression) else {
                break;
            };
            if expr.kind == SyntaxKind::StringLiteral as u16 {
                if let Some(lit) = arena.get_literal(expr)
                    && lit.text == "use strict"
                {
                    return true;
                }
            } else {
                break; // Non-string expression, stop looking for prologues
            }
        }
        false
    }

    /// Bind a source file using `NodeArena`.
    /// # Panics
    ///
    /// Panics if the resolved identifier cache lock is poisoned.
    pub fn bind_source_file(&mut self, arena: &NodeArena, root: NodeIndex) {
        if let Some(node) = arena.get(root)
            && let Some(sf) = arena.get_source_file(node)
        {
            self.set_debug_file(&sf.file_name);
        }

        // Binding mutates scope/symbol tables, so stale identifier resolution entries
        // from prior passes must be dropped.
        self.resolved_identifier_cache
            .write()
            .expect("RwLock not poisoned")
            .clear();

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
        self.is_external_module = Self::source_file_is_external_module(arena, root);

        if let Some(node) = arena.get(root)
            && let Some(sf) = arena.get_source_file(node)
        {
            // Detect strict mode: "use strict" prologue or --alwaysStrict option
            self.is_strict_scope = self.options.always_strict
                || Self::has_use_strict_prologue(arena, &sf.statements.nodes);

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

            // Re-process `export = X` statements that may have failed on the first
            // pass due to forward-reference ordering (e.g., `export = React` appears
            // before `declare namespace React { ... }`). All declarations are bound
            // now, so the target name is resolvable in current_scope.
            self.resolve_deferred_export_assignment(arena, &sf.statements.nodes);

            // Re-process `export { X, Y }` statements that may have failed on
            // the first pass due to forward references (e.g., `export { Hash }`
            // appearing before `interface Hash<T> { ... }`). All declarations
            // are bound now, so we can mark them as exported.
            self.resolve_deferred_named_exports(arena, &sf.statements.nodes);

            // Populate module_exports for cross-file import resolution
            // This enables type-only import elision and proper import validation
            let file_name = sf.file_name.clone();
            self.populate_module_exports_from_file_symbols(arena, &file_name);
            self.recompute_module_export_equals_non_module();
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

        // Stamp all non-lib symbols with the driver-assigned file_idx.
        // This enables per-file invalidation in the DefinitionStore.
        if self.file_idx != u32::MAX {
            self.stamp_file_idx();
        }
    }

    /// Stamp all symbols and `semantic_defs` with `self.file_idx`.
    ///
    /// Only stamps symbols whose `decl_file_idx` is still `u32::MAX` (i.e.,
    /// not already assigned by a multi-file merge). Lib symbols (tracked in
    /// `lib_symbol_ids`) are skipped to avoid overwriting their original
    /// file provenance.
    fn stamp_file_idx(&mut self) {
        let idx = self.file_idx;

        // Stamp symbols
        for sym in self.symbols.iter_mut() {
            if sym.decl_file_idx == u32::MAX && !self.lib_symbol_ids.contains(&sym.id) {
                sym.decl_file_idx = idx;
            }
        }

        // Stamp semantic_defs
        for entry in self.semantic_defs.values_mut() {
            if entry.file_id == u32::MAX {
                entry.file_id = idx;
            }
        }
    }

    /// Populate `module_exports` from file-level module symbols.
    ///
    /// This enables cross-file import resolution and type-only import elision.
    /// After binding a source file, we collect all module-level exports and
    /// add them to the `module_exports` table keyed by the file name.
    ///
    /// # Arguments
    /// * `arena` - The `NodeArena` containing the AST
    /// * `file_name` - The name of the file being bound (used as the key in `module_exports`)
    fn populate_module_exports_from_file_symbols(&mut self, _arena: &NodeArena, file_name: &str) {
        use crate::symbol_flags;

        // Collect all exports from all module-level symbols in this file
        // Start from any exports recorded during binding that intentionally do not create
        // file-local bindings (for example `export * as ns from "./mod"`).
        let mut file_exports = self.module_exports.remove(file_name).unwrap_or_default();
        let mut export_equals_target: Option<SymbolId> = None;

        // Iterate through file_locals to find modules and their exports
        for (name, &sym_id) in self.file_locals.iter() {
            // Skip lib/global symbols merged into file_locals from lib.d.ts.
            // These are global builtins (e.g. `escape`, `unescape`) that should
            // not appear in a user module's module_exports.
            if self.lib_symbol_ids.contains(&sym_id) {
                continue;
            }
            if name == "export=" {
                export_equals_target = Some(sym_id);
            }
            if let Some(symbol) = self.symbols.get(sym_id) {
                // Skip lib/global symbols merged into file_locals from lib.d.ts.
                // These are global builtins that should not appear in a user
                // module's module_exports.
                if self.lib_symbol_ids.contains(&sym_id) {
                    continue;
                }

                // Check if this is a module/namespace symbol
                if symbol.is_exported
                    && (symbol.flags
                        & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE))
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

                // Also collect symbols that are explicitly exported via `export { X }`
                // or `export` modifier. These may not be module/namespace symbols but
                // need to be in module_exports for cross-file import resolution.
                if symbol.is_exported || name == "export=" {
                    file_exports.set(name.clone(), sym_id);
                }
            }
        }

        // `export = target` should expose namespace members from `target`.
        if let Some(target_sym_id) = export_equals_target
            && let Some(target_symbol) = self.symbols.get(target_sym_id)
        {
            if let Some(target_exports) = target_symbol.exports.as_ref() {
                for (export_name, &export_sym_id) in target_exports.iter() {
                    if export_name != "default" && !file_exports.has(export_name) {
                        file_exports.set(export_name.clone(), export_sym_id);
                    }
                }
            }
            if let Some(target_members) = target_symbol.members.as_ref() {
                for (member_name, &member_sym_id) in target_members.iter() {
                    if member_name != "default" && !file_exports.has(member_name) {
                        file_exports.set(member_name.clone(), member_sym_id);
                    }
                }
            }
        }

        if !file_exports.is_empty() {
            self.module_exports
                .insert(file_name.to_string(), file_exports);
        }
    }

    /// Retry `export = X` binding for forward-reference cases.
    ///
    /// When a `.d.ts` file writes `export = React` before `declare namespace React { ... }`,
    /// the first-pass binding of the `export =` node fails to resolve `React` (because it
    /// hasn't been declared yet) and leaves `file_locals["export="]` unset. This method is
    /// called after ALL statements have been bound so every top-level declaration is in
    /// `current_scope`. If `file_locals["export="]` is still missing, we scan for the first
    /// `export = <Identifier>` statement and resolve it now.
    fn resolve_deferred_export_assignment(&mut self, arena: &NodeArena, statements: &[NodeIndex]) {
        // Fast path: already resolved during the main binding pass.
        if self.file_locals.has("export=") {
            return;
        }

        for &stmt_idx in statements {
            let Some(node) = arena.get(stmt_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::EXPORT_ASSIGNMENT {
                continue;
            }
            let Some(assign) = arena.get_export_assignment(node) else {
                continue;
            };
            if !assign.is_export_equals {
                continue; // skip `export default X`
            }
            let Some(name) = Self::get_identifier_name(arena, assign.expression) else {
                continue;
            };
            let Some(sym_id) = self
                .current_scope
                .get(name)
                .or_else(|| self.file_locals.get(name))
            else {
                continue;
            };

            self.file_locals.set("export=".to_string(), sym_id);

            // Also expose the namespace's own exports at file level so that
            // named imports like `import { Component } from 'react'` work.
            if let Some(symbol) = self.symbols.get(sym_id)
                && let Some(ref exports) = symbol.exports.clone()
            {
                let entries: Vec<(String, SymbolId)> =
                    exports.iter().map(|(k, &v)| (k.clone(), v)).collect();
                for (export_name, export_sym_id) in entries {
                    if self.file_locals.get(&export_name).is_none() {
                        self.file_locals.set(export_name, export_sym_id);
                    }
                }
            }

            break; // Only process the first `export =` statement.
        }
    }

    /// Re-process `export { X, Y }` (without `from`) statements for forward
    /// references. On the first pass the target symbols may not have been bound
    /// yet, so `is_exported` was never set. Now that all declarations are
    /// bound we can mark them as exported.
    fn resolve_deferred_named_exports(&mut self, arena: &NodeArena, statements: &[NodeIndex]) {
        use tsz_parser::parser::syntax_kind_ext;

        for &stmt_idx in statements {
            let Some(node) = arena.get(stmt_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = arena.get_export_decl(node) else {
                continue;
            };
            // Only handle local `export { X }`, not re-exports `export { X } from "mod"`
            if export.module_specifier.is_some() {
                continue;
            }
            if export.export_clause.is_none() {
                continue;
            }
            let Some(clause_node) = arena.get(export.export_clause) else {
                continue;
            };
            // get_named_imports is used for both NamedImports and NamedExports
            let Some(named) = arena.get_named_imports(clause_node) else {
                continue;
            };
            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = arena.get_specifier(spec_node) else {
                    continue;
                };
                // The original (local) name:
                // For `export { foo }`, property_name is NONE, name is "foo"
                // For `export { foo as bar }`, property_name is "foo", name is "bar"
                let orig_name = if spec.property_name.is_none() {
                    Self::get_identifier_name(arena, spec.name)
                } else {
                    Self::get_identifier_name(arena, spec.property_name)
                };
                let Some(orig) = orig_name else {
                    continue;
                };
                // Try to resolve the symbol now that all declarations are bound
                let resolved = self
                    .current_scope
                    .get(orig)
                    .or_else(|| self.file_locals.get(orig));
                if let Some(sym_id) = resolved
                    && let Some(sym) = self.symbols.get_mut(sym_id)
                    && !sym.is_exported
                {
                    sym.is_exported = true;
                }
            }
        }
    }

    fn symbol_has_namespace_shape(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.symbols.get(sym_id) else {
            return false;
        };

        if (symbol.flags
            & (symbol_flags::MODULE | symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
            != 0
        {
            return true;
        }

        if symbol.exports.as_ref().is_some_and(|tbl| !tbl.is_empty())
            || symbol.members.as_ref().is_some_and(|tbl| !tbl.is_empty())
        {
            return true;
        }

        let mut declarations = symbol.declarations.clone();
        if symbol.value_declaration.is_some() && !declarations.contains(&symbol.value_declaration) {
            declarations.push(symbol.value_declaration);
        }

        declarations.into_iter().any(|decl_idx| {
            if decl_idx.is_none() {
                return false;
            }
            let Some(arena) = self
                .declaration_arenas
                .get(&(sym_id, decl_idx))
                .and_then(|v| v.first())
            else {
                return false;
            };
            let Some(node) = arena.get(decl_idx) else {
                return false;
            };
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                return false;
            }
            let Some(module_decl) = arena.get_module(node) else {
                return false;
            };
            if module_decl.body.is_none() {
                return false;
            }
            let Some(body_node) = arena.get(module_decl.body) else {
                return false;
            };
            if body_node.kind == syntax_kind_ext::MODULE_BLOCK
                && let Some(block) = arena.get_module_block(body_node)
                && let Some(statements) = block.statements.as_ref()
            {
                return !statements.nodes.is_empty();
            }
            true
        })
    }

    fn compute_module_export_equals_non_module(&self, exports: &SymbolTable) -> Option<bool> {
        let export_assignment_targets = |sym: &Symbol| -> Vec<String> {
            let mut targets = Vec::new();
            let mut declarations = sym.declarations.clone();
            if sym.value_declaration.is_some() && !declarations.contains(&sym.value_declaration) {
                declarations.push(sym.value_declaration);
            }

            for decl_idx in declarations {
                if decl_idx.is_none() {
                    continue;
                }
                let Some(arena) = self
                    .declaration_arenas
                    .get(&(sym.id, decl_idx))
                    .and_then(|v| v.first())
                else {
                    continue;
                };
                let Some(node) = arena.get(decl_idx) else {
                    continue;
                };
                if node.kind != syntax_kind_ext::EXPORT_ASSIGNMENT {
                    continue;
                }
                let Some(assign) = arena.get_export_assignment(node) else {
                    continue;
                };
                if !assign.is_export_equals {
                    continue;
                }
                let Some(expr_node) = arena.get(assign.expression) else {
                    continue;
                };
                let Some(id) = arena.get_identifier(expr_node) else {
                    continue;
                };
                if !targets.contains(&id.escaped_text) {
                    targets.push(id.escaped_text.clone());
                }
            }

            targets
        };

        let export_equals_sym_id = exports.get("export=")?;

        let export_equals_symbol = self.symbols.get(export_equals_sym_id)?;

        let mut target_names = Vec::new();
        if !export_equals_symbol.escaped_name.is_empty() {
            target_names.push(export_equals_symbol.escaped_name.clone());
        }
        for target_name in export_assignment_targets(export_equals_symbol) {
            if !target_names.contains(&target_name) {
                target_names.push(target_name);
            }
        }

        let has_distinct_named_exports = exports.iter().any(|(name, _)| {
            name != "export=" && !target_names.iter().any(|target| target == name)
        });

        let mut candidate_ids = Vec::new();
        let mut push_candidate = |candidate_id: SymbolId| {
            if !candidate_ids.contains(&candidate_id) {
                candidate_ids.push(candidate_id);
            }
        };

        push_candidate(export_equals_sym_id);
        for target_name in &target_names {
            for &candidate_id in self.symbols.find_all_by_name(target_name) {
                push_candidate(candidate_id);
            }
        }

        let has_namespace_shape = candidate_ids
            .into_iter()
            .any(|candidate_id| self.symbol_has_namespace_shape(candidate_id));

        Some(!has_namespace_shape && !has_distinct_named_exports)
    }

    /// Recompute `export =` non-module classification for all known module exports.
    pub fn recompute_module_export_equals_non_module(&mut self) {
        self.module_export_equals_non_module.clear();
        for (module_name, exports) in self.module_exports.clone() {
            if let Some(non_module) = self.compute_module_export_equals_non_module(&exports) {
                self.module_export_equals_non_module
                    .insert(module_name, non_module);
            }
        }
    }

    /// Merge lib file symbols into the current scope.
    ///
    /// This is called during binder initialization to ensure global symbols
    /// from lib.d.ts (like `Object`, `Function`, `console`, etc.) are available
    /// during type checking.
    ///
    /// This method now uses `merge_lib_contexts_into_binder` which properly
    /// remaps `SymbolIds` to avoid collisions across lib binders.
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
    /// # Panics
    ///
    /// Panics if the resolved identifier cache lock is poisoned.
    pub fn merge_lib_symbols(&mut self, lib_files: &[Arc<lib_loader::LibFile>]) {
        // Merging lib globals changes visible symbols, so invalidate identifier cache.
        self.resolved_identifier_cache
            .write()
            .expect("RwLock not poisoned")
            .clear();

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
    /// - `arena`: The `NodeArena` containing the AST
    /// - `root`: The root node index of the source file
    /// - `lib_files`: Optional slice of Arc<LibFile> containing lib files
    /// # Panics
    ///
    /// Panics if the resolved identifier cache lock is poisoned.
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
    /// # Panics
    ///
    /// Panics if the resolved identifier cache lock is poisoned.
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
        self.resolved_identifier_cache
            .write()
            .expect("RwLock not poisoned")
            .clear();

        let Some(&last_prefix) = prefix_statements.last() else {
            return false;
        };
        let Some(&start_flow) = self.top_level_flow.get(&last_prefix.0) else {
            return false;
        };
        if self.scopes.is_empty() {
            return false;
        }

        self.is_external_module = Self::source_file_is_external_module(arena, root);

        // Detect strict mode for incremental rebinding
        if let Some(node) = arena.get(root)
            && let Some(sf) = arena.get_source_file(node)
        {
            self.is_strict_scope = self.options.always_strict
                || Self::has_use_strict_prologue(arena, &sf.statements.nodes);
        }

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
                sym.first_declaration_span = sym
                    .declarations
                    .first()
                    .and_then(|decl| arena.get(*decl).map(|n| (n.pos, n.end)));
                if sym.value_declaration == node {
                    sym.value_declaration =
                        sym.declarations.first().copied().unwrap_or(NodeIndex::NONE);
                    sym.value_declaration_span = if sym.value_declaration.is_some() {
                        arena.get(sym.value_declaration).map(|n| (n.pos, n.end))
                    } else {
                        None
                    };
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

        // Stamp any newly created symbols with the driver-assigned file_idx.
        if self.file_idx != u32::MAX {
            self.stamp_file_idx();
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
}
