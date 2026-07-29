use crate::state::CheckerState;
use tsz_binder::SymbolId;

impl<'a> CheckerState<'a> {
    pub(crate) fn query_file_is_declaration_file(&self, file_idx: Option<usize>) -> bool {
        file_idx.is_some_and(|file_idx| self.file_index_is_declaration_file(file_idx))
    }

    pub(crate) fn file_index_is_declaration_file(&self, file_idx: usize) -> bool {
        self.ctx
            .all_arenas
            .as_ref()
            .and_then(|arenas| arenas.get(file_idx))
            .and_then(|arena| arena.source_files.first())
            .is_some_and(|source_file| source_file.is_declaration_file)
    }

    pub(crate) fn get_symbol_from_registered_file_target(
        &self,
        sym_id: SymbolId,
    ) -> Option<&tsz_binder::Symbol> {
        let file_idx = self.ctx.resolve_symbol_file_index(sym_id)?;
        self.ctx.get_binder_for_file(file_idx)?.get_symbol(sym_id)
    }

    pub(crate) fn symbol_has_class_declaration(&self, sym_id: SymbolId) -> bool {
        self.ctx.get_existing_def_id(sym_id).is_some_and(|def_id| {
            self.ctx.definition_store.get_kind(def_id) == Some(tsz_solver::def::DefKind::Class)
        }) || self
            .resolved_import_target_symbol(sym_id)
            .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::CLASS))
    }

    /// Resolve a lib-merged symbol back to its originating lib context.
    ///
    /// `merge_lib_contexts_into_binder` clones lib symbols into the program
    /// binder under fresh `SymbolId`s, so a merged id exists in no per-file
    /// binder and raw-id cross-file lookups cannot find it. The binder keeps
    /// the reverse mapping in `lib_symbol_reverse_remap`; this returns the
    /// owning lib binder (matched by pointer identity against
    /// `ctx.lib_contexts`) together with the lib-local `SymbolId`.
    pub(crate) fn lib_merged_symbol_origin(
        &self,
        sym_id: SymbolId,
    ) -> Option<(&crate::context::LibContext, SymbolId)> {
        let &(binder_ptr, local_id) = self.ctx.binder.lib_symbol_reverse_remap.get(&sym_id)?;
        let lib_ctx = self
            .ctx
            .lib_contexts
            .iter()
            .find(|lib| std::sync::Arc::as_ptr(&lib.binder) as usize == binder_ptr)?;
        lib_ctx.binder.get_symbol(local_id)?;
        Some((lib_ctx, local_id))
    }

    pub(crate) fn clear_delegated_symbol_cache_collisions(
        &self,
        checker: &mut CheckerState<'_>,
        delegate_binder: &tsz_binder::BinderState,
        preserve_class_sym: SymbolId,
    ) {
        for delegate_symbol in delegate_binder.symbols.iter() {
            let sym_id = delegate_symbol.id;
            let collides_with_parent_local =
                self.ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|parent_symbol| {
                        parent_symbol.escaped_name != delegate_symbol.escaped_name
                            || parent_symbol.decl_file_idx != delegate_symbol.decl_file_idx
                            || parent_symbol.flags != delegate_symbol.flags
                    });
            if collides_with_parent_local {
                checker.ctx.symbol_types.remove(&sym_id);
                checker.ctx.symbol_instance_types.remove(&sym_id);
                checker.ctx.symbol_to_def.borrow_mut().remove(&sym_id);
                checker.ctx.symbol_resolution_set.remove(&sym_id);
                if sym_id != preserve_class_sym {
                    checker.ctx.class_instance_resolution_set.remove(&sym_id);
                }
                checker.ctx.class_constructor_resolution_set.remove(&sym_id);
            }
        }
    }
}
