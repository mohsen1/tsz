use crate::module_resolution::module_specifier_candidates;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Resolve a namespace import (import * as ns) from another file using cross-file resolution.
    ///
    /// Returns a `SymbolTable` containing all exports from the target module.
    pub(crate) fn resolve_cross_file_namespace_exports(
        &self,
        module_specifier: &str,
    ) -> Option<tsz_binder::SymbolTable> {
        let cache_key = (self.ctx.current_file_idx, module_specifier.to_string());
        if let Some(cached) = self
            .ctx
            .namespace_exports_cache
            .borrow()
            .get(&cache_key)
            .cloned()
        {
            return cached;
        }

        if let Some(exports) = self.resolve_ambient_module_namespace_exports(module_specifier) {
            self.ctx
                .namespace_exports_cache
                .borrow_mut()
                .insert(cache_key, Some(exports.clone()));
            return Some(exports);
        }

        let Some(target_file_idx) = self.ctx.resolve_import_target(module_specifier) else {
            self.ctx
                .namespace_exports_cache
                .borrow_mut()
                .insert(cache_key, None);
            return None;
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_file_idx) else {
            self.ctx
                .namespace_exports_cache
                .borrow_mut()
                .insert(cache_key, None);
            return None;
        };
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(target_file_name) = target_arena
            .source_files
            .first()
            .map(|source_file| source_file.file_name.clone())
        else {
            self.ctx
                .namespace_exports_cache
                .borrow_mut()
                .insert(cache_key, None);
            return None;
        };

        // Helper: record cross-file origin for all symbols in a table.
        let record_symbols = |table: &tsz_binder::SymbolTable| {
            for (_, &sym_id) in table.iter() {
                self.ctx
                    .register_symbol_file_target(sym_id, target_file_idx);
            }
        };

        // Try to find exports in the target binder's module_exports.
        // Prefer canonical file key first, then module specifier fallback.
        let direct_exports = self
            .ctx
            .module_exports_for_module(target_binder, &target_file_name)
            .or_else(|| {
                self.ctx
                    .module_exports_for_module(target_binder, module_specifier)
            });

        if let Some(exports) = direct_exports {
            let mut combined = exports.clone();
            self.merge_export_equals_members(target_binder, exports, &mut combined);
            if let Some(export_equals_sym_id) = exports.get("export=")
                && let Some(export_equals_symbol) = target_binder.get_symbol(export_equals_sym_id)
            {
                let _ = self.merge_export_equals_import_type_members(
                    export_equals_symbol,
                    Some(target_file_idx),
                    &mut combined,
                );
            }
            let mut visited = rustc_hash::FxHashSet::default();
            self.collect_reexported_symbols(
                target_file_idx,
                Some(module_specifier),
                &mut combined,
                &mut visited,
            );
            self.merge_module_augmentation_namespace_exports(
                &mut combined,
                target_file_idx,
                Some(module_specifier),
            );
            record_symbols(&combined);
            self.ctx
                .namespace_exports_cache
                .borrow_mut()
                .insert(cache_key, Some(combined.clone()));
            return Some(combined);
        }

        // No direct exports found, but the module may still re-export symbols
        // via `export * from './other'` or `export { X } from './other'`.
        // Collect re-exported symbols even when there are no direct exports.
        let has_reexports = self
            .ctx
            .wildcard_reexports_for_file(target_binder, &target_file_name)
            .is_some()
            || self
                .ctx
                .reexports_for_file(target_binder, &target_file_name)
                .is_some();
        if has_reexports {
            let mut combined = tsz_binder::SymbolTable::new();
            let mut visited = rustc_hash::FxHashSet::default();
            self.collect_reexported_symbols(
                target_file_idx,
                Some(module_specifier),
                &mut combined,
                &mut visited,
            );
            self.merge_module_augmentation_namespace_exports(
                &mut combined,
                target_file_idx,
                Some(module_specifier),
            );
            if !combined.is_empty() {
                record_symbols(&combined);
            }
            // Return the table even if empty — the module exists but may have only
            // type-only exports (e.g., `export type * from '...'`). An empty namespace
            // object type is correct and will produce TS2339 for value access, instead
            // of falling through to "module not found" → TypeId::ANY.
            self.ctx
                .namespace_exports_cache
                .borrow_mut()
                .insert(cache_key, Some(combined.clone()));
            return Some(combined);
        }

        self.ctx
            .namespace_exports_cache
            .borrow_mut()
            .insert(cache_key, None);
        None
    }

    /// Like `resolve_cross_file_namespace_exports` but with a pre-resolved target file index.
    /// Used when the module specifier was already resolved from a different source file.
    fn resolve_cross_file_namespace_exports_for_file(
        &self,
        target_file_idx: usize,
        module_specifier: Option<&str>,
    ) -> Option<tsz_binder::SymbolTable> {
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_file_name = target_arena.source_files.first()?.file_name.clone();

        let record_symbols = |table: &tsz_binder::SymbolTable| {
            for (_, &sym_id) in table.iter() {
                self.ctx
                    .register_symbol_file_target(sym_id, target_file_idx);
            }
        };

        let direct_exports = self
            .ctx
            .module_exports_for_module(target_binder, &target_file_name)
            .or_else(|| {
                module_specifier.and_then(|specifier| {
                    self.ctx.module_exports_for_module(target_binder, specifier)
                })
            });

        if let Some(exports) = direct_exports {
            let mut combined = exports.clone();
            self.merge_export_equals_members(target_binder, exports, &mut combined);
            let mut visited = rustc_hash::FxHashSet::default();
            self.collect_reexported_symbols(
                target_file_idx,
                module_specifier,
                &mut combined,
                &mut visited,
            );
            self.merge_module_augmentation_namespace_exports(
                &mut combined,
                target_file_idx,
                module_specifier,
            );
            record_symbols(&combined);
            return Some(combined);
        }

        let has_reexports = self
            .ctx
            .wildcard_reexports_for_file(target_binder, &target_file_name)
            .is_some()
            || self
                .ctx
                .reexports_for_file(target_binder, &target_file_name)
                .is_some();
        if has_reexports {
            let mut combined = tsz_binder::SymbolTable::new();
            let mut visited = rustc_hash::FxHashSet::default();
            self.collect_reexported_symbols(
                target_file_idx,
                module_specifier,
                &mut combined,
                &mut visited,
            );
            self.merge_module_augmentation_namespace_exports(
                &mut combined,
                target_file_idx,
                module_specifier,
            );
            if !combined.is_empty() {
                record_symbols(&combined);
            }
            return Some(combined);
        }

        // The target file is a real ES module (has top-level `import`/`export`
        // statements or a module file extension) but its public surface is
        // empty — e.g. `main.mts` only declares `import` aliases, no exports.
        // tsc still types `import * as ns from "./main.mjs"` as the empty
        // module namespace `{}`, so `ns.default` / `ns.imported` correctly
        // report TS2339 instead of leaking the local imports as members.
        // Returning an empty table here matches that behavior; falling
        // through to `None` would let the caller widen the namespace to
        // `any`, which silently accepts any property access.
        if target_binder.is_external_module {
            return Some(tsz_binder::SymbolTable::new());
        }

        None
    }

    pub(crate) fn merge_module_augmentation_namespace_exports(
        &self,
        exports: &mut tsz_binder::SymbolTable,
        target_file_idx: usize,
        module_specifier: Option<&str>,
    ) {
        // Skip the wildcard-chain helper cost when no augmentations exist.
        if !self.ctx.program_has_module_augmentations() {
            return;
        }
        let mut names: Vec<String> = Vec::new();

        if let Some(module_specifier) = module_specifier {
            for name in self.collect_module_augmentation_names(module_specifier) {
                if !names.iter().any(|existing| existing == &name) {
                    names.push(name);
                }
            }
        }

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        if let Some(target_file_name) = target_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.as_str())
        {
            for name in self.collect_module_augmentation_names(target_file_name) {
                if !names.iter().any(|existing| existing == &name) {
                    names.push(name);
                }
            }
        }

        for name in names {
            if exports.get(name.as_str()).is_some() {
                continue;
            }
            if let Some((sym_id, owner_file_idx)) =
                self.resolve_module_augmentation_export_for_file(target_file_idx, &name)
            {
                exports.set(name, sym_id);
                self.ctx.register_symbol_file_target(sym_id, owner_file_idx);
            }
        }
    }

    /// Resolve a module's effective export surface.
    ///
    /// This canonicalizes module-specifier variants and ensures `export =` target
    /// members are merged into the result. Prefer this over ad-hoc lookups against
    /// `binder.module_exports`.
    pub(crate) fn resolve_effective_module_exports(
        &self,
        module_specifier: &str,
    ) -> Option<tsz_binder::SymbolTable> {
        self.resolve_effective_module_exports_from_file(module_specifier, None)
    }

    /// Like `resolve_effective_module_exports` but uses an explicit `resolution-mode`
    /// override from import attributes (e.g., `with { "resolution-mode": "require" }`).
    /// Falls back to the non-mode-aware path when no override is provided.
    pub(crate) fn resolve_effective_module_exports_with_mode(
        &self,
        module_specifier: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) -> Option<tsz_binder::SymbolTable> {
        if let Some(mode) = resolution_mode
            && let Some(target_idx) = self.ctx.resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_specifier,
                Some(mode),
            )
        {
            if let Some(exports) = self
                .resolve_cross_file_namespace_exports_for_file(target_idx, Some(module_specifier))
            {
                return Some(exports);
            }
            return Some(tsz_binder::SymbolTable::new());
        }
        self.resolve_effective_module_exports_from_file(
            module_specifier,
            Some(self.ctx.current_file_idx),
        )
        .or_else(|| self.resolve_effective_module_exports(module_specifier))
    }

    /// Like `resolve_effective_module_exports` but optionally resolves relative paths
    /// from a specific source file. This is needed for cross-file namespace re-exports
    /// where the module specifier (e.g., `"./b"`) is relative to the declaring file,
    /// not the current file being checked.
    pub(crate) fn resolve_effective_module_exports_from_file(
        &self,
        module_specifier: &str,
        source_file_idx: Option<usize>,
    ) -> Option<tsz_binder::SymbolTable> {
        if let Some(source_idx) = source_file_idx
            && let Some(target_idx) = self
                .ctx
                .resolve_import_target_from_file(source_idx, module_specifier)
            && let Some(exports) = self
                .resolve_cross_file_namespace_exports_for_file(target_idx, Some(module_specifier))
        {
            return Some(exports);
        }

        if let Some(target_idx) = self.ctx.resolve_import_target(module_specifier)
            && let Some(exports) = self
                .resolve_cross_file_namespace_exports_for_file(target_idx, Some(module_specifier))
        {
            return Some(exports);
        }

        for candidate in module_specifier_candidates(module_specifier) {
            // When resolving from a specific source file (cross-file symbol),
            // also try resolving the module specifier from that file's perspective
            if let Some(source_idx) = source_file_idx
                && let Some(target_idx) = self
                    .ctx
                    .resolve_import_target_from_file(source_idx, &candidate)
                && let Some(exports) =
                    self.resolve_cross_file_namespace_exports_for_file(target_idx, Some(&candidate))
            {
                return Some(exports);
            }

            if let Some(exports) = self.resolve_cross_file_namespace_exports(&candidate) {
                return Some(exports);
            }

            if let Some(exports) = self
                .ctx
                .module_exports_for_module(self.ctx.binder, &candidate)
            {
                let mut combined = exports.clone();
                self.merge_export_equals_members(self.ctx.binder, exports, &mut combined);
                if let Some(export_equals_sym_id) = exports.get("export=")
                    && let Some(export_equals_symbol) =
                        self.ctx.binder.get_symbol(export_equals_sym_id)
                {
                    let _ = self.merge_export_equals_import_type_members(
                        export_equals_symbol,
                        source_file_idx.or_else(|| self.ctx.resolve_import_target(&candidate)),
                        &mut combined,
                    );
                }
                return Some(combined);
            }
        }

        None
    }

    fn resolve_ambient_module_namespace_exports(
        &self,
        module_specifier: &str,
    ) -> Option<tsz_binder::SymbolTable> {
        let binders = self.ctx.all_binders.as_ref()?;
        // Use O(1) module binder index when available.
        if let Some(file_indices) = self.ctx.files_for_module_specifier(module_specifier) {
            for &file_idx in file_indices {
                if let Some(binder) = binders.get(file_idx)
                    && let Some(exports) =
                        self.ctx.module_exports_for_module(binder, module_specifier)
                {
                    let mut combined = exports.clone();
                    self.merge_export_equals_members(binder, exports, &mut combined);
                    if let Some(export_equals_sym_id) = exports.get("export=")
                        && let Some(export_equals_symbol) = binder.get_symbol(export_equals_sym_id)
                    {
                        let _ = self.merge_export_equals_import_type_members(
                            export_equals_symbol,
                            Some(file_idx),
                            &mut combined,
                        );
                    }
                    return Some(combined);
                }
            }
        } else {
            for (file_idx, binder) in binders.iter().enumerate() {
                if let Some(exports) = self.ctx.module_exports_for_module(binder, module_specifier)
                {
                    let mut combined = exports.clone();
                    self.merge_export_equals_members(binder, exports, &mut combined);
                    if let Some(export_equals_sym_id) = exports.get("export=")
                        && let Some(export_equals_symbol) = binder.get_symbol(export_equals_sym_id)
                    {
                        let _ = self.merge_export_equals_import_type_members(
                            export_equals_symbol,
                            Some(file_idx),
                            &mut combined,
                        );
                    }
                    return Some(combined);
                }
            }
        }
        None
    }

    fn merge_export_equals_members(
        &self,
        binder: &tsz_binder::BinderState,
        exports: &tsz_binder::SymbolTable,
        combined: &mut tsz_binder::SymbolTable,
    ) {
        let Some(export_equals_sym_id) = exports.get("export=") else {
            return;
        };
        let Some(export_equals_symbol) = binder.get_symbol(export_equals_sym_id) else {
            return;
        };

        if let Some(symbol_exports) = export_equals_symbol.exports.as_ref() {
            for (name, sym_id) in symbol_exports.iter() {
                if name != "default" && !combined.has(name) {
                    combined.set(name.to_string(), *sym_id);
                }
            }
        }

        // The `.members` table on a class symbol holds INSTANCE members (e.g. `bar`
        // from `class D { bar: string; }`). Those live on D's prototype and on
        // instances of D — they are never accessible at the module-namespace level.
        // Merging them here would synthesize a phantom `{ bar }` namespace surface
        // and force the import type to be `typeof D & { bar }` instead of `typeof D`.
        // tsc treats `import x = require()` of an `export = D` module as `typeof D`
        // directly. Static members and namespace augmentations live in `.exports`,
        // which we already merged above.
        let is_class = export_equals_symbol.has_any_flags(tsz_binder::symbol_flags::CLASS);
        if !is_class && let Some(symbol_members) = export_equals_symbol.members.as_ref() {
            for (name, sym_id) in symbol_members.iter() {
                if name != "default" && !combined.has(name) {
                    combined.set(name.to_string(), *sym_id);
                }
            }
        }
    }

    /// When `export =` targets a `typeof import("./...")` declaration, the binder symbol
    /// itself has no exports table. Re-hydrate the referenced module's named exports so
    /// namespace imports see the same surface as the imported module.
    pub(crate) fn merge_export_equals_import_type_members(
        &self,
        export_equals_symbol: &tsz_binder::Symbol,
        fallback_decl_file_idx: Option<usize>,
        combined: &mut tsz_binder::SymbolTable,
    ) -> Option<String> {
        let decl_file_idx = if export_equals_symbol.decl_file_idx == u32::MAX {
            fallback_decl_file_idx?
        } else {
            export_equals_symbol.decl_file_idx as usize
        };
        let binder = self.ctx.get_binder_for_file(decl_file_idx)?;
        let arena = self.ctx.get_arena_for_file(decl_file_idx as u32);

        let module_specifier_from_decl = |decl_idx: NodeIndex| -> Option<String> {
            let node = arena.get(decl_idx)?;
            if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                return None;
            }
            let var_decl = arena.get_variable_declaration(node)?;
            if !var_decl.type_annotation.is_some() {
                return None;
            }
            self.import_type_module_specifier_from_type_node(arena, var_decl.type_annotation)
        };

        let mut module_specifier = export_equals_symbol
            .value_declaration
            .into_option()
            .and_then(module_specifier_from_decl)
            .or_else(|| {
                export_equals_symbol
                    .declarations
                    .iter()
                    .find_map(|&decl_idx| module_specifier_from_decl(decl_idx))
            });

        // Handle `export = x` where `x` carries the import-type annotation.
        if module_specifier.is_none() {
            let export_assign_decl = export_equals_symbol
                .value_declaration
                .into_option()
                .and_then(|decl_idx| {
                    arena.get(decl_idx).and_then(|node| {
                        (node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT).then_some(decl_idx)
                    })
                })
                .or_else(|| {
                    export_equals_symbol
                        .declarations
                        .iter()
                        .find_map(|&decl_idx| {
                            arena.get(decl_idx).and_then(|node| {
                                (node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT)
                                    .then_some(decl_idx)
                            })
                        })
                });

            if let Some(export_assign_idx) = export_assign_decl
                && let Some(assign) = arena
                    .get(export_assign_idx)
                    .and_then(|node| arena.get_export_assignment(node))
                && let Some(target_sym_id) = binder
                    .get_node_symbol(assign.expression)
                    .or_else(|| binder.resolve_identifier(arena, assign.expression))
            {
                let resolved_target = {
                    let mut visited = AliasCycleTracker::new();
                    self.resolve_alias_symbol(target_sym_id, &mut visited)
                        .unwrap_or(target_sym_id)
                };
                let target_symbol = binder
                    .get_symbol(resolved_target)
                    .or_else(|| self.get_symbol_globally(resolved_target))
                    .or_else(|| self.get_cross_file_symbol(resolved_target));
                if let Some(target_symbol) = target_symbol {
                    module_specifier = target_symbol
                        .value_declaration
                        .into_option()
                        .and_then(module_specifier_from_decl)
                        .or_else(|| {
                            target_symbol
                                .declarations
                                .iter()
                                .find_map(|&decl_idx| module_specifier_from_decl(decl_idx))
                        });
                }
            }
        }

        let module_specifier = module_specifier?;

        let Some(nested_exports) =
            self.resolve_effective_module_exports_from_file(&module_specifier, Some(decl_file_idx))
        else {
            return Some(module_specifier);
        };
        let nested_target_idx = nested_exports
            .iter()
            .find_map(|(_, &sym_id)| self.ctx.resolve_symbol_file_index_stable(sym_id))
            .or_else(|| {
                self.ctx
                    .resolve_import_target_from_file(decl_file_idx, &module_specifier)
            })
            .or_else(|| self.ctx.resolve_import_target(&module_specifier));

        for (name, sym_id) in nested_exports.iter() {
            if let Some(target_idx) = nested_target_idx {
                self.ctx.register_symbol_file_target(*sym_id, target_idx);
            }
            if name != "export=" && !combined.has(name) {
                combined.set(name.to_string(), *sym_id);
            }
        }
        Some(module_specifier)
    }

    fn import_type_module_specifier_from_type_node(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        type_idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(type_idx)?;
        if node.kind != syntax_kind_ext::TYPE_QUERY {
            return None;
        }
        let type_query = arena.get_type_query(node)?;
        let call_idx = self.leftmost_import_call_in_entity_name(arena, type_query.expr_name)?;
        let call = arena.get_call_expr(arena.get(call_idx)?)?;
        let args = call.arguments.as_ref()?;
        let &first_arg = args.nodes.first()?;
        let arg_node = arena.get(first_arg)?;
        let literal = arena.get_literal(arg_node)?;
        Some(literal.text.clone())
    }

    fn leftmost_import_call_in_entity_name(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        mut idx: NodeIndex,
    ) -> Option<NodeIndex> {
        const MAX_DEPTH: usize = 64;
        for _ in 0..MAX_DEPTH {
            let node = arena.get(idx)?;
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let qn = arena.get_qualified_name(node)?;
                idx = qn.left;
                continue;
            }
            if node.kind != syntax_kind_ext::CALL_EXPRESSION {
                return None;
            }
            let call = arena.get_call_expr(node)?;
            let expr_node = arena.get(call.expression)?;
            return (expr_node.kind == SyntaxKind::ImportKeyword as u16).then_some(idx);
        }
        None
    }
}
