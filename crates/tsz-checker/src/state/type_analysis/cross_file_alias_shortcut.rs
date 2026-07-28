//! Cross-arena alias shortcut and the symbol-arena/cross-file symbol-type
//! cache key plumbing used by `delegate_cross_arena_symbol_resolution`.
//!
//! Split out of `cross_file.rs` (2000-line shard cap).

use super::cross_file_direct_alias_chain::SourceFileAliasSymbol;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::perf_counters::CrossArenaAliasShortcutOutcome;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Resolve the local pure type alias named by an explicit
    /// `export default Alias` declaration.
    ///
    /// This deliberately excludes `export =`, synthetic defaults, values,
    /// merged symbols, and re-export aliases. Top-level default-import
    /// shortcuts set `require_generic`; nested alias-proof leaves also admit
    /// non-generic pure aliases. The caller still requires the existing
    /// direct-source proof to lower the body, so a rejected body stays on the
    /// child-checker fallback.
    pub(super) fn explicit_default_export_pure_type_alias_target<'b>(
        &'b self,
        module_name: &str,
        alias_source_file_idx: usize,
        require_generic: bool,
    ) -> Option<SourceFileAliasSymbol<'b>> {
        let target_file_idx = self
            .ctx
            .resolve_import_target_from_file(alias_source_file_idx, module_name)?;
        self.explicit_default_export_pure_type_alias_in_file(target_file_idx, require_generic)
            .map(|(_, target)| target)
    }

    /// Resolve the declaration identity named by an explicit default export.
    ///
    /// Type-reference construction consumes only the owner-qualified identity;
    /// the direct alias path remains responsible for proving and registering
    /// the body and parameters.
    pub(crate) fn explicit_default_export_pure_type_alias_identity(
        &self,
        module_name: &str,
        alias_source_file_idx: usize,
        require_generic: bool,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        let target = self.explicit_default_export_pure_type_alias_target(
            module_name,
            alias_source_file_idx,
            require_generic,
        )?;
        Some((target.sym_id, target.file_idx?))
    }

    /// Resolve an explicit default-export symbol and the pure generic alias it
    /// names within a known source file.
    ///
    /// Returning both identities lets direct cross-file delegation replace the
    /// binder's synthetic `default` export symbol without relying on its
    /// value-like flags or rendered name.
    pub(super) fn explicit_default_export_pure_type_alias_in_file<'b>(
        &'b self,
        target_file_idx: usize,
        require_generic: bool,
    ) -> Option<(SymbolId, SourceFileAliasSymbol<'b>)> {
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_file_name = &target_arena.source_files.first()?.file_name;
        let exports = self
            .ctx
            .module_exports_for_module(target_binder, target_file_name)?;

        // `resolve_export_from_table(..., "default")` intentionally treats an
        // `export =` entry as a default provider. This shortcut must not: the
        // export-equals and synthetic-default compatibility paths own those
        // semantics.
        if exports.get("export=").is_some() {
            return None;
        }

        let default_sym_id = exports.get("default")?;
        let default_symbol = target_binder.get_symbol(default_sym_id)?;
        if default_symbol.flags & symbol_flags::ALIAS == 0
            || default_symbol.import_module().is_some()
        {
            return None;
        }

        // The binder's explicit-default symbol points at the export clause. A
        // bare type-alias identifier currently carries value-like metadata on
        // that synthetic symbol, so establish type-only meaning from the local
        // target's flags below. Require the structural parent instead of
        // accepting any symbol named `default`, then resolve the clause's
        // identifier through file locals.
        let (exported_name, target_name_idx) = default_symbol
            .declarations
            .iter()
            .copied()
            .find_map(|decl_idx| {
                let decl_node = target_arena.get(decl_idx)?;
                let export_decl_idx = target_arena.get_extended(decl_idx)?.parent;
                let export_decl_node = target_arena.get(export_decl_idx)?;
                let export_decl = target_arena.get_export_decl(export_decl_node)?;
                if !export_decl.is_default_export
                    || export_decl.module_specifier.is_some()
                    || export_decl.export_clause != decl_idx
                {
                    return None;
                }

                if decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                    let alias = target_arena.get_type_alias(decl_node)?;
                    return target_arena
                        .get(alias.name)
                        .and_then(|name| target_arena.get_identifier(name))
                        .map(|identifier| (identifier.escaped_text.as_str(), alias.name));
                }

                target_arena
                    .get_identifier(decl_node)
                    .map(|identifier| (identifier.escaped_text.as_str(), decl_idx))
            })?;

        // Library stamping can displace a module-local declaration in
        // `file_locals` when both use the same spelling (for example a local
        // alias named `Record`). The lexical scope table still owns the exact
        // declaration. Resolve from the export target node first and use
        // `file_locals` only as the ordinary fast fallback.
        let target_sym_id = target_binder
            .resolve_identifier_with_filter(target_arena, target_name_idx, &[], |candidate| {
                target_binder.get_symbol(candidate).is_some_and(|symbol| {
                    symbol.escaped_name == exported_name
                        && symbol.flags & symbol_flags::TYPE_ALIAS != 0
                })
            })
            .or_else(|| target_binder.file_locals.get(exported_name))?;
        if target_sym_id == default_sym_id {
            return None;
        }
        let target_symbol = target_binder.get_symbol(target_sym_id)?;
        if target_symbol.flags & symbol_flags::TYPE_ALIAS == 0
            || target_symbol.flags
                & (symbol_flags::ALIAS
                    | symbol_flags::VALUE
                    | symbol_flags::CLASS
                    | symbol_flags::INTERFACE
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE)
                != 0
            || target_symbol.declarations.len() != 1
            || self
                .ctx
                .alias_partner_for(target_binder, target_sym_id)
                .is_some()
        {
            return None;
        }

        let target_decl = target_arena.get(target_symbol.declarations[0])?;
        let target_alias = target_arena.get_type_alias(target_decl)?;
        if require_generic
            && target_alias
                .type_parameters
                .as_ref()
                .is_none_or(|params| params.nodes.is_empty())
        {
            return None;
        }

        Some((
            default_sym_id,
            SourceFileAliasSymbol {
                arena: target_arena,
                binder: target_binder,
                file_idx: Some(target_file_idx),
                sym_id: target_sym_id,
            },
        ))
    }

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
            // A local default import is resolved relative to the requester even
            // after its colliding target raw id has been registered in the
            // cross-file overlay. Keep named-import ownership unchanged.
            let alias_source_file_idx =
                if import_name == "default" && self.local_import_alias(sym_id).is_some() {
                    Some(self.ctx.current_file_idx)
                } else {
                    (symbol.decl_file_idx != u32::MAX)
                        .then_some(symbol.decl_file_idx as usize)
                        .or_else(|| self.ctx.resolve_symbol_file_index(sym_id))
                };
            let Some(alias_source_file_idx) = alias_source_file_idx else {
                tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                    AliasOutcome::MissingAliasFile,
                );
                return None;
            };
            (module_name, import_name, alias_source_file_idx)
        };

        let is_default_import = import_name == "default";
        // A requester-local default import can share its raw `SymbolId` with
        // the exported target. Its declaration already provides the exact
        // requester owner; do not re-read the lossy raw-id owner map for this
        // alias cache key. Preserve the existing named-import behavior.
        let alias_cache_file_idx = if is_default_import {
            alias_source_file_idx
        } else {
            self.ctx
                .resolve_symbol_file_index(sym_id)
                .unwrap_or(alias_source_file_idx)
        };
        let (target_sym_id, explicit_default_target_file_idx) = if is_default_import {
            let Some(target) = self.explicit_default_export_pure_type_alias_target(
                &module_name,
                alias_source_file_idx,
                true,
            ) else {
                tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                    AliasOutcome::DefaultImport,
                );
                return None;
            };
            (target.sym_id, target.file_idx)
        } else {
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
            (target_sym_id, None)
        };
        // `SymbolId` is binder-local. Equal raw ids are only a self-target when
        // they also belong to the same file; independent source binders commonly
        // allocate both the import alias and its exported target as `SymbolId(0)`.
        let target_is_same_symbol = target_sym_id == sym_id
            && explicit_default_target_file_idx
                .is_none_or(|target_file_idx| target_file_idx == alias_source_file_idx);
        if target_is_same_symbol {
            tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                AliasOutcome::SelfTarget,
            );
            return None;
        }

        // The explicit-default branch already proved the target's owning file.
        // Read it from that binder so a colliding requester-local raw id cannot
        // redirect this query back to the import alias. Preserve the existing
        // cross-file lookup for named imports.
        let target_symbol = explicit_default_target_file_idx
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .and_then(|binder| binder.get_symbol(target_sym_id))
            .or_else(|| self.get_cross_file_symbol(target_sym_id));
        let Some(target_flags) = target_symbol.map(|symbol| symbol.flags) else {
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

        let target_binder = explicit_default_target_file_idx
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .or_else(|| {
                self.ctx
                    .resolve_symbol_file_index(target_sym_id)
                    .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            })
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

        let target_file_idx = explicit_default_target_file_idx
            .or_else(|| self.ctx.resolve_symbol_file_index(target_sym_id))
            .or_else(|| {
                self.ctx
                    .resolve_import_target_from_file(alias_source_file_idx, &module_name)
            });
        if !is_default_import && let Some(file_idx) = target_file_idx {
            self.ctx
                .register_symbol_file_target(target_sym_id, file_idx);
        }
        let (mut result, params) = if target_flags & symbol_flags::TYPE_ALIAS != 0 {
            let direct_cache_scope =
                is_default_import.then(|| self.ctx.source_file_symbol_type_cache_scope());
            let cached_or_direct = target_file_idx
                .and_then(|file_idx| {
                    if let Some(scope) = direct_cache_scope {
                        self.ctx.cached_stable_source_file_symbol_arena_type(
                            target_sym_id,
                            file_idx as u32,
                            scope,
                        )
                    } else {
                        self.ctx
                            .cached_cross_file_symbol_type(target_sym_id, file_idx as u32)
                    }
                })
                .map(|(cached_type, cached_params)| (cached_type, cached_params.as_ref().clone()))
                .or_else(|| {
                    let previous_target_override = if is_default_import {
                        target_file_idx.map(|file_idx| {
                            let previous =
                                self.ctx.local_symbol_file_target_override(target_sym_id);
                            self.ctx
                                .register_symbol_file_target(target_sym_id, file_idx);
                            previous
                        })
                    } else {
                        None
                    };
                    let resolved = self.direct_source_file_type_alias_result(
                        target_sym_id,
                        target_file_idx,
                        true,
                    );
                    if let Some(previous) = previous_target_override {
                        self.ctx
                            .restore_local_symbol_file_target_override(target_sym_id, previous);
                    }
                    let resolved = resolved?;
                    if let Some(file_idx) = target_file_idx {
                        if let Some(scope) = direct_cache_scope {
                            self.ctx.cache_stable_source_file_symbol_arena_type(
                                target_sym_id,
                                file_idx as u32,
                                scope,
                                resolved.0,
                                resolved.1.clone(),
                            );
                        } else {
                            self.ctx.cache_cross_file_symbol_type(
                                target_sym_id,
                                file_idx as u32,
                                resolved.0,
                                resolved.1.clone(),
                            );
                        }
                    }
                    Some(resolved)
                });
            if is_default_import {
                let Some(resolved) = cached_or_direct else {
                    tsz_common::perf_counters::record_cross_arena_alias_shortcut_outcome(
                        AliasOutcome::DefaultImport,
                    );
                    return None;
                };
                resolved
            } else {
                cached_or_direct.unwrap_or_else(|| {
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
            }
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

        // `symbol_types` is keyed by the current binder's raw `SymbolId`.
        // A nested source-file alias is resolved by this checker too, but its
        // binder-local id can name an unrelated requester value. Keep foreign
        // results only in the owner-qualified cache below; the raw fast cache
        // is valid only for an alias declared in the current file.
        if alias_source_file_idx == self.ctx.current_file_idx {
            self.ctx.symbol_types.insert(sym_id, result);
        }
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
