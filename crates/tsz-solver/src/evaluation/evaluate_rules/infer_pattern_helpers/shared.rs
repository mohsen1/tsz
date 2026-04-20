use super::*;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    pub(super) fn erase_type_params_to_constraints(
        &self,
        type_params: &[TypeParamInfo],
    ) -> Option<TypeSubstitution> {
        if type_params.is_empty() {
            return None;
        }

        let mut subst = TypeSubstitution::new();
        for tp in type_params {
            subst.insert(tp.name, tp.constraint.unwrap_or(TypeId::UNKNOWN));
        }
        Some(subst)
    }

    pub(super) fn instantiate_signature_for_infer(
        &self,
        params: &[ParamInfo],
        return_type: TypeId,
        type_params: &[TypeParamInfo],
    ) -> (Vec<ParamInfo>, TypeId) {
        let Some(subst) = self.erase_type_params_to_constraints(type_params) else {
            return (params.to_vec(), return_type);
        };

        let params = params
            .iter()
            .map(|param| ParamInfo {
                name: param.name,
                type_id: instantiate_type(self.interner(), param.type_id, &subst),
                optional: param.optional,
                rest: param.rest,
            })
            .collect();
        let return_type = instantiate_type(self.interner(), return_type, &subst);
        (params, return_type)
    }

    pub(super) fn match_rest_infer_tuple(
        &self,
        source_params: &[ParamInfo],
        infer_ty: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let source_tuple_or_array = if source_params.len() == 1 && source_params[0].rest {
            source_params[0].type_id
        } else if source_params.iter().any(|param| param.rest) {
            return false;
        } else {
            let tuple_elems: Vec<TupleElement> = source_params
                .iter()
                .map(|p| TupleElement {
                    type_id: p.type_id,
                    name: p.name,
                    optional: p.optional,
                    rest: p.rest,
                })
                .collect();
            self.interner().tuple(tuple_elems)
        };
        let mut local_visited = FxHashSet::default();
        self.match_infer_pattern(
            source_tuple_or_array,
            infer_ty,
            bindings,
            &mut local_visited,
            checker,
        )
    }

    pub(super) fn match_signature_params_for_infer(
        &self,
        source_params: &[ParamInfo],
        pattern_params: &[ParamInfo],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let trailing_rest_param = pattern_params.last().filter(|param| param.rest);
        let fixed_param_count = if trailing_rest_param.is_some() {
            pattern_params.len().saturating_sub(1)
        } else {
            pattern_params.len()
        };

        if source_params.len() < fixed_param_count {
            return false;
        }

        let mut local_visited = FxHashSet::default();
        for (source_param, pattern_param) in source_params
            .iter()
            .take(fixed_param_count)
            .zip(pattern_params.iter().take(fixed_param_count))
        {
            let source_param_type = if source_param.optional {
                crate::narrowing::remove_nullish(self.interner(), source_param.type_id)
            } else {
                source_param.type_id
            };
            if !self.match_infer_pattern(
                source_param_type,
                pattern_param.type_id,
                bindings,
                &mut local_visited,
                checker,
            ) {
                return false;
            }
        }

        if let Some(rest_param) = trailing_rest_param {
            let remaining_params = &source_params[fixed_param_count..];
            if self.type_contains_infer(rest_param.type_id) {
                if !self.match_rest_infer_tuple(
                    remaining_params,
                    rest_param.type_id,
                    bindings,
                    checker,
                ) {
                    return false;
                }
            } else {
                let mut local_visited = FxHashSet::default();
                for source_param in remaining_params {
                    let source_param_type = if source_param.optional {
                        crate::narrowing::remove_nullish(self.interner(), source_param.type_id)
                    } else {
                        source_param.type_id
                    };
                    if !self.match_infer_pattern(
                        source_param_type,
                        rest_param.type_id,
                        bindings,
                        &mut local_visited,
                        checker,
                    ) {
                        return false;
                    }
                }
            }
        }

        true
    }
}
