use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn contextual_mixed_overload_param_type_for_call(
        &mut self,
        callable_type: TypeId,
        index: usize,
        arg_count: usize,
    ) -> Option<TypeId> {
        let signatures = crate::query_boundaries::common::call_signatures_for_type(
            self.ctx.types,
            callable_type,
        )?;
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

    /// Whether a single callable union member can serve as the contextual
    /// signature for a function expression that declares `arg_count`
    /// parameters. Mirrors tsc's `isAritySmaller`: a call signature is viable
    /// when it has a rest parameter or at least `arg_count` parameters.
    /// Non-callable members are not viable.
    pub(crate) fn callable_member_accepts_callback_arity(
        &self,
        member: TypeId,
        arg_count: usize,
    ) -> bool {
        let accepts = |params: &[tsz_solver::ParamInfo]| {
            params.iter().any(|param| param.rest) || params.len() >= arg_count
        };
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, member)
        {
            return accepts(&shape.params);
        }
        if let Some(signatures) =
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, member)
        {
            return signatures.iter().any(|sig| accepts(&sig.params));
        }
        false
    }

    /// Contextual type of parameter `index` taken from a single callable union
    /// member, considering only call signatures that can accept a callback of
    /// `arg_count` parameters (tsc's `isAritySmaller`). Returns `None` when no
    /// viable signature covers the position. Complements
    /// `contextual_mixed_overload_param_type_for_call`, which only handles
    /// members carrying two or more mixed generic/non-generic overloads; this
    /// covers the common case of a plain single-signature function member.
    pub(crate) fn contextual_callable_member_param_type_for_call(
        &mut self,
        member: TypeId,
        index: usize,
        arg_count: usize,
    ) -> Option<TypeId> {
        let signature_params: Vec<Vec<tsz_solver::ParamInfo>> = if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, member)
        {
            vec![shape.params.clone()]
        } else {
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, member)?
                .into_iter()
                .map(|sig| sig.params)
                .collect()
        };
        for params in &signature_params {
            let has_rest = params.iter().any(|param| param.rest);
            if !has_rest && params.len() < arg_count {
                continue;
            }
            if let Some(param) = params.get(index) {
                return Some(if param.rest {
                    self.rest_argument_element_type_with_env(param.type_id)
                } else {
                    self.evaluate_type_with_env(param.type_id)
                });
            }
            if has_rest
                && let Some(last) = params.last()
                && last.rest
            {
                return Some(self.rest_argument_element_type_with_env(last.type_id));
            }
        }
        None
    }
}
