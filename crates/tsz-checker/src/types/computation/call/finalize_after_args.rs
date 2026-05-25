use crate::call_checker::CallableContext;
use crate::query_boundaries::assignability as assign_query;
use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::checkers::call::is_type_parameter_type;
use crate::query_boundaries::common;
use crate::query_boundaries::common::{CallResult, ContextualTypeContext};
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

use super::super::call_result::CallResultContext;
use super::post_generic::PostGenericCallDiagnostics;

pub(super) struct CallAfterArgumentCollectionCtx<'a> {
    pub(super) idx: NodeIndex,
    pub(super) callee_expr: NodeIndex,
    pub(super) args: &'a [NodeIndex],
    pub(super) arg_types: Vec<TypeId>,
    pub(super) callee_type: TypeId,
    pub(super) callee_type_for_resolution: TypeId,
    pub(super) base_contextual_param_types: &'a [Option<TypeId>],
    pub(super) non_generic_contextual_types: Option<&'a [Option<TypeId>]>,
    pub(super) check_excess_properties: bool,
    pub(super) callable_ctx: CallableContext,
    pub(super) is_generic_call: bool,
    pub(super) contextual_type: Option<TypeId>,
    pub(super) force_bivariant_callbacks: bool,
    pub(super) actual_this_type: Option<TypeId>,
    pub(super) is_super_call: bool,
    pub(super) is_optional_chain: bool,
    pub(super) had_return_context_substitution: bool,
    pub(super) pushed_this_type_from_shape: bool,
    pub(super) checker_round2_substitution: Option<&'a common::TypeSubstitution>,
    pub(super) checker_round2_shape: Option<&'a common::FunctionShape>,
    pub(super) direct_literal_conflict_substitution: Option<&'a common::TypeSubstitution>,
    pub(super) original_callee_shape: Option<&'a common::FunctionShape>,
    pub(super) prev_generic_excess_skip: Option<Vec<bool>>,
}

impl<'a> CheckerState<'a> {
    pub(super) fn finish_call_after_argument_collection(
        &mut self,
        ctx: CallAfterArgumentCollectionCtx<'_>,
    ) -> TypeId {
        let CallAfterArgumentCollectionCtx {
            idx,
            callee_expr,
            args,
            mut arg_types,
            callee_type,
            callee_type_for_resolution,
            base_contextual_param_types,
            non_generic_contextual_types,
            check_excess_properties,
            callable_ctx,
            is_generic_call,
            contextual_type,
            force_bivariant_callbacks,
            actual_this_type,
            is_super_call,
            is_optional_chain,
            had_return_context_substitution,
            pushed_this_type_from_shape,
            checker_round2_substitution,
            checker_round2_shape,
            direct_literal_conflict_substitution,
            original_callee_shape,
            prev_generic_excess_skip,
        } = ctx;
        // NOTE: generic_excess_skip is NOT restored here. It's kept until after all
        // excess property checks are done (including recovery paths and handle_call_result).
        // It's restored right before handle_call_result at the end of this function.
        // Keep shape_this_type on the stack through finalize_generic_call_result
        // and handle_call_result. Without this, post-inference rechecks triggered by
        // the call result handler would see an empty this_type_stack and fall back to
        // the wrong contextual type, causing false TS2339 errors.
        // We pop it at the end of this function.
        self.ensure_relation_input_ready(callee_type_for_resolution);

        // Resolve applications/lazy refs to callable forms before solver dispatch.
        let callee_type_for_call = self.evaluate_application_type(callee_type_for_resolution);
        let callee_type_for_call = self.resolve_lazy_type(callee_type_for_call);
        // For union types, resolve Lazy members so the solver can inspect their
        // callable shapes (e.g., for `this` type checks in TS2684). The solver's
        // NoopResolver can't resolve Lazy types, so we do it here.
        let callee_type_for_call = self.resolve_lazy_members_in_union(callee_type_for_call);

        // Boxed/global `Function` is callable in TS even without explicit signatures.
        let callee_type_for_call =
            self.replace_function_type_for_call(callee_type, callee_type_for_call);
        if callee_type_for_call == TypeId::ANY {
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None,
                check_excess_properties,
                None, // No skipping needed
                CallableContext::none(),
            );
            return if is_optional_chain {
                common::union_with_undefined(self.ctx.types, TypeId::ANY)
            } else {
                TypeId::ANY
            };
        }

