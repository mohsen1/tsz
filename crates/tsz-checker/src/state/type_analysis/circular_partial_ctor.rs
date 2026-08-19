use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn circular_class_partial_constructor_type(
        &self,
        sym_id: SymbolId,
    ) -> Option<TypeId> {
        let partial = self.ctx.symbol_types.get(&sym_id)?;
        let is_partial_ctor =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, partial)
                .is_some();
        if is_partial_ctor {
            // Serving a mid-resolution partial constructor: taint in-flight
            // evaluations so nothing persists a result derived from it
            // (issue #16055).
            self.ctx.note_provisional_class_value();
        }
        is_partial_ctor.then_some(partial)
    }
}
