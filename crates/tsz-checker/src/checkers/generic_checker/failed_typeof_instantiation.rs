//! Typeof-instantiation helpers for generic constraint validation.

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Return `true` when `type_arg` is the type of an instantiation expression
    /// `typeof fn<TArgs>` whose `TArgs` do not match the type-parameter arity
    /// of any call/construct signature on the underlying function.
    pub(super) fn is_failed_typeof_instantiation_arg(&mut self, type_arg: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some((base, args)) = query::application_base_and_args(db, type_arg) else {
            return false;
        };

        if query::is_named_type_reference(db, base) {
            return false;
        }

        let num_args = args.len();
        let resolved = self.resolve_lazy_type(base);
        let resolved = self.evaluate_type_for_assignability(resolved);
        let db = self.ctx.types.as_type_database();
        let Some(shape) =
            crate::query_boundaries::common::get_callable_shape_for_type(db, resolved)
        else {
            return false;
        };

        let call_match = shape
            .call_signatures
            .iter()
            .any(|s| s.type_params.len() == num_args);
        let construct_match = shape
            .construct_signatures
            .iter()
            .any(|s| s.type_params.len() == num_args);
        !(call_match || construct_match)
    }
}
