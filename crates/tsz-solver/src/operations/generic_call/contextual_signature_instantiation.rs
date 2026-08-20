//! Contextual generic function instantiation helpers.

use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{
    FunctionShape, ParamInfo, TupleElement, TypeData, TypeId, TypeParamInfo, TypePredicate,
};
use rustc_hash::{FxHashMap, FxHashSet};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Whether a deferred argument is a *concrete* function-like value: at
    /// least one signature with parameters, none of them `any`. Such an
    /// argument supplies real Round-2 inference for the variables its target
    /// references, so the contextual return type must not pre-seed them;
    /// lambdas with `any`-typed parameters genuinely need the pre-fix for
    /// contextual typing.
    pub(super) fn arg_is_concrete_function_like(&self, arg_type: TypeId) -> bool {
        match self.interner.lookup(arg_type) {
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                !shape.params.is_empty() && shape.params.iter().all(|p| p.type_id != TypeId::ANY)
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                    .any(|sig| {
                        !sig.params.is_empty()
                            && sig.params.iter().all(|p| p.type_id != TypeId::ANY)
                    })
            }
            _ => false,
        }
    }

    /// Record tsc's `inference.isFixed` for the literal-widening gate: every
    /// inference variable mentioned by a context-sensitive callback argument's
    /// contextual signature — in its parameter AND return positions, because
    /// tsc's fixing mapper instantiates the whole signature
    /// (`makeFixingMapperForContext`) — becomes contextually fixed, and a fixed
    /// inference widens its fresh literal candidates even at the return type's
    /// top level (`getCovariantInference`'s `widenLiteralTypes` gate).
    ///
    /// Context sensitivity is read from the AST-level
    /// `arg_callback_param_unannotated` mask first: on a checker re-resolution
    /// the callback argument arrives already checked (a concrete function
    /// type), so the type-level `is_contextually_sensitive` probe reports
    /// `false` on exactly the pass whose result feeds the call's type (#17282
    /// records the same re-resolution asymmetry). The type-level probe still
    /// applies as a fallback for shapes the mask cannot see (object literals
    /// carrying context-sensitive members, spread-shifted positions).
    pub(super) fn mark_contextually_fixed_inference_vars(
        &mut self,
        infer_ctx: &mut InferenceContext,
        arg_types: &[TypeId],
        instantiated_params: &[ParamInfo],
        var_map: &FxHashMap<TypeId, InferenceVar>,
        placeholder_probe_map: &mut FxHashMap<TypeId, InferenceVar>,
        placeholder_visited: &mut FxHashSet<TypeId>,
    ) {
        for (i, &arg_type) in arg_types.iter().enumerate() {
            let mask_context_sensitive = self
                .arg_callback_param_unannotated
                .get(i)
                .is_some_and(|mask| mask.iter().any(|&unannotated| unannotated));
            if !mask_context_sensitive && !self.is_contextually_sensitive(arg_type) {
                continue;
            }
            let Some(target_type) =
                self.param_type_for_arg_index(instantiated_params, i, arg_types.len())
            else {
                continue;
            };
            let Some(shape) = Self::get_contextual_signature_cached(self.interner, target_type)
            else {
                continue;
            };
            for position_type in shape
                .params
                .iter()
                .map(|callback_param| callback_param.type_id)
                .chain(std::iter::once(shape.return_type))
            {
                placeholder_visited.clear();
                for var in self.collect_placeholder_vars_in_type(
                    position_type,
                    var_map,
                    placeholder_probe_map,
                    placeholder_visited,
                ) {
                    infer_ctx.mark_contextually_fixed(var);
                }
            }
        }
    }

    fn direct_type_param_info(&self, type_id: TypeId) -> Option<TypeParamInfo> {
        crate::type_param_info(self.interner.as_type_database(), type_id)
    }

    fn function_uses_only_naked_type_params(
        &self,
        func: &FunctionShape,
        type_params: &[TypeParamInfo],
    ) -> bool {
        if func.params.is_empty() {
            return false;
        }
        let params_are_naked = func.params.iter().all(|param| {
            self.direct_type_param_info(param.type_id)
                .is_some_and(|info| {
                    type_params
                        .iter()
                        .any(|type_param| type_param.is_same_binder(info))
                })
        });
        params_are_naked
            && self
                .direct_type_param_info(func.return_type)
                .is_some_and(|info| {
                    type_params
                        .iter()
                        .any(|type_param| type_param.is_same_binder(info))
                })
    }

    pub(super) fn constrain_return_context_params_with_rest(
        &mut self,
        infer_ctx: &mut InferenceContext<'_>,
        var_map: &FxHashMap<TypeId, InferenceVar>,
        source_params: &[ParamInfo],
        target_params: &[ParamInfo],
        priority: crate::types::InferencePriority,
    ) -> bool {
        use crate::type_queries::unpack_tuple_rest_parameter;

        let source_params: Vec<_> = source_params
            .iter()
            .flat_map(|param| unpack_tuple_rest_parameter(self.interner, param))
            .collect();
        let target_params: Vec<_> = target_params
            .iter()
            .flat_map(|param| unpack_tuple_rest_parameter(self.interner, param))
            .collect();
        for (index, source_param) in source_params.iter().enumerate() {
            if !source_param.rest {
                continue;
            }

            for (fixed_source, fixed_target) in source_params
                .iter()
                .take(index)
                .zip(target_params.iter().take(index))
            {
                let nested_structural = self.constrain_return_context_structure(
                    infer_ctx,
                    var_map,
                    fixed_target.type_id,
                    fixed_source.type_id,
                    priority,
                );
                if !nested_structural {
                    self.constrain_types(
                        infer_ctx,
                        var_map,
                        fixed_target.type_id,
                        fixed_source.type_id,
                        priority,
                    );
                }
            }

            let target_type = if let Some(target_param) = target_params.get(index)
                && target_param.rest
                && index + 1 == target_params.len()
            {
                target_param.type_id
            } else {
                let remaining = target_params[index..]
                    .iter()
                    .map(|param| TupleElement {
                        type_id: param.type_id,
                        name: param.name,
                        optional: param.optional,
                        rest: param.rest,
                    })
                    .collect();
                self.interner.tuple(remaining)
            };

            let nested_structural = self.constrain_return_context_structure(
                infer_ctx,
                var_map,
                target_type,
                source_param.type_id,
                priority,
            );
            if !nested_structural {
                self.constrain_types(
                    infer_ctx,
                    var_map,
                    target_type,
                    source_param.type_id,
                    priority,
                );
            }
            return true;
        }

        let Some(target_rest) = target_params.last().filter(|param| param.rest) else {
            return false;
        };
        let Some(&var) = var_map.get(&target_rest.type_id) else {
            return false;
        };

        let fixed_count = target_params.len().saturating_sub(1);
        for (source_param, target_param) in source_params
            .iter()
            .take(fixed_count)
            .zip(target_params.iter().take(fixed_count))
        {
            let nested_structural = self.constrain_return_context_structure(
                infer_ctx,
                var_map,
                target_param.type_id,
                source_param.type_id,
                priority,
            );
            if !nested_structural {
                self.constrain_types(
                    infer_ctx,
                    var_map,
                    target_param.type_id,
                    source_param.type_id,
                    priority,
                );
            }
        }

        if source_params.len() > fixed_count {
            let tuple_elements = source_params[fixed_count..]
                .iter()
                .map(|param| TupleElement {
                    type_id: if param.optional {
                        self.interner.union2(param.type_id, TypeId::UNDEFINED)
                    } else {
                        param.type_id
                    },
                    name: param.name,
                    optional: param.optional,
                    rest: param.rest,
                })
                .collect();
            infer_ctx.add_candidate(
                var,
                self.interner.tuple(tuple_elements),
                crate::types::InferencePriority::NakedTypeVariable,
            );
        }

        true
    }

    pub(crate) fn has_conflicting_contextual_signature_instantiation(
        &mut self,
        source_ty: TypeId,
        target_ty: TypeId,
    ) -> bool {
        self.conflicting_contextual_signature_instantiation_type(source_ty, target_ty)
            .is_some()
    }

    pub(crate) fn conflicting_contextual_signature_instantiation_type(
        &mut self,
        source_ty: TypeId,
        target_ty: TypeId,
    ) -> Option<TypeId> {
        let source_fn = Self::get_contextual_signature_cached(self.interner, source_ty)?;
        let target_fn = Self::get_contextual_signature_cached(self.interner, target_ty)?;

        let substitution =
            self.conflicting_contextual_param_candidate_substitution(&source_fn, &target_fn)?;
        let instantiated = FunctionShape {
            type_params: vec![],
            params: target_fn.params.clone(),
            this_type: target_fn.this_type,
            return_type: instantiate_type(self.interner, source_fn.return_type, &substitution),
            type_predicate: source_fn.type_predicate.as_ref().map(|pred| TypePredicate {
                asserts: pred.asserts,
                target: pred.target,
                type_id: pred
                    .type_id
                    .map(|ty| instantiate_type(self.interner, ty, &substitution)),
                parameter_index: pred.parameter_index,
            }),
            is_constructor: target_fn.is_constructor,
            is_method: target_fn.is_method,
        };
        Some(self.interner.function(instantiated))
    }

    /// Check whether a generic function argument's type-parameter constraints are
    /// strictly stronger than the corresponding outer type parameters of the call
    /// site, which would make the argument structurally incompatible.
    ///
    /// Structural rule (mirrors PR #11702's same-arity check for assignment):
    /// `<U extends C>(x: U) => U` is NOT assignable to `<T>(x: T) => T` when
    /// `C` strictly narrows `T`'s effective constraint (`unknown` when `T` is
    /// unconstrained). Only fires when the source and the outer target have the
    /// same arity (same number of relevant type parameters).
    ///
    /// Returns `Some(generic_target_id)` (the reconstructed generic target, used
    /// as the "expected" type in `ArgumentTypeMismatch`) when the check fails;
    /// returns `None` when the argument is compatible.
    pub(crate) fn check_generic_arg_stricter_constraint_mismatch(
        &mut self,
        arg_type: TypeId,
        raw_param_type: TypeId,
        outer_type_params: &[TypeParamInfo],
    ) -> Option<TypeId> {
        if outer_type_params.is_empty() {
            return None;
        }

        // Source must be a generic function with at least one constraint.
        let source_fn = Self::get_contextual_signature_cached(self.interner, arg_type)?;
        tracing::trace!(
            arg_type = arg_type.0,
            source_tp_count = source_fn.type_params.len(),
            "check_generic_arg_stricter_constraint_mismatch: source_fn"
        );
        let all_source_tps_constrained = source_fn
            .type_params
            .iter()
            .all(|tp| tp.constraint.is_some());
        let has_strict_source_constraint = source_fn
            .type_params
            .iter()
            .filter_map(|tp| tp.constraint)
            .any(|constraint| constraint != TypeId::UNKNOWN);
        if source_fn.type_params.is_empty()
            || !all_source_tps_constrained
            || !has_strict_source_constraint
            || !self.function_uses_only_naked_type_params(&source_fn, &source_fn.type_params)
        {
            tracing::trace!(
                "check_generic_arg_stricter_constraint_mismatch: source shape is not strict naked generic, skip"
            );
            return None;
        }

        // Quick guard: raw_param_type must reference type parameters at all.
        if !crate::visitor::contains_type_parameters(self.interner, raw_param_type) {
            tracing::trace!(
                raw_param_type = raw_param_type.0,
                "check_generic_arg_stricter_constraint_mismatch: no type params in raw_param_type, skip"
            );
            return None;
        }

        // Get the target fn shape first so we know which names are local
        // (bound inside raw_param_type itself, e.g. `<V>(x: T, y: V) => T`).
        let target_fn = Self::get_contextual_signature_cached(self.interner, raw_param_type)?;
        if !self.function_uses_only_naked_type_params(&target_fn, outer_type_params) {
            tracing::trace!(
                "check_generic_arg_stricter_constraint_mismatch: target shape is not naked outer generic, skip"
            );
            return None;
        }

        let mut all_type_params_in_param = Vec::new();
        for ty in target_fn
            .params
            .iter()
            .map(|param| param.type_id)
            .chain(std::iter::once(target_fn.return_type))
        {
            let info = self.direct_type_param_info(ty)?;
            if target_fn
                .type_params
                .iter()
                .any(|type_param| type_param.is_same_binder(info))
            {
                return None;
            }
            if !all_type_params_in_param
                .iter()
                .any(|type_param: &TypeParamInfo| type_param.is_same_binder(info))
            {
                all_type_params_in_param.push(info);
            }
        }

        let relevant_outer_tps: Vec<&TypeParamInfo> = outer_type_params
            .iter()
            .filter(|tp| {
                all_type_params_in_param
                    .iter()
                    .any(|info| tp.is_same_binder(*info))
            })
            .collect();

        tracing::trace!(
            relevant_count = relevant_outer_tps.len(),
            source_tp_count = source_fn.type_params.len(),
            "check_generic_arg_stricter_constraint_mismatch: arity check"
        );

        // Only apply when the outer target arity matches the source arity.
        if relevant_outer_tps.is_empty() || relevant_outer_tps.len() != source_fn.type_params.len()
        {
            tracing::trace!("check_generic_arg_stricter_constraint_mismatch: arity mismatch, skip");
            return None;
        }

        // Build a locally-generic version of the target function by promoting the
        // outer type params to local quantifiers. This is equivalent to what tsc
        // does when it reconstructs a canonical `<T>(x: T) => T` generic for the
        // comparison — the outer T becomes a fresh local quantifier, and the
        // PR #11702 same-arity constraint check in `checking.rs` handles the rest.
        let generic_target = FunctionShape {
            type_params: relevant_outer_tps.iter().map(|&&tp| tp).collect(),
            params: target_fn.params.clone(),
            return_type: target_fn.return_type,
            this_type: target_fn.this_type,
            type_predicate: target_fn.type_predicate,
            is_constructor: target_fn.is_constructor,
            is_method: target_fn.is_method,
        };
        let generic_target_id = self.interner.function(generic_target);

        // Delegate to the standard assignability check. PR #11702's fix in
        // `checking.rs` (same-arity generic constraint comparison) handles
        // detecting when the source constraint is strictly stronger.
        let assignable = self.checker.is_assignable_to(arg_type, generic_target_id);
        tracing::trace!(
            arg_type = arg_type.0,
            generic_target_id = generic_target_id.0,
            assignable,
            "check_generic_arg_stricter_constraint_mismatch: assignability result"
        );
        if assignable {
            return None;
        }

        Some(generic_target_id)
    }

    pub(crate) fn arg_mismatch(
        &mut self,
        arg_type: TypeId,
        raw_param_type: TypeId,
        final_param_type: TypeId,
        func: &FunctionShape,
    ) -> Option<TypeId> {
        if let Some(expected) =
            self.conflicting_contextual_signature_instantiation_type(arg_type, final_param_type)
        {
            return Some(expected);
        }
        self.check_generic_arg_stricter_constraint_mismatch(
            arg_type,
            raw_param_type,
            &func.type_params,
        )
    }

    pub(super) fn conflicting_contextual_param_candidate_substitution(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> Option<TypeSubstitution> {
        use crate::type_queries::unpack_tuple_rest_parameter;

        let mut tracked_type_params = source.type_params.clone();
        for source_ty in source
            .params
            .iter()
            .map(|param| param.type_id)
            .chain(std::iter::once(source.return_type))
        {
            for nested in
                crate::visitor::collect_all_types(self.interner.as_type_database(), source_ty)
            {
                if let Some(info) = crate::type_param_info(self.interner.as_type_database(), nested)
                    && info.is_infer_source()
                    && !tracked_type_params
                        .iter()
                        .any(|type_param| type_param.is_same_binder(info))
                {
                    tracked_type_params.push(info);
                }
            }
        }
        if tracked_type_params.is_empty() {
            return None;
        }

        let source_params: Vec<_> = source
            .params
            .iter()
            .flat_map(|param| unpack_tuple_rest_parameter(self.interner, param))
            .collect();
        let target_params: Vec<_> = target
            .params
            .iter()
            .flat_map(|param| unpack_tuple_rest_parameter(self.interner, param))
            .collect();

        let mut contextual_candidates: FxHashMap<_, Vec<TypeId>> = FxHashMap::default();
        for (source_param, target_param) in source_params.iter().zip(target_params.iter()) {
            let source_effective = if source_param.optional {
                self.interner
                    .union2(source_param.type_id, TypeId::UNDEFINED)
            } else {
                source_param.type_id
            };
            let target_effective = if target_param.optional {
                self.interner
                    .union2(target_param.type_id, TypeId::UNDEFINED)
            } else {
                target_param.type_id
            };
            if target_effective.is_any_unknown_or_error() {
                continue;
            }

            if let Some(info) =
                crate::type_param_info(self.interner.as_type_database(), source_effective)
                && tracked_type_params
                    .iter()
                    .any(|type_param| type_param.is_same_binder(info))
            {
                contextual_candidates
                    .entry(info.name)
                    .or_default()
                    .push(target_effective);
            }
        }

        let has_conflict = contextual_candidates.values().any(|candidates| {
            for (idx, &left) in candidates.iter().enumerate() {
                for &right in candidates.iter().skip(idx + 1) {
                    if left == right {
                        continue;
                    }
                    if !self.checker.is_assignable_to(left, right)
                        && !self.checker.is_assignable_to(right, left)
                    {
                        return true;
                    }
                }
            }
            false
        });

        if !has_conflict {
            return None;
        }

        let mut substitution = TypeSubstitution::new();
        substitution.protect_type_parameters(&source.type_params);
        for tracked_type_param in &tracked_type_params {
            let tp_name = tracked_type_param.name;
            let is_source_placeholder = tracked_type_param.is_infer_source();
            let replacement = contextual_candidates
                .get(&tp_name)
                .and_then(|candidates| candidates.first().copied())
                .or_else(|| {
                    source
                        .type_params
                        .iter()
                        .find(|type_param| type_param.is_same_binder(*tracked_type_param))
                        .and_then(|tp| tp.constraint)
                });
            let Some(replacement) =
                replacement.or_else(|| (!is_source_placeholder).then_some(TypeId::UNKNOWN))
            else {
                continue;
            };
            substitution.insert(tp_name, replacement);
        }
        Some(substitution)
    }
}