        self.ensure_relation_input_ready(callee_type_for_call);

        // `super()` uses construct signatures, not call signatures.
        let (generic_inference_arg_types, sanitized_generic_inference) = if is_generic_call {
            self.sanitize_generic_inference_arg_types(callee_expr, args, &arg_types)
        } else {
            (std::borrow::Cow::Borrowed(arg_types.as_slice()), false)
        };
        let generic_inference_arg_source_markers = if is_generic_call {
            self.call_arg_source_type_annotation_markers(args, generic_inference_arg_types.len())
        } else {
            Vec::new()
        };
        let call_resolution_contextual_type = if is_generic_call {
            // Generic calls in contextual positions need the outer request at the
            // solver boundary, even when they have arguments. The checker-side
            // round-1/round-2 passes refine argument shapes, but higher-order
            // cases like `map(xs, identity)`, `compose(list, box)`, and
            // `consumeClass(createClass(x => ...))` still require return-context
            // seeding in the final generic solve step to instantiate parameter and
            // callback types from the contextual result.
            contextual_type
        } else {
            contextual_type
        };

        let (mut result, mut instantiated_predicate, mut generic_instantiated_params) =
            if is_super_call {
                (
                    self.resolve_new_with_checker_adapter(
                        callee_type_for_call,
                        &generic_inference_arg_types,
                        force_bivariant_callbacks,
                        call_resolution_contextual_type,
                    ),
                    None,
                    None,
                )
            } else if generic_inference_arg_source_markers.iter().any(|&m| m) {
                self.resolve_call_with_checker_adapter_and_arg_sources(
                    callee_type_for_call,
                    &generic_inference_arg_types,
                    force_bivariant_callbacks,
                    call_resolution_contextual_type,
                    actual_this_type,
                    &generic_inference_arg_source_markers,
                )
            } else {
                self.resolve_call_with_checker_adapter(
                    callee_type_for_call,
                    &generic_inference_arg_types,
                    force_bivariant_callbacks,
                    call_resolution_contextual_type,
                    actual_this_type,
                )
            };
        // When the checker's intra-expression Round 2 produced a substitution that
        // pins type parameters the solver could not (the solver's single-pass
        // inference dropped the binding because the same parameter appears in a
        // homomorphic-mapped + `infer` return position that fails reverse
        // inference), refine `instantiated_params` so the post-call assignability
        // recheck sees the tighter expected types. We only override when the
        // solver effectively defaulted to the type parameter's constraint.
        if is_generic_call
            && !is_super_call
            && let Some(checker_sub) = checker_round2_substitution
            && let Some(orig_shape) = checker_round2_shape
            && let Some(params) = generic_instantiated_params.as_mut()
        {
            self.refine_instantiated_params_with_checker_substitution(
                orig_shape,
                params,
                checker_sub,
            );
        }
        if is_generic_call
            && !is_super_call
            && let Some(conflicts) = direct_literal_conflict_substitution
            && let Some(orig_shape) = checker_round2_shape
            && let Some(params) = generic_instantiated_params.as_mut()
        {
            self.refine_bare_instantiated_params_with_direct_literal_conflicts(
                orig_shape, params, conflicts,
            );
        }
        let needs_real_type_recheck = is_generic_call
            && (!is_super_call
                || args.iter().enumerate().any(|(i, &arg_idx)| {
                    self.argument_needs_refresh_for_contextual_call(
                        arg_idx,
                        base_contextual_param_types.get(i).copied().flatten(),
                    )
                }));

