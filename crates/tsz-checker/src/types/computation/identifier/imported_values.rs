//! Imported value helpers for identifier type computation.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn imported_alias_value_type(
        &mut self,
        sym_id: SymbolId,
        idx: NodeIndex,
    ) -> Option<TypeId> {
        if self.is_identifier_in_type_position(idx) {
            return None;
        }

        let target_sym_id = self.ctx.resolve_import_alias_and_register(sym_id)?;
        if target_sym_id == sym_id {
            return None;
        }

        let (target_value_decl, target_declarations, target_file_idx) = self
            .get_cross_file_symbol(target_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(target_sym_id))
            .and_then(|target_symbol| {
                let tflags = target_symbol.flags;
                if (tflags & symbol_flags::VALUE) == 0
                    || (tflags & symbol_flags::ALIAS) != 0
                    || target_symbol.import_module.is_some()
                    || !target_symbol.value_declaration.is_some()
                {
                    return None;
                }

                let target_file_idx = self.ctx.resolve_symbol_file_index(target_sym_id)?;
                Some((
                    target_symbol.value_declaration,
                    target_symbol.declarations.clone(),
                    target_file_idx,
                ))
            })?;

        let preferred_value_decl = self
            .preferred_value_declaration(target_sym_id, target_value_decl, &target_declarations)
            .unwrap_or(target_value_decl);
        let target_has_type_annotation = self
            .ctx
            .get_arena_for_file(target_file_idx as u32)
            .get(preferred_value_decl)
            .and_then(|node| {
                self.ctx
                    .get_arena_for_file(target_file_idx as u32)
                    .get_variable_declaration(node)
            })
            .is_some_and(|decl| decl.type_annotation.is_some());

        let value_type = if target_has_type_annotation {
            let declared = self.type_of_value_declaration_for_cross_file_symbol(
                target_sym_id,
                preferred_value_decl,
                target_file_idx,
            );
            if declared != TypeId::UNKNOWN && declared != TypeId::ERROR {
                declared
            } else {
                self.ctx
                    .cached_cross_file_symbol_type(target_sym_id, target_file_idx as u32)
                    .map(|(cached_type, _)| cached_type)
                    .unwrap_or(declared)
            }
        } else {
            self.ctx
                .cached_cross_file_symbol_type(target_sym_id, target_file_idx as u32)
                .map(|(cached_type, _)| cached_type)
                .filter(|cached_type| {
                    *cached_type != TypeId::UNKNOWN && *cached_type != TypeId::ERROR
                })
                .unwrap_or_else(|| {
                    self.type_of_value_declaration_for_cross_file_symbol(
                        target_sym_id,
                        preferred_value_decl,
                        target_file_idx,
                    )
                })
        };

        (value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR).then_some(value_type)
    }
}
