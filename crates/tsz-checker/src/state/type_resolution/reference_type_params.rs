//! Type-reference type-parameter lookup and import-alias target helpers.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

impl<'a> CheckerState<'a> {
    pub(crate) fn declaration_file_type_shadow_for_lib_name(
        &self,
        name: &str,
        file_idx: Option<usize>,
    ) -> bool {
        use tsz_binder::symbol_flags;

        let Some(file_idx) = file_idx else {
            return self.ctx.file_local_type_shadow_for_lib_name(name);
        };
        let Some(binder) = self.ctx.get_binder_for_file(file_idx) else {
            return false;
        };
        if !binder.is_external_module() {
            return false;
        }
        binder.file_locals.get(name).is_some_and(|sym_id| {
            let is_actual_or_merged_lib = self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                || binder.lib_symbol_ids.contains(&sym_id);
            !is_actual_or_merged_lib
                && binder
                    .get_symbol(sym_id)
                    .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE))
        })
    }

    pub(crate) fn resolve_declaration_file_type_symbol_for_lowering(
        &self,
        name: &str,
        file_idx: Option<usize>,
    ) -> Option<SymbolId> {
        let file_idx = file_idx?;
        let binder = self.ctx.get_binder_for_file(file_idx)?;
        let sym_id = binder.file_locals.get(name)?;
        let symbol = binder.get_symbol(sym_id)?;
        let is_actual_or_merged_lib = self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
            || binder.lib_symbol_ids.contains(&sym_id);
        if is_actual_or_merged_lib {
            return None;
        }
        if symbol.has_any_flags(symbol_flags::ALIAS)
            && let Some(module_name) = symbol.import_module.as_ref()
        {
            let import_name = symbol.import_name.as_deref().unwrap_or(name);
            let target_sym_id =
                self.resolve_cross_file_export_from_file(module_name, import_name, Some(file_idx))?;
            if let Some(target_file_idx) = self
                .ctx
                .resolve_symbol_file_index_stable(target_sym_id)
                .or_else(|| self.ctx.resolve_symbol_file_index(target_sym_id))
            {
                self.ctx
                    .register_symbol_file_target(target_sym_id, target_file_idx);
            }
            return Some(target_sym_id);
        }
        if symbol.has_any_flags(symbol_flags::TYPE) {
            self.ctx.register_symbol_file_target(sym_id, file_idx);
            return Some(sym_id);
        }
        None
    }

    pub(crate) fn resolve_declaration_file_type_def_id_for_lowering(
        &self,
        name: &str,
        file_idx: Option<usize>,
    ) -> Option<tsz_solver::def::DefId> {
        self.resolve_declaration_file_type_symbol_for_lowering(name, file_idx)
            .map(|sym_id| self.ctx.get_or_create_def_id_for_symbol_name(sym_id, name))
    }

    pub(crate) fn get_reference_type_params_for_symbol(
        &mut self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        let cache_key = self.reference_type_params_cache_key(sym_id, expected_name);
        if let Some(cached) = self
            .ctx
            .type_reference_validation_caches
            .ref_type_params
            .get(&cache_key)
        {
            return cached.clone();
        }
        let declared =
            self.extract_declared_type_params_for_reference_symbol(cache_key.0, expected_name);
        let result = if !declared.is_empty() {
            declared
        } else {
            self.get_display_type_params_for_symbol(cache_key.0)
        };
        self.ctx
            .type_reference_validation_caches
            .ref_type_params
            .insert(cache_key, result.clone());
        result
    }

    pub(crate) fn count_required_reference_type_params(
        &mut self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> usize {
        let cache_key = self.reference_type_params_cache_key(sym_id, expected_name);
        if let Some(cached) = self
            .ctx
            .type_reference_validation_caches
            .ref_type_params
            .get(&cache_key)
        {
            return cached.iter().filter(|p| p.default.is_none()).count();
        }
        let declared =
            self.extract_declared_type_params_for_reference_symbol(cache_key.0, expected_name);
        if !declared.is_empty() {
            let count = declared
                .iter()
                .filter(|param| param.default.is_none())
                .count();
            self.ctx
                .type_reference_validation_caches
                .ref_type_params
                .insert(cache_key, declared);
            return count;
        }
        self.count_required_type_params(cache_key.0)
    }

    fn reference_type_params_cache_key(
        &self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> (SymbolId, Option<usize>, String) {
        if let Some(local_sym_id) =
            self.current_non_import_reference_symbol_id(sym_id, expected_name)
        {
            return (local_sym_id, None, expected_name.to_owned());
        }
        let (sym_id, file_idx) = self
            .reference_type_params_import_target(sym_id, expected_name)
            .unwrap_or_else(|| (sym_id, self.ctx.resolve_symbol_file_index(sym_id)));
        (sym_id, file_idx, expected_name.to_owned())
    }

    pub(crate) fn current_non_import_reference_symbol_id(
        &self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> Option<SymbolId> {
        if self.reference_symbol_is_current_non_import(sym_id, expected_name) {
            return Some(sym_id);
        }
        if let Some(local_sym_id) = self.ctx.binder.file_locals.get(expected_name)
            && self.reference_symbol_is_current_non_import(local_sym_id, expected_name)
        {
            return Some(local_sym_id);
        }
        None
    }

    fn reference_symbol_is_current_non_import(
        &self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> bool {
        self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
            symbol.escaped_name == expected_name && !self.reference_symbol_is_import_alias(symbol)
        })
    }

    fn reference_type_params_import_target(
        &self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> Option<(SymbolId, Option<usize>)> {
        let alias_symbol = self
            .ctx
            .binder
            .file_locals
            .get(expected_name)
            .and_then(|alias_sym_id| self.ctx.binder.get_symbol(alias_sym_id))
            .or_else(|| self.ctx.binder.get_symbol(sym_id))?;
        if !self.reference_symbol_is_import_alias(alias_symbol) {
            return None;
        }
        self.reference_import_alias_export_target(alias_symbol, expected_name)
    }

    pub(crate) fn reference_import_alias_export_target(
        &self,
        alias_symbol: &tsz_binder::Symbol,
        expected_name: &str,
    ) -> Option<(SymbolId, Option<usize>)> {
        let module_specifier = alias_symbol.import_module.as_ref()?;
        let import_name = alias_symbol.import_name.as_deref().unwrap_or(expected_name);
        let source_file_idx = if alias_symbol.decl_file_idx == u32::MAX {
            self.ctx.current_file_idx
        } else {
            alias_symbol.decl_file_idx as usize
        };
        if let Some(target_file_idx) = self
            .ctx
            .resolve_import_target_from_file(source_file_idx, module_specifier)
            && let Some((target_sym_id, actual_file_idx)) =
                self.resolve_reexport_chain_to_declaration(target_file_idx, import_name)
        {
            self.ctx
                .register_symbol_file_target(target_sym_id, actual_file_idx);
            return Some((target_sym_id, Some(actual_file_idx)));
        }

        let target_sym_id = self.resolve_cross_file_export_from_file(
            module_specifier,
            import_name,
            Some(source_file_idx),
        )?;
        let target_file_idx = self
            .ctx
            .resolve_symbol_file_index_stable(target_sym_id)
            .or_else(|| self.ctx.resolve_symbol_file_index(target_sym_id));
        if let Some(file_idx) = target_file_idx {
            self.ctx
                .register_symbol_file_target(target_sym_id, file_idx);
        }
        Some((target_sym_id, target_file_idx))
    }

    pub(crate) fn reference_symbol_is_import_alias(&self, symbol: &tsz_binder::Symbol) -> bool {
        let arena = if symbol.decl_file_idx == u32::MAX {
            self.ctx.arena
        } else {
            self.ctx.get_arena_for_file(symbol.decl_file_idx)
        };
        symbol.has_any_flags(symbol_flags::ALIAS)
            && symbol.import_module.is_some()
            && symbol
                .declarations
                .iter()
                .copied()
                .any(|decl_idx| self.reference_decl_is_import_alias_syntax(arena, decl_idx))
    }

    fn reference_decl_is_import_alias_syntax(
        &self,
        arena: &NodeArena,
        mut decl_idx: NodeIndex,
    ) -> bool {
        for _ in 0..4 {
            let Some(node) = arena.get(decl_idx) else {
                return false;
            };
            if node.kind == syntax_kind_ext::IMPORT_SPECIFIER
                || node.kind == syntax_kind_ext::IMPORT_CLAUSE
                || node.kind == syntax_kind_ext::NAMESPACE_IMPORT
                || node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                return true;
            }
            let Some(extended) = arena.get_extended(decl_idx) else {
                return false;
            };
            let parent_idx = extended.parent;
            if parent_idx == NodeIndex::NONE {
                return false;
            }
            decl_idx = parent_idx;
        }
        false
    }
}