        if !is_generic_call
            && let CallResult::ArgumentTypeMismatch {
                index,
                fallback_return,
                ..
            } = result.clone()
            && let Some(expected) = non_generic_contextual_types
                .as_ref()
                .and_then(|types| types.get(index).copied().flatten())
                .map(|expected| self.evaluate_contextual_type(expected))
            && let Some(&arg_idx) = args.get(index)
            && let Some(actual) = Some(self.refreshed_generic_call_arg_type_with_context(
                arg_idx,
                arg_types.get(index).copied().unwrap_or(TypeId::UNKNOWN),
                Some(expected),
            ))
        {
            let fresh_subtype = assign_query::is_fresh_subtype_of(self.ctx.types, actual, expected);
            let recover_object_literal =
                fresh_subtype
                    && !self.object_literal_has_computed_property_names(arg_idx)
                    && self.ctx.arena.get(arg_idx).is_some_and(|node| {
                        node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    });
            if recover_object_literal {
                // Skip excess property checking when the original parameter type was a
                // type parameter (captured via generic_excess_skip during arg collection).
                let skip_epc_for_generic = self
                    .ctx
                    .generic_excess_skip
                    .as_ref()
                    .is_some_and(|skip| index < skip.len() && skip[index]);
                if expected != TypeId::ANY
                    && expected != TypeId::UNKNOWN
                    && !is_type_parameter_type(self.ctx.types, expected)
                    && !skip_epc_for_generic
                    && !self.contextual_type_is_unresolved_for_argument_refresh(expected)
                {
                    self.check_object_literal_excess_properties(actual, expected, arg_idx);
                }
                let recovered_return = if fallback_return != TypeId::ERROR {
                    Some(fallback_return)
                } else {
                    assign_query::get_function_return_type(self.ctx.types, callee_type_for_call)
                };
                if let Some(return_type) = recovered_return {
                    result = CallResult::Success(return_type);
                }
            }
        }

        let retry_contextual_param_types = if is_generic_call && had_return_context_substitution {
            generic_instantiated_params.as_ref().map(|params| {
                self.contextual_param_types_from_instantiated_params(params, args.len())
            })
        } else {
            None
        };
        let has_contextual_signature_instantiation_arg =
            args.iter().enumerate().any(|(i, &arg_idx)| {
                let expected_type = retry_contextual_param_types
                    .as_ref()
                    .and_then(|types| types.get(i).copied().flatten())
                    .or_else(|| base_contextual_param_types.get(i).copied().flatten());
                self.expression_needs_contextual_signature_instantiation(arg_idx, expected_type)
            });
        let has_contextual_refresh_arg = args.iter().enumerate().any(|(i, &arg_idx)| {
            self.argument_needs_refresh_for_contextual_call(
                arg_idx,
                retry_contextual_param_types
                    .as_ref()
                    .and_then(|types| types.get(i).copied().flatten())
                    .or_else(|| base_contextual_param_types.get(i).copied().flatten()),
            )
        });
        let should_retry_generic_call = if is_generic_call
            && (!had_return_context_substitution || has_contextual_signature_instantiation_arg)
            && has_contextual_refresh_arg
        {
            if let Some(ctx_type) = contextual_type {
                match &result {
                    CallResult::Success(ret) => {
                        let contextual_return = self.evaluate_contextual_type(ctx_type);
                        !self.is_assignable_to_with_env(*ret, contextual_return)
                    }
                    _ => true,
                }
            } else {
                true
            }
        } else {
            false
        };

