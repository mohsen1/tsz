impl<'a> CheckerState<'a> {
    // =========================================================================
    // Import Member Validation
    // =========================================================================

    /// Check if a symbol exists locally in the target module and whether it's
    /// exported under a different name.
    ///
    /// Returns (`exists_locally`, `exported_as`) where:
    /// - `exists_locally`: true if the symbol is declared in the module's scope
    /// - `exported_as`: Some(name) if the symbol is exported under a different name,
    ///                None if not exported or exported with the same name
    #[tracing::instrument(level = "debug", skip(self), fields(module = %module_name, import = %import_name))]
    fn check_local_symbol_and_renamed_export(
        &self,
        module_name: &str,
        import_name: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) -> (bool, Option<String>) {
        tracing::trace!("Checking if symbol exists locally and is renamed");

        // Try to get the target module's binder
        let resolved_target = self.ctx.resolve_import_target_from_file_with_mode(
            self.ctx.current_file_idx,
            module_name,
            resolution_mode,
        );
        let target_binder = if let Some(target_idx) = resolved_target {
            tracing::trace!(target_idx, "Resolved import target");
            self.ctx.get_binder_for_file(target_idx)
        } else {
            tracing::trace!("Could not resolve import target");
            None
        };

        let target_binder = match target_binder {
            Some(binder) => {
                tracing::trace!("Found target binder directly");
                binder
            }
            None => {
                // Only fall back to all-binders scan when we couldn't resolve
                // the import target at all. If resolve_import_target succeeded
                // but get_binder_for_file returned None, we still know which
                // file the module points to — scanning all binders would find
                // symbols from unrelated files and cause false TS2459.
                if resolved_target.is_some() {
                    tracing::trace!(
                        "Import target resolved but binder not found, returning (false, None)"
                    );
                    return (false, None);
                }
                tracing::trace!("No direct target binder, checking all binders");
                // Use the global module binder index for O(1) lookup when available.
                if let Some(ref idx) = self.ctx.global_module_binder_index {
                    let normalized = module_name.trim_matches('"').trim_matches('\'');
                    let candidate_indices = idx
                        .get(module_name)
                        .into_iter()
                        .flatten()
                        .chain(idx.get(normalized).into_iter().flatten());
                    if let Some(all_binders) = &self.ctx.all_binders {
                        let mut seen = rustc_hash::FxHashSet::default();
                        for &binder_idx in candidate_indices {
                            if !seen.insert(binder_idx) {
                                continue;
                            }
                            if let Some(binder) = all_binders.get(binder_idx) {
                                tracing::trace!(binder_idx, "Found matching binder via index");
                                if let Some(exists) = self.check_symbol_in_binder(
                                    binder,
                                    import_name,
                                    module_name,
                                    resolution_mode,
                                ) {
                                    return exists;
                                }
                            }
                        }
                    }
                } else if let Some(all_binders) = &self.ctx.all_binders {
                    // Fallback: O(N) scan when index not built
                    let normalized = module_name.trim_matches('"').trim_matches('\'');
                    tracing::trace!(
                        num_binders = all_binders.len(),
                        "Checking all binders (fallback)"
                    );
                    for binder in all_binders.iter() {
                        if self.ctx.module_exports_contains_module(binder, module_name)
                            || self.ctx.module_exports_contains_module(binder, normalized)
                        {
                            tracing::trace!("Found matching binder via exports");
                            if let Some(exists) = self.check_symbol_in_binder(
                                binder,
                                import_name,
                                module_name,
                                resolution_mode,
                            ) {
                                return exists;
                            }
                        }
                    }
                }
                tracing::trace!("No binder found, returning (false, None)");
                return (false, None);
            }
        };

        if let Some(result) =
            self.check_symbol_in_binder(target_binder, import_name, module_name, resolution_mode)
        {
            tracing::trace!(exists_locally = result.0, renamed = ?result.1, "Got result from check_symbol_in_binder");
            result
        } else {
            tracing::trace!("check_symbol_in_binder returned None");
            (false, None)
        }
    }

    /// Helper to check if a symbol exists in a specific binder and whether it's renamed on export.
    #[tracing::instrument(level = "trace", skip(self, binder), fields(import = %import_name, module = %module_name))]
    fn check_symbol_in_binder(
        &self,
        binder: &tsz_binder::BinderState,
        import_name: &str,
        module_name: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) -> Option<(bool, Option<String>)> {
        // Check if the symbol exists in the binder's file-level symbol table
        // (not just the arena, which doesn't include all declarations).
        // Per-file binders share user symbols across files (lib_symbols_merged
        // contamination), so file_locals.has(name) alone is too permissive — it
        // can return true for a name declared in an unrelated file.
        // Verify the symbol's declaration is from THIS file (decl_file_idx).
        let target_file_idx = self
            .ctx
            .resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                resolution_mode,
            )
            .map(|i| i as u32);
        let mut symbol_exists = binder.file_locals.has(import_name)
            && match (
                binder
                    .file_locals
                    .get(import_name)
                    .and_then(|sym_id| binder.get_symbol(sym_id)),
                target_file_idx,
            ) {
                (Some(sym), Some(target_idx)) => {
                    sym.decl_file_idx == u32::MAX || sym.decl_file_idx == target_idx
                }
                _ => true,
            };
        if symbol_exists
            && let Some(sym_id) = binder.file_locals.get(import_name)
            && let Some(sym) = self.get_symbol_globally(sym_id)
            && let Some(augs) = self.ctx.binder.global_augmentations.get(import_name)
        {
            let all_are_global = sym
                .declarations
                .iter()
                .all(|d| augs.iter().any(|a| a.node == *d));
            if all_are_global {
                symbol_exists = false;
            }
        }
        tracing::trace!(symbol_exists, "Checked if symbol exists in binder");

        if !symbol_exists {
            return None;
        }

        // Symbol exists locally. Now check if it's exported under a different name.
        // We need to look at the module's export specifications to find renames.

        // Get the module's export table to check renamed exports
        let normalized = module_name.trim_matches('"').trim_matches('\'');
        let module_keys = [
            module_name,
            normalized,
            &format!("\"{normalized}\""),
            &format!("'{normalized}'"),
        ];

        // Also try to get the target file's name if available
        let (file_name, target_arena) = if let Some(target_idx) =
            self.ctx.resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                resolution_mode,
            ) {
            let arena = self.ctx.get_arena_for_file(target_idx as u32);
            (
                arena.source_files.first().map(|sf| sf.file_name.as_str()),
                Some(arena),
            )
        } else {
            (None, None)
        };

        if let Some(arena) = target_arena
            && let Some(renamed_as) = self.local_named_export_alias_for_import(arena, import_name)
        {
            return Some((true, Some(renamed_as)));
        }

        for &key in &module_keys {
            if let Some(exports) = self.ctx.module_exports_for_module(binder, key) {
                // Check if the symbol is exported under a different name
                // by looking through all export names. Skip the synthetic
                // `"export="` key — `export = Foo` is not a "renamed export"
                // for TS2460 purposes; tsc falls through to TS2497/TS2616
                // ("module can only be referenced via default-export") in
                // that case, so we let the caller emit the export-equals
                // diagnostic.
                for (export_name, sym_id) in exports.iter() {
                    if export_name.as_str() == "export=" {
                        continue;
                    }
                    if let Some(sym) = binder.symbols.get(*sym_id) {
                        let decl_arena = if sym.decl_file_idx == u32::MAX {
                            self.ctx.arena
                        } else {
                            self.ctx.get_arena_for_file(sym.decl_file_idx)
                        };
                        // Check if this symbol has a declaration with the import_name
                        let has_matching_name = sym.declarations.iter().any(|&decl_idx| {
                            Self::declaration_name_matches_string(decl_arena, decl_idx, import_name)
                        });

                        if has_matching_name && export_name.as_str() != import_name {
                            // Symbol is exported under a different name
                            return Some((true, Some(export_name.clone())));
                        }
                    }
                }
            }
        }

        // Also check with file name
        if let Some(fname) = file_name
            && let Some(exports) = self.ctx.module_exports_for_module(binder, fname)
        {
            for (export_name, sym_id) in exports.iter() {
                if export_name.as_str() == "export=" {
                    continue;
                }
                if let Some(sym) = binder.symbols.get(*sym_id) {
                    let decl_arena = if sym.decl_file_idx == u32::MAX {
                        self.ctx.arena
                    } else {
                        self.ctx.get_arena_for_file(sym.decl_file_idx)
                    };
                    let has_matching_name = sym.declarations.iter().any(|&decl_idx| {
                        Self::declaration_name_matches_string(decl_arena, decl_idx, import_name)
                    });

                    if has_matching_name && export_name.as_str() != import_name {
                        return Some((true, Some(export_name.clone())));
                    }
                }
            }
        }

        // If the module uses `export =`, the symbol may be the export target itself.
        // In that case it IS exported (just not as a named export), so don't report
        // it as "locally declared but not exported" (TS2459). Let the caller fall
        // through to the appropriate `export =` diagnostic (TS2616/TS2595/TS2597).
        let has_export_equals = module_keys.iter().any(|key| {
            binder
                .module_exports
                .get(*key)
                .is_some_and(|exports| exports.has("export="))
        }) || file_name.is_some_and(|fname| {
            binder
                .module_exports
                .get(fname)
                .is_some_and(|exports| exports.has("export="))
        });
        if has_export_equals {
            return None;
        }

        // Symbol exists locally but is not exported
        Some((true, None))
    }
}
