//! Overload resolution for call expressions.
//!
//! Split from the parent `call_checker` module — pure code motion.

use crate::query_boundaries::checkers::call::lazy_def_id_for_type;
use crate::query_boundaries::common::{ContextualTypeContext, PendingDiagnosticBuilder};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

use super::{CallableContext, OverloadResolution};

impl<'a> CheckerState<'a> {
    /// Resolve an overloaded call by trying each signature.
    ///
    /// This method iterates through overload signatures and returns the first
    /// one that successfully matches the provided arguments.
    ///
    /// # Parameters
    /// - `args`: The argument node indices
    /// - `signatures`: The overload signatures to try
    ///
    /// # Returns
    /// - `Some(OverloadResolution)` if overload resolution was attempted
    /// - `None` if there were no overload signatures to resolve
    pub(crate) fn resolve_overloaded_call_with_signatures(
        &mut self,
        args: &[NodeIndex],
        signatures: &[tsz_solver::CallSignature],
        force_bivariant_callbacks: bool,
        contextual_type: Option<TypeId>,
        actual_this_type: Option<TypeId>,
    ) -> Option<OverloadResolution> {
        use crate::query_boundaries::common::{CallResult, FunctionShape};

        tracing::debug!(
            "resolve_overloaded_call_with_signatures: signatures = {:?}, args = {:?}",
            signatures,
            args
        );
        if signatures.is_empty() {
            return None;
        }

        // Overload contextual typing baseline.
        // First pass collects argument types once using a union of overload signatures.
        // If that fails to find a match, we run a second pass that re-collects arguments
        // per candidate signature with signature-specific contextual types. This helps
        // avoid false TS2345/TS2322 when the union contextual type is too lossy.
        let factory = self.ctx.types.factory();

        // Create a union of all overload signatures for contextual typing
        let signature_types: Vec<TypeId> = signatures
            .iter()
            .map(|sig| {
                let func_shape = FunctionShape {
                    params: sig.params.clone(),
                    this_type: sig.this_type,
                    return_type: sig.return_type,
                    type_params: sig.type_params.clone(),
                    type_predicate: sig.type_predicate,
                    is_constructor: false,
                    is_method: sig.is_method,
                };
                factory.function(func_shape)
            })
            .collect();

        // Union of all signatures provides contextual typing
        let union_contextual =
            tsz_solver::utils::union_or_single(self.ctx.types, signature_types.clone());

        let ctx_helper = ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            union_contextual,
            self.ctx.compiler_options.no_implicit_any,
        );
        // Include parenthesized expressions in contextual refresh args
        // so that `(callback)` gets the correct contextual type per-overload.
        let contextual_refresh_args: Vec<_> = args
            .iter()
            .copied()
            .filter(|&arg_idx| {
                if self.argument_needs_contextual_type(arg_idx) {
                    return true;
                }
                // Also include parenthesized expressions that might contain callbacks
                let mut current = arg_idx;
                for _ in 0..10 {
                    let Some(node) = self.ctx.arena.get(current) else {
                        return false;
                    };
                    if node.kind == syntax_kind_ext::ARROW_FUNCTION
                        || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    {
                        return true;
                    }
                    if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                        && let Some(paren) = self.ctx.arena.get_parenthesized(node)
                    {
                        current = paren.expression;
                        continue;
                    }
                    return false;
                }
                false
            })
            .collect();
        let refresh_all_args = |this: &mut Self| {
            for &arg_idx in args {
                this.invalidate_expression_for_contextual_retry(arg_idx);
                this.ctx.daa_error_nodes.remove(&arg_idx.0);
                this.ctx.flow_narrowed_nodes.remove(&arg_idx.0);
            }
        };

        let mut original_node_types = std::mem::take(&mut self.ctx.node_types);

        // Snapshot all speculative state before overload resolution begins.
        // This captures diagnostics, emitted_diagnostics dedup set, TS2454
        // dedup state, TS2307 module dedup, and implicit-any-checked-closures.
        // On failure paths we roll back to this snapshot; on success paths
        // we selectively keep diagnostics via the transaction API.
        let overload_snap = self.ctx.snapshot_full();

        // Collect argument types ONCE with union contextual type.
        // Diagnostics produced during this pass are speculative: if no overload
        // matches, TypeScript reports the overload failure and suppresses these
        // nested callback/body diagnostics.
        self.ctx.node_types = Default::default();
        // Clear the contextual resolution cache once before the loop — the cache
        // is shared and needs clearing before any arg is re-evaluated, but clearing
        // it per-arg was redundant (empty after the first iteration).
        self.clear_contextual_resolution_cache();
        for &arg_idx in &contextual_refresh_args {
            self.invalidate_expression_for_contextual_retry(arg_idx);
        }
        let union_callable_ctx = CallableContext::new(union_contextual);
        // Preserve literal types during overload argument collection so that
        // string/number literal arguments keep their literal types (e.g., "canvas"
        // stays as literal "canvas" instead of widening to string).  This is
        // critical for overload resolution: without it, the union contextual type
        // (which collapses literal | string → string) causes all literal overloads
        // to fail matching.
        let prev_preserve_literals = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let arg_types = self.collect_call_argument_types_with_context(
            args,
            |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
            false,
            None, // No skipping needed for overload resolution
            union_callable_ctx,
        );
        self.ctx.preserve_literal_types = prev_preserve_literals;
        let temp_node_types = std::mem::take(&mut self.ctx.node_types);