        let mut retried_arg_types = None;
        if is_generic_call
            && should_retry_generic_call
            && let Some(instantiated_params) = generic_instantiated_params.as_ref()
        {
            self.clear_contextual_resolution_cache();
            for (i, &arg_idx) in args.iter().enumerate() {
                if self.argument_needs_refresh_for_contextual_call(
                    arg_idx,
                    base_contextual_param_types.get(i).copied().flatten(),
                ) {
                    self.invalidate_expression_for_contextual_retry(arg_idx);
                }
            }
            let instantiated_params = call_checker::get_contextual_signature_for_arity(
                self.ctx.types,
                callee_type_for_call,
                args.len(),
            )
            .map(|shape| {
                self.resolve_signature_parameter_type_queries(&shape.params, instantiated_params)
            })
            .unwrap_or_else(|| instantiated_params.clone());
            let refreshed_contextual_types = self
                .contextual_param_types_from_instantiated_params(&instantiated_params, args.len())
                .into_iter()
                .map(|param_type| {
                    param_type
                        .map(|param_type| self.normalize_contextual_call_param_type(param_type))
                })
                .collect::<Vec<_>>();
            let retry_arg_diag_snap = self.ctx.snapshot_diagnostics();
            let refreshed_arg_types = self.collect_call_argument_types_with_context(
                args,
                |i, _arg_count| {
                    refreshed_contextual_types
                        .get(i)
                        .copied()
                        .flatten()
                        // A `never` contextual type is uninformative: it only
                        // arises when the instantiated parameter reduced to `never`
                        // (a forbidden argument). Using it to re-type the argument
                        // would spuriously widen a literal (`'a'` -> `string`) and
                        // mask the TS2345 the first resolve already found. Fall back
                        // to the base contextual type instead.
                        .filter(|&t| t != TypeId::NEVER)
                        .or_else(|| base_contextual_param_types.get(i).copied().flatten())
                },
                check_excess_properties,
                None,
                callable_ctx,
            );
            let retry_has_callback_body_errors =
                self.overload_candidate_has_callback_body_errors(args, &retry_arg_diag_snap);
            let retry_has_callback_like_arg = args
                .iter()
                .copied()
                .any(|arg_idx| self.is_callback_like_argument(arg_idx));

            let (retry_generic_arg_types, retry_sanitized) =
                self.sanitize_generic_inference_arg_types(callee_expr, args, &refreshed_arg_types);
            let retry_arg_source_markers =
                self.call_arg_source_type_annotation_markers(args, retry_generic_arg_types.len());
            let mut retry = if is_super_call {
                (
                    self.resolve_new_with_checker_adapter(
                        callee_type_for_call,
                        &retry_generic_arg_types,
                        force_bivariant_callbacks,
                        contextual_type,
                    ),
                    None,
                    None,
                )
            } else if retry_arg_source_markers.iter().any(|&m| m) {
                self.resolve_call_with_checker_adapter_and_arg_sources(
                    callee_type_for_call,
                    &retry_generic_arg_types,
                    force_bivariant_callbacks,
                    contextual_type,
                    actual_this_type,
                    &retry_arg_source_markers,
                )
            } else {
                self.resolve_call_with_checker_adapter(
                    callee_type_for_call,
                    &retry_generic_arg_types,
                    force_bivariant_callbacks,
                    contextual_type,
                    actual_this_type,
                )
            };
            // Apply the same checker-side substitution refinement to the retry's
            // freshly-inferred params, so the recheck below sees the tighter
            // expected types for the post-call assignability check.
            if let Some(checker_sub) = checker_round2_substitution
                && let Some(orig_shape) = checker_round2_shape
                && let Some(retry_params) = retry.2.as_mut()
            {
                self.refine_instantiated_params_with_checker_substitution(
                    orig_shape,
                    retry_params,
                    checker_sub,
                );
            }
            if let Some(conflicts) = direct_literal_conflict_substitution
                && let Some(orig_shape) = checker_round2_shape
                && let Some(retry_params) = retry.2.as_mut()
            {
                self.refine_bare_instantiated_params_with_direct_literal_conflicts(
                    orig_shape,
                    retry_params,
                    conflicts,
                );
            }
            result = if (retry_sanitized || needs_real_type_recheck)
                && !retry_has_callback_body_errors
                && !retry_has_callback_like_arg
            {
                if let Some(instantiated_params) = retry.2.as_ref() {
                    self.recheck_generic_call_arguments_with_real_types(
                        retry.0.clone(),
                        instantiated_params,
                        args,
                        &refreshed_arg_types,
                    )
                } else {
                    retry.0
                }
            } else {
                retry.0
            };
            instantiated_predicate = retry.1;
            generic_instantiated_params = retry.2;
            retried_arg_types = Some(refreshed_arg_types);
        }

