use crate::context::speculation::FullSnapshot;
use crate::query_boundaries::common::{
    CallResult, TypeSubstitution, contains_infer_types, contains_type_parameters, instantiate_type,
};
use crate::query_boundaries::construct_signatures::{
    function_shape_from_call_signature_preserving_method, function_type_from_parts,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{CallSignature, ParamInfo, TypeId};

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
    /// Pristine caller `node_types` snapshot; the retry's speculative
    /// collection runs in an overlay over it (read-through, isolated writes).
    pub(super) caller_node_types: &'s crate::context::NodeTypeCache,
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
            caller_node_types,
        } = input;

        if !matches!(result, CallResult::ArgumentTypeMismatch { .. })
            || sig.type_params.is_empty()
            || !has_contextual_refresh_args
        {
            return None;
        }
        if let CallResult::ArgumentTypeMismatch {
            actual, expected, ..
        } = result
            && crate::query_boundaries::assignability::relation_contains_declared_bare_function_rest(
                self.ctx.types,
                &self.ctx,
                *actual,
                *expected,
            )
        {
            // Contextual collection may retype a declared callback argument
            // against the very parameter it failed to satisfy. Preserve a raw
            // rest-binder mismatch instead of turning that circular retry into
            // success.
            return None;
        }

        let sig_shape = function_shape_from_call_signature_preserving_method(sig, false);
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
            self.merge_return_context_substitution(
                &mut combined_sub,
                &sig.type_params,
                &return_sub_for_retry,
            );
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
        self.ctx.node_types = caller_node_types.overlay();
        for &arg_idx in args {
            self.invalidate_expression_for_contextual_retry(arg_idx);
            self.ctx.daa_error_nodes.remove(&arg_idx.0);
            self.ctx.flow_narrowed_nodes.remove(&arg_idx.0);
        }

        let sig_callable_ctx = {
            let instantiated_func = function_type_from_parts(
                self.ctx.types,
                Vec::new(),
                instantiated_params.clone(),
                sig.this_type,
                retry_return_type,
                sig.type_predicate,
                false,
                sig.is_method,
            );
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

        let retry_requires_readonly_argument_context =
            Self::signature_const_type_params_require_readonly_argument_context(
                self.ctx.types,
                &sig.type_params,
            );
        let refreshed_arg_types = self.with_overload_contextual_retry_inference_context(
            retry_requires_readonly_argument_context,
            |this| {
                if used_return_context_sub {
                    let tracked_type_params: rustc_hash::FxHashSet<_> =
                        sig.type_params.iter().map(|tp| tp.name).collect();
                    let mut progressive_sub =
                        retry_substitution.unwrap_or_else(TypeSubstitution::new);
                    let mut progressive_args = Vec::with_capacity(args.len());
                    for (i, &arg_idx) in args.iter().enumerate() {
                        let contextual_type = this.instantiated_contextual_param_type_at(
                            &sig.params,
                            i,
                            &progressive_sub,
                        );
                        let arg_type = this.compute_single_call_argument_type(
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
                                this.instantiate_generic_function_argument_against_target_params(
                                    arg_type, expected,
                                )
                            })
                            .unwrap_or(arg_type);
                        progressive_args.push(arg_for_refinement);
                        if let Some(shape_param) =
                            sig.params.get(i).map(|p| p.type_id).or_else(|| {
                                let last = sig.params.last()?;
                                last.rest.then_some(last.type_id)
                            })
                        {
                            let mut arg_substitution = TypeSubstitution::new();
                            let mut visited = rustc_hash::FxHashSet::default();
                            this.collect_return_context_substitution(
                                shape_param,
                                arg_for_refinement,
                                &tracked_type_params,
                                &mut arg_substitution,
                                &mut visited,
                            );
                            for (&name, &ty) in arg_substitution.map() {
                                if ty == TypeId::UNKNOWN
                                    || ty == TypeId::ERROR
                                    || this.target_contains_blocking_return_context_type_params(
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
                                            || contains_type_parameters(this.ctx.types, existing)
                                            || contains_infer_types(this.ctx.types, existing)
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
                    this.collect_call_argument_types_with_context(
                        args,
                        |i, _arg_count| refreshed_contextual_types.get(i).copied().flatten(),
                        false,
                        None,
                        sig_callable_ctx,
                    )
                }
            },
        );

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
                self.merge_return_context_substitution(
                    &mut combined_sub,
                    &sig.type_params,
                    &return_sub_for_retry,
                );
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

    fn with_overload_contextual_retry_inference_context<R>(
        &mut self,
        requires_readonly_argument_context: bool,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let previous_preserve_literals = self.ctx.preserve_literal_types;
        let previous_in_const_assertion = self.ctx.in_const_assertion;
        self.ctx.preserve_literal_types = true;
        if requires_readonly_argument_context {
            self.ctx.in_const_assertion = true;
        }

        let result = f(self);

        self.ctx.preserve_literal_types = previous_preserve_literals;
        self.ctx.in_const_assertion = previous_in_const_assertion;
        result
    }
}
