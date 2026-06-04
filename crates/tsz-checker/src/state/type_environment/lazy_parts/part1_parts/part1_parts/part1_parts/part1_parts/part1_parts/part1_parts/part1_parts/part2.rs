impl<'a> CheckerState<'a> {
    /// Insert `type_id` for `def_id` into both type environments, carrying type params
    /// when present. Safe to call during recursive resolution; failed borrows are logged.
    fn try_insert_def_in_type_env(&mut self, def_id: tsz_solver::DefId, type_id: TypeId) {
        // insert_def_with_params with empty params is equivalent to insert_def, so we
        // unify both paths and avoid a conditional.
        let params = self.ctx.get_def_type_params(def_id).unwrap_or_default();
        match self.ctx.type_env.try_borrow_mut() {
            Ok(mut env) => env.insert_def_with_params(def_id, type_id, params.clone()),
            Err(e) => tracing::warn!(
                target_env = "type_env",
                error = ?e,
                "try_insert_def_in_type_env: borrow failed; insert skipped"
            ),
        }
        match self.ctx.type_environment.try_borrow_mut() {
            Ok(mut env) => env.insert_def_with_params(def_id, type_id, params),
            Err(e) => tracing::warn!(
                target_env = "type_environment",
                error = ?e,
                "try_insert_def_in_type_env: borrow failed; insert skipped"
            ),
        }
    }

    /// Resolve a `DefId` to a concrete type and insert a `DefId` mapping into the type environment.
    ///
    /// Returns the resolved type when a symbol bridge exists; returns `None` when the `DefId`
    /// is unknown to the checker. For `ANY`/`ERROR`, we intentionally skip env insertion.
    pub(crate) fn resolve_and_insert_def_type(
        &mut self,
        def_id: tsz_solver::DefId,
    ) -> Option<TypeId> {
        let lib_name = self.ctx.definition_store.get(def_id).and_then(|info| {
            (info.file_id == Some(u32::MAX)).then(|| self.ctx.types.resolve_atom(info.name))
        });
        if let Some(name) = lib_name
            && Self::in_cross_arena_interface_delegation()
            && self.ctx.has_lib_loaded()
        {
            if let Some(resolved) = self.resolve_lib_type_by_name(&name) {
                self.try_insert_def_in_type_env(def_id, resolved);
                return Some(resolved);
            }
            return Some(self.ctx.types.lazy(def_id));
        }

        let (sym_id, owner_file_idx) = self.ctx.def_symbol_identity(def_id)?;
        if let Some(file_idx) = owner_file_idx
            && file_idx != self.ctx.current_file_idx
        {
            self.ctx.register_symbol_file_target(sym_id, file_idx);
        }
        let resolved = if let Some(symbol) = self.get_cross_file_symbol(sym_id) {
            if symbol.has_any_flags(symbol_flags::CLASS) {
                // Keep class references in type position as instance types to avoid
                // constructor/instance split diagnostics (e.g. `Type 'Dataset' is not
                // assignable to type 'Dataset'` in parser harness regressions).
                // Also check class_instance_type_cache for in-progress builds
                // (Phase 2 partial type), preventing constructor type fallback.
                self.ctx
                    .symbol_instance_types
                    .get(&sym_id)
                    .copied()
                    .or_else(|| {
                        symbol
                            .primary_declaration()
                            .and_then(|idx| self.ctx.class_instance_type_cache.get(&idx).copied())
                    })
                    .or_else(|| {
                        owner_file_idx
                            .filter(|file_idx| *file_idx != self.ctx.current_file_idx)
                            .and_then(|file_idx| {
                                self.ctx
                                    .cached_cross_file_class_instance_type(sym_id, file_idx as u32)
                                    .map(|(instance_type, _)| instance_type)
                            })
                    })
                    .or_else(|| {
                        owner_file_idx
                            .filter(|file_idx| *file_idx != self.ctx.current_file_idx)
                            .and_then(|_| {
                                self.delegate_cross_arena_class_instance_type(sym_id)
                                    .map(|(instance_type, _)| instance_type)
                            })
                    })
                    .unwrap_or_else(|| self.get_type_of_symbol(sym_id))
            } else {
                self.get_type_of_symbol(sym_id)
            }
        } else {
            self.get_type_of_symbol(sym_id)
        };

        // If `get_type_of_symbol` returned the Lazy placeholder for this same def_id
        // (cycle-break), inserting it into `type_env` would shadow the DefinitionStore
        // fallback and cause the `resolved == type_id` guard in the caller to short-circuit.
        // Prefer the concrete body from DefinitionStore when it is already available.
        if lazy_def_id(self.ctx.types, resolved) == Some(def_id) {
            if let Some(body) = self.ctx.definition_store.get_body(def_id)
                && body != resolved
                && body != TypeId::ERROR
                && body != TypeId::ANY
            {
                self.try_insert_def_in_type_env(def_id, body);
                return Some(body);
            }
            return Some(resolved);
        }

        if resolved != TypeId::ERROR && resolved != TypeId::ANY {
            // Carry type params so Application evaluation via TypeEnvironment can
            // instantiate generic types correctly across checker contexts.
            self.try_insert_def_in_type_env(def_id, resolved);
        }
        Some(resolved)
    }
}
