impl<'a> CheckerContext<'a> {
    /// Returns `true` if the shared `DefinitionStore` has been pre-populated
    /// (i.e., it contains definitions registered at merge time, not just an
    /// empty store created by the default constructor).
    ///
    /// When true, `warm_local_caches_from_shared_store()` can replace the
    /// more expensive `pre_populate_def_ids_from_binder()` +
    /// `pre_populate_def_ids_from_lib_binders()` calls.
    pub fn has_shared_store(&self) -> bool {
        !self.definition_store.is_empty()
    }

    /// Resolve heritage for definitions whose extends/implements targets were
    /// not found during their batch's pass 2 (cross-batch heritage).
    ///
    /// This handles the common case where a user class extends a lib type
    /// (e.g., `class MyError extends Error`): when `pre_populate_def_ids_from_binder`
    /// processes the user file, the lib type's `DefId` hasn't been registered yet
    /// (lib binders are pre-populated separately). After ALL pre-population batches
    /// complete, this method resolves the remaining heritage using the
    /// `DefinitionStore`'s name index, which now contains entries from all batches.
    ///
    /// Called once during checker construction after all `pre_populate_*` methods.
    /// Returns the number of heritage links resolved.
    pub fn resolve_cross_batch_heritage(&self) -> usize {
        use tsz_solver::def::DefKind;

        let mut resolved_count = 0;

        // Collect all semantic_defs from all sources (primary binder + all_binders).
        // The shared DefinitionStore's name_to_defs index is already populated from
        // all pre-population batches, so name-based lookups will find targets from
        // any batch (user files, lib files, cross-file binders).
        let sources: Vec<
            &rustc_hash::FxHashMap<tsz_binder::SymbolId, tsz_binder::SemanticDefEntry>,
        > = {
            // `&*x.semantic_defs` dereferences the `Arc<FxHashMap<...>>` so the
            // resulting reference targets the underlying map (the type the Vec
            // expects), not the Arc wrapper.
            let mut v = vec![&*self.binder.semantic_defs];
            for lib_ctx in self.lib_contexts.iter() {
                v.push(&*lib_ctx.binder.semantic_defs);
            }
            if let Some(ref binders) = self.all_binders {
                for binder in binders.iter() {
                    v.push(&*binder.semantic_defs);
                }
            }
            v
        };

        for source in &sources {
            for (&sym_id, entry) in *source {
                let def_id = match self.definition_store.find_def_by_symbol(sym_id.0) {
                    Some(id) => id,
                    None => continue,
                };

                // Skip if extends is already wired (from pre-populate Pass 3)
                if let Some(info) = self.definition_store.get(def_id)
                    && info.extends.is_some()
                {
                    continue;
                }

                // Resolve extends_names → extends
                for name_str in &entry.extends_names {
                    if name_str.contains('.') {
                        continue;
                    }
                    let name_atom = self.types.intern_string(name_str);
                    if let Some(candidates) = self.definition_store.find_defs_by_name(name_atom) {
                        for &candidate_id in &candidates {
                            if candidate_id == def_id {
                                continue;
                            }
                            if let Some(info) = self.definition_store.get(candidate_id)
                                && matches!(info.kind, DefKind::Class | DefKind::Interface)
                            {
                                self.definition_store.set_extends(def_id, candidate_id);
                                resolved_count += 1;
                                break;
                            }
                        }
                    }
                    break; // only first extends name
                }

                // Resolve implements_names → implements
                if !entry.implements_names.is_empty() {
                    let mut resolved = Vec::new();
                    for name_str in &entry.implements_names {
                        if name_str.contains('.') {
                            continue;
                        }
                        let name_atom = self.types.intern_string(name_str);
                        if let Some(candidates) = self.definition_store.find_defs_by_name(name_atom)
                        {
                            for &candidate_id in &candidates {
                                if candidate_id == def_id {
                                    continue;
                                }
                                if let Some(info) = self.definition_store.get(candidate_id)
                                    && matches!(info.kind, DefKind::Interface | DefKind::Class)
                                {
                                    resolved.push(candidate_id);
                                    break;
                                }
                            }
                        }
                    }
                    if !resolved.is_empty() {
                        self.definition_store
                            .set_implements(def_id, resolved.clone());
                        resolved_count += resolved.len();
                    }
                }
            }
        }

        resolved_count
    }
}
