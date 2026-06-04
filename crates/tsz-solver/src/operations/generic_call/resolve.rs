//! Core generic call resolution (`resolve_generic_call_inner`).

include!("resolve_large_methods/resolve_generic_call_inner_12_7.rs");

use super::visited::with_resolve_visited;
use crate::inference::infer::{InferenceContext, InferenceError, InferenceVar};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::widening;
use crate::operations::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::types::{
    FunctionShape, ParamInfo, TupleElement, TypeData, TypeId, TypeParamInfo, TypePredicate,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{debug, trace};

use super::inference_helpers::{is_bare_foreign_type_param, is_substantive_inference_candidate};
use super::readonly_direct_inference;
use super::{
    constraint_contains_primitive_constrained_type_param,
    constraint_is_primitive_type_with_resolver, instantiate_call_type, type_implies_literals_deep,
    type_references_placeholder, write_placeholder_name,
};

mod duplicate_shape;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    fn object_constraint_properties_are_any(&self, constraint: TypeId) -> bool {
        let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
            self.interner.lookup(constraint)
        else {
            return false;
        };
        let shape = self.interner.object_shape(shape_id);
        !shape.properties.is_empty()
            && shape
                .properties
                .iter()
                .all(|prop| prop.type_id == TypeId::ANY && prop.write_type == TypeId::ANY)
    }

    fn raw_instantiated_constraint_may_satisfy(&self, constraint: TypeId) -> bool {
        match self.interner.lookup(constraint) {
            Some(
                TypeData::Application(_)
                | TypeData::IndexAccess(_, _)
                | TypeData::KeyOf(_)
                | TypeData::Mapped(_)
                | TypeData::StringIntrinsic { .. },
            ) => true,
            Some(TypeData::Union(members) | TypeData::Intersection(members)) => self
                .interner
                .type_list(members)
                .iter()
                .any(|&member| self.raw_instantiated_constraint_may_satisfy(member)),
            Some(
                TypeData::Array(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner),
            ) => self.raw_instantiated_constraint_may_satisfy(inner),
            _ => false,
        }
    }

    fn satisfies_raw_instantiated_constraint(
        &mut self,
        source: TypeId,
        constraint: TypeId,
    ) -> bool {
        if !self.raw_instantiated_constraint_may_satisfy(constraint) {
            return false;
        }
        if self.checker.is_assignable_to(source, constraint) {
            return true;
        }
        self.checker
            .expand_type_alias_application(constraint)
            .is_some_and(|expanded| self.checker.is_assignable_to(source, expanded))
    }

    pub(crate) fn top_rest_any_callable_constraint(&self, constraint: TypeId) -> bool {
        if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(constraint)
            && let Some(constraint) = tp.constraint
        {
            return self.top_rest_any_callable_constraint(constraint);
        }
        let Some(shape) = Self::get_contextual_signature_cached(self.interner, constraint) else {
            return false;
        };
        if shape.is_constructor || shape.params.len() != 1 || !shape.params[0].rest {
            return false;
        }
        let rest_type = self.unwrap_readonly(shape.params[0].type_id);
        let rest_elem = if let Some(TypeData::Tuple(tuple_id)) = self.interner.lookup(rest_type) {
            let elems = self.interner.tuple_list(tuple_id);
            elems
                .iter()
                .find(|elem| elem.rest)
                .and_then(|elem| {
                    crate::type_queries::get_array_element_type(
                        self.interner.as_type_database(),
                        elem.type_id,
                    )
                    .or(Some(elem.type_id))
                })
                .unwrap_or(rest_type)
        } else {
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), rest_type)
                .unwrap_or(rest_type)
        };
        rest_elem.is_any_or_unknown() && shape.return_type.is_any_or_unknown()
    }

    pub(crate) fn callable_satisfies_top_rest_any_constraint(
        &self,
        candidate: TypeId,
        constraint: TypeId,
    ) -> bool {
        self.top_rest_any_callable_constraint(constraint)
            && Self::get_contextual_signature_cached(self.interner, candidate)
                .is_some_and(|shape| !shape.is_constructor)
    }

    fn constrain_types_for_arg_source(
        &mut self,
        arg_index: usize,
        infer_ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source: TypeId,
        target: TypeId,
        priority: crate::types::InferencePriority,
    ) {
        let source = readonly_direct_inference::wrap_readonly_annotation_source(
            self.interner.as_type_database(),
            source,
            self.arg_source_is_readonly_annotation
                .get(arg_index)
                .copied()
                .unwrap_or(false),
        );

        if !self
            .arg_source_is_type_annotation
            .get(arg_index)
            .copied()
            .unwrap_or(false)
        {
            self.constrain_types(infer_ctx, var_map, source, target, priority);
            return;
        }

        let was_type_annotation = infer_ctx.source_is_type_annotation;
        infer_ctx.source_is_type_annotation = true;
        self.constrain_types(infer_ctx, var_map, source, target, priority);
        infer_ctx.source_is_type_annotation = was_type_annotation;
    }

    fn generic_rest_tuple_callback_arity_mismatch(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> Option<CallResult> {
        let rest_param = func.params.last().filter(|param| param.rest)?;
        let rest_type_param =
            self.type_param_name_if_generic_rest_tuple_param(func, rest_param.type_id)?;
        let rest_start = func.params.len().saturating_sub(1);
        let rest_arg_count = arg_types.len().saturating_sub(rest_start);

        for (index, param) in func.params.iter().take(rest_start).enumerate() {
            let Some(target_shape) =
                Self::get_contextual_signature_cached(self.interner, param.type_id)
            else {
                continue;
            };
            let target_shape = self.normalize_function_shape_params_for_context(&target_shape);
            let Some(target_rest) = target_shape.params.last().filter(|param| param.rest) else {
                continue;
            };
            if self.type_param_name_if_generic_rest_tuple_param(func, target_rest.type_id)
                != Some(rest_type_param)
            {
                continue;
            }

            let Some(source_type) = arg_types.get(index).copied() else {
                continue;
            };
            let Some(source_shape) =
                Self::get_contextual_signature_cached(self.interner, source_type)
            else {
                continue;
            };
            let source_shape = self.normalize_function_shape_params_for_context(&source_shape);
            let (callback_min, callback_max) = self.arg_count_bounds(&source_shape.params);

            if rest_arg_count < callback_min || callback_max.is_some_and(|max| rest_arg_count > max)
            {
                return Some(CallResult::ArgumentCountMismatch {
                    expected_min: rest_start + callback_min,
                    expected_max: callback_max.map(|max| rest_start + max),
                    actual: arg_types.len(),
                });
            }
        }

        None
    }

    __tsz_split_resolve_resolve_generic_call_inner_12_7!();

    fn apply_callback_optional_rest_slots(
        &mut self,
        func: &FunctionShape,
        final_args: &[TypeId],
        instantiated_params: &mut [ParamInfo],
    ) {
        let Some(raw_rest_param) = func.params.last().filter(|param| param.rest) else {
            return;
        };
        let rest_index = func.params.len().saturating_sub(1);
        let Some(instantiated_rest_param) = instantiated_params.get_mut(rest_index) else {
            return;
        };
        if !instantiated_rest_param.rest {
            return;
        }

        let rest_type = self.unwrap_readonly(instantiated_rest_param.type_id);
        let rest_type = self.evaluate_rest_param_type(rest_type);
        let Some(TypeData::Tuple(elements_id)) = self.interner.lookup(rest_type) else {
            return;
        };
        let mut elements = self.interner.tuple_list(elements_id).to_vec();
        let mut changed = false;

        for (param_index, raw_param) in func.params[..rest_index].iter().enumerate() {
            let Some(target_fn) =
                Self::get_contextual_signature_cached(self.interner, raw_param.type_id)
            else {
                continue;
            };
            let target_uses_same_rest = target_fn
                .params
                .last()
                .is_some_and(|param| param.rest && param.type_id == raw_rest_param.type_id);
            if !target_uses_same_rest {
                continue;
            }

            let Some(&source_arg) = final_args.get(param_index) else {
                continue;
            };
            let Some(source_fn) = Self::get_contextual_signature_cached(self.interner, source_arg)
            else {
                continue;
            };
            let source_params: Vec<ParamInfo> = source_fn
                .params
                .iter()
                .flat_map(|param| {
                    crate::type_queries::unpack_tuple_rest_parameter(self.interner, param)
                })
                .collect();

            for (element, source_param) in elements.iter_mut().zip(source_params.iter()) {
                if source_param.optional && !element.rest && !element.optional {
                    element.optional = true;
                    changed = true;
                }
            }
        }

        if changed {
            instantiated_rest_param.type_id = self.interner.tuple(elements);
        }
    }

    /// Returns `true` when `ty` is or structurally contains a `TypeParameter` that
    /// does not belong to the current generic call (i.e. is absent from `var_map`).
    ///
    /// "Foreign" covers two cases:
    ///  - A bare `__infer_*` placeholder from an enclosing call scope.
    ///  - The original, user-named `TypeParameter` (e.g. `T`) from the enclosing
    ///    function — which appears when `generic_function_shape_for_inference`
    ///    renames the callee's type params but the argument type still carries
    ///    the outer scope's unsubstituted `TypeParameter`.
    ///
    /// Intrinsic and concrete types (primitives, objects, etc.) are never foreign.
    /// The caller is responsible for ensuring `has_usable_contra_candidates` is
    /// true before using this result, to prevent false positives for independent
    /// generic calls like `identity(value[key])`.
    fn type_contains_any_foreign_type_param(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
    ) -> bool {
        if ty.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(ty) {
            // Any TypeParameter not registered in this call's var_map is foreign.
            Some(TypeData::TypeParameter(_)) => !var_map.contains_key(&ty),
            Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => self
                .interner
                .type_list(list_id)
                .iter()
                .any(|&m| self.type_contains_any_foreign_type_param(m, var_map)),
            Some(TypeData::IndexAccess(obj, idx)) => {
                self.type_contains_any_foreign_type_param(obj, var_map)
                    || self.type_contains_any_foreign_type_param(idx, var_map)
            }
            Some(TypeData::Array(elem)) => self.type_contains_any_foreign_type_param(elem, var_map),
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                self.type_contains_any_foreign_type_param(app.base, var_map)
                    || app
                        .args
                        .iter()
                        .any(|&a| self.type_contains_any_foreign_type_param(a, var_map))
            }
            _ => false,
        }
    }

    fn application_expands_to_conditional_alias_for_return_display(
        &mut self,
        type_id: TypeId,
    ) -> bool {
        if !matches!(
            self.interner.lookup(type_id),
            Some(TypeData::Application(_))
        ) {
            return false;
        }
        self.checker
            .expand_type_alias_application(type_id)
            .is_some_and(|expanded| {
                matches!(
                    self.interner.lookup(expanded),
                    Some(TypeData::Conditional(_))
                )
            })
    }
}
