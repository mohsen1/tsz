use crate::query_boundaries;
use crate::state::CheckerState;
use tsz_solver::{ParamInfo, TypeId};

impl<'a> CheckerState<'a> {
    pub(super) fn contextual_signature_accepts_required_callback_params(
        &mut self,
        expected: TypeId,
        required_param_count: usize,
    ) -> bool {
        if required_param_count == 0 {
            return true;
        }
        let expected = self.normalize_contextual_signature_with_env(expected);
        if expected == TypeId::ANY || expected == TypeId::UNKNOWN || expected == TypeId::ERROR {
            return true;
        }
        if let Some(members) = query_boundaries::common::union_members(self.ctx.types, expected) {
            let mut saw_callable = false;
            for member in members {
                if let Some(accepts) = self
                    .contextual_callable_type_accepts_required_callback_params(
                        member,
                        required_param_count,
                    )
                {
                    saw_callable = true;
                    if accepts {
                        return true;
                    }
                }
            }
            return !saw_callable;
        }
        self.contextual_callable_type_accepts_required_callback_params(
            expected,
            required_param_count,
        )
        .unwrap_or(true)
    }

    fn contextual_callable_type_accepts_required_callback_params(
        &self,
        ty: TypeId,
        required_param_count: usize,
    ) -> Option<bool> {
        if let Some(shape) = query_boundaries::common::function_shape_for_type(self.ctx.types, ty) {
            return Some(callback_signature_accepts_required_params(
                &shape.params,
                required_param_count,
            ));
        }
        if let Some(shape) = query_boundaries::common::callable_shape_for_type(self.ctx.types, ty) {
            let accepts_call = shape.call_signatures.iter().any(|sig| {
                callback_signature_accepts_required_params(&sig.params, required_param_count)
            });
            let accepts_construct = shape.construct_signatures.iter().any(|sig| {
                callback_signature_accepts_required_params(&sig.params, required_param_count)
            });
            return Some(accepts_call || accepts_construct);
        }
        query_boundaries::checkers::call::get_contextual_signature(self.ctx.types, ty).map(
            |shape| callback_signature_accepts_required_params(&shape.params, required_param_count),
        )
    }
}

fn callback_signature_accepts_required_params(
    params: &[ParamInfo],
    required_param_count: usize,
) -> bool {
    params.iter().any(|param| param.rest) || params.len() >= required_param_count
}
