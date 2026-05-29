//! Contextual generic function instantiation helpers.

use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{FunctionShape, ParamInfo, TupleElement, TypeId, TypeParamInfo, TypePredicate};
use rustc_hash::FxHashMap;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
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
        if source_fn.type_params.is_empty()
            || !source_fn
                .type_params
                .iter()
                .any(|tp| tp.constraint.is_some())
        {
            tracing::trace!(
                "check_generic_arg_stricter_constraint_mismatch: no constrained tp, skip"
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
        let local_tp_names: rustc_hash::FxHashSet<tsz_common::Atom> =
            target_fn.type_params.iter().map(|tp| tp.name).collect();

        // Collect outer type param names that appear (non-locally) in raw_param_type.
        // TypeParameters created with `fresh_type_param` have unique TypeIds that
        // won't match a re-interned version, so we match by name (Atom) instead of TypeId.
        let all_tp_names_in_param: rustc_hash::FxHashSet<tsz_common::Atom> =
            crate::visitor::collect_all_types(self.interner, raw_param_type)
                .into_iter()
                .filter_map(|ty| crate::type_param_info(self.interner.as_type_database(), ty))
                .map(|info| info.name)
                .filter(|name| !local_tp_names.contains(name))
                .collect();

        let relevant_outer_tps: Vec<&TypeParamInfo> = outer_type_params
            .iter()
            .filter(|tp| all_tp_names_in_param.contains(&tp.name))
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

    pub(super) fn conflicting_contextual_param_candidate_substitution(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> Option<TypeSubstitution> {
        use crate::type_queries::unpack_tuple_rest_parameter;

        let mut tracked_type_params: Vec<_> = source.type_params.iter().map(|tp| tp.name).collect();
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
                    && self
                        .interner
                        .resolve_atom(info.name)
                        .as_str()
                        .starts_with("__infer_src_")
                    && !tracked_type_params.contains(&info.name)
                {
                    tracked_type_params.push(info.name);
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
                && tracked_type_params.contains(&info.name)
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
        for &tp_name in &tracked_type_params {
            let is_source_placeholder = self
                .interner
                .resolve_atom(tp_name)
                .as_str()
                .starts_with("__infer_src_");
            let replacement = contextual_candidates
                .get(&tp_name)
                .and_then(|candidates| candidates.first().copied())
                .or_else(|| {
                    source
                        .type_params
                        .iter()
                        .find(|tp| tp.name == tp_name)
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
