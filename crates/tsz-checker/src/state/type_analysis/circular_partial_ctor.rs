use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn circular_class_partial_constructor_type(
        &self,
        sym_id: SymbolId,
    ) -> Option<TypeId> {
        let partial = self.ctx.symbol_types.get(&sym_id)?;
        crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, partial)
            .is_some()
            .then_some(partial)
    }
}
