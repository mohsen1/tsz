//! Cross-arena alias shortcut and the symbol-arena/cross-file symbol-type
//! cache key plumbing used by `delegate_cross_arena_symbol_resolution`.
//!
//! Split out of `cross_file.rs` (2000-line shard cap).

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::perf_counters::CrossArenaAliasShortcutOutcome;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn try_resolve_cross_arena_named_alias_without_child(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        use CrossArenaAliasShortcutOutcome as AliasOutcome;

        let (module_name, import_name, alias_source_file_idx) = {
            let Some(symbol) = self.get_cross_file_symbol(sym_id) else {
                tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                    AliasOutcome::MissingSymbol,
                );
                return None;
            };
            if symbol.flags & symbol_flags::ALIAS == 0 {
                tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                    AliasOutcome::NotAlias,
                );
                return None;
            }
            let Some(module_name) = symbol.import_module().map(str::to_string) else {
                tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                    AliasOutcome::MissingModule,
                );
                return None;
            };
            let Some(import_name) = symbol.import_name().map(str::to_string) else {
                tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                    AliasOutcome::MissingImportName,
                );
                return None;
            };
            if import_name == "*" {
                tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                    AliasOutcome::NamespaceImport,
                );
                return None;
            }
            if import_name == "default" {
                tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                    AliasOutcome::DefaultImport,
                );
                return None;
            }
            let Some(alias_source_file_idx) = (symbol.decl_file_idx != u32::MAX)
                .then_some(symbol.decl_file_idx as usize)
                .or_else(|| self.ctx.resolve_symbol_file_index(sym_id))
            else {
                tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                    AliasOutcome::MissingAliasFile,
                );
                return None;
            };
            (module_name, import_name, alias_source_file_idx)
        };

        let alias_cache_file_idx = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .unwrap_or(alias_source_file_idx);
        let Some(target_sym_id) = self.resolve_cross_file_export_from_file(
            &module_name,
            &import_name,
            Some(alias_source_file_idx),
        ) else {
            tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                AliasOutcome::MissingTarget,
            );
            return None;
        };
        if target_sym_id == sym_id {
            tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                AliasOutcome::SelfTarget,
            );
            return None;
        }

        let Some(target_flags) = self
            .get_cross_file_symbol(target_sym_id)
            .map(|symbol| symbol.flags)
        else {
            tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                AliasOutcome::MissingTargetSymbol,
            );
            return None;
        };
        if target_flags & symbol_flags::ALIAS != 0 {
            tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                AliasOutcome::TargetAlias,
            );
            return None;
        }

        let target_binder = self
            .ctx
            .resolve_symbol_file_index(target_sym_id)
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .unwrap_or(self.ctx.binder);
        if self
            .ctx
            .alias_partner_for(target_binder, target_sym_id)
            .is_some()
        {
            tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                AliasOutcome::AliasPartner,
            );
            return None;
        }

        let target_is_interface_value_merge = target_flags & symbol_flags::INTERFACE != 0
            && target_flags & (symbol_flags::VARIABLE | symbol_flags::FUNCTION) != 0;
        if target_is_interface_value_merge {
            tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                AliasOutcome::InterfaceValueMerge,
            );
            return None;
        }

        let target_file_idx = self
            .ctx
            .resolve_symbol_file_index(target_sym_id)
            .or_else(|| {
                self.ctx
                    .resolve_import_target_from_file(alias_source_file_idx, &module_name)
            });
        if let Some(file_idx) = target_file_idx {
            self.ctx
                .register_symbol_file_target(target_sym_id, file_idx);
        }
        let (mut result, params) = if target_flags & symbol_flags::TYPE_ALIAS != 0 {
            target_file_idx
                .and_then(|file_idx| {
                    self.ctx
                        .cached_cross_file_symbol_type(target_sym_id, file_idx as u32)
                })
                .map(|(cached_type, cached_params)| (cached_type, cached_params.as_ref().clone()))
                .or_else(|| {
                    let resolved = self.direct_source_file_type_alias_result(
                        target_sym_id,
                        target_file_idx,
                        true,
                    )?;
                    if let Some(file_idx) = target_file_idx {
                        self.ctx.cache_cross_file_symbol_type(
                            target_sym_id,
                            file_idx as u32,
                            resolved.0,
                            resolved.1.clone(),
                        );
                    }
                    Some(resolved)
                })
                .unwrap_or_else(|| {
                    let resolved = self.type_reference_symbol_type_with_params(target_sym_id);
                    if let Some(file_idx) = target_file_idx {
                        self.ctx.cache_cross_file_symbol_type(
                            target_sym_id,
                            file_idx as u32,
                            resolved.0,
                            resolved.1.clone(),
                        );
                    }
                    resolved
                })
        } else {
            (self.get_type_of_symbol(target_sym_id), Vec::new())
        };
        result = self.apply_module_augmentations(&module_name, &import_name, result);
        if result == TypeId::ERROR {
            tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                AliasOutcome::ErrorResult,
            );
            return None;
        }
        if result == TypeId::UNKNOWN {
            tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                AliasOutcome::UnknownResult,
            );
            return None;
        }

        self.ctx.symbol_types.insert(sym_id, result);
        self.ctx.cache_cross_file_symbol_type(
            sym_id,
            alias_cache_file_idx as u32,
            result,
            params.clone(),
        );
        tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(AliasOutcome::Success);

        Some((result, params))
    }

    pub(super) fn cached_symbol_arena_or_cross_file_symbol_type(
        &self,
        sym_id: SymbolId,
        file_idx: usize,
        source_cache_scope: u64,
        symbol_type_cache_from_symbol_arena: bool,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        let file_idx = file_idx as u32;
        let cached = if symbol_type_cache_from_symbol_arena {
            self.ctx.cached_stable_source_file_symbol_arena_type(
                sym_id,
                file_idx,
                source_cache_scope,
            )
        } else {
            self.ctx.cached_cross_file_symbol_type(sym_id, file_idx)
        };
        cached.map(|(cached_type, cached_params)| (cached_type, cached_params.as_ref().clone()))
    }

    pub(super) fn cache_symbol_arena_or_cross_file_symbol_type(
        &self,
        sym_id: SymbolId,
        file_idx: usize,
        source_cache_scope: u64,
        symbol_type_cache_from_symbol_arena: bool,
        type_id: TypeId,
        type_params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        let file_idx = file_idx as u32;
        if !symbol_type_cache_from_symbol_arena {
            self.ctx
                .cache_cross_file_symbol_type(sym_id, file_idx, type_id, type_params);
            return;
        }

        self.ctx.cache_stable_source_file_symbol_arena_type(
            sym_id,
            file_idx,
            source_cache_scope,
            type_id,
            type_params,
        );
    }
}
