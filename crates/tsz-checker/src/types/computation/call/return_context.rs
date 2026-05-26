use crate::query_boundaries::assignability as assign_query;
use crate::query_boundaries::common;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Merge the return-context inference contributed by one call argument into
    /// the in-progress Round 2 substitution. Type parameters that Round 1 left
    /// uninformative (any/unknown/error/still-generic, or a freshly widened
    /// supertype) are filled in; concrete inferences are preserved. A literal
    /// contribution to a non-`const` type parameter is widened, matching tsc's
    /// literal widening during inference. `null`/`undefined` contributions from
    /// a context-sensitive (callback) argument are ignored.
    pub(super) fn merge_arg_return_context_into_round2(
        &mut self,
        shape: &common::FunctionShape,
        shape_param_type: TypeId,
        arg_type: TypeId,
        tracked_type_params: &FxHashSet<tsz_common::Atom>,
        arg_is_sensitive: bool,
        round2_substitution: &mut crate::query_boundaries::common::TypeSubstitution,
    ) {
        let mut arg_substitution = crate::query_boundaries::common::TypeSubstitution::new();
        let mut visited = FxHashSet::default();
        self.collect_return_context_substitution(
            shape_param_type,
            arg_type,
            tracked_type_params,
            &mut arg_substitution,
            &mut visited,
        );
        for (&name, &raw_ty) in arg_substitution.map().iter() {
            let ty = if shape
                .type_params
                .iter()
                .find(|tp| tp.name == name)
                .is_some_and(|tp| !tp.is_const)
            {
                if crate::query_boundaries::common::object_shape_for_type(self.ctx.types, raw_ty)
                    .is_some()
                {
                    raw_ty
                } else {
                    self.widen_literal_type(raw_ty)
                }
            } else {
                raw_ty
            };
            if ty == TypeId::UNKNOWN
                || ty == TypeId::ERROR
                || (arg_is_sensitive && (ty == TypeId::NULL || ty == TypeId::UNDEFINED))
                || self.target_contains_blocking_return_context_type_params(ty, tracked_type_params)
            {
                continue;
            }
            let should_update = match round2_substitution.get(name) {
                None => true,
                Some(existing) if existing == ty => false,
                Some(existing) => {
                    existing == TypeId::UNKNOWN
                        || existing == TypeId::ERROR
                        || self.inference_type_is_anyish(existing)
                        || common::contains_infer_types(self.ctx.types, existing)
                        || common::contains_type_parameters(self.ctx.types, existing)
                        || assign_query::is_fresh_subtype_of(self.ctx.types, ty, existing)
                }
            };
            if should_update {
                round2_substitution.insert(name, ty);
            }
        }
    }
}
