use crate::context::speculation::FullSnapshot;
use crate::query_boundaries::common::{
    CallResult, TypeSubstitution, contains_infer_types, contains_type_parameters, instantiate_type,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{CallSignature, FunctionShape, ParamInfo, TypeId};

use super::{CallableContext, SelectedTypePredicate};

pub(super) struct ContextualRetryInput<'s> {
    pub(super) result: &'s CallResult,
    pub(super) sig: &'s CallSignature,
    pub(super) instantiated_params: Option<&'s Vec<ParamInfo>>,
    pub(super) resolved_func_type: TypeId,
    pub(super) args: &'s [NodeIndex],
    pub(super) force_bivariant_callbacks: bool,
    pub(super) contextual_type: Option<TypeId>,
    pub(super) actual_this_type: Option<TypeId>,
    pub(super) overload_snap: &'s FullSnapshot,
    pub(super) has_contextual_refresh_args: bool,
}

impl<'a> CheckerState<'a> {
    pub(super) fn retry_overload_after_contextual_refresh_mismatch(
        &mut self,
        input: ContextualRetryInput<'_>,
        selected_type_predicate: &mut SelectedTypePredicate,
    ) -> Option<CallResult> {
        let ContextualRetryInput {
            result,
            sig,
            instantiated_params,
            resolved_func_type,
            args,
            force_bivariant_callbacks,
            contextual_type,
            actual_this_type,
            overload_snap,
            has_contextual_refresh_args,
        } = input;

        if !matches!(result, CallResult::ArgumentTypeMismatch { .. })
            || sig.type_params.is_empty()
            || !has_contextual_refresh_args
        {
            return None;
        }

        let sig_shape = FunctionShape {
            params: sig.params.clone(),
            return_type: sig.return_type,
            this_type: sig.this_type,
            type_params: sig.type_params.clone(),
            type_predicate: sig.type_predicate,
            is_constructor: false,
            is_method: sig.is_method,
        };
        let return_sub_for_retry = if contextual_type.is_some() {
            self.compute_return_context_substitution_from_shape(&sig_shape, contextual_type)
        } else {
            TypeSubstitution::new()
        };

        let mut retry_substitution = None;
        let retry_params = if !return_sub_for_retry.is_empty() {
            let mut combined_sub = if let Some(inst) = instantiated_params {
                self.extract_arg_inference_substitution(&sig.params, inst, &sig.type_params)
            } else {
                TypeSubstitution::new()
            };
            for tp in &sig.type_params {
                if let Some(ty) = return_sub_for_retry.get(tp.name) {
                    combined_sub.insert(tp.name, ty);
                }
            }
            retry_substitution = Some(combined_sub.clone());
            Some(
                sig.params
                    .iter()
                    .map(|param| {
                        let mut instantiated_param = *param;
                        instantiated_param.type_id =
                            instantiate_type(self.ctx.types, param.type_id, &combined_sub);
                        instantiated_param
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            instantiated_params.cloned()
        };
        let retry_params = retry_params
            .map(|params| self.resolve_signature_parameter_type_queries(&sig.params, &params));
        let instantiated_params = retry_params.as_ref()?;

        let before_retry_snap = self.snapshot_overload_retry_state();
        let retry_return_type = match result {
            CallResult::ArgumentTypeMismatch {
                fallback_return, ..
            } => *fallback_return,
            _ => sig.return_type,
        };
        self.rollback_overload_retry_state(overload_snap);
        self.ctx.node_types = Default::default();
        for &arg_idx in args {
            self.invalidate_expression_for_contextual_retry(arg_idx);
            self.ctx.daa_error_nodes.remove(&arg_idx.0);
            self.ctx.flow_narrowed_nodes.remove(&arg_idx.0);
        }

        let sig_callable_ctx = {
            let instantiated_func = self.ctx.types.factory().function(FunctionShape {
                params: instantiated_params.clone(),
                return_type: retry_return_type,
                this_type: sig.this_type,
                type_params: vec![],
                type_predicate: sig.type_predicate,
                is_constructor: false,
                is_method: sig.is_method,
            });
            CallableContext::new(instantiated_func)
        };
        let used_return_context_sub = !return_sub_for_retry.is_empty();
        let refreshed_contextual_types = if used_return_context_sub {
            (0..args.len())
                .map(|i| {
                    let param = instantiated_params
                        .get(i)
                        .map(|p| (p.type_id, p.rest))
                        .or_else(|| {
                            let last = instantiated_params.last()?;
                            last.rest.then_some((last.type_id, true))
                        })?;
                    let param_type = if param.1 {
                        self.rest_argument_element_type_with_env(param.0)
                    } else {
                        param.0
                    };
                    Some(self.normalize_contextual_call_param_type(param_type))
                })
                .collect()
        } else {
            self.contextual_param_types_from_instantiated_params(instantiated_params, args.len())
        };

        let prev_preserve_literals_retry = self.ctx.preserve_literal_types;
        let prev_in_const_assertion_retry = self.ctx.in_const_assertion;
        self.ctx.preserve_literal_types = true;
        if Self::signature_const_type_params_require_readonly_argument_context(
            self.ctx.types,
            &sig.type_params,
        ) {
            self.ctx.in_const_assertion = true;
        }
        let refreshed_arg_types = if used_return_context_sub {
            let tracked_type_params: rustc_hash::FxHashSet<_> =
                sig.type_params.iter().map(|tp| tp.name).collect();
            let mut progressive_sub = retry_substitution.unwrap_or_else(TypeSubstitution::new);
            let mut progressive_args = Vec::with_capacity(args.len());
            for (i, &arg_idx) in args.iter().enumerate() {
                let contextual_params = sig
                    .params
                    .iter()
                    .map(|param| {
                        let mut instantiated_param = *param;
                        instantiated_param.type_id =
                            instantiate_type(self.ctx.types, param.type_id, &progressive_sub);
                        instantiated_param
                    })
                    .collect::<Vec<_>>();
                let contextual_type = contextual_params
                    .get(i)
                    .map(|p| (p.type_id, p.rest))
                    .or_else(|| {
                        let last = contextual_params.last()?;
                        last.rest.then_some((last.type_id, true))
                    })
                    .map(|(param_type, rest)| {
                        let param_type = if rest {
                            self.rest_argument_element_type_with_env(param_type)
                        } else {
                            param_type
                        };
                        self.normalize_contextual_call_param_type(param_type)
                    });
                let arg_type = self.compute_single_call_argument_type(
                    arg_idx,
                    contextual_type,
                    false,
                    i,
                    args.len(),
                    true,
                    sig_callable_ctx,
                );
                let arg_for_refinement = contextual_type
                    .map(|expected| {
                        self.instantiate_generic_function_argument_against_target_params(
                            arg_type, expected,
                        )
                    })
                    .unwrap_or(arg_type);
                progressive_args.push(arg_for_refinement);
                if let Some(shape_param) = sig.params.get(i).map(|p| p.type_id).or_else(|| {
                    let last = sig.params.last()?;
                    last.rest.then_some(last.type_id)
                }) {
                    let mut arg_substitution = TypeSubstitution::new();
                    let mut visited = rustc_hash::FxHashSet::default();
                    self.collect_return_context_substitution(
                        shape_param,
                        arg_for_refinement,
                        &tracked_type_params,
                        &mut arg_substitution,
                        &mut visited,
                    );
                    for (&name, &ty) in arg_substitution.map() {
                        if ty == TypeId::UNKNOWN
                            || ty == TypeId::ERROR
                            || self.target_contains_blocking_return_context_type_params(
                                ty,
                                &tracked_type_params,
                            )
                        {
                            continue;
                        }
                        if return_sub_for_retry.get(name).is_some() {
                            continue;
                        }
                        let should_update = match progressive_sub.get(name) {
                            None => true,
                            Some(existing) if existing == ty => false,
                            Some(existing) => {
                                existing == TypeId::UNKNOWN
                                    || existing == TypeId::ERROR
                                    || contains_type_parameters(self.ctx.types, existing)
                                    || contains_infer_types(self.ctx.types, existing)
                            }
                        };
                        if should_update {
                            progressive_sub.insert(name, ty);
                        }
                    }
                }
            }
            progressive_args
        } else {
            self.collect_call_argument_types_with_context(
                args,
                |i, _arg_count| refreshed_contextual_types.get(i).copied().flatten(),
                false,
                None,
                sig_callable_ctx,
            )
        };
        self.ctx.preserve_literal_types = prev_preserve_literals_retry;
        self.ctx.in_const_assertion = prev_in_const_assertion_retry;

        let (retry_result, retry_predicate, _) = self.resolve_call_with_checker_adapter(
            resolved_func_type,
            &refreshed_arg_types,
            force_bivariant_callbacks,
            contextual_type,
            actual_this_type,
        );
        if let CallResult::Success(retry_return_type) = retry_result {
            if retry_predicate.is_some() {
                *selected_type_predicate = retry_predicate;
            }
            let final_return_type = if used_return_context_sub {
                let mut combined_sub = self.extract_arg_inference_substitution(
                    &sig.params,
                    instantiated_params,
                    &sig.type_params,
                );
                for tp in &sig.type_params {
                    if let Some(ty) = return_sub_for_retry.get(tp.name) {
                        combined_sub.insert(tp.name, ty);
                    }
                }
                instantiate_type(self.ctx.types, sig.return_type, &combined_sub)
            } else {
                retry_return_type
            };
            Some(CallResult::Success(final_return_type))
        } else {
            self.rollback_overload_retry_state(&before_retry_snap);
            None
        }
    }
}
