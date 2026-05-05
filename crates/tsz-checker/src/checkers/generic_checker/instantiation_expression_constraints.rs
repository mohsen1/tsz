use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Return `true` when `type_arg` is the type of an instantiation expression
    /// `typeof fn<TArgs>` whose `TArgs` do not match the type-parameter arity
    /// of any call/construct signature on the underlying function. Such
    /// expressions also raise TS2635 at the instantiation site; tsc treats the
    /// resulting type as `errorType`, which then fails any non-trivial
    /// type-parameter constraint check (TS2344).
    pub(crate) fn is_failed_typeof_instantiation_arg(&mut self, type_arg: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some((base, args)) = query::application_base_and_args(db, type_arg) else {
            return false;
        };

        // Generic-type-reference Applications (`Foo<X>` for a type alias /
        // class / interface) use a `Lazy(DefId)` / `Recursive` / `BoundParameter`
        // base. Their arity mismatches are reported elsewhere (TS2305 / TS2558)
        // — not the typeof-instantiation flow.
        if query::is_named_type_reference(db, base) {
            return false;
        }

        let Some(shape) = self.typeof_instantiation_callable_shape(base) else {
            return false;
        };
        let num_args = args.len();
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

    pub(crate) fn is_successful_typeof_instantiation_arg(&mut self, type_arg: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some((base, args)) = query::application_base_and_args(db, type_arg) else {
            return false;
        };
        if query::is_named_type_reference(db, base) {
            return false;
        }

        let Some(shape) = self.typeof_instantiation_callable_shape(base) else {
            return false;
        };
        let num_args = args.len();
        shape
            .call_signatures
            .iter()
            .any(|s| s.type_params.len() == num_args)
            || shape
                .construct_signatures
                .iter()
                .any(|s| s.type_params.len() == num_args)
    }

    pub(crate) fn constraint_is_callable_or_constructable(&mut self, constraint: TypeId) -> bool {
        let constraint = self.resolve_lazy_type(constraint);
        crate::query_boundaries::common::function_shape_for_type(self.ctx.types, constraint)
            .is_some()
            || crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, constraint)
                .is_some_and(|sigs| !sigs.is_empty())
            || crate::query_boundaries::common::construct_signatures_for_type(
                self.ctx.types,
                constraint,
            )
            .is_some_and(|sigs| !sigs.is_empty())
    }

    fn typeof_instantiation_callable_shape(
        &mut self,
        base: TypeId,
    ) -> Option<std::sync::Arc<tsz_solver::CallableShape>> {
        let resolved = self.resolve_lazy_type(base);
        let resolved = self.evaluate_type_for_assignability(resolved);
        crate::query_boundaries::common::get_callable_shape_for_type(
            self.ctx.types.as_type_database(),
            resolved,
        )
    }
}
