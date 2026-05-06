use crate::types::{ParamInfo, TupleElement, TypeId};

use super::super::super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check if source params are compatible with target params.
    /// Extracted to support union-of-tuple rest parameter handling,
    /// where we need to try multiple target param variants.
    pub(super) fn check_params_compatible(
        &mut self,
        source_params: &[ParamInfo],
        target_params: &[ParamInfo],
        is_method: bool,
    ) -> SubtypeResult {
        let target_has_rest = target_params.last().is_some_and(|p| p.rest);
        let source_has_rest = source_params.last().is_some_and(|p| p.rest);
        let rest_elem_type = if target_has_rest {
            target_params
                .last()
                .map(|param| self.get_array_element_type(param.type_id))
        } else {
            None
        };
        let rest_is_top = self.allow_bivariant_rest
            && matches!(rest_elem_type, Some(TypeId::ANY | TypeId::UNKNOWN));

        let target_fixed_count = if target_has_rest {
            target_params.len().saturating_sub(1)
        } else {
            target_params.len()
        };
        let source_fixed_count = if source_has_rest {
            source_params.len().saturating_sub(1)
        } else {
            source_params.len()
        };

        let source_required = self.required_param_count(source_params);
        let target_rest_min_required = if target_has_rest {
            target_params
                .last()
                .map(|param| self.rest_param_min_required_arg_count(param.type_id))
                .unwrap_or(0)
        } else {
            0
        };
        let guard_target_rest_arity = target_has_rest
            && target_params
                .last()
                .is_some_and(|param| self.rest_param_needs_min_arity_guard(param.type_id));
        if (!target_has_rest || guard_target_rest_arity)
            && source_required
                > target_fixed_count
                    + if target_has_rest {
                        target_rest_min_required
                    } else {
                        0
                    }
        {
            let extra_are_void = source_params
                .iter()
                .skip(target_fixed_count)
                .take(source_required.saturating_sub(target_fixed_count + target_rest_min_required))
                .all(|param| self.param_type_contains_void(param.type_id));
            if !extra_are_void {
                return SubtypeResult::False;
            }
        }

        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        for i in 0..fixed_compare_count {
            let s_param = &source_params[i];
            let t_param = &target_params[i];
            let (s_effective, t_effective) = self.effective_param_type_pair(s_param, t_param);
            if !self.are_parameters_compatible_impl(s_effective, t_effective, is_method) {
                return SubtypeResult::False;
            }
        }

        if target_has_rest {
            let Some(rest_elem_type) = rest_elem_type else {
                return SubtypeResult::False;
            };
            if rest_is_top {
                return SubtypeResult::True;
            }

            for s_param in source_params
                .iter()
                .skip(target_fixed_count)
                .take(source_fixed_count.saturating_sub(target_fixed_count))
            {
                if !self.are_parameters_compatible_impl(s_param.type_id, rest_elem_type, is_method)
                {
                    return SubtypeResult::False;
                }
            }

            if source_has_rest {
                let Some(s_rest_param) = source_params.last() else {
                    return SubtypeResult::False;
                };
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                if !self.are_parameters_compatible_impl(s_rest_elem, rest_elem_type, is_method) {
                    return SubtypeResult::False;
                }
            }
        }

        if source_has_rest {
            let Some(rest_param) = source_params.last() else {
                return SubtypeResult::False;
            };
            if self.is_tuple_list_rest_type(rest_param.type_id)
                && target_fixed_count > source_fixed_count
            {
                let tuple_elements: Vec<TupleElement> = target_params
                    .iter()
                    .skip(source_fixed_count)
                    .take(target_fixed_count.saturating_sub(source_fixed_count))
                    .map(|param| TupleElement {
                        type_id: param.type_id,
                        name: param.name,
                        optional: param.optional,
                        rest: false,
                    })
                    .collect();
                let target_rest_tuple = self.interner.tuple(tuple_elements);
                if !self.are_parameters_compatible_impl(
                    rest_param.type_id,
                    target_rest_tuple,
                    is_method,
                ) {
                    return SubtypeResult::False;
                }
                return SubtypeResult::True;
            }
            let rest_elem_type = self.get_array_element_type(rest_param.type_id);
            let rest_is_top = self.allow_bivariant_rest && rest_elem_type.is_any_or_unknown();

            if !rest_is_top {
                for t_param in target_params
                    .iter()
                    .skip(source_fixed_count)
                    .take(target_fixed_count.saturating_sub(source_fixed_count))
                {
                    if !self.are_parameters_compatible_impl(
                        rest_elem_type,
                        t_param.type_id,
                        is_method,
                    ) {
                        return SubtypeResult::False;
                    }
                }
            }
        }

        SubtypeResult::True
    }
}
