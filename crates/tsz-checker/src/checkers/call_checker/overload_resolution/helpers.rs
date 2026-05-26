//! Small standalone helpers for overload resolution — pure code motion from
//! the parent `overload_resolution` module.

use crate::query_boundaries::common::CallResult;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn signature_const_type_params_require_readonly_argument_context(
        db: &dyn tsz_solver::construction::TypeDatabase,
        type_params: &[tsz_solver::TypeParamInfo],
    ) -> bool {
        type_params.iter().any(|type_param| {
            type_param.is_const
                && !type_param.constraint.is_some_and(|constraint| {
                    Self::constraint_allows_mutable_array_like(db, constraint)
                })
        })
    }

    pub(super) fn overload_string_argument_array_parameter_mismatch(
        &mut self,
        sig: &tsz_solver::CallSignature,
        arg_types: &[TypeId],
    ) -> Option<CallResult> {
        arg_types
            .iter()
            .copied()
            .enumerate()
            .find_map(|(index, actual)| {
                if actual != TypeId::STRING
                    && !crate::query_boundaries::common::is_string_type(self.ctx.types, actual)
                    && crate::query_boundaries::common::string_literal_value(self.ctx.types, actual)
                        .is_none()
                {
                    return None;
                }
                let expected = sig
                    .params
                    .get(index)
                    .map(|param| param.type_id)
                    .or_else(|| {
                        sig.params
                            .last()
                            .and_then(|param| param.rest.then_some(param.type_id))
                    })?;
                self.is_array_like_type(expected)
                    .then_some(CallResult::ArgumentTypeMismatch {
                        index,
                        expected,
                        actual,
                        fallback_return: sig.return_type,
                    })
            })
    }
}
