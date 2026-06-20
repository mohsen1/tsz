//! Small standalone helpers for overload resolution — pure code motion from
//! the parent `overload_resolution` module.

use crate::query_boundaries::checkers::call::array_element_type_for_type;
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
        // Position of the trailing rest parameter, if any. Arguments at or past
        // this index are matched element-wise against the rest's element type,
        // not against the rest's (array-like) declared type — so a `string`
        // spread element landing on a `...items: string[]` rest is *not* a
        // mismatch. Without this, a non-tuple spread (`f(..., ...stringArr)`)
        // whose element type was collected as `string` would be wrongly rejected
        // here as "string argument vs array parameter".
        let rest_start = sig
            .params
            .last()
            .filter(|param| param.rest)
            .map(|_| sig.params.len() - 1);
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
                let expected = if rest_start.is_some_and(|start| index >= start) {
                    // `index` lands on the trailing rest parameter: compare
                    // against its element type.
                    let rest_type = sig.params.last()?.type_id;
                    array_element_type_for_type(self.ctx.types, rest_type).unwrap_or(rest_type)
                } else {
                    sig.params.get(index).map(|param| param.type_id)?
                };
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
