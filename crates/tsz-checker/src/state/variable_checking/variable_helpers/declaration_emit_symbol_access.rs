//! Symbol accessibility helpers for declaration emit: cross-binder lookup,
//! import reachability, local value resolution, and module specifier derivation.

use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;

impl<'a> CheckerState<'a> {
    pub(crate) fn get_symbol_from_any_binder(
        &self,
        sym_id: SymbolId,
    ) -> Option<&tsz_binder::Symbol> {
        self.ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| {
                // O(1) fast-path via resolve_symbol_file_index
                let file_idx = self.ctx.resolve_symbol_file_index(sym_id);
                if let Some(file_idx) = file_idx
                    && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
                    && let Some(sym) = binder.get_symbol(sym_id)
                {
                    return Some(sym);
                }
                self.ctx
                    .all_binders
                    .as_ref()
                    .and_then(|binders| binders.iter().find_map(|binder| binder.get_symbol(sym_id)))
            })
            .or_else(|| {
                self.ctx
                    .lib_contexts
                    .iter()
                    .find_map(|ctx| ctx.binder.get_symbol(sym_id))
            })
    }

    /// Returns true if the symbol is reachable from the current file through
    /// any module specifier already imported by the file, following named and
    /// wildcard re-export chains across files. In that case dts emit can
    /// synthesize a `typeof import("<specifier>").<name>` (or qualify through
    /// an existing alias) without requiring the symbol to have a direct local
    /// alias — matching tsc's `isSymbolAccessible` behaviour for declaration
    /// emit.
    ///
    /// This is intentionally name-agnostic: it works for any builtin or user
    /// symbol because reachability is decided by binder export tables and the
    /// checker's resolved-module map, not by matching identifier spellings in
    /// the source.
    pub(crate) fn symbol_reachable_via_local_imports(&self, target_sym_id: SymbolId) -> bool {
        if !target_sym_id.is_some() {
            return false;
        }
        let Some(target_symbol) = self.get_symbol_from_any_binder(target_sym_id) else {
            return false;
        };
        let target_name = target_symbol.escaped_name.clone();
        if target_name.is_empty() {
            return false;
        }

        let source_file_idx = self.ctx.current_file_idx;
        // The cross-file resolver lands on re-export alias symbols (the
        // `export { foo }` alias in the re-exporting file); resolve aliases
        // before comparing so the chain reaches the underlying declaration.
        let resolves_to_target = |export_name: &str, specifier: &str| -> bool {
            self.resolve_cross_file_export_from_file(specifier, export_name, Some(source_file_idx))
                .is_some_and(|resolved| {
                    let final_id = self
                        .resolve_alias_symbol(resolved, &mut AliasCycleTracker::new())
                        .unwrap_or(resolved);
                    final_id == target_sym_id
                })
        };

        let mut tried: FxHashSet<String> = FxHashSet::default();
        for (_, &local_sym_id) in self.ctx.binder.file_locals.iter() {
            let Some(local_sym) = self.ctx.binder.get_symbol(local_sym_id) else {
                continue;
            };
            if local_sym.flags & tsz_binder::symbol_flags::ALIAS == 0 {
                continue;
            }
            let Some(specifier) = local_sym.import_module.as_deref() else {
                continue;
            };
            if !tried.insert(specifier.to_string()) {
                continue;
            }

            // Fast path: the common case (no rename) is that the public
            // export name in the imported module equals the symbol's own
            // escaped name. Resolution flows through `program_module_exports`
            // and the program-wide re-export indices, which are the canonical
            // tables in the parallel pipeline (per-file binder maps can be
            // empty there).
            if resolves_to_target(&target_name, specifier) {
                return true;
            }

            // Slow path: walk every named export of the specifier. Covers
            // export-side renames (`export { internal as external }`) and
            // wildcard chains where the public name is not the symbol's own
            // escaped name. Bounded by the package's export count and gated
            // behind the fast-path miss.
            let Some(target_idx) = self
                .ctx
                .resolve_import_target_from_file(source_file_idx, specifier)
            else {
                continue;
            };
            let Some(target_binder) = self.ctx.get_binder_for_file(target_idx) else {
                continue;
            };
            let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
            let Some(target_file_name) = target_arena
                .source_files
                .first()
                .map(|sf| sf.file_name.clone())
            else {
                continue;
            };

            if let Some(module_table) = self
                .ctx
                .module_exports_for_module(target_binder, &target_file_name)
                && module_table
                    .iter()
                    .any(|(export_name, _)| resolves_to_target(export_name, specifier))
            {
                return true;
            }
        }

        false
    }

    pub(crate) fn local_value_name_resolves_to(&self, target_sym_id: SymbolId) -> bool {
        self.ctx
            .binder
            .file_locals
            .iter()
            .any(|(_, &local_sym_id)| {
                let Some(symbol) = self.ctx.binder.get_symbol(local_sym_id) else {
                    return false;
                };
                if symbol.is_type_only {
                    return false;
                }
                // Skip symbols that came from other files via globals merge.
                // In the merged program, file_locals includes globals from all files.
                // For TS4023 "cannot be named" checks, only symbols that are actually
                // declared in or imported into the current file count as accessible.
                // A symbol from another file that ended up in globals is NOT nameable
                // in the current file's declaration emit unless it's explicitly imported.
                let is_from_current_file = symbol.decl_file_idx == u32::MAX
                    || symbol.decl_file_idx == self.ctx.current_file_idx as u32;
                let is_import = symbol.flags & tsz_binder::symbol_flags::ALIAS != 0;
                if !is_from_current_file && !is_import {
                    return false;
                }
                if local_sym_id == target_sym_id {
                    return true;
                }

                self.ctx.binder.resolve_import_symbol(local_sym_id) == Some(target_sym_id)
            })
    }

    pub(crate) fn module_specifier_for_file(&self, file_idx: u32) -> Option<String> {
        if let Some(specifier) = self.ctx.module_specifiers.get(&file_idx) {
            return Some(specifier.clone());
        }

        let arena = self.ctx.get_arena_for_file(file_idx);
        let source_file = arena.source_files.first()?;
        let file_name = &source_file.file_name;
        let stem = file_name
            .rsplit_once('.')
            .map(|(base, _)| base)
            .unwrap_or(file_name);
        let basename = stem.rsplit_once('/').map(|(_, name)| name).unwrap_or(stem);
        Some(basename.to_string())
    }
}
