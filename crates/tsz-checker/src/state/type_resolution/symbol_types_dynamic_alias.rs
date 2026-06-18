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

        // A genuine cross-file conditional type alias whose `extends` operand is
        // a bare provider-private named type must be lowered in its declaring
        // arena: its body references types invisible in the consumer scope (e.g.
        // a non-exported `type Rec = object` used by
        // `export type Pick2<T> = T extends Rec ? T : number`). Re-lowering that
        // body in the consumer scope leaves `Rec` an unresolved name, so the
        // conditional's extends operand never binds and the alias degrades (the
        // false branch / a `T | false-branch` union — the cross-arena
        // `error`/`never`-in-type-argument family, #13618).
        //
        // The gate is deliberately tight. The broad form (delegate every
        // imported alias) regressed JSX element/prop checking, because library
        // conditional helpers such as React's `PropsWithRef`/`ElementType`
        // instantiate the *caller's* type argument and must stay in the consumer
        // arena. `symbol_has_local_type_alias_declaration` returns `false`
        // exactly when the alias is NOT declared in the current arena
        // (distinguishing a real import from a same-`SymbolId` local collision,
        // still handled by the name/def heuristics below), and either delegation
        // predicate then confirms the imported body genuinely depends on the
        // provider's own scope:
        //   - `cross_file_alias_body_is_private_extends_conditional` — the
        //     original narrow #13618 shape: a top-level conditional whose
        //     `extends` operand is a bare named reference; and
        //   - `cross_file_alias_body_references_provider_private_type` — the
        //     general form: any body shape (conditional with a parameterized
        //     `extends` operand, mapped type with a private key filter, …) that
        //     references a **non-exported** type declared in the provider module.
        // Both exclude library helpers (those reference only the caller's type
        // parameter, globals, or *exported* types), so neither widens delegation
        // to the JSX-regressing cases. `delegate_cross_arena_symbol_resolution`
        // re-checks the same locality predicate, so requesting delegation here is
        // safe.
        if !self.symbol_has_local_type_alias_declaration(local_symbol, sym_id)
            && (self.cross_file_alias_body_is_private_extends_conditional(sym_id)
                || self.cross_file_alias_body_references_provider_private_type(sym_id))
        {
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
