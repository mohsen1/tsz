//! Text-based entity-name -> `DefId` resolution for cross-arena type lowering.
//!
//! Split out of `symbol_resolver` (at the per-file line ceiling). Resolves a
//! simple or qualified type name through the merged checker binder's export
//! graph, following barrel re-export chains to the declaration.

use crate::query_boundaries::type_predicates::is_compiler_managed_type;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::symbol_flags;

impl CheckerState<'_> {
    /// Resolve a simple or qualified type name through the merged checker binder.
    ///
    /// Cross-arena lowering cannot trust raw `NodeIndex` values because the same
    /// index may refer to unrelated nodes in different declaration arenas. This
    /// helper uses the text form (`A` or `A.B.C`) and walks the merged binder's
    /// export graph to recover the correct `DefId`.
    pub(crate) fn resolve_entity_name_text_to_def_id_for_lowering(
        &self,
        name: &str,
    ) -> Option<tsz_solver::def::DefId> {
        if !name.contains('.') && self.ctx.type_parameter_scope.contains_key(name) {
            return None;
        }

        if is_compiler_managed_type(name) {
            return None;
        }

        if let Some(cached) = self
            .ctx
            .lowering_entity_name_resolution_cache
            .borrow()
            .get(name)
            .copied()
        {
            // A miss recorded before lib contexts were attached is not stable
            // for child/cross-arena checkers. Retry once libs are available so
            // imported declaration files can resolve globals like `Error`.
            //
            // Likewise, a `None` cached for a qualified name like
            // `util.OmitKeys` may have been recorded by an earlier checker
            // state whose binder couldn't see the imported namespace
            // member yet. Retry such misses so a later checker state with
            // the merged binder graph can recover the correct `DefId`.
            // Without this retry, the first failed lookup poisons the cache
            // and silently strands the alias body's downstream consumers
            // (object spread, intersection reduction) on
            // `UnresolvedTypeName`.
            let retry_dotted_miss = cached.is_none() && name.contains('.');
            if cached.is_some() || (!self.ctx.has_lib_loaded() && !retry_dotted_miss) {
                return cached;
            }
        }

        let mut segments = name.split('.');
        let root_name = segments.next()?;
        let lib_binders = self.get_lib_binders();
        let mut current_sym = self
            .ctx
            .binder
            .file_locals
            .get(root_name)
            .or_else(|| {
                self.ctx
                    .binder
                    .get_global_type_with_libs(root_name, &lib_binders)
            })
            .or_else(|| {
                self.ctx
                    .global_file_locals_index
                    .as_ref()
                    .and_then(|idx| idx.get(root_name))
                    .and_then(|entries| entries.iter().max_by_key(|(_, sym)| sym.0))
                    .map(|&(_, sym)| sym)
            })
            .or_else(|| {
                lib_binders
                    .iter()
                    .find_map(|binder| binder.file_locals.get(root_name))
            })
            .or_else(|| self.resolve_global_augmentation_root_symbol(root_name, &lib_binders))?;

        // Bare reference to an import alias from an unresolved module: same
        // poison-to-`any` rule as the `NodeIndex` path above (qualified members
        // like `E.URI` fail in the segment walk below and return `None` there).
        if segments.clone().next().is_none() && self.is_unresolved_import_symbol_id(current_sym) {
            self.ctx
                .lowering_entity_name_resolution_cache
                .borrow_mut()
                .insert(name.to_string(), None);
            return None;
        }

        for segment in segments {
            let mut visited_aliases = AliasCycleTracker::new();
            current_sym = self
                .resolve_alias_symbol(current_sym, &mut visited_aliases)
                .unwrap_or(current_sym);

            let Some(symbol) = self.get_cross_file_symbol(current_sym).or_else(|| {
                self.ctx
                    .binder
                    .get_symbol_with_libs(current_sym, &lib_binders)
            }) else {
                self.ctx
                    .lowering_entity_name_resolution_cache
                    .borrow_mut()
                    .insert(name.to_string(), None);
                return None;
            };

            if let Some(member_sym) = symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(segment))
                .or_else(|| {
                    symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(segment))
                })
            {
                current_sym = member_sym;
                continue;
            }

            if let Some(module_specifier) = symbol.import_module() {
                let mut visited_aliases = AliasCycleTracker::new();
                if let Some(member_sym) = self.resolve_reexported_member_symbol(
                    module_specifier,
                    segment,
                    &mut visited_aliases,
                ) {
                    current_sym = member_sym;
                    continue;
                }
            }

            if symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0
                && let Some(member_sym) = self.resolve_namespace_member_from_all_binders(
                    symbol.escaped_name.as_str(),
                    segment,
                )
            {
                current_sym = member_sym;
                continue;
            }

            self.ctx
                .lowering_entity_name_resolution_cache
                .borrow_mut()
                .insert(name.to_string(), None);
            return None;
        }

        let mut visited_aliases = AliasCycleTracker::new();
        let resolved_sym = self
            .resolve_alias_symbol(current_sym, &mut visited_aliases)
            .unwrap_or(current_sym);
        let canonical_name = name.rsplit('.').next().unwrap_or(name);
        // A bare reference reaching its declaration through a barrel re-export
        // must key its `DefId` to the declaration, not an intermediate
        // re-export file (see `reexported_declaration_def_id_for_lowering`).
        if let Some(def_id) =
            self.reexported_declaration_def_id_for_lowering(current_sym, root_name)
        {
            self.ctx
                .lowering_entity_name_resolution_cache
                .borrow_mut()
                .insert(name.to_string(), Some(def_id));
            return Some(def_id);
        }
        let expected_name = self
            .get_cross_file_symbol(resolved_sym)
            .or_else(|| {
                self.ctx
                    .binder
                    .get_symbol_with_libs(resolved_sym, &lib_binders)
            })
            .map_or(canonical_name, |symbol| symbol.escaped_name.as_str());
        let def_id = if self.ctx.has_lib_loaded()
            && self.ctx.symbol_is_from_actual_or_cloned_lib(resolved_sym)
        {
            self.ctx
                .get_canonical_lib_def_id(expected_name, resolved_sym)
        } else {
            self.ctx
                .get_or_create_def_id_for_symbol_name(resolved_sym, expected_name)
        };
        self.ctx
            .lowering_entity_name_resolution_cache
            .borrow_mut()
            .insert(name.to_string(), Some(def_id));
        Some(def_id)
    }
}
