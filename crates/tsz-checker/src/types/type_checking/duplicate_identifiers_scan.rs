//! Pass-1 symbol scan for `check_duplicate_identifiers`.
//!
//! Collects the per-file duplicate-check working set: the survivor symbols pass 2
//! must re-examine, the `global_scope_conflict_cache`, and the module/external
//! flags. Extracted verbatim from `duplicate_identifiers.rs` to keep that shard
//! under the size limit.

use super::{DuplicateDeclList, DuplicateIdentifierScanState};
use crate::state::CheckerState;
use rustc_hash::FxHashMap;

impl<'a> CheckerState<'a> {
    /// Pass 1 of `check_duplicate_identifiers`: build the survivor working set and
    /// cross-file conflict state consumed by the per-symbol pass.
    pub(super) fn duplicate_identifiers_scan_symbols(&mut self) -> DuplicateIdentifierScanState {
        let has_libs = self.ctx.has_lib_loaded() || !self.ctx.binder.lib_symbol_ids.is_empty();
        let is_external_module = self
            .ctx
            .is_external_module_by_file
            .as_ref()
            .and_then(|m| crate::context::lookup_is_external_module_in_map(m, &self.ctx.file_name))
            .unwrap_or_else(|| self.ctx.binder.is_external_module());

        // Collect the user-code symbols this file's pass must examine. When libs
        // are loaded this skips the merged lib symbols that share the scope tables
        // (see `collect_duplicate_check_symbol_ids`).
        let mut symbol_ids = self.collect_duplicate_check_symbol_ids(has_libs);

        // Declarations inside `declare module {}` / `declare global {}` blocks are not
        // guaranteed to appear in top-level scope tables, but they still participate in
        // duplicate-name checks for the current file.
        self.extend_duplicate_symbol_ids_with_local_augmentation_decls(&mut symbol_ids);
        // One entry per distinct symbol name; bounded by the symbol set (#11617).
        let mut global_scope_conflict_cache: FxHashMap<String, DuplicateDeclList> =
            FxHashMap::with_capacity_and_hasher(symbol_ids.len(), Default::default());
        // Symbols that survive pass 1's conflict-free skip (declarations <= 1 with
        // no cross-file / augmentation / global-scope / jsx-runtime / default-import
        // / module-block conflicts). Pass 2 re-derives the same per-symbol conflict
        // helpers + declaration scan; for a typical file ~all symbols are
        // conflict-free, so re-running pass 2 over the full set only to re-hit the
        // identical skip is pure duplicate work (two identical hot scans in the
        // profile). Recording the survivors here lets pass 2 iterate just them.
        // `symbol_ids` is an `FxHashSet`; pass 1 (`iter`) and pass 2 (`into_iter`)
        // visit it in the same internal-table order, so pushing in pass-1 order
        // preserves pass 2's original relative order — byte-identical diagnostics.
        let mut pass2_symbol_ids: Vec<tsz_binder::SymbolId> = Vec::with_capacity(symbol_ids.len());
        let may_have_default_import_alias_conflicts = !is_external_module
            && self.ctx.all_arenas.is_some()
            && self.current_file_has_named_default_export_identifier();
        for &sym_id in &symbol_ids {
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };
            let module_augmentation_declarations = self
                .module_augmentation_conflict_declarations_for_current_file(&symbol.escaped_name);
            let script_scope_declarations = if self
                .symbol_is_current_file_top_level_script_declaration(&symbol.escaped_name, sym_id)
            {
                self.same_name_top_level_script_declarations_for_current_file(&symbol.escaped_name)
            } else {
                Vec::new()
            };
            let global_scope_declarations = if let Some(cached) =
                global_scope_conflict_cache.get(symbol.escaped_name.as_str())
            {
                cached.clone()
            } else {
                let declarations =
                    self.global_scope_conflict_declarations_for_current_file(&symbol.escaped_name);
                global_scope_conflict_cache
                    .insert(symbol.escaped_name.clone(), declarations.clone());
                declarations
            };
            let jsx_runtime_conflict_declarations =
                self.jsx_runtime_conflict_declarations_for_current_file(&symbol.escaped_name);
            let default_import_alias_conflicts = if may_have_default_import_alias_conflicts {
                self.default_import_alias_conflict_declarations_for_current_file(
                    &symbol.escaped_name,
                )
            } else {
                Vec::new()
            };
            let module_block_scoped_conflicts = self
                .module_file_block_scoped_conflict_declarations_for_current_file(
                    &symbol.escaped_name,
                    symbol.flags,
                );

            // Check if single NodeIndex has multiple arenas (cross-file duplicate with
            // same NodeIndex due to identical file structure). In this case, declarations
            // list has only 1 entry but represents 2+ actual declarations.
            if symbol.declarations.len() <= 1 {
                let has_cross_file = symbol.declarations.iter().any(|&decl_idx| {
                    self.ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .is_some_and(|arenas| arenas.len() > 1)
                });
                if !has_cross_file
                    && module_augmentation_declarations.is_empty()
                    && script_scope_declarations.is_empty()
                    && global_scope_declarations.is_empty()
                    && jsx_runtime_conflict_declarations.is_empty()
                    && default_import_alias_conflicts.is_empty()
                    && module_block_scoped_conflicts.is_empty()
                {
                    continue;
                }
            }

            // Survived the conflict-free skip: pass 2 must re-examine this symbol.
            // Record it so pass 2 iterates only survivors instead of the full set.
            // tsc 7.0.2 (typescript-go `reportMergeSymbolError`) always emits
            // per-declaration TS2300 for cross-file duplicate identifiers; the legacy
            // TS6.x whole-file TS6200 batch summary (`>= 8` conflicts) was removed, so
            // pass 2 owns every emission with no batching gate here.
            pass2_symbol_ids.push(sym_id);
        }

        DuplicateIdentifierScanState {
            has_libs,
            is_external_module,
            global_scope_conflict_cache,
            may_have_default_import_alias_conflicts,
            pass2_symbol_ids,
        }
    }
}
