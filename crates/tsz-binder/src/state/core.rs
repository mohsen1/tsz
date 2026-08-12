//! Core implementation of `BinderState` methods.
//!
//! Extracted from `mod.rs` to follow the thin-mod.rs pattern.

use super::{BinderState, BinderStateScopeInputs};
use crate::modules::resolution_debug::ModuleResolutionDebugger;
use crate::{
    ContainerKind, FlowNodeArena, FlowNodeId, Scope, ScopeId, Symbol, SymbolArena, SymbolId,
    SymbolTable, flow_flags, symbol_flags,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::{BinderOptions, FileFeatures};
use tsz_common::options::module_detection::ModuleDetectionKind;

/// Returns true if the file extension forces module semantics (.mts, .cts,
/// .mjs, .cjs) under `moduleDetection: auto`.
///
/// Mirrors `tsc`'s `isFileForcedToBeModuleByFormat`, which is explicitly
/// declaration-file-aware: `.d.mts` and `.d.cts` are *not* forced, because a
/// declaration file with no module syntax still declares globals. Callers that
/// want the format-blind rule want `file_has_module_syntax_indicator` instead.
pub(super) fn is_module_file_extension(file_name: &str) -> bool {
    if is_declaration_file_name(file_name) {
        return false;
    }
    // Check for .mts, .cts (TypeScript module extensions)
    // and .mjs, .cjs (JavaScript module extensions)
    file_name.ends_with(".mts")
        || file_name.ends_with(".cts")
        || file_name.ends_with(".mjs")
        || file_name.ends_with(".cjs")
}

/// Returns true for a declaration file name (`.d.ts`, `.d.mts`, `.d.cts`).
///
/// Mirrors `tsc`'s `SourceFile.isDeclarationFile`, which both
/// `isFileForcedToBeModuleByFormat` and the `moduleDetection: force` rule
/// consult: a declaration file is never made a module by its extension or by
/// `force`, only by carrying module syntax of its own.
fn is_declaration_file_name(file_name: &str) -> bool {
    file_name.ends_with(".d.ts") || file_name.ends_with(".d.mts") || file_name.ends_with(".d.cts")
}

pub(super) fn is_js_like_file_name(file_name: &str) -> bool {
    file_name.ends_with(".js")
        || file_name.ends_with(".jsx")
        || file_name.ends_with(".mjs")
        || file_name.ends_with(".cjs")
}

pub(super) const fn next_persistent_scope_id(scope_count: usize) -> Option<ScopeId> {
    // `ScopeId(u32::MAX)` is reserved as `ScopeId::NONE`, so valid persistent
    // scope IDs are limited to `0..u32::MAX`.
    if scope_count >= u32::MAX as usize {
        return None;
    }
    Some(ScopeId(scope_count as u32))
}

impl BinderStateScopeInputs {
    pub(super) fn with_scopes(
        scopes: Arc<Vec<Scope>>,
        node_scope_ids: Arc<FxHashMap<u32, ScopeId>>,
    ) -> Self {
        Self {
            scopes,
            node_scope_ids,
            flow_nodes: Arc::new(FlowNodeArena::new()),
            ..Self::default()
        }
    }
}

impl BinderState {
    /// Whether a file-local `name -> sym_id` entry owned by `file_idx` is
    /// visible in the cross-file global scope (the checker's
    /// `global_file_locals_index` fallback).
    ///
    /// Mirrors [`Symbol::is_cross_file_global`] so that index agrees with
    /// `program.globals`: a module's top-level exports (value or type) stay
    /// file-scoped and are reachable only through an explicit import. Only
    /// symbols genuinely declared in this file are classified; lib symbols and
    /// globals folded in from other files (which carry a different
    /// `decl_file_idx`) are always kept, so lib/global resolution is
    /// unaffected. A raw, unmerged per-file binder leaves
    /// `decl_file_idx == u32::MAX` while owning all of its locals.
    #[must_use]
    pub fn cross_file_local_is_visible(
        &self,
        file_idx: usize,
        name: &str,
        sym_id: SymbolId,
    ) -> bool {
        let Some(sym) = self.get_symbol(sym_id) else {
            return true;
        };
        if self.lib_symbol_ids.contains(&sym_id) {
            return true;
        }
        if sym.decl_file_idx != u32::MAX && sym.decl_file_idx != file_idx as u32 {
            return true;
        }
        sym.is_cross_file_global(
            self.is_external_module,
            self.global_augmentations.contains_key(name),
        )
    }

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
            file_locals: SymbolTable::new(),
            program_globals: SymbolTable::new(),
            expando_properties: Arc::new(FxHashMap::default()),
            expando_host_members: FxHashMap::default(),
            declared_modules: Arc::new(FxHashSet::default()),
            is_external_module: false,
            is_strict_scope: false,
            flow_nodes: Arc::new(flow_nodes),
            current_flow: FlowNodeId::NONE,
            unreachable_flow,
            node_symbols: Arc::new(FxHashMap::with_capacity_and_hasher(256, Default::default())),
            module_declaration_exports_publicly: Arc::new(FxHashMap::default()),
            symbol_arenas: Arc::new(FxHashMap::default()),
            declaration_arenas: Arc::new(FxHashMap::default()),
            sym_to_decl_indices: Arc::new(FxHashMap::default()),
            cross_file_node_symbols: Arc::new(FxHashMap::default()),
            node_flow: Arc::new(FxHashMap::with_capacity_and_hasher(128, Default::default())),
            top_level_flow: Arc::new(FxHashMap::default()),
            switch_clause_to_switch: Arc::new(FxHashMap::default()),
            hoisted_vars: Vec::new(),
            hoisted_functions: Vec::new(),
            scopes: Arc::new(Vec::with_capacity(32)),
            node_scope_ids: Arc::new(FxHashMap::with_capacity_and_hasher(64, Default::default())),
            current_scope_id: ScopeId::NONE,
            debugger: ModuleResolutionDebugger::new(),
            global_augmentations: Arc::new(FxHashMap::default()),
            in_global_augmentation: false,
            module_augmentations: Arc::new(FxHashMap::default()),
            in_module_augmentation: false,
            current_augmented_module: None,
            augmentation_target_modules: Arc::new(FxHashMap::default()),
            module_augmentation_symbols: FxHashMap::default(),
            lib_binders: Arc::new(Vec::new()),
            lib_symbol_ids: Arc::new(FxHashSet::default()),
            lib_symbol_reverse_remap: Arc::new(FxHashMap::default()),
            lib_type_namespace: Arc::new(FxHashMap::default()),
            module_exports: Arc::new(FxHashMap::default()),
            reexports: Arc::new(FxHashMap::default()),
            wildcard_reexports: Arc::new(FxHashMap::default()),
            resolved_export_cache: Default::default(),
            resolved_export_type_only_cache: Default::default(),
            resolved_identifier_cache: Default::default(),
            find_enclosing_scope_cache: Default::default(),
            shorthand_ambient_modules: Arc::new(FxHashSet::default()),
            module_export_equals_non_module: FxHashMap::default(),
            lib_symbols_merged: false,
            break_targets: Vec::new(),
            continue_targets: Vec::new(),
            return_targets: Vec::new(),
            finally_entry_targets: Vec::new(),
            file_features: FileFeatures::NONE,
            alias_partners: Arc::new(FxHashMap::default()),
            semantic_defs: Arc::new(FxHashMap::default()),
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
        self.file_locals.clear();
        self.program_globals.clear();
        Arc::make_mut(&mut self.expando_properties).clear();
        self.expando_host_members.clear();
        Arc::make_mut(&mut self.declared_modules).clear();
        self.is_external_module = false;
        self.is_strict_scope = false;
        {
            let flow_nodes = Arc::make_mut(&mut self.flow_nodes);
            flow_nodes.clear();
            self.unreachable_flow = flow_nodes.alloc(flow_flags::UNREACHABLE);
        }
        self.current_flow = FlowNodeId::NONE;
        Arc::make_mut(&mut self.node_symbols).clear();
        Arc::make_mut(&mut self.module_declaration_exports_publicly).clear();
        Arc::make_mut(&mut self.symbol_arenas).clear();
        Arc::make_mut(&mut self.declaration_arenas).clear();
        Arc::make_mut(&mut self.sym_to_decl_indices).clear();
        Arc::make_mut(&mut self.cross_file_node_symbols).clear();
        Arc::make_mut(&mut self.node_flow).clear();
        Arc::make_mut(&mut self.top_level_flow).clear();
        Arc::make_mut(&mut self.switch_clause_to_switch).clear();
        self.hoisted_vars.clear();
        self.hoisted_functions.clear();
        Arc::make_mut(&mut self.scopes).clear();
        Arc::make_mut(&mut self.node_scope_ids).clear();
        self.current_scope_id = ScopeId::NONE;
        self.debugger.clear();
        Arc::make_mut(&mut self.global_augmentations).clear();
        self.in_global_augmentation = false;
        Arc::make_mut(&mut self.module_augmentations).clear();
        self.in_module_augmentation = false;
        self.current_augmented_module = None;
        self.module_augmentation_symbols.clear();
        Arc::make_mut(&mut self.lib_binders).clear();
        Arc::make_mut(&mut self.lib_symbol_ids).clear();
        Arc::make_mut(&mut self.lib_symbol_reverse_remap).clear();
        Arc::make_mut(&mut self.lib_type_namespace).clear();
        Arc::make_mut(&mut self.module_exports).clear();
        Arc::make_mut(&mut self.reexports).clear();
        Arc::make_mut(&mut self.wildcard_reexports).clear();
        self.clear_resolution_caches();
        Arc::make_mut(&mut self.shorthand_ambient_modules).clear();
        self.module_export_equals_non_module.clear();
        self.lib_symbols_merged = false;
        self.break_targets.clear();
        self.continue_targets.clear();
        self.return_targets.clear();
        Arc::make_mut(&mut self.semantic_defs).clear();
        Arc::make_mut(&mut self.alias_partners).clear();
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

    /// Resolve the arena that owns a declaration, falling back to a caller-provided
    /// arena when no cross-file mapping exists.
    ///
    /// Callers frequently need the concrete `&NodeArena` that a declaration was
    /// parsed into (e.g. to read its `kind`, children, or identifier text) and
    /// want to default to the arena they are currently iterating over if the
    /// declaration is purely local. This helper collapses the common
    /// `get_arena_for_declaration(..).map_or(fallback, |arc| arc.as_ref())`
    /// pattern into one call.
    #[inline]
    pub fn arena_for_declaration_or<'a>(
        &'a self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
        fallback: &'a NodeArena,
    ) -> &'a NodeArena {
        self.get_arena_for_declaration(sym_id, decl_idx)
            .map_or(fallback, Arc::as_ref)
    }

    /// Create a `BinderState` from pre-parsed lib data.
    ///
    /// This is used for loading pre-parsed lib files where we only have
    /// symbols and `file_locals` (no `node_symbols` or other binding state).
    #[must_use]
    pub fn from_preparsed(symbols: SymbolArena, file_locals: SymbolTable) -> Self {
        Self::from_bound_state(symbols, file_locals, Arc::new(FxHashMap::default()))
    }

    /// Create a `BinderState` from existing bound state.
    ///
    /// This is used for type checking after parallel binding and symbol merging.
    /// The symbols and `node_symbols` come from the merged program state.
    #[must_use]
    pub fn from_bound_state(
        symbols: SymbolArena,
        file_locals: SymbolTable,
        node_symbols: Arc<FxHashMap<u32, SymbolId>>,
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
        node_symbols: Arc<FxHashMap<u32, SymbolId>>,
    ) -> Self {
        let mut flow_nodes = FlowNodeArena::new();
        let unreachable_flow = flow_nodes.alloc(flow_flags::UNREACHABLE);

        let mut binder = Self {
            options,
            symbols,
            file_locals,
            program_globals: SymbolTable::new(),
            expando_properties: Arc::new(FxHashMap::default()),
            expando_host_members: FxHashMap::default(),
            declared_modules: Arc::new(FxHashSet::default()),
            is_external_module: false,
            is_strict_scope: false,
            flow_nodes: Arc::new(flow_nodes),
            current_flow: FlowNodeId::NONE,
            unreachable_flow,
            node_symbols,
            module_declaration_exports_publicly: Arc::new(FxHashMap::default()),
            symbol_arenas: Arc::new(FxHashMap::default()),
            declaration_arenas: Arc::new(FxHashMap::default()),
            sym_to_decl_indices: Arc::new(FxHashMap::default()),
            cross_file_node_symbols: Arc::new(FxHashMap::default()),
            node_flow: Arc::new(FxHashMap::default()),
            top_level_flow: Arc::new(FxHashMap::default()),
            switch_clause_to_switch: Arc::new(FxHashMap::default()),
            hoisted_vars: Vec::new(),
            hoisted_functions: Vec::new(),
            scopes: Arc::new(Vec::new()),
            node_scope_ids: Arc::new(FxHashMap::default()),
            current_scope_id: ScopeId::NONE,
            debugger: ModuleResolutionDebugger::new(),
            global_augmentations: Arc::new(FxHashMap::default()),
            in_global_augmentation: false,
            module_augmentations: Arc::new(FxHashMap::default()),
            in_module_augmentation: false,
            current_augmented_module: None,
            augmentation_target_modules: Arc::new(FxHashMap::default()),
            module_augmentation_symbols: FxHashMap::default(),
            lib_binders: Arc::new(Vec::new()),
            lib_symbol_ids: Arc::new(FxHashSet::default()),
            lib_symbol_reverse_remap: Arc::new(FxHashMap::default()),
            lib_type_namespace: Arc::new(FxHashMap::default()),
            module_exports: Arc::new(FxHashMap::default()),
            reexports: Arc::new(FxHashMap::default()),
            wildcard_reexports: Arc::new(FxHashMap::default()),
            resolved_export_cache: Default::default(),
            resolved_export_type_only_cache: Default::default(),
            resolved_identifier_cache: Default::default(),
            find_enclosing_scope_cache: Default::default(),
            shorthand_ambient_modules: Arc::new(FxHashSet::default()),
            module_export_equals_non_module: FxHashMap::default(),
            lib_symbols_merged: false,
            break_targets: Vec::new(),
            continue_targets: Vec::new(),
            return_targets: Vec::new(),
            finally_entry_targets: Vec::new(),
            file_features: FileFeatures::NONE,
            alias_partners: Arc::new(FxHashMap::default()),
            semantic_defs: Arc::new(FxHashMap::default()),
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
        node_symbols: Arc<FxHashMap<u32, SymbolId>>,
        scopes: Arc<Vec<Scope>>,
        node_scope_ids: Arc<FxHashMap<u32, ScopeId>>,
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
        node_symbols: Arc<FxHashMap<u32, SymbolId>>,
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
            symbol_arenas,
            declaration_arenas,
            sym_to_decl_indices,
            cross_file_node_symbols,
            shorthand_ambient_modules,
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
            file_locals,
            program_globals: SymbolTable::new(),
            expando_properties,
            expando_host_members: FxHashMap::default(),
            declared_modules: Arc::new(FxHashSet::default()),
            is_external_module: false,
            is_strict_scope: false,
            flow_nodes,
            current_flow: FlowNodeId::NONE,
            unreachable_flow,
            node_symbols,
            module_declaration_exports_publicly,
            symbol_arenas,
            declaration_arenas,
            sym_to_decl_indices,
            cross_file_node_symbols,
            node_flow,
            top_level_flow: Arc::new(FxHashMap::default()),
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
            module_augmentation_symbols: FxHashMap::default(),
            lib_binders: Arc::new(Vec::new()),
            lib_symbol_ids: Arc::new(FxHashSet::default()),
            lib_symbol_reverse_remap: Arc::new(FxHashMap::default()),
            lib_type_namespace: Arc::new(FxHashMap::default()),
            module_exports,
            reexports,
            wildcard_reexports,
            resolved_export_cache: Default::default(),
            resolved_export_type_only_cache: Default::default(),
            resolved_identifier_cache: Default::default(),
            find_enclosing_scope_cache: Default::default(),
            shorthand_ambient_modules,
            module_export_equals_non_module: FxHashMap::default(),
            lib_symbols_merged: false,
            break_targets: Vec::new(),
            continue_targets: Vec::new(),
            return_targets: Vec::new(),
            finally_entry_targets: Vec::new(),
            file_features: FileFeatures::NONE,
            alias_partners,
            semantic_defs: Arc::new(FxHashMap::default()),
            file_import_sources: Vec::new(),
            file_idx: u32::MAX,
        };
        if !binder.scopes.is_empty() {
            binder.current_scope_id = ScopeId(0);
        }
        binder.recompute_module_export_equals_non_module();
        binder
    }

    /// Enter a new persistent scope (in addition to legacy scope chain).
    /// This method is called when binding begins for a scope-creating node.
    #[expect(dead_code)]
    pub(crate) fn enter_persistent_scope(&mut self, kind: ContainerKind, node: NodeIndex) {
        self.enter_persistent_scope_with_capacity(kind, node, 0);
    }

    /// Enter a persistent scope with a pre-allocated symbol table capacity.
    /// This avoids hash map resizing for scopes where the approximate member
    /// count is known (e.g., class bodies).
    pub(crate) fn enter_persistent_scope_with_capacity(
        &mut self,
        kind: ContainerKind,
        node: NodeIndex,
        capacity: usize,
    ) {
        // Create new scope linked to current
        let Some(new_scope_id) = next_persistent_scope_id(self.scopes.len()) else {
            tracing::warn!(
                scope_count = self.scopes.len(),
                "persistent scope count exceeded representable ScopeId range; skipping scope push"
            );
            return;
        };
        let new_scope = if capacity > 0 {
            Scope::with_capacity(self.current_scope_id, kind, node, capacity)
        } else {
            Scope::new(self.current_scope_id, kind, node)
        };
        Arc::make_mut(&mut self.scopes).push(new_scope);

        // Map node to this scope
        if node.is_some() {
            Arc::make_mut(&mut self.node_scope_ids).insert(node.0, new_scope_id);
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

    /// The persistent scope currently being bound (`scopes[current_scope_id]`).
    pub(crate) fn current_persistent_scope(&self) -> Option<&Scope> {
        if self.current_scope_id.is_none() {
            return None;
        }
        self.scopes.get(self.current_scope_id.0 as usize)
    }

    /// The symbol table of the scope currently being bound.
    ///
    /// This is the single live declaration table: `scopes[current_scope_id].table`.
    /// When no scope is active (pre-bind root state) it returns a shared empty
    /// table so callers can `.get`/`.has`/`.iter` without a `None` branch.
    pub fn current_scope(&self) -> &SymbolTable {
        static EMPTY: std::sync::OnceLock<SymbolTable> = std::sync::OnceLock::new();
        self.current_persistent_scope()
            .map(|scope| &scope.table)
            .unwrap_or_else(|| EMPTY.get_or_init(SymbolTable::new))
    }

    /// Mutable handle to the scope currently being bound, if one is active.
    pub(crate) fn current_scope_mut(&mut self) -> Option<&mut SymbolTable> {
        if self.current_scope_id.is_none() {
            return None;
        }
        Arc::make_mut(&mut self.scopes)
            .get_mut(self.current_scope_id.0 as usize)
            .map(|scope| &mut scope.table)
    }

    /// The nearest enclosing scope that is a *declaration container*, i.e. the
    /// scope `tsc` would record a non-block-scoped declaration in.
    ///
    /// `tsc` keeps two separate cursors while binding: `container` (source
    /// file, module body, class body, or any function-like node) and
    /// `blockScopeContainer` (additionally every plain `Block`). Only
    /// block-scoped declarations — `let`, `const`, `class`, and friends — land
    /// in the block scope; everything else is declared in the container. tsz
    /// models both cursors with the single `current_scope_id`, so a caller that
    /// needs the container half asks for it here.
    ///
    /// The walk skips `ContainerKind::Block` scopes, with one exception: a
    /// class static block is function-like in `tsc` and therefore *is* a
    /// container, even though tsz gives its body a block scope. Stopping there
    /// keeps a declaration inside a static block out of the class body's table.
    pub(crate) fn nearest_declaration_container_scope(&self, arena: &NodeArena) -> ScopeId {
        let mut scope_id = self.current_scope_id;
        while let Some(scope) = self.scopes.get(scope_id.0 as usize) {
            if scope.kind != ContainerKind::Block || scope.parent.is_none() {
                break;
            }
            let is_static_block = arena
                .get(scope.container_node)
                .is_some_and(|node| node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION);
            if is_static_block {
                break;
            }
            scope_id = scope.parent;
        }
        scope_id
    }

    /// Run `f` with the declaration cursor retargeted to `scope_id`.
    ///
    /// Every declaration path reads `current_scope_id` — `current_scope`,
    /// `current_scope_mut`, `current_container_symbol` and
    /// `declare_in_persistent_scope_with_atom` all resolve through it — so
    /// swapping it for the duration of `f` retargets the whole path, including
    /// the merge/duplicate lookup, rather than only the final table write.
    pub(crate) fn with_declaration_scope<R>(
        &mut self,
        scope_id: ScopeId,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let saved = std::mem::replace(&mut self.current_scope_id, scope_id);
        let result = f(self);
        self.current_scope_id = saved;
        result
    }

    /// Symbol of the container node owning the current persistent scope
    /// (namespace, class, function, ...), if one has been bound.
    pub(crate) fn current_container_symbol(&self) -> Option<SymbolId> {
        self.current_persistent_scope()
            .and_then(|scope| self.get_node_symbol(scope.container_node))
    }

    /// Declare a symbol in the current scope's table.
    ///
    /// `scopes[current_scope_id].table` is the single declaration target.
    /// Module-augmentation isolation is handled by the boundary-scope
    /// save/restore in `modules::binding` (the augmented body binds in-place
    /// at the parent scope, whose table is snapshotted and restored), so this
    /// no longer needs a special augmentation skip.
    pub(crate) fn declare_in_persistent_scope(&mut self, name: String, sym_id: SymbolId) {
        self.declare_in_persistent_scope_with_atom(name, None, sym_id);
    }

    pub(crate) fn declare_in_persistent_scope_with_atom(
        &mut self,
        name: String,
        atom_key: Option<(usize, tsz_common::interner::AstAtom)>,
        sym_id: SymbolId,
    ) {
        if let Some(table) = self.current_scope_mut() {
            table.set_with_atom(name, atom_key, sym_id);
        }
    }

    /// Decide whether `root`'s source file is an external module under the
    /// binder's resolved `moduleDetection` setting.
    ///
    /// Mirrors `tsc`'s `getSetExternalModuleIndicator`, which dispatches on
    /// `getEmitModuleDetectionKind(options)` and installs one of three
    /// predicates. Only the `Auto` arm consults file format; `Legacy` is module
    /// syntax alone, and `Force` makes every non-declaration file a module.
    pub(crate) fn detect_external_module(&self, arena: &NodeArena, root: NodeIndex) -> bool {
        match self.options.module_detection {
            ModuleDetectionKind::Auto => Self::source_file_is_external_module(arena, root),
            ModuleDetectionKind::Legacy => Self::file_has_module_syntax_indicator(arena, root),
            ModuleDetectionKind::Force => {
                let Some(source) = arena.get_source_file_at(root) else {
                    return false;
                };
                !is_declaration_file_name(&source.file_name)
                    || Self::file_has_module_syntax_indicator(arena, root)
            }
        }
    }

    /// Whether module-ness may be forced by file format (`moduleDetection: auto`).
    pub(crate) const fn auto_module_detection(&self) -> bool {
        matches!(self.options.module_detection, ModuleDetectionKind::Auto)
    }

    /// `tsc`'s `isFileProbablyExternalModule`: does the file carry module
    /// syntax of its own?
    ///
    /// An import/export declaration, an `import ... = require(...)`, an
    /// `export =`, any exported declaration, or `import.meta`. Deliberately
    /// format-blind — file extensions are the `Auto` arm's business.
    ///
    /// A `NamespaceExportDeclaration` (`export as namespace N;`) is
    /// deliberately NOT an indicator, matching tsc's
    /// `isAnExternalModuleIndicatorNode`, which lists only import
    /// declarations, external `import =` references, `export =`, export
    /// declarations, and nodes carrying an `export` modifier. The omission is
    /// load-bearing rather than an oversight: `checkNamespaceExportDeclaration`
    /// reports TS1314 (`Global module exports may only appear in module
    /// files.`) precisely for the file whose only export-shaped syntax is the
    /// `export as namespace` itself, and that diagnostic is unreachable if the
    /// declaration is allowed to make its own file a module. Real UMD
    /// declaration files pair it with an `export =` or `export {}`, which are
    /// indicators on their own, so this exclusion only reclassifies files that
    /// tsc rejects outright.
    fn file_has_module_syntax_indicator(arena: &NodeArena, root: NodeIndex) -> bool {
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
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    return true;
                }
                // Only `import X = require("...")` (an external module
                // reference) is a module indicator, matching tsc's
                // `isAnExternalModuleIndicatorNode`
                // (`isImportEqualsDeclaration(node) &&
                // isExternalModuleReference(node.moduleReference)`). An
                // internal `import X = A.B` (entity-name reference) is a
                // namespace alias, not external module syntax, so it must not
                // force its file to be a module — otherwise an
                // `await`/`yield`-as-identifier at the top level wrongly
                // becomes reserved (TS1262) in a file tsc treats as a script.
                // Expressed as a match guard rather than a nested `if`: a
                // non-external `import X = A.B` falls through to the `_` arm
                // and is then still considered by the `is_node_exported` check
                // below, exactly as before.
                syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    if Self::import_equals_is_external_module_reference(arena, stmt) =>
                {
                    return true;
                }
                _ => {}
            }
            if Self::is_node_exported(arena, stmt_idx) {
                return true;
            }
        }

        Self::source_file_contains_import_meta(arena, root)
    }

    /// Whether an `import X = ...` statement references an external module
    /// (`= require("...")`) rather than an entity name (`= A.B`). tsz's parser
    /// currently flattens the `require` argument to a bare `StringLiteral`
    /// stored as the `module_specifier`; an entity-name reference is an
    /// `Identifier`/`QualifiedName` instead. `EXTERNAL_MODULE_REFERENCE` is
    /// also accepted so this stays correct if the parser is later changed to
    /// wrap `require(...)` in that node the way tsc's AST does.
    fn import_equals_is_external_module_reference(
        arena: &NodeArena,
        stmt: &tsz_parser::parser::node::Node,
    ) -> bool {
        let Some(import) = arena.get_import_decl(stmt) else {
            return false;
        };
        arena.get(import.module_specifier).is_some_and(|spec| {
            spec.kind == SyntaxKind::StringLiteral as u16
                || spec.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
        })
    }

    /// The `moduleDetection: auto` predicate — module syntax, plus the formats
    /// and declaration-file accommodations that force module-ness on their own.
    pub(crate) fn source_file_is_external_module(arena: &NodeArena, root: NodeIndex) -> bool {
        // Note: .mts/.cts/.mjs/.cjs file extension check is handled by the caller
        // via `is_module_file_extension()`, since this static method doesn't have
        // access to the file name string.
        let Some(source) = arena.get_source_file_at(root) else {
            return false;
        };

        if Self::file_has_module_syntax_indicator(arena, root) {
            return true;
        }

        // Files with extensions that unambiguously imply a module format (Node16+
        // CJS/ESM extensions) are modules regardless of statement content.
        // Matches tsc's `isFileForcedToBeModuleByFormat` under `moduleDetection: auto`,
        // including its exclusion of declaration files (`.d.mts`, `.d.cts`), which
        // still require an explicit module indicator — otherwise their ambient
        // declarations stop seeding the global scope.
        if is_module_file_extension(&source.file_name) {
            return true;
        }

        // Declaration files that only contain `declare global { ... }` still need
        // to behave as importable modules. Otherwise package entrypoints like
        // `@types/react/index.d.ts` spuriously trigger TS2306 despite explicitly
        // opting into global augmentation semantics.
        if source.file_name.ends_with(".d.ts")
            && Self::source_file_has_top_level_global_augmentation(arena, &source.statements.nodes)
        {
            return true;
        }

        // Check for CommonJS module indicator: `module.exports = ...` or `exports.x = ...`
        Self::source_file_has_commonjs_indicator(arena, &source.statements.nodes)
    }

    fn source_file_has_top_level_global_augmentation(
        arena: &NodeArena,
        stmts: &[NodeIndex],
    ) -> bool {
        for &stmt_idx in stmts {
            if stmt_idx.is_none() {
                continue;
            }
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            if stmt.is_global_augmentation() {
                return true;
            }
            let Some(module) = arena.get_module(stmt) else {
                continue;
            };
            let Some(name_node) = arena.get(module.name) else {
                continue;
            };
            if name_node.kind == SyntaxKind::GlobalKeyword as u16 {
                return true;
            }
            if let Some(ident) = arena.get_identifier(name_node)
                && ident.escaped_text == "global"
            {
                return true;
            }
        }

        false
    }

    /// Check if a source file contains a CommonJS module.exports or exports.x assignment.
    /// This detects patterns like:
    /// - `module.exports = { ... }`
    /// - `module.exports.x = ...`
    /// - `exports.x = ...`
    fn source_file_has_commonjs_indicator(arena: &NodeArena, stmts: &[NodeIndex]) -> bool {
        let mut stack: Vec<NodeIndex> = stmts.iter().copied().filter(|idx| idx.is_some()).collect();

        while let Some(idx) = stack.pop() {
            let Some(node) = arena.get(idx) else {
                continue;
            };
            match node.kind {
                syntax_kind_ext::BINARY_EXPRESSION => {
                    // Check left side for `module.exports` or `exports.x` pattern.
                    if let Some(binary) = arena.get_binary_expr(node)
                        && binary.operator_token == SyntaxKind::EqualsToken as u16
                        && Self::is_commonjs_export_target(arena, binary.left)
                    {
                        return true;
                    }
                }
                syntax_kind_ext::CALL_EXPRESSION
                    if Self::is_commonjs_define_property_export_call(arena, idx) =>
                {
                    return true;
                }
                _ => {}
            }

            for child in arena.get_children(idx) {
                stack.push(child);
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

    /// Check for `Object.defineProperty(exports, ...)` or
    /// `Object.defineProperty(module.exports, ...)` as a CommonJS export marker.
    fn is_commonjs_define_property_export_call(arena: &NodeArena, idx: NodeIndex) -> bool {
        let Some(call_node) = arena.get(idx) else {
            return false;
        };
        let Some(call) = arena.get_call_expr(call_node) else {
            return false;
        };
        let Some(callee_node) = arena.get(call.expression) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(callee) = arena.get_access_expr(callee_node) else {
            return false;
        };
        let is_object_define_property = arena
            .get_identifier_at(callee.expression)
            .is_some_and(|ident| ident.escaped_text == "Object")
            && arena
                .get_identifier_at(callee.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "defineProperty");
        if !is_object_define_property {
            return false;
        }
        let Some(args) = &call.arguments else {
            return false;
        };
        if args.nodes.len() < 3 {
            return false;
        }
        Self::is_commonjs_export_base(arena, args.nodes[0])
    }

    /// Check if a node is a CommonJS export base: `exports` or `module.exports`.
    fn is_commonjs_export_base(arena: &NodeArena, idx: NodeIndex) -> bool {
        if arena
            .get_identifier_at(idx)
            .is_some_and(|ident| ident.escaped_text == "exports")
        {
            return true;
        }

        let Some(node) = arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = arena.get_access_expr(node) else {
            return false;
        };
        arena
            .get_identifier_at(access.expression)
            .is_some_and(|ident| ident.escaped_text == "module")
            && arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports")
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
    pub(super) fn has_use_strict_prologue(arena: &NodeArena, stmts: &[NodeIndex]) -> bool {
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
                    && tsz_common::directives::is_use_strict_directive(
                        lit.raw_text.as_deref(),
                        &lit.text,
                    )
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
    /// Panics if either resolution cache lock is poisoned.
    pub fn bind_source_file(&mut self, arena: &NodeArena, root: NodeIndex) {
        // Reset per-file binder stack guard so a pathological earlier file on
        // this thread does not prevent subsequent files from being bound.
        crate::binding::stack_guard::reset_stack_overflow_flag();

        if let Some(node) = arena.get(root)
            && let Some(sf) = arena.get_source_file(node)
        {
            self.set_debug_file(&sf.file_name);
        }

        // Binding mutates scope/symbol tables and assigns new SymbolIds; both
        // resolution caches must be cleared so callers don't receive stale ids.
        self.clear_resolution_caches();

        // Preserve lib symbols that were merged before binding (e.g., in parallel.rs)
        // When merge_lib_symbols is called before bind_source_file, lib symbols are stored
        // in file_locals and need to be preserved across the binding process.
        let lib_symbols: FxHashMap<String, SymbolId> = self
            .file_locals
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        let has_lib_symbols = !lib_symbols.is_empty();

        // Estimate top-level declaration count for pre-sizing hash maps.
        // This avoids repeated resizing for files with many declarations (e.g., 5000 const exports).
        let estimated_decl_count = arena
            .get(root)
            .and_then(|node| arena.get_source_file(node))
            .map_or(0, |sf| sf.statements.nodes.len());

        // Pre-size node_symbols and node_flow maps based on estimated AST node count.
        // A rough estimate: ~3-5 nodes per top-level statement.
        if estimated_decl_count > 16 {
            let estimated_nodes = estimated_decl_count * 4;
            {
                let node_symbols = Arc::make_mut(&mut self.node_symbols);
                node_symbols.clear();
                node_symbols.reserve(estimated_nodes);
            }
            {
                let node_flow = Arc::make_mut(&mut self.node_flow);
                node_flow.clear();
                node_flow.reserve(estimated_nodes);
            }
        }

        // Initialize persistent scope system
        Arc::make_mut(&mut self.scopes).clear();
        Arc::make_mut(&mut self.node_scope_ids).clear();
        self.current_scope_id = ScopeId::NONE;
        Arc::make_mut(&mut self.top_level_flow).clear();

        // Create root persistent scope for the source file, pre-sized for
        // top-level declarations. This is the single live declaration table.
        self.enter_persistent_scope_with_capacity(
            ContainerKind::SourceFile,
            root,
            estimated_decl_count,
        );

        // Pre-populate the root scope with lib symbols if they were merged before
        // binding. This ensures symbols like console, Array, Promise are available
        // during binding.
        if has_lib_symbols && let Some(root_scope) = Arc::make_mut(&mut self.scopes).first_mut() {
            for (name, sym_id) in &lib_symbols {
                if !root_scope.table.has(name) {
                    root_scope.table.set(name.clone(), *sym_id);
                }
            }
        }

        // Pre-reserve symbol arena capacity based on estimated declarations.
        // Each top-level declaration creates at least 1 symbol; classes/interfaces create more.
        if estimated_decl_count > 16 {
            let current_len = self.symbols.len();
            let target = current_len + estimated_decl_count * 2;
            // symbols.symbols is Vec<Symbol>, reserve additional capacity
            self.symbols.reserve(target.saturating_sub(current_len));
        }

        // Create START flow node for the file
        let start_flow = Arc::make_mut(&mut self.flow_nodes).alloc(flow_flags::START);
        self.current_flow = start_flow;
        self.is_external_module = self.detect_external_module(arena, root);

        if let Some(node) = arena.get(root)
            && let Some(sf) = arena.get_source_file(node)
        {
            // .mts/.cts/.mjs/.cjs files are always modules regardless of content.
            // This must happen after source_file_is_external_module which only checks
            // for import/export statements, not file extensions.
            // Only under `moduleDetection: auto`: `legacy` never forces module-ness
            // by format, and `force` has already decided by declaration-file-ness.
            if self.auto_module_detection()
                && !self.is_external_module
                && is_module_file_extension(&sf.file_name)
            {
                self.is_external_module = true;
            }
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
                Arc::make_mut(&mut self.top_level_flow).insert(stmt_idx.0, self.current_flow);
            }

            self.bind_jsdoc_import_tags(arena, sf, root);

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
        // lib_symbols was captured before binding, so user shadow symbols are already in
        // file_locals; when a lib TYPE symbol is blocked, record it in lib_type_namespace.
        if has_lib_symbols {
            for (name, sym_id) in &lib_symbols {
                if !self.file_locals.has(name) {
                    self.file_locals.set(name.clone(), *sym_id);
                } else if self.lib_symbol_ids.contains(sym_id) {
                    self.try_record_lib_type_shadow(name, *sym_id);
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
    ///
    /// Also finalizes `StableLocation::file_idx` on every symbol's
    /// `stable_declarations` and `stable_value_declaration`. During single-
    /// file binding these stable locations are recorded with
    /// `file_idx = u32::MAX`; this pass promotes them to the driver-assigned
    /// index. This is Phase 1 plumbing for the
    /// [global query graph architecture][plan]; the parallel `NodeIndex`
    /// fields remain authoritative for existing consumers.
    ///
    /// [plan]: ../../../../docs/plan/ROADMAP.md
    pub(super) fn stamp_file_idx(&mut self) {
        let idx = self.file_idx;
        let lib_symbol_ids = &self.lib_symbol_ids;

        // Stamp symbols
        for sym in self.symbols.iter_mut() {
            let is_lib = lib_symbol_ids.contains(&sym.id);
            if sym.decl_file_idx == u32::MAX && !is_lib {
                sym.decl_file_idx = idx;
            }
            // Stable locations: only stamp entries that are still unassigned
            // and only for non-lib symbols. Lib stable locations keep their
            // own file provenance once it is assigned.
            if !is_lib {
                for stable in &mut sym.stable_declarations {
                    stable.set_file_idx_if_unassigned(idx);
                }
                sym.stable_value_declaration.set_file_idx_if_unassigned(idx);
            }
        }

        // Stamp semantic_defs
        for entry in Arc::make_mut(&mut self.semantic_defs).values_mut() {
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
        let mut file_exports = Arc::make_mut(&mut self.module_exports)
            .remove(file_name)
            .unwrap_or_default();
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

                // Check if this is a module/namespace symbol.
                // Type-only namespace imports (e.g., `import type * as X from 'mod'`)
                // must NOT leak their members into file_exports — those members are
                // only visible in type position, not value position.
                if symbol.is_exported
                    && !symbol.is_type_only
                    && (symbol.flags
                        & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE))
                        != 0
                {
                    // If the module has an exports table, merge it into file_exports.
                    //
                    // A symbol declared *inside* a namespace
                    // (`export declare namespace X { export type Result = ... }`)
                    // is a NAMESPACE MEMBER: its `parent` points at the namespace
                    // symbol `X`. tsc exposes only the namespace `X` at the file's
                    // top level (member access is qualified as `X.Result`), never the
                    // bare member `Result`. Hoisting such members would leak them into
                    // the file's top-level export set — under a barrel `export *` this
                    // produces phantom TS2308 collisions and resolves bare imports to
                    // the wrong (namespace-member) binding. So skip members whose
                    // `parent` is this namespace symbol; the namespace symbol itself is
                    // still exposed by the `is_exported` branch below.
                    if let Some(module_exports) = symbol.exports.as_ref() {
                        for (export_name, &export_sym_id) in module_exports.iter() {
                            let is_namespace_member = self
                                .symbols
                                .get(export_sym_id)
                                .is_some_and(|member| member.parent == sym_id);
                            if is_namespace_member {
                                continue;
                            }
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
                    // When a namespace MODULE symbol overwrites an existing `export * as N`
                    // ALIAS in file_exports, preserve the import_module link via
                    // alias_partners. The checker's member-resolution path already follows
                    // alias_partners to bridge locally-declared namespace members with the
                    // re-exported ones from the source module.
                    if (symbol.flags & symbol_flags::MODULE) != 0
                        && let Some(existing_id) = file_exports.get(name)
                        && self.symbols.get(existing_id).is_some_and(|s| {
                            (s.flags & symbol_flags::ALIAS) != 0
                                && s.import_name() == Some("*")
                                && !s.is_umd_export
                        })
                    {
                        Arc::make_mut(&mut self.alias_partners).insert(sym_id, existing_id);
                    }
                    if !self.export_surface_keeps_existing_value(
                        name,
                        file_exports.get(name),
                        sym_id,
                    ) {
                        file_exports.set(name.clone(), sym_id);
                    }
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
            // The `.members` table on a class symbol holds INSTANCE members (e.g. `bar`
            // from `class D { bar: string; }`). Those are accessible via an instance
            // (`new D().bar`) — never at the module-namespace level. Static members and
            // namespace augmentations live in `.exports`, which is merged above.
            // Without this guard, `import x = require()` of an `export = D` module would
            // synthesize a phantom `{ bar }` namespace surface and produce
            // `typeof D & { bar }` instead of tsc's plain `typeof D`.
            let target_is_class = (target_symbol.flags & symbol_flags::CLASS) != 0;
            if !target_is_class && let Some(target_members) = target_symbol.members.as_ref() {
                for (member_name, &member_sym_id) in target_members.iter() {
                    if member_name != "default" && !file_exports.has(member_name) {
                        file_exports.set(member_name.clone(), member_sym_id);
                    }
                }
            }
        }

        if !file_exports.is_empty() {
            Arc::make_mut(&mut self.module_exports).insert(file_name.to_string(), file_exports);
        }
    }

    fn export_surface_keeps_existing_value(
        &self,
        export_name: &str,
        existing_id: Option<SymbolId>,
        incoming_id: SymbolId,
    ) -> bool {
        const TYPE_ONLY_DECL: u32 =
            symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS | symbol_flags::TYPE_PARAMETER;

        let Some(existing_id) = existing_id else {
            return false;
        };
        let Some(existing) = self.symbols.get(existing_id) else {
            return false;
        };
        let Some(incoming) = self.symbols.get(incoming_id) else {
            return false;
        };

        if export_name == "default"
            && existing.has_any_flags(symbol_flags::ALIAS)
            && existing.import_module().is_some()
            && incoming.has_any_flags(symbol_flags::ALIAS)
            && incoming.import_module().is_none()
        {
            return true;
        }

        let incoming_is_type_only = incoming.is_type_only
            || (incoming.has_any_flags(TYPE_ONLY_DECL)
                && !incoming.has_any_flags(symbol_flags::VALUE)
                && incoming.import_module().is_none());
        if incoming_is_type_only
            && existing.has_any_flags(symbol_flags::ALIAS)
            && existing.import_name() == Some("*")
            && !existing.is_umd_export
        {
            return false;
        }
        let existing_can_provide_value = !existing.is_type_only
            && (existing.has_any_flags(symbol_flags::VALUE)
                || (existing.has_any_flags(symbol_flags::ALIAS)
                    && existing.import_module().is_some()));

        incoming_is_type_only && existing_can_provide_value
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
                .current_scope()
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
                    .current_scope()
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

        symbol.all_declarations().into_iter().any(|decl_idx| {
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

    pub(super) fn compute_module_export_equals_non_module(
        &self,
        exports: &SymbolTable,
    ) -> Option<bool> {
        let export_assignment_targets = |sym: &Symbol| -> Vec<String> {
            let mut targets = Vec::new();
            for decl_idx in sym.all_declarations() {
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
                if !targets.iter().any(|t| *t == id.escaped_text) {
                    targets.push(id.escaped_text.to_string());
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
}
