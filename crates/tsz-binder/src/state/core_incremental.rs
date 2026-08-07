//! Lib-merged binding entry points and incremental re-binding.
//!
//! Split out of `core.rs` to stay under the 2000-LOC boundary (#16733).

use super::core::is_module_file_extension;
use super::{BinderState, LibContext};
use crate::ScopeId;
use crate::lib_loader;
use rustc_hash::FxHashSet;
use std::sync::Arc;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::{NodeIndex, NodeList};

impl BinderState {
    /// Recompute `export =` non-module classification for all known module exports.
    pub fn recompute_module_export_equals_non_module(&mut self) {
        self.module_export_equals_non_module.clear();
        // `Arc::clone` is cheap; the inner iteration borrows the shared map
        // while we mutate `self.module_export_equals_non_module`.
        let module_exports = Arc::clone(&self.module_exports);
        for (module_name, exports) in module_exports.iter() {
            if let Some(non_module) = self.compute_module_export_equals_non_module(exports) {
                self.module_export_equals_non_module
                    .insert(module_name.clone(), non_module);
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
    /// Panics if either resolution cache lock is poisoned.
    pub fn merge_lib_symbols(&mut self, lib_files: &[Arc<lib_loader::LibFile>]) {
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

        // Merge into the root persistent scope (the single live declaration table).
        // The persistent scope arena is append-only and only holds more than the
        // root scope once binding has entered a nested scope, so this runs in the
        // pre-binding root state (every caller merges libs before
        // `bind_source_file` populates the arena).
        if let Some(root_scope) = Arc::make_mut(&mut self.scopes).first_mut() {
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
            Arc::make_mut(&mut self.lib_binders).push(Arc::clone(&lib.binder));
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
    /// Panics if either resolution cache lock is poisoned.
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
    /// Panics if either resolution cache lock is poisoned.
    pub fn bind_source_file_incremental(
        &mut self,
        arena: &NodeArena,
        root: NodeIndex,
        prefix_statements: &[NodeIndex],
        old_suffix_statements: &[NodeIndex],
        new_suffix_statements: &[NodeIndex],
        reparse_start: u32,
    ) -> bool {
        // Incremental binding mutates scopes and can reassign SymbolIds; clear
        // both caches so callers don't receive stale ids after the re-bind.
        self.clear_resolution_caches();

        let Some(&last_prefix) = prefix_statements.last() else {
            return false;
        };
        let Some(&start_flow) = self.top_level_flow.get(&last_prefix.0) else {
            return false;
        };
        if self.scopes.is_empty() {
            return false;
        }

        self.is_external_module = self.detect_external_module(arena, root);

        // Detect strict mode for incremental rebinding
        if let Some(node) = arena.get(root)
            && let Some(sf) = arena.get_source_file(node)
        {
            if self.auto_module_detection()
                && !self.is_external_module
                && is_module_file_extension(&sf.file_name)
            {
                self.is_external_module = true;
            }
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
            if let Some(scope) = Arc::make_mut(&mut self.scopes).get_mut(0) {
                scope.table.remove(&name);
            }
        }

        let mut symbol_nodes = Vec::new();
        self.collect_statement_symbol_nodes(arena, old_suffix_statements, &mut symbol_nodes);
        for node in symbol_nodes {
            if let Some(sym_id) = Arc::make_mut(&mut self.node_symbols).remove(&node.0)
                && let Some(sym) = self.symbols.get_mut(sym_id)
            {
                // Keep `declarations` and `stable_declarations` in lockstep —
                // they share a positional invariant established in
                // `Symbol::add_declaration`.
                let mut i = 0;
                while i < sym.declarations.len() {
                    if sym.declarations[i] == node {
                        sym.declarations.remove(i);
                        if i < sym.stable_declarations.len() {
                            sym.stable_declarations.remove(i);
                        }
                    } else {
                        i += 1;
                    }
                }
                if sym.value_declaration == node {
                    sym.value_declaration =
                        sym.declarations.first().copied().unwrap_or(NodeIndex::NONE);
                    let value_span = if sym.value_declaration.is_some() {
                        arena.pos_end_at(sym.value_declaration)
                    } else {
                        None
                    };
                    sym.stable_value_declaration =
                        crate::symbols::StableLocation::from_span(self.file_idx, value_span);
                }
            }
        }

        for stmt_idx in old_suffix_statements {
            Arc::make_mut(&mut self.top_level_flow).remove(&stmt_idx.0);
        }

        // Reset transient binding state while keeping existing symbols and scopes.
        // Seed the root scope's table (the single live declaration table) from the
        // accumulated file locals so suffix declarations bind against them.
        self.current_scope_id = ScopeId(0);
        let seeded = self.file_locals.clone();
        if let Some(table) = self.current_scope_mut() {
            *table = seeded;
        }
        self.hoisted_vars.clear();
        self.hoisted_functions.clear();
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
            Arc::make_mut(&mut self.top_level_flow).insert(stmt_idx.0, self.current_flow);
        }

        // Store file locals, preserving any existing lib symbols
        // This ensures symbols from merge_lib_symbols() are not lost
        let existing_file_locals = std::mem::take(&mut self.file_locals);
        self.file_locals = self.current_scope().clone();
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

        Arc::make_mut(&mut self.node_flow).retain(|node_id, _| keep_node(node_id));
        Arc::make_mut(&mut self.node_scope_ids).retain(|node_id, _| keep_node(node_id));
        Arc::make_mut(&mut self.switch_clause_to_switch).retain(|node_id, _| keep_node(node_id));
    }
}