        self.ctx.node_types = std::mem::take(&mut original_node_types);

        // Snapshot diagnostics AFTER union-contextual argument collection.
        // The union-contextual pass can produce speculative callback body errors
        // (e.g., TS2322 from checking `return [a]` against the union contextual
        // return type `number | U`). These errors are from the union context, NOT
        // from individual overload attempts. Using `overload_snap` (taken before
        // arg collection) would cause `overload_candidate_has_callback_body_errors`
        // to see these stale union-context diagnostics and incorrectly reject
        // overloads that the solver successfully resolves (e.g., generic overloads
        // like `reduce<U>`). This post-collection snapshot ensures only diagnostics
        // from the current overload attempt are checked.
        let post_union_arg_diag_snap = self.ctx.snapshot_diagnostics();

        // First pass: try each signature with union-contextual argument types.
        // When an overload succeeds but its return context substitution is empty
        // (couldn't infer type params from contextual return type), defer it as
        // a fallback and continue trying later overloads which might have better
        // return context inference.
        let mut no_rcs_fallback: Option<(
            Vec<TypeId>,
            TypeId,
            crate::context::speculation::FullSnapshot,
        )> = None;
        for (idx, (sig, &func_type)) in signatures.iter().zip(signature_types.iter()).enumerate() {
            tracing::debug!("Trying overload {} with {} args", idx, arg_types.len());
            self.ensure_relation_input_ready(func_type);
            let resolved_func_type =
                if let Some(def_id) = lazy_def_id_for_type(self.ctx.types, func_type) {
                    self.ctx
                        .type_env
                        .borrow()
                        .get_def(def_id)
                        .unwrap_or(func_type)
                } else {
                    func_type
                };
            let (result, _instantiated_predicate, instantiated_params) = self
                .resolve_call_with_checker_adapter(
                    resolved_func_type,
                    &arg_types,
                    force_bivariant_callbacks,
                    contextual_type,
                    None,
                );

            match &result {
                CallResult::ArgumentTypeMismatch {
                    index,
                    expected,
                    actual,
                    ..
                } => {
                    tracing::debug!("Overload {} failed: arg {} type mismatch", idx, index);
                    tracing::debug!("  Expected TypeId: {:?}", expected);
                    tracing::debug!("  Actual TypeId: {:?}", actual);
                }
                _ => {
                    tracing::debug!("Overload {} result: {:?}", idx, result);
                }
            }
            match result {
                CallResult::Success(return_type) => {
                    if self
                        .current_block_body_callback_return_mismatch_arg(args, |checker, index| {
                            ContextualTypeContext::with_expected_and_options(
                                checker.ctx.types,
                                func_type,
                                checker.ctx.compiler_options.no_implicit_any,
                            )
                            .get_parameter_type_for_call(index, args.len())
                        })
                        .is_some()
                    {
                        self.prune_callback_body_diagnostics(args, &overload_snap.diag);
                        continue;
                    }
                    // The union-context pass can successfully match an overload while still
                    // leaving inline callback bodies typed under a lossy contextual union.
                    // Defer those candidates to the signature-specific pass, which can
                    // re-evaluate callbacks with per-overload parameter types.
                    if signatures.len() > 1
                        && self.overload_candidate_has_callback_body_errors(
                            args,
                            &post_union_arg_diag_snap,
                        )
                    {
                        self.prune_callback_body_diagnostics(args, &overload_snap.diag);
                        continue;
                    }
                    // For generic overloads that will undergo an instantiated retry,
                    // defer the hard-error check. The first-pass argument collection
                    // uses unresolved type parameters, which can cause false errors
                    // (e.g., TS2339 for `this.bar` when ThisType<Data & ...> has
                    // uninstantiated Data). The retry re-evaluates with concrete
                    // types, eliminating these false positives.
                    let will_do_instantiated_retry = !sig.type_params.is_empty()
                        && !contextual_refresh_args.is_empty()
                        && instantiated_params.is_some();
                    if !will_do_instantiated_retry
                        && self.overload_candidate_has_hard_non_callback_arg_errors(
                            args,
                            &overload_snap.diag,
                        )
                    {
                        self.prune_callback_body_diagnostics(args, &overload_snap.diag);
                        continue;
                    }
                    // When the matched overload is generic and has contextual refresh args,
                    // re-collect argument types with instantiated parameter types. The first
                    // pass used the union-contextual type which has unresolved type parameters,
                    // causing false diagnostics in callback bodies (e.g., TS2339 for `this.b`
                    // when `this` has type `TContext` instead of the inferred `{b: string}`).
                    let mut did_instantiated_retry = false;
                    let mut used_return_context_sub_outer = false;
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
                        self.compute_return_context_substitution_from_shape(
                            &sig_shape,
                            contextual_type,
                        )
                    } else {
                        crate::query_boundaries::common::TypeSubstitution::new()
                    };
                    let retry_params = if !return_sub_for_retry.is_empty() {
                        Some(
                            sig.params
                                .iter()
                                .map(|param| {
                                    let mut instantiated_param = *param;
                                    instantiated_param.type_id =
                                        crate::query_boundaries::common::instantiate_type(
                                            self.ctx.types,
                                            param.type_id,
                                            &return_sub_for_retry,
                                        );
                                    instantiated_param
                                })
                                .collect::<Vec<_>>(),
                        )
                    } else {
                        instantiated_params.clone()
                    };
                    let (final_arg_types, final_return_type) = if !sig.type_params.is_empty()
                        && !contextual_refresh_args.is_empty()
                        && let Some(instantiated_params) = retry_params.as_ref()
                    {
                        let contextual_closures: Vec<_> = self
                            .ctx
                            .implicit_any_contextual_closures
                            .iter()
                            .copied()
                            .collect();
                        self.ctx.rollback_full(&overload_snap);
                        self.ctx
                            .implicit_any_checked_closures
                            .extend(contextual_closures);
                        self.ctx.node_types = Default::default();
                        refresh_all_args(self);

                        // Build an instantiated callable context so that the
                        // ThisType<T> fallback extraction in
                        // collect_call_argument_types_with_context uses the
                        // instantiated parameter types (with concrete type args)
                        // rather than the original uninstantiated ones. Without
                        // this, ThisType<Data & Readonly<Props> & Instance> keeps
                        // unresolved Data/Props, causing false TS2339 on `this`
                        // property accesses in methods (e.g., Vue Options API).
                        let sig_callable_ctx = {
                            let instantiated_func =
                                self.ctx.types.factory().function(FunctionShape {
                                    params: instantiated_params.clone(),
                                    return_type,
                                    this_type: sig.this_type,
                                    type_params: vec![],
                                    type_predicate: sig.type_predicate,
                                    is_constructor: false,
                                    is_method: sig.is_method,
                                });
                            CallableContext::new(instantiated_func)
                        };
                        // When contextual_type is available, also compute return-context
                        // substitution from the overload's return type. This handles
                        // cases like Object.freeze<T>(o: T): Readonly<T> where the
                        // contextual return type (e.g. readonly [string, number][])
                        // provides better type parameter inference than the arguments
                        // (which were typed without useful contextual types in the
                        // union-contextual first pass).
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
                            self.contextual_param_types_from_instantiated_params(
                                instantiated_params,
                                args.len(),
                            )
                        };
                        let refreshed_arg_types = self.collect_call_argument_types_with_context(
                            args,
                            |i, _arg_count| refreshed_contextual_types.get(i).copied().flatten(),
                            false,
                            None,
                            sig_callable_ctx,
                        );
                        // When return-context substitution was used to provide better
                        // contextual types, re-resolve the call with the correctly-typed
                        // arguments to get the right return type. Without this, the return
                        // type would still reflect T inferred from the badly-typed first
                        // pass (e.g., Readonly<(string|number)[][]> instead of
                        // Readonly<[string,number][]>).
                        let final_return_type = if used_return_context_sub {
                            let (re_result, _, _) = self.resolve_call_with_checker_adapter(
                                resolved_func_type,
                                &refreshed_arg_types,
                                force_bivariant_callbacks,
                                contextual_type,
                                actual_this_type,
                            );
                            match re_result {
                                CallResult::Success(rt) => rt,
                                _ => return_type,
                            }
                        } else {
                            return_type
                        };
                        did_instantiated_retry = true;
                        used_return_context_sub_outer = used_return_context_sub;
                        (refreshed_arg_types, final_return_type)
                    } else {
                        (arg_types.clone(), return_type)
                    };

                    // After the instantiated retry, the callback body has been fully
                    // evaluated with concrete contextual types. If it produced errors
                    // (e.g., a failing `.concat()` inside the callback), reject this
                    // overload. The degenerate type parameter inference (e.g.,
                    // `U = never[]` from `reduce`) masks real type errors that should
                    // cause a fallback to a different overload or NoOverloadMatch.
                    if signatures.len() > 1
                        && did_instantiated_retry
                        && self
                            .overload_candidate_has_callback_body_errors(args, &overload_snap.diag)
                    {
                        self.prune_callback_body_diagnostics(args, &overload_snap.diag);
                        continue;
                    }

                    // After the instantiated retry, re-check hard non-callback
                    // argument errors. The pre-retry check was skipped for generic
                    // overloads because first-pass errors (e.g., false TS2339 from
                    // unresolved ThisType markers) might be resolved by the retry.
                    // If hard errors persist after the retry, reject the overload.
                    if did_instantiated_retry
                        && self.overload_candidate_has_hard_non_callback_arg_errors(
                            args,
                            &overload_snap.diag,
                        )
                    {
                        self.prune_callback_body_diagnostics(args, &overload_snap.diag);
                        continue;
                    }

                    // When return context substitution failed and there are more
                    // overloads to try, defer this overload as a fallback. A later
                    // overload might use return context inference successfully
                    // (e.g., Promise.all's iterable overload can infer T from
                    // Awaited<T>[] matching a contextual tuple type).
                    if !used_return_context_sub_outer
                        && did_instantiated_retry
                        && idx + 1 < signatures.len()
                        && contextual_type.is_some()
                        && no_rcs_fallback.is_none()
                    {
                        no_rcs_fallback = Some((
                            final_arg_types.clone(),
                            final_return_type,
                            self.ctx.snapshot_full(),
                        ));
                        continue;
                    }

                    // Merge the node types inferred during argument collection.
                    // If we did the instantiated retry, node_types already contains the
                    // refreshed entries; otherwise merge the first-pass temp_node_types.
                    if !did_instantiated_retry {
                        self.ctx.node_types.merge(&temp_node_types);
                    }
                    self.validate_non_tuple_spreads_for_signature(args, func_type);

                    // CRITICAL FIX - Check excess properties against the MATCHED signature,
                    // not the union. Using the union would allow properties that exist in other overloads
                    // but not in the selected one, causing false negatives.
                    let matched_sig_helper = ContextualTypeContext::with_expected_and_options(
                        self.ctx.types,
                        func_type,
                        self.ctx.compiler_options.no_implicit_any,
                    );
                    self.check_call_argument_excess_properties(
                        args,
                        &final_arg_types,
                        |i, arg_count| matched_sig_helper.get_parameter_type_for_call(i, arg_count),
                    );

                    return Some(OverloadResolution {
                        arg_types: final_arg_types,
                        result: CallResult::Success(final_return_type),
                    });
                }
                CallResult::ArgumentTypeMismatch { index, .. } => {
                    if let Some(spread_idx) =
                        self.find_prior_non_tuple_spread_for_mismatch(args, index)
                    {
                        self.error_spread_must_be_tuple_or_rest_at(spread_idx);
                        self.ctx.node_types.merge(&temp_node_types);
                        return Some(OverloadResolution {
                            arg_types: arg_types.clone(),
                            result: CallResult::Success(sig.return_type),
                        });
                    }
                }
                CallResult::TypeParameterConstraintViolation { return_type, .. } => {
                    // Constraint violation from callback return - overload matched
                    // but with constraint error. If there are more overloads to try,
                    // continue to the next one (e.g., Object.freeze overload 0 is
                    // `T extends Function` which is violated for object args — we
                    // must try overload 1 `T extends {[idx:string]:U}`).
                    if signatures.len() > 1 {
                        continue;
                    }
                    self.ctx.node_types.merge(&temp_node_types);
                    return Some(OverloadResolution {
                        arg_types: arg_types.clone(),
                        result: CallResult::Success(return_type),
                    });
                }
                _ => {}
            }
        }

        // If the first pass deferred an overload without return context substitution
        // but no later overload succeeded, accept the deferred fallback.
        if let Some((fallback_arg_types, fallback_return_type, fallback_snap)) = no_rcs_fallback {
            self.ctx.rollback_full(&fallback_snap);
            self.ctx.node_types.merge(&temp_node_types);
            return Some(OverloadResolution {
                arg_types: fallback_arg_types,
                result: CallResult::Success(fallback_return_type),
            });
        }

        // Second pass: signature-specific contextual typing.
        // Some overload sets require contextual typing from a specific candidate to
        // type callback/object-literal arguments correctly. The union pass above can
        // miss those, producing false negatives and downstream false TS2345/TS2322.
        let mut failures = Vec::new();
        // When a signature returns TypeParameterConstraintViolation and there are more
        // overloads to try, store it as a fallback and continue. Used for Object.freeze
        // where overload 0 (T extends Function) is violated but overload 1 works.
        let _constraint_violation_fallback: Option<(TypeId, Vec<TypeId>)> = None;
        let mut all_arg_count_mismatches = true;
        let mut any_has_rest = false;
        let mut exact_expected_counts = std::collections::BTreeSet::new();
        let mut min_expected = usize::MAX;
        let mut max_expected = 0usize;
        let mut type_mismatch_count = 0usize;
        let mut has_non_count_non_type_failure = false;
        let mut best_type_mismatch: Option<(OverloadResolution, crate::context::NodeTypeCache)> =
            None;
        let mut mismatch_recovery_return: Option<TypeId> = None;
        // When an overload returns TypeParameterConstraintViolation and there are
        // more overloads to try, we store it as a fallback and continue. If no
        // later overload succeeds, we use this fallback (e.g., for single-overload
        // constraint violations that must still resolve to a return type).
        let mut constraint_violation_fallback: Option<(TypeId, Vec<TypeId>)> = None;
        for (sig, &func_type) in signatures.iter().zip(signature_types.iter()) {
            self.ctx.rollback_full(&overload_snap);
            let sig_helper = ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                func_type,
                self.ctx.compiler_options.no_implicit_any,
            );

            // Reset TS2454 state to the pre-overload baseline for each candidate.
            self.ctx
                .restore_ts2454_state(&overload_snap.emitted_ts2454_errors);
            // Snapshot per-candidate diagnostic state so we can roll back on mismatch.
            let candidate_snap = self.ctx.snapshot_diagnostics();
            let candidate_ts2454_errors = self.ctx.emitted_ts2454_errors.clone();
            self.ctx.node_types = Default::default();
            refresh_all_args(self);
            let resolved_func_type =
                if let Some(def_id) = lazy_def_id_for_type(self.ctx.types, func_type) {
                    self.ctx
                        .type_env
                        .borrow()
                        .get_def(def_id)
                        .unwrap_or(func_type)
                } else {
                    func_type
                };

            let candidate_callable_ctx = CallableContext::new(func_type);
            let candidate_param_types: Vec<Option<TypeId>> = (0..args.len())
                .map(|i| {
                    self.contextual_parameter_type_for_call_with_env_from_expected(
                        func_type,
                        i,
                        args.len(),
                    )
                    .or_else(|| sig_helper.get_parameter_type_for_call(i, args.len()))
                    .map(|param_type| self.normalize_contextual_call_param_type(param_type))
                })
                .collect();
            let candidate_refresh_args: Vec<bool> = args
                .iter()
                .enumerate()
                .map(|(i, &arg_idx)| {
                    self.argument_needs_refresh_for_contextual_call(
                        arg_idx,
                        candidate_param_types.get(i).copied().flatten(),
                    )
                })
                .collect();
            let should_preinfer_candidate = !sig.type_params.is_empty()
                && candidate_refresh_args
                    .iter()
                    .copied()
                    .any(std::convert::identity);

            let prev_preserve_literals2 = self.ctx.preserve_literal_types;
            self.ctx.preserve_literal_types = true;
            // When the signature has `const` type parameters (e.g., `<const T>`
            // or JSDoc `@template const T`), set const-assertion context so that
            // argument expressions get readonly tuple / readonly object / literal
            // inference — matching tsc's behavior for const type parameters.
            let has_const_type_params = sig.type_params.iter().any(|tp| tp.is_const);
            let prev_in_const_assertion = self.ctx.in_const_assertion;
            if has_const_type_params {
                self.ctx.in_const_assertion = true;
            }
            let mut sig_arg_types = if should_preinfer_candidate {
                let round1_arg_types = self.collect_call_argument_types_with_context(
                    args,
                    |i, _arg_count| {
                        if candidate_refresh_args.get(i).copied().unwrap_or(false) {
                            None
                        } else {
                            candidate_param_types.get(i).copied().flatten()
                        }
                    },
                    false,
                    Some(&candidate_refresh_args),
                    candidate_callable_ctx,
                );
                let instantiated_params = self
                    .resolve_call_with_checker_adapter(
                        resolved_func_type,
                        &round1_arg_types,
                        force_bivariant_callbacks,
                        contextual_type,
                        actual_this_type,
                    )
                    .2;
                let sig_shape = FunctionShape {
                    params: sig.params.clone(),
                    return_type: sig.return_type,
                    this_type: sig.this_type,
                    type_params: sig.type_params.clone(),
                    type_predicate: sig.type_predicate,
                    is_constructor: false,
                    is_method: sig.is_method,
                };
                let return_sub_for_preinfer = if contextual_type.is_some() {
                    self.compute_return_context_substitution_from_shape(&sig_shape, contextual_type)
                } else {
                    crate::query_boundaries::common::TypeSubstitution::new()
                };
                let retry_params = if !return_sub_for_preinfer.is_empty() {
                    Some(
                        sig.params
                            .iter()
                            .map(|param| {
                                let mut instantiated_param = *param;
                                instantiated_param.type_id =
                                    crate::query_boundaries::common::instantiate_type(
                                        self.ctx.types,
                                        param.type_id,
                                        &return_sub_for_preinfer,
                                    );
                                instantiated_param
                            })
                            .collect::<Vec<_>>(),
                    )
                } else {
                    instantiated_params.clone()
                };

                if let Some(instantiated_params) = retry_params.as_ref() {
                    self.ctx
                        .rollback_diagnostics_filtered(&candidate_snap, |diag| {
                            Self::should_preserve_speculative_call_diagnostic(diag)
                        });
                    self.ctx.restore_ts2454_state(&candidate_ts2454_errors);
                    self.clear_contextual_resolution_cache();
                    self.ctx.node_types = Default::default();
                    refresh_all_args(self);
                    for (i, &arg_idx) in args.iter().enumerate() {
                        if self.argument_needs_refresh_for_contextual_call(
                            arg_idx,
                            candidate_param_types.get(i).copied().flatten(),
                        ) {
                            self.invalidate_expression_for_contextual_retry(arg_idx);
                        }
                    }

                    let refreshed_contextual_types = if !return_sub_for_preinfer.is_empty() {
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
                            .collect::<Vec<_>>()
                    } else {
                        self.contextual_param_types_from_instantiated_params(
                            instantiated_params,
                            args.len(),
                        )
                        .into_iter()
                        .map(|param_type| {
                            param_type.map(|param_type| {
                                self.normalize_contextual_call_param_type(param_type)
                            })
                        })
                        .collect::<Vec<_>>()
                    };
                    self.collect_call_argument_types_with_context(
                        args,
                        |i, _arg_count| {
                            refreshed_contextual_types
                                .get(i)
                                .copied()
                                .flatten()
                                .or_else(|| candidate_param_types.get(i).copied().flatten())
                        },
                        false,
                        None,
                        candidate_callable_ctx,
                    )
                } else {
                    self.collect_call_argument_types_with_context(
                        args,
                        |i, _arg_count| candidate_param_types.get(i).copied().flatten(),
                        false,
                        None,
                        candidate_callable_ctx,
                    )
                }
            } else {
                self.collect_call_argument_types_with_context(
                    args,
                    |i, _arg_count| candidate_param_types.get(i).copied().flatten(),
                    false,
                    None,
                    candidate_callable_ctx,
                )
            };
            self.ctx.preserve_literal_types = prev_preserve_literals2;
            self.ctx.in_const_assertion = prev_in_const_assertion;

            self.ensure_relation_input_ready(func_type);

            let (mut result, _instantiated_predicate, instantiated_params) = self
                .resolve_call_with_checker_adapter(
                    resolved_func_type,
                    &sig_arg_types,
                    force_bivariant_callbacks,
                    contextual_type,
                    actual_this_type,
                );
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
                crate::query_boundaries::common::TypeSubstitution::new()
            };
            let retry_params = if !return_sub_for_retry.is_empty() {
                Some(
                    sig.params
                        .iter()
                        .map(|param| {
                            let mut instantiated_param = *param;
                            instantiated_param.type_id =
                                crate::query_boundaries::common::instantiate_type(
                                    self.ctx.types,
                                    param.type_id,
                                    &return_sub_for_retry,
                                );
                            instantiated_param
                        })
                        .collect::<Vec<_>>(),
                )
            } else {
                instantiated_params.clone()
            };
            if !sig.type_params.is_empty()
                && !contextual_refresh_args.is_empty()
                && let Some(instantiated_params) = retry_params.as_ref()
            {
                let candidate_first_pass_end = self.ctx.snapshot_diagnostics();
                let preserved_candidate_arg_diags = self.collect_non_callback_diagnostics_between(
                    args,
                    &candidate_snap,
                    &candidate_first_pass_end,
                );
                self.ctx
                    .rollback_diagnostics_filtered(&candidate_snap, |diag| {
                        Self::should_preserve_speculative_call_diagnostic(diag)
                    });
                if !preserved_candidate_arg_diags.is_empty() {
                    let mut merged = self.preserved_speculative_call_diagnostics(&candidate_snap);
                    self.extend_unique_diagnostics(&mut merged, preserved_candidate_arg_diags);
                    self.ctx
                        .rollback_and_replace_diagnostics(&candidate_snap, merged);
                }
                self.ctx.restore_ts2454_state(&candidate_ts2454_errors);
                self.ctx.node_types = Default::default();
                refresh_all_args(self);

                // Restore const-assertion and literal-preservation context for the
                // contextual retry pass. These flags were cleared at the end of the
                // preinference rounds (above), but the retry still needs them so that
                // nested array/object literals in arguments of `const` type parameter
                // functions are correctly inferred as readonly tuples / readonly objects.
                let prev_preserve_literals_retry = self.ctx.preserve_literal_types;
                let prev_in_const_assertion_retry = self.ctx.in_const_assertion;
                self.ctx.preserve_literal_types = true;
                if has_const_type_params {
                    self.ctx.in_const_assertion = true;
                }

                let retry_callable_ctx = CallableContext::new(func_type);
                let refreshed_contextual_types = if !return_sub_for_retry.is_empty() {
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
                    self.contextual_param_types_from_instantiated_params(
                        instantiated_params,
                        args.len(),
                    )
                };
                let refreshed_arg_types = self.collect_call_argument_types_with_context(
                    args,
                    |i, _arg_count| refreshed_contextual_types.get(i).copied().flatten(),
                    false,
                    None,
                    retry_callable_ctx,
                );

                self.ctx.preserve_literal_types = prev_preserve_literals_retry;
                self.ctx.in_const_assertion = prev_in_const_assertion_retry;

                let (retry_result, _retry_predicate, _retry_instantiated_params) = self
                    .resolve_call_with_checker_adapter(
                        resolved_func_type,
                        &refreshed_arg_types,
                        force_bivariant_callbacks,
                        contextual_type,
                        actual_this_type,
                    );
                match retry_result {
                    CallResult::Success(_)
                    | CallResult::ArgumentTypeMismatch { .. }
                    | CallResult::TypeParameterConstraintViolation { .. } => {
                        sig_arg_types = refreshed_arg_types;
                        result = retry_result;
                    }
                    _ => {}
                }
            }

            match result {
                CallResult::Success(return_type) => {
                    if let Some((index, actual, expected)) = self
                        .current_block_body_callback_return_mismatch_arg(args, |checker, index| {
                            sig_helper
                                .get_parameter_type_for_call(index, args.len())
                                .or_else(|| {
                                    ContextualTypeContext::with_expected_and_options(
                                        checker.ctx.types,
                                        func_type,
                                        checker.ctx.compiler_options.no_implicit_any,
                                    )
                                    .get_parameter_type_for_call(index, args.len())
                                })
                        })
                    {
                        all_arg_count_mismatches = false;
                        type_mismatch_count += 1;
                        if type_mismatch_count == 1 {
                            best_type_mismatch = Some((
                                OverloadResolution {
                                    arg_types: sig_arg_types.clone(),
                                    result: CallResult::ArgumentTypeMismatch {
                                        index,
                                        expected,
                                        actual,
                                        fallback_return: return_type,
                                    },
                                },
                                std::mem::take(&mut self.ctx.node_types),
                            ));
                        }
                        failures.push(PendingDiagnosticBuilder::argument_not_assignable(
                            actual, expected,
                        ));
                        self.ctx
                            .rollback_diagnostics_filtered(&candidate_snap, |diag| {
                                Self::should_preserve_speculative_call_diagnostic(diag)
                            });
                        continue;
                    }
                    // Reject candidates with callback body errors (e.g., inner
                    // call failures) — same rationale as the first-pass check.
                    if signatures.len() > 1
                        && self.overload_candidate_has_callback_body_errors(args, &candidate_snap)
                    {
                        all_arg_count_mismatches = false;
                        has_non_count_non_type_failure = true;
                        self.ctx
                            .rollback_diagnostics_filtered(&candidate_snap, |diag| {
                                Self::should_preserve_speculative_call_diagnostic(diag)
                            });
                        continue;
                    }
                    if self
                        .overload_candidate_has_hard_non_callback_arg_errors(args, &candidate_snap)
                    {
                        let preserved_first_pass_diags = self
                            .collect_non_callback_diagnostics_between(
                                args,
                                &overload_snap.diag,
                                &candidate_snap,
                            );
                        let kept_candidate_diags =
                            self.ctx.take_speculative_diagnostics(&candidate_snap);
                        // Merge: preserve hard speculative call diagnostics
                        // (e.g. TS2302/TS2708), then append first-pass and
                        // candidate diagnostics without duplication.
                        let mut merged =
                            self.preserved_speculative_call_diagnostics(&overload_snap.diag);
                        self.extend_unique_diagnostics(&mut merged, preserved_first_pass_diags);
                        self.extend_unique_diagnostics(&mut merged, kept_candidate_diags);
                        self.ctx
                            .rollback_and_replace_diagnostics(&overload_snap.diag, merged);
                        let sig_node_types = std::mem::take(&mut self.ctx.node_types);
                        self.ctx.node_types = std::mem::take(&mut original_node_types);
                        self.ctx.node_types.merge_owned(sig_node_types);
                        self.validate_non_tuple_spreads_for_signature(args, func_type);
                        self.check_call_argument_excess_properties(
                            args,
                            &sig_arg_types,
                            |i, arg_count| sig_helper.get_parameter_type_for_call(i, arg_count),
                        );

                        return Some(OverloadResolution {
                            arg_types: sig_arg_types,
                            result: CallResult::Success(return_type),
                        });
                    }
                    let preserved_first_pass_diags = self.collect_non_callback_diagnostics_between(
                        args,
                        &overload_snap.diag,
                        &candidate_snap,
                    );
                    let kept_candidate_diags =
                        self.ctx.take_speculative_diagnostics(&candidate_snap);
                    // Merge: preserve hard speculative call diagnostics
                    // (e.g. TS2302/TS2708), then append first-pass and
                    // candidate diagnostics without duplication.
                    let mut merged =
                        self.preserved_speculative_call_diagnostics(&overload_snap.diag);
                    self.extend_unique_diagnostics(&mut merged, preserved_first_pass_diags);
                    self.extend_unique_diagnostics(&mut merged, kept_candidate_diags);
                    self.ctx
                        .rollback_and_replace_diagnostics(&overload_snap.diag, merged);
                    let sig_node_types = std::mem::take(&mut self.ctx.node_types);
                    self.ctx.node_types = std::mem::take(&mut original_node_types);
                    self.ctx.node_types.merge_owned(sig_node_types);
                    self.validate_non_tuple_spreads_for_signature(args, func_type);

                    self.check_call_argument_excess_properties(
                        args,
                        &sig_arg_types,
                        |i, arg_count| sig_helper.get_parameter_type_for_call(i, arg_count),
                    );

                    return Some(OverloadResolution {
                        arg_types: sig_arg_types,
                        result: CallResult::Success(return_type),
                    });
                }
                CallResult::ArgumentTypeMismatch { index, .. } => {
                    if let Some(spread_idx) =
                        self.find_prior_non_tuple_spread_for_mismatch(args, index)
                    {
                        self.ctx.node_types = std::mem::take(&mut original_node_types);
                        self.error_spread_must_be_tuple_or_rest_at(spread_idx);
                        return Some(OverloadResolution {
                            arg_types: sig_arg_types,
                            result: CallResult::Success(sig.return_type),
                        });
                    }

                    all_arg_count_mismatches = false;
                    if let CallResult::ArgumentTypeMismatch {
                        expected,
                        actual,
                        fallback_return,
                        ..
                    } = result
                    {
                        if mismatch_recovery_return.is_none()
                            && !fallback_return.is_any_unknown_or_error()
                            && !crate::query_boundaries::common::is_type_deeply_any(
                                self.ctx.types,
                                fallback_return,
                            )
                            && !crate::query_boundaries::common::contains_type_parameters(
                                self.ctx.types,
                                fallback_return,
                            )
                        {
                            mismatch_recovery_return = Some(fallback_return);
                        }
                        type_mismatch_count += 1;
                        if type_mismatch_count == 1 {
                            best_type_mismatch = Some((
                                OverloadResolution {
                                    arg_types: sig_arg_types.clone(),
                                    result: CallResult::ArgumentTypeMismatch {
                                        index,
                                        expected,
                                        actual,
                                        fallback_return,
                                    },
                                },
                                std::mem::take(&mut self.ctx.node_types),
                            ));
                        }
                        failures.push(PendingDiagnosticBuilder::argument_not_assignable(
                            actual, expected,
                        ));
                    }
                }
                CallResult::ArgumentCountMismatch {
                    expected_min,
                    expected_max,
                    actual,
                } => {
                    if expected_max.is_none() {
                        any_has_rest = true;
                    } else if expected_min == expected_max.unwrap_or(expected_min) {
                        exact_expected_counts.insert(expected_min);
                    }
                    let max = expected_max.unwrap_or(expected_min);
                    min_expected = min_expected.min(expected_min);
                    max_expected = max_expected.max(max);
                    failures.push(PendingDiagnosticBuilder::argument_count_mismatch(
                        expected_min,
                        max,
                        actual,
                    ));
                }
                CallResult::TypeParameterConstraintViolation { return_type, .. } => {
                    // If more overloads remain, store this as a fallback and try next.
                    // This handles cases like Object.freeze where overload 0
                    // (T extends Function) is violated for object args but overload 1
                    // (T extends {[idx:string]:U}) should be tried next.
                    if signatures.len() > 1 && constraint_violation_fallback.is_none() {
                        constraint_violation_fallback = Some((return_type, sig_arg_types.clone()));
                        self.ctx
                            .rollback_diagnostics_filtered(&candidate_snap, |diag| {
                                Self::should_preserve_speculative_call_diagnostic(diag)
                            });
                        continue;
                    }
                    let preserved_first_pass_diags = self.collect_non_callback_diagnostics_between(
                        args,
                        &overload_snap.diag,
                        &candidate_snap,
                    );
                    let kept_candidate_diags =
                        self.ctx.take_speculative_diagnostics(&candidate_snap);
                    let mut merged =
                        self.preserved_speculative_call_diagnostics(&overload_snap.diag);
                    self.extend_unique_diagnostics(&mut merged, preserved_first_pass_diags);
                    self.extend_unique_diagnostics(&mut merged, kept_candidate_diags);
                    self.ctx
                        .rollback_and_replace_diagnostics(&overload_snap.diag, merged);
                    let sig_node_types = std::mem::take(&mut self.ctx.node_types);
                    self.ctx.node_types = std::mem::take(&mut original_node_types);
                    self.ctx.node_types.merge_owned(sig_node_types);
                    return Some(OverloadResolution {
                        arg_types: sig_arg_types,
                        result: CallResult::Success(return_type),
                    });
                }
                _ => {
                    all_arg_count_mismatches = false;
                    has_non_count_non_type_failure = true;
                }
            }

            self.ctx
                .rollback_diagnostics_filtered(&candidate_snap, |diag| {
                    Self::should_preserve_speculative_call_diagnostic(diag)
                });
        }

        if !has_non_count_non_type_failure
            && type_mismatch_count == 1
            && let Some((best_type_mismatch, sig_node_types)) = best_type_mismatch
        {
            self.ctx
                .rollback_diagnostics_filtered(&overload_snap.diag, |diag| {
                    Self::should_preserve_speculative_call_diagnostic(diag)
                });
            self.ctx
                .restore_ts2454_state(&overload_snap.emitted_ts2454_errors);
            self.ctx.node_types = std::mem::take(&mut original_node_types);
            self.ctx.node_types.merge_owned(sig_node_types);
            return Some(best_type_mismatch);
        }

        // If we encountered a TypeParameterConstraintViolation while trying overloads
        // but no later overload succeeded cleanly, use the constraint-violation result
        // as a successful resolution (the return type is still valid; only the
        // constraint check itself failed, which is reported separately).
        if let Some((fallback_return_type, fallback_arg_types)) = constraint_violation_fallback {
            self.ctx
                .rollback_diagnostics_filtered(&overload_snap.diag, |diag| {
                    Self::should_preserve_speculative_call_diagnostic(diag)
                });
            self.ctx.node_types = original_node_types;
            return Some(OverloadResolution {
                arg_types: fallback_arg_types,
                result: CallResult::Success(fallback_return_type),
            });
        }

        // No overload matched: drop speculative diagnostics from overload argument
        // collection and keep only overload-level diagnostics.
        // Roll back diagnostics and TS2454 state to the pre-overload snapshot
        // so the fallback path can re-evaluate cleanly.
        self.ctx
            .rollback_diagnostics_filtered(&overload_snap.diag, |diag| {
                Self::should_preserve_speculative_call_diagnostic(diag)
            });
        self.ctx
            .restore_ts2454_state(&overload_snap.emitted_ts2454_errors);

        // Restore original state if no overload matched
        self.ctx.node_types = original_node_types;
        if all_arg_count_mismatches && !failures.is_empty() {
            if !any_has_rest
                && !exact_expected_counts.is_empty()
                && !exact_expected_counts.contains(&args.len())
            {
                let mut lower = None;
                let mut upper = None;
                for &count in &exact_expected_counts {
                    if count < args.len() {
                        lower = Some(lower.map_or(count, |prev: usize| prev.max(count)));
                    } else if count > args.len() {
                        upper = Some(upper.map_or(count, |prev: usize| prev.min(count)));
                    }
                }
                if let (Some(expected_low), Some(expected_high)) = (lower, upper) {
                    return Some(OverloadResolution {
                        arg_types,
                        result: CallResult::OverloadArgumentCountMismatch {
                            actual: args.len(),
                            expected_low,
                            expected_high,
                        },
                    });
                }
            }

            return Some(OverloadResolution {
                arg_types,
                result: CallResult::ArgumentCountMismatch {
                    expected_min: min_expected,
                    expected_max: if any_has_rest {
                        None
                    } else if max_expected > min_expected {
                        Some(max_expected)
                    } else {
                        Some(min_expected)
                    },
                    actual: args.len(),
                },
            });
        }

        // When no overload matched, use the last overload's return type as the
        // fallback (matching tsc behavior). tsc always uses the last signature's
        // return type for error recovery so that downstream code sees the expected
        // shape rather than `never`. For example, `[].concat(...)` on `never[]`
        // should still produce `never[]`, not `never`.
        let fallback_return = mismatch_recovery_return.unwrap_or_else(|| {
            signatures
                .last()
                .map(|s| s.return_type)
                .unwrap_or(TypeId::NEVER)
        });
        Some(OverloadResolution {
            arg_types: arg_types.clone(),
            result: CallResult::NoOverloadMatch {
                func_type: TypeId::ANY,
                arg_types,
                failures,
                fallback_return,
            },
        })
    }
}
