//! `Symbol()` / `Symbol.for(...)` const value identity helpers.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Upgrade the value type of an unannotated `const X = Symbol()` /
    /// `const X = Symbol.for(...)` declaration to its `unique symbol` value
    /// identity (`UniqueSymbol(SymbolRef(X))`), keyed on the variable's own
    /// binder symbol.
    ///
    /// `tsc` gives an unannotated `const` initialized with a global
    /// `Symbol(...)`/`Symbol.for(...)` factory call the distinct value identity
    /// `typeof X` (a `unique symbol`), not the general `symbol` type the call
    /// signature returns. The annotation form (`const X: unique symbol`) is
    /// centralized in `const_unique_symbol_value_type`; this is the sibling
    /// upgrade for the initializer form, so every value-typing path agrees on
    /// the same identity.
    ///
    /// Returns `None` when the declaration is not a `const` whose initializer is
    /// a verified global `Symbol`/`Symbol.for` factory call.
    pub(crate) fn const_symbol_factory_unique_value_type(
        &self,
        decl_idx: NodeIndex,
    ) -> Option<TypeId> {
        let var_decl = self
            .ctx
            .arena
            .get(decl_idx)
            .and_then(|node| self.ctx.arena.get_variable_declaration(node))?;
        if var_decl.initializer.is_none() || !self.is_const_variable_declaration(decl_idx) {
            return None;
        }
        if !(self.is_symbol_call_initializer(var_decl.initializer)
            || self.is_symbol_for_call_initializer(var_decl.initializer))
        {
            return None;
        }
        let sym_id = self.ctx.binder.get_node_symbol(var_decl.name)?;
        Some(
            self.ctx
                .types
                .unique_symbol(tsz_solver::SymbolRef(sym_id.0)),
        )
    }
}
