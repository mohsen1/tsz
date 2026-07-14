//! Pass-1 symbol scan for `check_duplicate_identifiers`.
//!
//! Collects the per-file duplicate-check working set: the survivor symbols pass 2
//! must re-examine, cross-file conflict names, the `global_scope_conflict_cache`,
//! and the module/external flags. Also emits the whole-file `TS6200` summary when
//! eight or more identifiers collide across files. Extracted verbatim from
//! `duplicate_identifiers.rs` to keep that shard under the size limit.

use super::{DuplicateDeclList, DuplicateIdentifierScanState};
use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_binder::symbol_flags;

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
        let mut cross_file_conflicts = Vec::new();
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
            pass2_symbol_ids.push(sym_id);

            let mut has_local = false;
            let mut has_remote = false;
            for &decl_idx in &symbol.declarations {
                if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    for arena in arenas {
                        let is_local = std::ptr::eq(&**arena, self.ctx.arena);
                        if let Some(_flags) = self.declaration_symbol_flags(arena, decl_idx) {
                            if has_libs
                                && is_local
                                && !self.declaration_name_matches(decl_idx, &symbol.escaped_name)
                            {
                                continue;
                            }
                            if is_local {
                                has_local = true;
                            } else {
                                has_remote = true;
                            }
                        }
                    }
                } else {
                    let is_local = true; // Fallback
                    if let Some(_flags) = self.declaration_symbol_flags(self.ctx.arena, decl_idx) {
                        if has_libs
                            && is_local
                            && !self.declaration_name_matches(decl_idx, &symbol.escaped_name)
                        {
                            continue;
                        }
                        if is_local {
                            has_local = true;
                        } else {
                            has_remote = true;
                        }
                    }
                }
            }

            if !(module_augmentation_declarations.is_empty()
                && script_scope_declarations.is_empty()
                && global_scope_declarations.is_empty()
                && jsx_runtime_conflict_declarations.is_empty()
                && default_import_alias_conflicts.is_empty()
                && module_block_scoped_conflicts.is_empty())
            {
                has_remote = true;
            }

            if has_local && has_remote {
                // Interfaces always merge with other interfaces across files in TypeScript.
                let is_interface_merge = symbol.has_any_flags(symbol_flags::INTERFACE)
                    && !symbol.has_any_flags(
                        symbol_flags::FUNCTION_SCOPED_VARIABLE
                            | symbol_flags::BLOCK_SCOPED_VARIABLE
                            | symbol_flags::TYPE_ALIAS
                            | symbol_flags::REGULAR_ENUM
                            | symbol_flags::CONST_ENUM,
                    );
                // var declarations merge across script files (non-modules).
                let is_var_merge = !is_external_module
                    && symbol.has_any_flags(symbol_flags::FUNCTION_SCOPED_VARIABLE)
                    && !symbol.has_any_flags(
                        symbol_flags::BLOCK_SCOPED_VARIABLE
                            | symbol_flags::CLASS
                            | symbol_flags::FUNCTION
                            | symbol_flags::REGULAR_ENUM
                            | symbol_flags::CONST_ENUM
                            | symbol_flags::TYPE_ALIAS,
                    );
                // Function declarations merge across files via module augmentation.
                let is_function_merge = symbol.has_any_flags(symbol_flags::FUNCTION)
                    && !module_augmentation_declarations.is_empty();
                // Import aliases referencing remote declarations are valid merges.
                let is_alias_import_merge = symbol.has_any_flags(symbol_flags::ALIAS)
                    && symbol
                        .declarations
                        .iter()
                        .any(|&d| self.is_import_alias_node(d));
                if !is_interface_merge
                    && !is_var_merge
                    && !is_function_merge
                    && !is_alias_import_merge
                {
                    cross_file_conflicts.push(symbol.escaped_name.clone());
                }
            }
        }

        let emit_ts6200 = cross_file_conflicts.len() >= 8;
        if emit_ts6200 {
            cross_file_conflicts.sort();
            let list = cross_file_conflicts.join(", ");
            let message = format_message(
                diagnostic_messages::DEFINITIONS_OF_THE_FOLLOWING_IDENTIFIERS_CONFLICT_WITH_THOSE_IN_ANOTHER_FILE,
                &[&list],
            );
            // Report at position 0 (start of file) — tsc anchors TS6200 at the
            // SourceFile node which has pos=0, length=0.
            self.error_at_position(
                0,
                0,
                &message,
                diagnostic_codes::DEFINITIONS_OF_THE_FOLLOWING_IDENTIFIERS_CONFLICT_WITH_THOSE_IN_ANOTHER_FILE,
            );
        }

        DuplicateIdentifierScanState {
            has_libs,
            is_external_module,
            cross_file_conflicts,
            global_scope_conflict_cache,
            may_have_default_import_alias_conflicts,
            emit_ts6200,
            pass2_symbol_ids,
        }
    }
}