        if is_generic_call
            && let CallResult::Success(return_type) = result
            && let Some(ctx_type) =
                contextual_type.filter(|&ct| ct != TypeId::ANY && ct != TypeId::UNKNOWN)
            && let Some(shape) = call_checker::get_contextual_signature_for_arity(
                self.ctx.types,
                callee_type_for_call,
                args.len(),
            )
        {
            let mut return_context_substitution =
                self.compute_return_context_substitution_from_shape(&shape, Some(ctx_type));
            let return_param_names: FxHashSet<_> = self
                .function_like_return_parameter_type_params(&shape)
                .into_iter()
                .collect();
            let same_return_context_application =
                common::application_info(self.ctx.types, shape.return_type)
                    .zip(common::application_info(self.ctx.types, ctx_type))
                    .is_some_and(|((return_base, _), (ctx_base, _))| return_base == ctx_base);
            let return_context_specializes_return_params = !return_param_names.is_empty()
                && self.contextual_return_type_specializes_wrapped_params(
                    shape.return_type,
                    ctx_type,
                    &return_param_names,
                    &mut FxHashSet::default(),
                );
            if !return_param_names.is_empty()
                && !same_return_context_application
                && !return_context_specializes_return_params
            {
                let mut filtered = crate::query_boundaries::common::TypeSubstitution::new();
                for (&name, &type_id) in return_context_substitution.map() {
                    if !return_param_names.contains(&name) {
                        filtered.insert(name, type_id);
                    }
                }
                return_context_substitution = filtered;
            }

            if !return_context_substitution.is_empty() {
                let has_callback_like_arg = args
                    .iter()
                    .copied()
                    .any(|arg| self.is_callback_like_argument(arg));
                let contextual_return_is_concrete =
                    !common::contains_type_parameters(self.ctx.types, ctx_type)
                        && !common::contains_infer_types(self.ctx.types, ctx_type)
                        && !common::contains_type_by_id(self.ctx.types, ctx_type, TypeId::UNKNOWN);
                if !has_callback_like_arg && contextual_return_is_concrete {
                    let instantiated_shape_return =
                        crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            shape.return_type,
                            &return_context_substitution,
                        );
                    let contextual_params_fit_args = args.iter().enumerate().all(|(i, _)| {
                        let Some(param) = shape.params.get(i).or_else(|| {
                            let last = shape.params.last()?;
                            last.rest.then_some(last)
                        }) else {
                            return true;
                        };
                        let instantiated_param = crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            param.type_id,
                            &return_context_substitution,
                        );
                        let expected = if param.rest {
                            self.rest_argument_element_type_with_env(instantiated_param)
                        } else {
                            instantiated_param
                        };
                        let actual = generic_inference_arg_types
                            .get(i)
                            .copied()
                            .or_else(|| {
                                retried_arg_types
                                    .as_ref()
                                    .and_then(|types| types.get(i).copied())
                            })
                            .or_else(|| arg_types.get(i).copied())
                            .unwrap_or(TypeId::UNKNOWN);
                        if matches!(actual, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) {
                            return false;
                        }
                        self.is_assignable_to_with_env(actual, expected)
                    });
                    if contextual_params_fit_args
                        && self.is_assignable_to_with_env(instantiated_shape_return, ctx_type)
                    {
                        result = CallResult::Success(instantiated_shape_return);
                    }
                }
                if let CallResult::Success(current_return) = result
                    && current_return == return_type
                    && (common::contains_type_parameters(self.ctx.types, return_type)
                        || common::contains_infer_types(self.ctx.types, return_type)
                        || common::contains_type_by_id(
                            self.ctx.types,
                            return_type,
                            TypeId::UNKNOWN,
                        ))
                {
                    let instantiated_return = crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        return_type,
                        &return_context_substitution,
                    );
                    if instantiated_return != return_type {
                        result = CallResult::Success(instantiated_return);
                    }
                }
                if let CallResult::Success(current_return) = result
                    && current_return != shape.return_type
                    && common::contains_type_by_id(self.ctx.types, current_return, TypeId::UNKNOWN)
                {
                    let instantiated_return = crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        shape.return_type,
                        &return_context_substitution,
                    );
                    if instantiated_return != shape.return_type
                        && self.is_assignable_to_with_env(instantiated_return, ctx_type)
                    {
                        result = CallResult::Success(instantiated_return);
                    }
                }
            }
        }
        drop(generic_inference_arg_types);
        if let Some(refreshed_arg_types) = retried_arg_types {
            arg_types = refreshed_arg_types;
        }

        // Store instantiated type predicate from generic call resolution
        // so flow narrowing can use the correct (inferred) predicate type.
        let stored_call_predicate = if let Some(predicate) = instantiated_predicate {
            let stored_predicate =
                call_checker::extract_predicate_signature(self.ctx.types, callee_type_for_call)
                    .filter(|sig| {
                        // Only defer to `resolve_generic_predicate` when the type parameter
                        // actually appears in a parameter type; otherwise use the instantiated
                        // predicate directly (T appears only in the predicate, not in params).
                        sig.predicate.type_id.is_some_and(|pred_ty| {
                            common::type_param_info(self.ctx.types, pred_ty).is_some_and(
                                |tp_info| {
                                    sig.params.iter().any(|p| {
                                        common::contains_type_parameter_named(
                                            self.ctx.types,
                                            p.type_id,
                                            tp_info.name,
                                        )
                                    })
                                },
                            )
                        })
                    })
                    .map(|sig| (sig.predicate, sig.params))
                    .unwrap_or(predicate);
            Some(stored_predicate)
        } else {
            // For non-generic calls with type predicates (e.g., `isString(x): x is string`),
            // extract the predicate from the callee's signature and store it in
            // call_type_predicates. This ensures flow narrowing can find the predicate
            // even when node_types is temporarily emptied during overload resolution
            // of a containing call expression (e.g., `console.log(thing.toUpperCase())`
            // triggers overload resolution which empties node_types before checking args).
            let is_sound_union = if common::is_union_type(self.ctx.types, callee_type_for_call) {
                call_checker::is_valid_union_predicate(self.ctx.types, callee_type_for_call)
            } else {
                true
            };
            if is_sound_union
                && let Some(extracted) =
                    call_checker::extract_predicate_signature(self.ctx.types, callee_type_for_call)
            {
                Some((extracted.predicate, extracted.params))
            } else {
                None
            }
        };

        if let Some(stored_predicate) = stored_call_predicate {
            self.store_call_type_predicate(idx, callee_expr, stored_predicate);
        }

        let (mut result, mut allow_contextual_mismatch_deferral) = self
            .finalize_generic_call_result(super::super::call_finalize::GenericCallFinalizeCtx {
                callee_type_for_call,
                generic_instantiated_params: generic_instantiated_params.as_ref(),
                args,
                arg_types: &arg_types,
                result,
                sanitized_generic_inference,
                needs_real_type_recheck,
            });
        let finalized_contextual_param_types = generic_instantiated_params
            .as_ref()
            .map(|params| self.contextual_param_types_from_instantiated_params(params, args.len()));
        self.run_post_generic_call_diagnostics(PostGenericCallDiagnostics {
            result: &mut result,
            allow_contextual_mismatch_deferral: &mut allow_contextual_mismatch_deferral,
            callee_type_for_call,
            args,
            arg_types: &arg_types,
            base_contextual_param_types,
            finalized_contextual_param_types: finalized_contextual_param_types.as_deref(),
            original_callee_shape,
            emit_unknown_callback_body_diagnostics: is_generic_call && contextual_type.is_none(),
            check_excess_properties,
            callable_ctx,
        });
        let forced_block_body_callback_mismatch = self
            .current_block_body_callback_return_mismatch_arg(args, |checker, index| {
                finalized_contextual_param_types
                    .as_ref()
                    .and_then(|types| types.get(index).copied().flatten())
                    .or_else(|| {
                        ContextualTypeContext::with_expected_and_options(
                            checker.ctx.types,
                            callee_type_for_call,
                            checker.ctx.compiler_options.no_implicit_any,
                        )
                        .get_parameter_type_for_call(index, args.len())
                    })
            })
            .inspect(|&(index, actual, expected)| {
                if let CallResult::Success(return_type) = result {
                    allow_contextual_mismatch_deferral = false;
                    result = CallResult::ArgumentTypeMismatch {
                        index,
                        expected,
                        actual,
                        fallback_return: return_type,
                    };
                }
            })
            .is_some();
        if forced_block_body_callback_mismatch {
            allow_contextual_mismatch_deferral = false;
        }
        let forced_binding_pattern_unknown_context_mismatch = self
            .current_binding_pattern_callback_unknown_context_arg(args, |checker, index| {
                finalized_contextual_param_types
                    .as_ref()
                    .and_then(|types| types.get(index).copied().flatten())
                    .or_else(|| {
                        ContextualTypeContext::with_expected_and_options(
                            checker.ctx.types,
                            callee_type_for_call,
                            checker.ctx.compiler_options.no_implicit_any,
                        )
                        .get_parameter_type_for_call(index, args.len())
                    })
            })
            .inspect(|&(index, actual, expected)| {
                if matches!(result, CallResult::Success(_))
                    && let Some(&arg_idx) = args.get(index)
                {
                    allow_contextual_mismatch_deferral = false;
                    self.error_argument_not_assignable_at(actual, expected, arg_idx);
                }
            })
            .is_some();
        if let CallResult::ArgumentTypeMismatch {
            actual: _,
            expected: _,
            fallback_return,
            ..
        } = result
            && !forced_block_body_callback_mismatch
            && !forced_binding_pattern_unknown_context_mismatch
            && fallback_return != TypeId::ERROR
        {
            // Keep the ArgumentTypeMismatch result to ensure TS2345 is emitted
            // Deferral logic removed to fix missing TS2345 errors
        }

        if let CallResult::ArgumentTypeMismatch {
            fallback_return, ..
        } = result
            && self.call_is_simple_evolving_array_mutation(callee_expr)
        {
            result = CallResult::Success(fallback_return);
        }

        if let CallResult::ArgumentTypeMismatch {
            index,
            fallback_return,
            ..
        } = result
            && fallback_return != TypeId::ERROR
            && let Some(&arg_idx) = args.get(index)
            && self.is_callback_like_argument(arg_idx)
            && self
                .callback_body_spans(arg_idx)
                .iter()
                .any(|(start, end)| {
                    self.ctx.diagnostics.iter().any(|diag| {
                        matches!(diag.code, 2322 | 2339 | 2345 | 2347 | 2769)
                            && diag.start >= *start
                            && diag.start < *end
                    })
                })
        {
            result = CallResult::Success(fallback_return);
        }

        if self.ctx.in_const_assertion
            && is_generic_call
            && args.len() == 1
            && let (CallResult::Success(return_type), Some(&arg_type)) =
                (result.clone(), arg_types.first())
            && return_type == common::widen_literal_type(self.ctx.types, arg_type)
            && return_type != arg_type
        {
            result = CallResult::Success(arg_type);
        }

        if let CallResult::Success(return_type) = result {
            for (index, &actual) in arg_types.iter().enumerate() {
                let expected = finalized_contextual_param_types
                    .as_ref()
                    .and_then(|types| types.get(index).copied().flatten())
                    .or_else(|| {
                        ContextualTypeContext::with_expected_and_options(
                            self.ctx.types,
                            callee_type_for_call,
                            self.ctx.compiler_options.no_implicit_any,
                        )
                        .get_parameter_type_for_call(index, args.len())
                    });
                if let Some(expected) = expected
                    && !(expected == TypeId::NEVER
                        && common::index_access_parts(self.ctx.types, actual).is_some_and(
                            |(_, index)| common::contains_type_parameters(self.ctx.types, index),
                        ))
                    && self
                        .checker_only_assignability_failure_reason(actual, expected)
                        .is_some()
                {
                    result = CallResult::ArgumentTypeMismatch {
                        index,
                        expected,
                        actual,
                        fallback_return: return_type,
                    };
                    allow_contextual_mismatch_deferral = false;
                    break;
                }
            }
        }

        let call_context = CallResultContext {
            callee_expr,
            call_idx: idx,
            args,
            arg_types: &arg_types,
            callee_type: callee_type_for_call,
            callee_has_declared_generic_signature: common::function_shape_for_type(
                self.ctx.types,
                callee_type_for_resolution,
            )
            .is_some_and(|shape| !shape.type_params.is_empty())
                || common::callable_shape_for_type(self.ctx.types, callee_type_for_resolution)
                    .is_some_and(|shape| {
                        shape
                            .call_signatures
                            .iter()
                            .any(|sig| !sig.type_params.is_empty())
                    }),
            is_super_call,
            is_optional_chain,
            allow_contextual_mismatch_deferral,
        };
        // Pop the shape_this_type that was kept on the stack since the
        // argument collection phase.
        if pushed_this_type_from_shape {
            self.ctx.this_type_stack.pop();
        }
        // Keep generic_excess_skip set through handle_call_result so that error
        // elaboration respects the skip flag for generic calls with type parameter
        // targets. Restore it after handle_call_result completes.
        let call_result = self.handle_call_result(result, call_context);
        self.ctx.generic_excess_skip = prev_generic_excess_skip;
        call_result
    }
}
