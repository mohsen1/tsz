//! Companion value helpers for identifiers that resolve through merged type/value symbols.

use crate::query_boundaries::common as common_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn same_file_value_symbol_for_type_symbol(
        &self,
        type_sym_id: tsz_binder::SymbolId,
    ) -> Option<(tsz_binder::SymbolId, NodeIndex, usize)> {
        let type_symbol = self.get_symbol_globally(type_sym_id)?;
        if (type_symbol.flags & tsz_binder::symbol_flags::VALUE) != 0 {
            return None;
        }
        let file_idx = self.ctx.resolve_symbol_file_index(type_sym_id)?;
        let binder = self.ctx.get_binder_for_file(file_idx)?;
        for &candidate_id in binder
            .get_symbols()
            .find_all_by_name(&type_symbol.escaped_name)
        {
            if candidate_id == type_sym_id {
                continue;
            }
            let Some(candidate) = binder.get_symbol(candidate_id) else {
                continue;
            };
            if candidate.escaped_name != type_symbol.escaped_name
                || (candidate.flags & tsz_binder::symbol_flags::VALUE) == 0
                || (candidate.flags & tsz_binder::symbol_flags::ALIAS) != 0
                || candidate.import_module.is_some()
                || !candidate.value_declaration.is_some()
            {
                continue;
            }
            self.ctx.register_symbol_file_target(candidate_id, file_idx);
            return Some((candidate_id, candidate.value_declaration, file_idx));
        }
        None
    }

    pub(crate) fn same_scope_value_type_shadowing_symbol(
        &mut self,
        idx: NodeIndex,
        shadowed_sym_id: tsz_binder::SymbolId,
    ) -> Option<(tsz_binder::SymbolId, TypeId)> {
        let lib_binders = self.get_lib_binders();
        let candidate_id = self.ctx.binder.resolve_identifier_with_filter(
            self.ctx.arena,
            idx,
            &lib_binders,
            |sid| {
                if sid == shadowed_sym_id {
                    return false;
                }
                self.ctx
                    .binder
                    .get_symbol_with_libs(sid, &lib_binders)
                    .is_some_and(|symbol| {
                        symbol.has_any_flags(tsz_binder::symbol_flags::VALUE)
                            && !symbol.is_type_only
                            && !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
                    })
            },
        )?;

        let candidate_symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(candidate_id, &lib_binders)?;
        let mut value_type = TypeId::UNKNOWN;
        if candidate_symbol.value_declaration.is_some() {
            value_type = if self
                .ctx
                .arena
                .get(candidate_symbol.value_declaration)
                .is_some()
            {
                self.type_of_value_declaration_for_symbol(
                    candidate_id,
                    candidate_symbol.value_declaration,
                )
            } else if let Some(file_idx) = self.ctx.resolve_symbol_file_index(candidate_id) {
                self.type_of_value_declaration_for_cross_file_symbol(
                    candidate_id,
                    candidate_symbol.value_declaration,
                    file_idx,
                )
            } else {
                TypeId::UNKNOWN
            };
        }
        if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
            value_type = self.get_type_of_symbol(candidate_id);
        }
        (value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR)
            .then_some((candidate_id, value_type))
    }

    pub(crate) fn current_file_value_type_named(
        &mut self,
        name: &str,
    ) -> Option<(tsz_binder::SymbolId, TypeId)> {
        let file_idx = self.ctx.current_file_idx;
        let candidates: Vec<_> = {
            let binder = self.ctx.get_binder_for_file(file_idx)?;
            binder
                .get_symbols()
                .find_all_by_name(name)
                .iter()
                .filter_map(|&candidate_id| {
                    let candidate = binder.get_symbol(candidate_id)?;
                    let is_namespace_value = candidate.has_any_flags(
                        tsz_binder::symbol_flags::MODULE
                            | tsz_binder::symbol_flags::NAMESPACE_MODULE
                            | tsz_binder::symbol_flags::VALUE_MODULE,
                    );
                    if candidate.escaped_name != name
                        || !candidate.has_any_flags(tsz_binder::symbol_flags::VALUE)
                        || candidate.is_type_only
                        || (candidate.value_declaration.is_some()
                            && self.is_inside_module_augmentation(candidate.value_declaration))
                        || (!candidate.value_declaration.is_some() && !is_namespace_value)
                    {
                        return None;
                    }
                    Some((
                        candidate_id,
                        candidate.value_declaration,
                        candidate.flags,
                        candidate.has_any_flags(tsz_binder::symbol_flags::ALIAS),
                    ))
                })
                .collect()
        };
        for allow_alias in [false, true] {
            for &(candidate_id, value_declaration, flags, is_alias) in &candidates {
                if is_alias && !allow_alias {
                    continue;
                }
                self.ctx.register_symbol_file_target(candidate_id, file_idx);
                let is_namespace_value = (flags
                    & (tsz_binder::symbol_flags::MODULE
                        | tsz_binder::symbol_flags::NAMESPACE_MODULE
                        | tsz_binder::symbol_flags::VALUE_MODULE))
                    != 0;
                let mut value_type = if is_namespace_value {
                    self.build_namespace_object_type(candidate_id)
                } else {
                    TypeId::UNKNOWN
                };
                if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                    value_type = self.type_of_value_declaration_for_cross_file_symbol(
                        candidate_id,
                        value_declaration,
                        file_idx,
                    );
                }
                if value_type != TypeId::UNKNOWN
                    && value_type != TypeId::ERROR
                    && !common_query::contains_type_parameters(self.ctx.types, value_type)
                {
                    value_type = self.evaluate_type_with_resolution(value_type);
                }
                if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                    return Some((candidate_id, value_type));
                }
            }
        }
        None
    }
}
