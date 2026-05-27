use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    /// Follow re-export chains across binder boundaries to find an exported symbol.
    /// Returns `(SymbolId, file_idx)` where `file_idx` is the actual file that owns
    /// the symbol, so callers can record the correct cross-file origin.
    pub(crate) fn resolve_export_in_file(
        &self,
        file_idx: usize,
        export_name: &str,
        visited: &mut rustc_hash::FxHashSet<usize>,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        self.resolve_export_in_file_with_module_key(file_idx, None, export_name, visited)
    }

    fn resolve_export_in_file_with_module_key(
        &self,
        file_idx: usize,
        module_key: Option<&str>,
        export_name: &str,
        visited: &mut rustc_hash::FxHashSet<usize>,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        if !visited.insert(file_idx) {
            return None;
        }

        let target_binder = self.ctx.get_binder_for_file(file_idx)?;

        let target_arena = self.ctx.get_arena_for_file(file_idx as u32);
        let target_file_name = target_arena.source_files.first()?.file_name.clone();

        // Files with an unambiguous ESM extension (.mjs/.mts/.d.mts) generally
        // do not synthesize a `default` export from `export =`, because
        // `export =` is a syntax error in ESM (TS1203). `module: preserve` is
        // the exception: it permits CJS and ESM syntax side-by-side and tsc
        // treats `export =` as the default-import target there.
        let target_is_explicit_esm = {
            let n = target_file_name.as_str();
            n.ends_with(".mjs") || n.ends_with(".mts")
        };
        let default_skips_export_equals = export_name == "default"
            && (target_is_explicit_esm || self.source_file_idx_is_js_with_esm_syntax(file_idx))
            && self.ctx.compiler_options.module != tsz_common::common::ModuleKind::Preserve;

        if let Some(exports) = self
            .ctx
            .module_exports_for_module(target_binder, &target_file_name)
            .or_else(|| {
                module_key.and_then(|key| self.ctx.module_exports_for_module(target_binder, key))
            })
        {
            let sym_id = if default_skips_export_equals {
                exports
                    .get("default")
                    .filter(|id| target_binder.get_symbol(*id).is_some())
            } else {
                self.resolve_export_from_table(target_binder, exports, export_name)
            };
            if let Some(sym_id) = sym_id {
                return Some((sym_id, file_idx));
            }
        }

        if let Some(reexports) = self
            .ctx
            .reexports_for_file(target_binder, &target_file_name)
            .or_else(|| module_key.and_then(|key| self.ctx.reexports_for_file(target_binder, key)))
            && let Some((source_module, original_name)) = reexports.get(export_name)
        {
            let name = original_name.as_deref().unwrap_or(export_name);
            if let Some(source_idx) = self
                .ctx
                .resolve_import_target_from_file(file_idx, source_module)
                && let Some(result) = self.resolve_export_in_file_with_module_key(
                    source_idx,
                    Some(source_module),
                    name,
                    visited,
                )
            {
                return Some(result);
            }
        }

        // Check wildcard re-exports before file_locals so that
        // `export * from './other'` is followed to the actual declaring file.
        // file_locals may contain merged globals that shadow re-exported symbols.
        if let Some(source_modules) = self
            .ctx
            .wildcard_reexports_for_file(target_binder, &target_file_name)
            .or_else(|| {
                module_key.and_then(|key| self.ctx.wildcard_reexports_for_file(target_binder, key))
            })
        {
            let source_modules = source_modules.clone();
            for source_module in &source_modules {
                if let Some(source_idx) = self
                    .ctx
                    .resolve_import_target_from_file(file_idx, source_module)
                    && let Some(result) = self.resolve_export_in_file_with_module_key(
                        source_idx,
                        Some(source_module),
                        export_name,
                        visited,
                    )
                {
                    return Some(result);
                }
            }
        }

        // Module augmentations should apply after direct exports and re-export chains,
        // so an augmentation does not mask a concrete exported declaration.
        if let Some((sym_id, augmenting_file_idx)) =
            self.resolve_module_augmentation_export_for_file(file_idx, export_name)
        {
            return Some((sym_id, augmenting_file_idx));
        }

        // Last resort: check file_locals only for script files or binding edge
        // cases where module_exports was not populated. Real external modules
        // must not leak local imports through their public surface.
        let has_module_exports = self
            .ctx
            .module_exports_for_module(target_binder, &target_file_name)
            .is_some_and(|e| !e.is_empty());
        if has_module_exports || target_binder.is_external_module {
            return None;
        }
        if export_name == "default"
            && !default_skips_export_equals
            && let Some(sym_id) = target_binder.file_locals.get("export=")
        {
            return Some((sym_id, file_idx));
        }
        if let Some(sym_id) = target_binder.file_locals.get(export_name) {
            let has_value = target_binder
                .get_symbol(sym_id)
                .is_some_and(|s| !s.is_type_only);
            if has_value {
                return Some((sym_id, file_idx));
            }
        }

        None
    }

    /// Collect all symbols reachable through re-export chains into the given `SymbolTable`.
    pub(super) fn collect_reexported_symbols(
        &self,
        file_idx: usize,
        module_key: Option<&str>,
        result: &mut tsz_binder::SymbolTable,
        visited: &mut rustc_hash::FxHashSet<usize>,
    ) {
        if !visited.insert(file_idx) {
            return;
        }

        let Some(target_binder) = self.ctx.get_binder_for_file(file_idx) else {
            return;
        };
        let Some(target_file_name) = self
            .ctx
            .get_arena_for_file(file_idx as u32)
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return;
        };

        if let Some(source_modules) = self
            .ctx
            .wildcard_reexports_for_file(target_binder, &target_file_name)
            .or_else(|| {
                module_key.and_then(|key| self.ctx.wildcard_reexports_for_file(target_binder, key))
            })
        {
            let source_modules = source_modules.clone();
            let type_only_flags = self
                .ctx
                .wildcard_reexports_type_only_for_file(target_binder, &target_file_name)
                .or_else(|| {
                    module_key.and_then(|key| {
                        self.ctx
                            .wildcard_reexports_type_only_for_file(target_binder, key)
                    })
                })
                .cloned();
            for (i, source_module) in source_modules.iter().enumerate() {
                let is_type_only = type_only_flags
                    .as_ref()
                    .and_then(|flags| flags.get(i).map(|(_, is_to)| *is_to))
                    .unwrap_or(false);
                if is_type_only {
                    continue;
                }
                if let Some(source_idx) = self
                    .ctx
                    .resolve_import_target_from_file(file_idx, source_module)
                    && let Some(source_binder) = self.ctx.get_binder_for_file(source_idx)
                {
                    let source_file_name = self
                        .ctx
                        .get_arena_for_file(source_idx as u32)
                        .source_files
                        .first()
                        .map(|sf| sf.file_name.clone());
                    if let Some(exports) = source_file_name
                        .as_ref()
                        .and_then(|file_name| {
                            self.ctx.module_exports_for_module(source_binder, file_name)
                        })
                        .or_else(|| {
                            self.ctx
                                .module_exports_for_module(source_binder, source_module)
                        })
                    {
                        for (name, sym_id) in exports.iter() {
                            if !result.has(name) {
                                result.set(name.to_string(), *sym_id);
                            }
                        }
                    }
                    self.collect_reexported_symbols(
                        source_idx,
                        Some(source_module),
                        result,
                        visited,
                    );
                }
            }
        }

        if let Some(reexports) = self
            .ctx
            .reexports_for_file(target_binder, &target_file_name)
            .or_else(|| module_key.and_then(|key| self.ctx.reexports_for_file(target_binder, key)))
        {
            let reexports = reexports.clone();
            for (exported_name, (source_module, original_name)) in &reexports {
                if !result.has(exported_name) {
                    let name = original_name.as_deref().unwrap_or(exported_name);
                    if let Some(source_idx) = self
                        .ctx
                        .resolve_import_target_from_file(file_idx, source_module)
                    {
                        let mut inner_visited = rustc_hash::FxHashSet::default();
                        inner_visited.extend(visited.iter().copied());
                        if let Some((sym_id, _actual_file_idx)) = self
                            .resolve_export_in_file_with_module_key(
                                source_idx,
                                Some(source_module),
                                name,
                                &mut inner_visited,
                            )
                        {
                            result.set(exported_name.to_string(), sym_id);
                        }
                    }
                }
            }
        }
    }
}
