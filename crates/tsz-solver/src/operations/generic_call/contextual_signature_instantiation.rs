//! Contextual generic function instantiation helpers.

use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{FunctionShape, ParamInfo, TupleElement, TypeId, TypePredicate};
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
