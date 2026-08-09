//! Alias display provenance helpers.

use crate::query_boundaries::checkers::generic as generic_query;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_solver::{TypeId, def::DefId};

impl CheckerState<'_> {
    pub(super) fn mark_tuple_spread_flattened_alias_def(
        &mut self,
        sym_id: SymbolId,
        def_id: DefId,
        result: TypeId,
        alias_is_non_generic: bool,
    ) {
        let body_has_top_level_spread =
            alias_is_non_generic && self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                symbol.declarations.iter().any(|&decl_idx| {
                    super::source_alias_attribution::tuple_alias_declaration_body_has_top_level_spread(
                        self.ctx.arena,
                        decl_idx,
                    )
                })
            });
        if !body_has_top_level_spread
            || generic_query::contains_free_type_parameters(self.ctx.types, result)
        {
            return;
        }

        // A spread element flattens into a fresh tuple only when it spreads a
        // fixed tuple (`...[a, b]` or `...Inner` where `Inner` is a fixed tuple).
        // A rest array (`...number[]`) stays variadic and keeps its alias name.
        let evaluated = self.evaluate_type_with_env(result);
        let is_non_variadic_tuple = crate::query_boundaries::common::tuple_elements(
            self.ctx.types.as_type_database(),
            evaluated,
        )
        .is_some_and(|elements| !elements.iter().any(|element| element.rest));
        if is_non_variadic_tuple {
            self.ctx
                .definition_store
                .mark_tuple_spread_flattened_alias(def_id);
        }
    }

    pub(crate) fn symbol_is_type_alias(&self, sym_id: SymbolId) -> bool {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .is_some_and(|s| s.has_any_flags(symbol_flags::TYPE_ALIAS))
    }
}
