use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn contextual_mixed_overload_param_type_for_call(
        &mut self,
        callable_type: TypeId,
        index: usize,
        arg_count: usize,
    ) -> Option<TypeId> {
        let signatures =
            tsz_solver::type_queries::get_call_signatures(self.ctx.types, callable_type)?;
        let accepts_arity = |params: &[tsz_solver::ParamInfo]| {
            let required_count = params.iter().filter(|param| !param.optional).count();
            let has_rest = params.iter().any(|param| param.rest);
            if has_rest {
                arg_count >= required_count
            } else {
                arg_count >= required_count && arg_count <= params.len()
            }
        };

        let matching: Vec<_> = signatures
            .iter()
            .filter(|sig| accepts_arity(&sig.params))
            .collect();
        if matching.len() < 2 {
            return None;
        }

        let has_generic = matching.iter().any(|sig| !sig.type_params.is_empty());
        let has_non_generic = matching.iter().any(|sig| sig.type_params.is_empty());
        if !(has_generic && has_non_generic) {
            return None;
        }

        let mut param_types = Vec::new();
        for sig in matching {
            let param_type = sig
                .params
                .get(index)
                .map(|param| {
                    if param.rest {
                        self.rest_argument_element_type_with_env(param.type_id)
                    } else {
                        self.evaluate_type_with_env(param.type_id)
                    }
                })
                .or_else(|| {
                    let last = sig.params.last()?;
                    last.rest
                        .then(|| self.rest_argument_element_type_with_env(last.type_id))
                });
            if let Some(param_type) = param_type {
                param_types.push(param_type);
            }
        }

        if param_types.len() > 1 && param_types.iter().any(|&ty| ty != TypeId::ANY) {
            param_types.retain(|&ty| ty != TypeId::ANY);
        }

        match param_types.len() {
            0 => None,
            1 => Some(param_types[0]),
            _ => Some(self.ctx.types.factory().union_preserve_members(param_types)),
        }
    }
}
