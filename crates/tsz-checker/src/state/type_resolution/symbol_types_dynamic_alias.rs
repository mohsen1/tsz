use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};

impl<'a> CheckerState<'a> {
    pub(crate) fn same_named_type_alias_for_value_symbol(
        &self,
        value_sym_id: SymbolId,
    ) -> Option<SymbolId> {
        let value_symbol = self.get_cross_file_symbol(value_sym_id)?;
        let file_idx = self.ctx.resolve_symbol_file_index(value_sym_id)?;
        let binder = self.ctx.get_binder_for_file(file_idx)?;
        binder
            .symbols
            .find_all_by_name(&value_symbol.escaped_name)
            .iter()
            .copied()
            .find_map(|candidate_id| {
                if candidate_id == value_sym_id {
                    return None;
                }
                let candidate = binder.symbols.get(candidate_id)?;
                if candidate.flags & symbol_flags::TYPE_ALIAS == 0
                    || candidate.escaped_name != value_symbol.escaped_name
                {
                    return None;
                }
                self.ctx.register_symbol_file_target(candidate_id, file_idx);
                Some(candidate_id)
            })
    }

    pub(crate) fn should_delegate_dynamic_type_alias_owner(
        &self,
        sym_id: SymbolId,
        file_idx: usize,
    ) -> bool {
        if file_idx == self.ctx.current_file_idx {
            return false;
        }

        let Some(target_symbol) = self
            .ctx
            .get_binder_for_file(file_idx)
            .and_then(|binder| binder.get_symbol(sym_id))
        else {
            return false;
        };
        if !target_symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
            return false;
        }

        let Some(local_symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return true;
        };

        if local_symbol.has_any_flags(symbol_flags::ALIAS) {
            return true;
        }

        if let Some(local_def) = self.ctx.symbol_to_def.borrow().get(&sym_id).copied()
            && let Some(local_def_name) = self.ctx.definition_store.get_name(local_def)
        {
            return self.ctx.types.resolve_atom(local_def_name) != local_symbol.escaped_name;
        }

        local_symbol.escaped_name != target_symbol.escaped_name
    }
}
