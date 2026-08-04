//! Function- and callable-signature failure explanation for subtype checking.
//!
//! Split out of `explain.rs` to keep each hand-authored shard under the
//! repository-wide file-size limit. These methods produce structured
//! `SubtypeFailureReason`s for function/callable relation failures and are
//! invoked from the main `explain_failure_inner` dispatch.

use crate::def::resolver::TypeResolver;
use crate::diagnostics::SubtypeFailureReason;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{
    CallSignature, CallableShape, FunctionShape, PropertyInfo, TupleElement, TypeId,
};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    pub(super) fn explain_function_failure(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> Option<SubtypeFailureReason> {
        self.with_provisional_rest_union_function_scope(
            |checker, allow_provisional_rest_union_at_this_depth| {
                // tsc's `compareSignaturesRelated` compares parameters before
                // return types, so when both are incompatible it surfaces the
                // parameter mismatch.
                if let Some(parameter_failure) = checker.explain_function_parameter_failure(
                    source,
                    target,
                    allow_provisional_rest_union_at_this_depth,
                ) {
                    return Some(parameter_failure);
                }
                if checker
                    .check_subtype(source.return_type, target.return_type)
                    .is_true()
                    || checker.allow_void_return && target.return_type == TypeId::VOID
                {
                    return checker.explain_type_predicate_failure(source, target);
                }
                let nested_reason = checker
                    .explain_failure(source.return_type, target.return_type)
                    .map(Box::new);
                Some(SubtypeFailureReason::ReturnTypeMismatch {
                    source_return: source.return_type,
                    target_return: target.return_type,
                    nested_reason,
                })
            },
        )
    }

    /// Explain a `check_function_subtype` failure caused solely by
    /// `are_type_predicates_compatible` (called only once the parameter and
    /// return-type legs have already passed, mirroring tsc's
    /// `compareSignaturesRelated`, which checks the predicate only after the
    /// return type itself relates).
    fn explain_type_predicate_failure(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> Option<SubtypeFailureReason> {
        if self.are_type_predicates_compatible(source, target) {
            return None;
        }
        match (&source.type_predicate, &target.type_predicate) {
            // TS1224: the target demands a type guard (`x is T`/`this is T`)
            // and the source has no predicate at all. An assertion-only
            // target (`asserts x`) is compatible without one and never
            // reaches this branch — see `are_type_predicates_compatible`.
            (None, Some(target_predicate)) => Some(SubtypeFailureReason::TypePredicateMismatch {
                source_predicate: None,
                target_predicate: *target_predicate,
                source_signature: Some(self.interner.function(source.clone())),
                nested_reason: None,
            }),
            // TS1226: both sides declare a predicate but they are
            // incompatible (different target, guard-vs-assertion, or
            // unrelated narrowed types).
            (Some(source_predicate), Some(target_predicate)) => {
                let nested_reason = match (source_predicate.type_id, target_predicate.type_id) {
                    (Some(source_type), Some(target_type)) => {
                        self.explain_failure(source_type, target_type).map(Box::new)
                    }
                    _ => None,
                };
                Some(SubtypeFailureReason::TypePredicateMismatch {
                    source_predicate: Some(*source_predicate),
                    target_predicate: *target_predicate,
                    source_signature: None,
                    nested_reason,
                })
            }
            // (None, None) and (Some(_), None) are always compatible.
            _ => None,
        }
    }

    fn explain_function_parameter_failure(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
        allow_provisional_rest_union_at_this_depth: bool,
    ) -> Option<SubtypeFailureReason> {
        // Check parameter count
        let target_has_rest = target.params.last().is_some_and(|p| p.rest);
        let rest_elem_type = if target_has_rest {
            target
                .params
                .last()
                .map(|param| self.get_array_element_type(param.type_id))
        } else {
            None
        };
        let rest_is_top = self.allow_bivariant_rest
            && matches!(rest_elem_type, Some(TypeId::ANY | TypeId::UNKNOWN));
        let source_required = self.required_param_count(&source.params);
        let target_fixed_count = if target_has_rest {
            target.params.len().saturating_sub(1)
        } else {
            target.params.len()
        };
        // Parameter variance follows the target declaration kind. Explicit
        // class constructors carry `is_method`; construct-signature types do not.
        let is_method_or_ctor = target.is_method;
        let allow_bivariant_param_count = self.allows_bivariant_param_count(is_method_or_ctor);
        // When the target has a rest parameter (e.g., ...args: number[]),
        // it can absorb unlimited arguments — skip the too-many check entirely
        // so we fall through to per-parameter type checking.
        if !rest_is_top
            && !target_has_rest
            && !allow_bivariant_param_count
            && source_required > target_fixed_count
        {
            return Some(SubtypeFailureReason::TooManyParameters {
                source_count: source_required,
                target_count: target_fixed_count,
            });
        }

        // Check parameter types
        let source_has_rest = source.params.last().is_some_and(|p| p.rest);
        let provisional_rest_union = allow_provisional_rest_union_at_this_depth
            && source
                .params
                .last()
                .filter(|param| param.rest)
                .zip(target.params.last().filter(|param| param.rest))
                .is_some_and(|(source_rest, target_rest)| {
                    self.is_bare_rest_type_param(source_rest.type_id)
                        && self.rest_type_has_union_surface(target_rest.type_id)
                });
        let source_fixed_count = if source_has_rest {
            source.params.len().saturating_sub(1)
        } else {
            source.params.len()
        };
        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        for i in 0..fixed_compare_count {
            let s_param = &source.params[i];
            let t_param = &target.params[i];
            // Compare declared parameter types, matching the subtype rules.
            // When both params are optional, strip `undefined` so
            // `(x?: T)` and `(x?: T | undefined)` compare as equivalent.
            let (s_effective, t_effective) = self.effective_param_type_pair(s_param, t_param);
            // Check parameter compatibility (contravariant in strict mode, bivariant in legacy)
            if !self.are_parameters_compatible_impl(s_effective, t_effective, is_method_or_ctor) {
                let inner_reason = self.explain_failure(t_effective, s_effective).map(Box::new);
                return Some(SubtypeFailureReason::ParameterTypeMismatch {
                    param_index: i,
                    source_param: s_effective,
                    target_param: t_effective,
                    inner_reason,
                });
            }
        }

        if target_has_rest {
            let Some(rest_elem_type) = rest_elem_type else {
                return None; // Invalid rest parameter
            };
            if source_has_rest {
                let source_rest = source.params.last()?;
                let target_rest = target.params.last()?;
                if self.bare_source_rest_compatibility(
                    source_rest.type_id,
                    target_rest.type_id,
                    is_method_or_ctor,
                    provisional_rest_union,
                ) == Some(false)
                {
                    let inner_reason = self
                        .explain_failure(target_rest.type_id, source_rest.type_id)
                        .map(Box::new);
                    return Some(SubtypeFailureReason::ParameterTypeMismatch {
                        param_index: source_fixed_count,
                        source_param: source_rest.type_id,
                        target_param: target_rest.type_id,
                        inner_reason,
                    });
                }
            }
            if rest_elem_type.is_any_or_unknown()
                && let Some((param_index, source_param)) =
                    self.first_top_rest_unassignable_source_param(&source.params)
            {
                let inner_reason = self
                    .explain_failure(rest_elem_type, source_param)
                    .map(Box::new);
                return Some(SubtypeFailureReason::ParameterTypeMismatch {
                    param_index,
                    source_param,
                    target_param: rest_elem_type,
                    inner_reason,
                });
            }
            if rest_is_top {
                return None;
            }

            for i in target_fixed_count..source_fixed_count {
                let s_param = &source.params[i];
                if !self.are_parameters_compatible_impl(
                    s_param.type_id,
                    rest_elem_type,
                    is_method_or_ctor,
                ) {
                    let inner_reason = self
                        .explain_failure(rest_elem_type, s_param.type_id)
                        .map(Box::new);
                    return Some(SubtypeFailureReason::ParameterTypeMismatch {
                        param_index: i,
                        source_param: s_param.type_id,
                        target_param: rest_elem_type,
                        inner_reason,
                    });
                }
            }

            if source_has_rest {
                let s_rest_param = source.params.last()?;
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                if !self.are_parameters_compatible_impl(
                    s_rest_elem,
                    rest_elem_type,
                    is_method_or_ctor,
                ) {
                    let inner_reason = self
                        .explain_failure(rest_elem_type, s_rest_elem)
                        .map(Box::new);
                    return Some(SubtypeFailureReason::ParameterTypeMismatch {
                        param_index: source_fixed_count,
                        source_param: s_rest_elem,
                        target_param: rest_elem_type,
                        inner_reason,
                    });
                }
            }
        }

        if provisional_rest_union {
            return None;
        }

        if source_has_rest {
            let rest_param = source.params.last()?;
            if target_fixed_count > source_fixed_count
                && self.is_bare_rest_type_param(rest_param.type_id)
            {
                let target_tuple = self.interner.tuple(
                    target
                        .params
                        .iter()
                        .skip(source_fixed_count)
                        .take(target_fixed_count.saturating_sub(source_fixed_count))
                        .map(|param| TupleElement {
                            type_id: param.type_id,
                            name: param.name,
                            optional: param.optional,
                            rest: false,
                        })
                        .collect(),
                );
                let inner_reason = self
                    .explain_failure(target_tuple, rest_param.type_id)
                    .map(Box::new);
                return Some(SubtypeFailureReason::ParameterTypeMismatch {
                    param_index: source_fixed_count,
                    source_param: rest_param.type_id,
                    target_param: target_tuple,
                    inner_reason,
                });
            }
            let rest_elem_type = self.get_array_element_type(rest_param.type_id);
            let rest_is_top = self.allow_bivariant_rest && rest_elem_type.is_any_or_unknown();

            if !rest_is_top {
                for i in source_fixed_count..target_fixed_count {
                    let t_param = &target.params[i];
                    if !self.are_parameters_compatible(rest_elem_type, t_param.type_id) {
                        let inner_reason = self
                            .explain_failure(t_param.type_id, rest_elem_type)
                            .map(Box::new);
                        return Some(SubtypeFailureReason::ParameterTypeMismatch {
                            param_index: i,
                            source_param: rest_elem_type,
                            target_param: t_param.type_id,
                            inner_reason,
                        });
                    }
                }
            }
        }

        None
    }

    pub(super) fn function_shape_from_call_signature(
        signature: &CallSignature,
        is_constructor: bool,
    ) -> FunctionShape {
        FunctionShape {
            params: signature.params.clone(),
            this_type: signature.this_type,
            return_type: signature.return_type,
            type_params: signature.type_params.clone(),
            type_predicate: signature.type_predicate,
            is_constructor,
            is_method: signature.is_method,
        }
    }

    pub(super) fn explain_function_to_callable_failure(
        &mut self,
        source: &FunctionShape,
        target: &CallableShape,
    ) -> Option<SubtypeFailureReason> {
        for target_signature in &target.call_signatures {
            let target_function = Self::function_shape_from_call_signature(target_signature, false);
            if !self
                .check_function_subtype(source, &target_function)
                .is_true()
            {
                return self.explain_function_failure(source, &target_function);
            }
        }
        None
    }

    pub(super) fn callable_properties_are_only_function_members(
        &self,
        properties: &[PropertyInfo],
    ) -> bool {
        properties.iter().all(|property| {
            matches!(
                self.interner.resolve_atom(property.name).as_str(),
                "apply" | "bind" | "call" | "length" | "name" | "prototype"
            )
        })
    }

    pub(super) fn explain_callable_to_callable_signature_failure(
        &mut self,
        source: &CallableShape,
        target: &CallableShape,
    ) -> Option<SubtypeFailureReason> {
        for target_signature in &target.call_signatures {
            let mut matching_source = None;
            for source_signature in &source.call_signatures {
                if self
                    .check_call_signature_subtype(source_signature, target_signature)
                    .is_true()
                {
                    matching_source = Some(source_signature);
                    break;
                }
            }
            if matching_source.is_none() {
                let source_signature = source.call_signatures.first()?;
                let source_function =
                    Self::function_shape_from_call_signature(source_signature, false);
                let target_function =
                    Self::function_shape_from_call_signature(target_signature, false);
                return self.explain_function_failure(&source_function, &target_function);
            }
        }
        None
    }
}
