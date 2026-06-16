use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};

impl<'a> CheckerState<'a> {
    pub(crate) fn should_delegate_dynamic_type_alias_owner(
        &self,
        sym_id: SymbolId,
        file_idx: usize,
    ) -> bool {
        let Some(target_symbol) = self
            .ctx
            .get_binder_for_file(file_idx)
            .and_then(|binder| binder.get_symbol(sym_id))
        else {
            return false;
        };
        if file_idx == self.ctx.current_file_idx
            && self
                .ctx
                .binder
                .get_symbol(sym_id)
                .is_some_and(|local_symbol| {
                    local_symbol.escaped_name == target_symbol.escaped_name
                        && local_symbol.flags == target_symbol.flags
                })
        {
            return false;
        }
        if !target_symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
            return false;
        }

        let Some(local_symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return true;
        };

        if local_symbol.has_any_flags(symbol_flags::ALIAS) {
            return true;
        }

        // A genuine cross-file type alias (the local handle is the imported
        // symbol itself, not a same-`SymbolId` local declaration) must be lowered
        // in its declaring arena: its body can reference provider-private types
        // (e.g. a non-exported `type Rec = object` used by
        // `export type Pick2<T> = T extends Rec ? T : number`). Re-lowering that
        // body in the consumer scope leaves such references as unresolved names,
        // so the conditional's extends operand never binds and the alias degrades
        // (the false branch / a `T | false-branch` union — the cross-arena
        // `error`/`never`-in-type-argument family, #13618).
        //
        // `symbol_has_local_type_alias_declaration` returns `false` exactly when
        // the alias is NOT declared in the current arena, which distinguishes a
        // real import from a same-`SymbolId` local collision (where a different
        // local declaration legitimately keeps resolution local — handled by the
        // name/def heuristics below). `delegate_cross_arena_symbol_resolution`
        // re-checks the same locality predicate, so requesting delegation here is
        // safe.
        if !self.symbol_has_local_type_alias_declaration(local_symbol, sym_id) {
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
