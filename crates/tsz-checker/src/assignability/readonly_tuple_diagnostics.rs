//! Readonly array/tuple diagnostic preflights for assignment reporting.

use crate::query_boundaries::common::{
    array_element_type, is_array_type, is_tuple_type, readonly_inner_type, tuple_list_id,
    type_param_info,
};
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn is_array_or_tuple_like_for_readonly_assignment(&mut self, type_id: TypeId) -> bool {
        let evaluated = self.evaluate_type_for_assignability(type_id);
        for candidate in [type_id, evaluated] {
            let candidate = readonly_inner_type(self.ctx.types, candidate).unwrap_or(candidate);
            if tuple_list_id(self.ctx.types, candidate).is_some()
                || array_element_type(self.ctx.types, candidate).is_some()
            {
                return true;
            }
            if let Some(constraint) =
                type_param_info(self.ctx.types, candidate).and_then(|info| info.constraint)
            {
                let evaluated_constraint = self.evaluate_type_for_assignability(constraint);
                for constraint_candidate in [constraint, evaluated_constraint] {
                    let constraint_candidate =
                        readonly_inner_type(self.ctx.types, constraint_candidate)
                            .unwrap_or(constraint_candidate);
                    if tuple_list_id(self.ctx.types, constraint_candidate).is_some()
                        || array_element_type(self.ctx.types, constraint_candidate).is_some()
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(crate) fn readonly_to_mutable_array_or_tuple_reason(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<tsz_solver::SubtypeFailureReason> {
        let evaluated_source = self.evaluate_type_for_assignability(source);
        let evaluated_target = self.evaluate_type_for_assignability(target);

        let readonly_source_inner = readonly_inner_type(self.ctx.types, source)
            .or_else(|| readonly_inner_type(self.ctx.types, evaluated_source))?;
        if readonly_inner_type(self.ctx.types, target).is_some()
            || readonly_inner_type(self.ctx.types, evaluated_target).is_some()
        {
            return None;
        }

        if !self.is_array_or_tuple_like_for_readonly_assignment(readonly_source_inner) {
            return None;
        }

        let target_is_mutable_array_or_tuple =
            is_tuple_type(self.ctx.types, target) || is_array_type(self.ctx.types, target);
        target_is_mutable_array_or_tuple.then_some(
            tsz_solver::SubtypeFailureReason::ReadonlyToMutableAssignment {
                source_type: source,
                target_type: target,
            },
        )
    }
}
