use crate::state::CheckerState;

/// Kill-switch for the program-wide `export =` fast path in
/// `resolve_named_export_via_export_equals_tracked`. When
/// `TSZ_DISABLE_EXPORT_EQUALS_FAST_PATH` is set to a non-empty, non-`0` value,
/// the resolver runs its full chain even when no module uses `export =`. Used
/// to verify the fast path produces byte-identical diagnostics.
pub(super) fn export_equals_fast_path_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("TSZ_DISABLE_EXPORT_EQUALS_FAST_PATH")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

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
        // Memoize only *root* resolutions: those entered with an empty `visited`
        // path and no module-key override. At that boundary the entry cycle
        // guard cannot fire (the root file is inserted fresh), so the result is
        // never the cycle-break sentinel `None`, and no inherited `visited`
        // truncation can perturb the traversal — the answer is the canonical,
        // path-independent target for `(file_idx, export_name)`. Inner recursive
        // calls (non-empty `visited`, or a `Some` module key) are path-sensitive
        // and bypass the cache entirely. This collapses the `O(names ×
        // export-edges)` re-walk barrel `export *` graphs otherwise pay when the
        // same export name is resolved from many import/usage sites.
        if !(visited.is_empty() && module_key.is_none()) {
            return self.resolve_export_in_file_uncached(
                file_idx,
                module_key,
                export_name,
                visited,
            );
        }

        let cache_key = (file_idx, export_name.to_owned());
        if let Some(cached) = self.ctx.reexport_resolution_cache.borrow().get(&cache_key) {
            return *cached;
        }

        let result =
            self.resolve_export_in_file_uncached(file_idx, module_key, export_name, visited);
        self.ctx
            .reexport_resolution_cache
            .borrow_mut()
            .insert(cache_key, result);
        result
    }

    fn resolve_export_in_file_uncached(
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

            // When multiple wildcard sources provide the same name, prefer a VALUE
            // export over type-only paths, including pure TYPE declarations and
            // value-bearing declarations reached through `export type *`.
            // TypeScript resolves type and value namespaces independently; a value
            // export from one `export *` source must not be shadowed by an earlier
            // type-only path in the list.
            let mut type_only_fallback: Option<(tsz_binder::SymbolId, usize)> = None;

            for (source_module, source_is_type_only) in &source_modules {
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
                    let is_pure_type = self
                        .ctx
                        .get_binder_for_file(source_idx)
                        .and_then(|b| b.get_symbol(result.0))
                        .is_some_and(|s| s.is_pure_type());
                    if (*source_is_type_only || is_pure_type) && type_only_fallback.is_none() {
                        type_only_fallback = Some(result);
                        continue;
                    }
                    return Some(result);
                }
            }

            if let Some(fallback) = type_only_fallback {
                return Some(fallback);
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

    /// Follow re-export alias chains from `start_file_idx`/`export_name` until reaching
    /// the original non-alias declaration.
    ///
    /// Named re-exports (`export type { X } from './impl'`) create an alias symbol in
    /// `module_exports` of the barrel file. `resolve_export_in_file` stops at that alias
    /// because it prioritises `module_exports` over the `reexports` table. This helper
    /// advances hop-by-hop: when the resolved symbol has `import_module` set it is itself
    /// a re-export alias, so we follow to the next target file/name until `import_module`
    /// is `None`. Cycles are detected via a `(file_idx, sym_id)` guard; the chain is
    /// bounded to 32 hops.
    pub(crate) fn resolve_reexport_chain_to_declaration(
        &self,
        start_file_idx: usize,
        export_name: &str,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        let mut current_file = start_file_idx;
        let mut current_name = export_name.to_owned();
        let mut chain_visited: rustc_hash::FxHashSet<(usize, u32)> =
            rustc_hash::FxHashSet::default();
        for _ in 0..32 {
            let mut visited = rustc_hash::FxHashSet::default();
            let (sym_id, actual_file) =
                self.resolve_export_in_file(current_file, &current_name, &mut visited)?;
            if !chain_visited.insert((actual_file, sym_id.0)) {
                return None;
            }
            let next_hop = self
                .ctx
                .get_binder_for_file(actual_file)
                .and_then(|binder| binder.get_symbol(sym_id))
                .and_then(|sym| {
                    sym.import_module().map(|m| {
                        let next_name = sym
                            .import_name()
                            .map(str::to_string)
                            .unwrap_or_else(|| current_name.clone());
                        let decl_file = if sym.decl_file_idx == u32::MAX {
                            actual_file
                        } else {
                            sym.decl_file_idx as usize
                        };
                        (m.to_string(), next_name, decl_file)
                    })
                });
            match next_hop {
                None => return Some((sym_id, actual_file)),
                Some((next_mod, next_name, decl_file)) => {
                    let next_file = self
                        .ctx
                        .resolve_import_target_from_file(decl_file, &next_mod)?;
                    current_file = next_file;
                    current_name = next_name;
                }
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
            for (source_module, is_type_only) in source_modules.iter() {
                if *is_type_only {
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
                    // Fold in module augmentations targeting the re-exported source
                    // module. `module_exports[source]` only contains the source
                    // file's own exports; augmentations declared in other files
                    // contribute additional names that must traverse every
                    // `export *` edge along with the source's direct exports.
                    self.merge_module_augmentation_namespace_exports(
                        result,
                        source_idx,
                        Some(source_module),
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
