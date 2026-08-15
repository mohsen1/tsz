//! Signature-walk phase of overload resolution.
//!
//! Extracted from the parent `overload_resolution` module to keep both files
//! under the 2000-LOC arch-guard limit. This is pure code motion: the method
//! remains an inherent method on `CheckerState`, so call sites are unchanged.

use crate::context::TypingRequest;
use crate::context::speculation::FullSpeculationSnapshot;
use crate::query_boundaries::checkers::call::lazy_def_id_for_type;
use crate::query_boundaries::common::{
    CallResult, ContextualTypeContext, PendingDiagnosticBuilder,
};
use crate::query_boundaries::construct_signatures::{
    function_shape_from_call_signature_preserving_method,
    function_type_from_call_signature_preserving_method, function_type_from_parts,
    function_type_from_shape,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

use super::super::{CallableContext, OverloadResolution, SelectedTypePredicate};
use super::retry_state::{BestTypeMismatch, NoReturnContextFallback, best_this_type_mismatch};

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
        tracing::debug!(
            "resolve_overloaded_call_with_signatures: signatures = {:?}, args = {:?}",
            signatures,
            args
        );
        if signatures.is_empty() {
            return None;
        }

        let arity_compatible_signature_count = signatures
            .iter()
            .filter(|sig| {
                let required = sig.params.iter().filter(|param| !param.optional).count();
                let has_rest = sig.params.iter().any(|param| param.rest);
                args.len() >= required && (has_rest || args.len() <= sig.params.len())
            })
            .count();
        let has_multiple_arity_compatible_signatures = arity_compatible_signature_count > 1;

        // An open-ended (non-tuple) array/iterable spread can only land on an
        // effective rest parameter, so fixed-arity candidates are skipped in
        // both overload-trial loops below when a reachable rest overload exists
        // (see `skip_non_rest_overloads_for_open_ended_spread` for the full
        // rationale and the reachability gate that preserves the genuine
        // TS2556).
        let skip_non_rest_for_spread =
            self.skip_non_rest_overloads_for_open_ended_spread(args, signatures);

        // Overload contextual typing baseline.
        // First pass collects argument types once using a union of overload signatures.
        // If that fails to find a match, we run a second pass that re-collects arguments
        // per candidate signature with signature-specific contextual types. This helps
        // avoid false TS2345/TS2322 when the union contextual type is too lossy.
        // Create a union of all overload signatures for contextual typing
        let signature_types: Vec<TypeId> = signatures
            .iter()
            .map(|sig| {
                function_type_from_call_signature_preserving_method(self.ctx.types, sig, false)
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
        let union_contextual_param_types: Vec<_> = args
            .iter()
            .enumerate()
            .map(|(i, _)| ctx_helper.get_parameter_type_for_call(i, args.len()))
            .collect();
        let contextual_refresh_args =
            self.contextual_refresh_args(args, &union_contextual_param_types);
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
        //
        // Use an overlay over the caller's cached expression types rather than
        // an empty map: speculative collection must still SEE previously
        // computed types (flow narrowing of an argument like `obj[k]` after
        // `obj[k] = rhs` reads the cached `rhs` type, exactly as on the
        // non-overloaded call path), while its own writes stay isolated in the
        // overlay layer for the selective merge below.
        self.ctx.node_types = original_node_types.overlay();
        // Clear the contextual resolution cache once before the loop — the cache
        // is shared and needs clearing before any arg is re-evaluated, but clearing
        // it per-arg was redundant (empty after the first iteration).
        self.clear_contextual_resolution_cache();
        for &arg_idx in &contextual_refresh_args {
            self.invalidate_expression_for_contextual_retry(arg_idx);
        }
        // With multiple overloads the contextual type is a synthetic union of
        // every signature. That union is an existential alternative set, so the
        // speculative non-tuple-spread arity check (which demands *every* union
        // member admit the spread) must not fire TS2556 here — a sibling
        // overload without a rest parameter would otherwise poison a valid
        // spread into another overload's rest (e.g. `splice`). The authoritative
        // TS2556 is reported per selected/failed signature below.
        let union_callable_ctx = if signatures.len() > 1 {
            CallableContext::new_overload_union(union_contextual)
        } else {
            CallableContext::new(union_contextual)
        };
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

        // Install a working copy of the caller's node types for the rest of
        // overload resolution, but keep `original_node_types` as a pristine
        // snapshot of the caller's cached expression types. Every restore site
        // below rebuilds the post-resolution cache as
        // `node_types = take(original_node_types)` followed by
        // `merge_owned(sig_node_types)` — i.e. "restore the caller's entries,
        // then layer the winning signature's entries on top". That invariant
        // only holds if `original_node_types` still contains the caller's
        // entries at the restore; previously it was moved out here, so the
        // restores silently rebuilt on an empty map and dropped every cached
        // expression type the caller (and its sibling statements) had already
        // computed. The clone is `Arc`-cheap (copy-on-write), so the snapshot is
        // only materialized if a restore actually mutates it.
        self.ctx.node_types = original_node_types.clone();

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
        // See `union_context_lossy_for_callback_args`: a pass-1 match computed
        // from a callback the union context could not type is as unreliable as
        // one with callback body errors — defer it to the signature-specific
        // pass, which retypes callbacks per candidate exactly like tsc. The
        // consumer below already requires multiple arity-compatible
        // signatures, so skip both checks otherwise.
        let union_arg_collection_unreliable = has_multiple_arity_compatible_signatures
            && (self.overload_candidate_has_callback_body_errors(args, &overload_snap.diag)
                || self.union_context_lossy_for_callback_args(
                    args,
                    &union_contextual_param_types,
                    &signature_types,
                ));

        // First pass: try each signature with union-contextual argument types.
        // When an overload succeeds but its return context substitution is empty
        // (couldn't infer type params from contextual return type), defer it as
        // a fallback and continue trying later overloads which might have better
        // return context inference.
        let mut no_rcs_fallback: Option<NoReturnContextFallback> = None;
        // tsc's `chooseOverload` runs twice over the candidate list: first
        // with the subtype relation (where an `any` argument is NOT related
        // to concrete parameter types, at every nesting level), then with the
        // assignable relation. A candidate that only succeeds under the
        // assignable relation must not win immediately: a later candidate may
        // still pass the subtype pass (e.g. a generic overload instantiated
        // with `U = any`). The first assignable-only success is stashed here
        // and accepted after the loop, preserving declaration order for the
        // fallback pass. Single-signature calls keep today's behavior.
        let overload_two_pass = signatures.len() > 1;
        // Candidates whose pass-1 (subtype-relation) resolution succeeded but
        // that the union pass deferred to the signature-specific second pass.
        // tsc's `chooseOverload` runs the subtype pass over ALL candidates
        // before the assignable pass, so the second pass must try these
        // candidates first: an `any` argument relates to an `any`-instantiated
        // generic parameter under the subtype relation but NOT to a concrete
        // one, and the generic candidate must win over an earlier concrete
        // overload that only passes under plain assignability.
        let mut phase1_subtype_successes: Vec<usize> = Vec::new();
        let mut assignable_pass_fallback: Option<NoReturnContextFallback> = None;
        // tsc tries every overload before committing TS2556 for a non-tuple
        // spread; stash the first fixed-arity rejection as the last-resort
        // fallback in case no later rest overload accepts it (#14319).
        let mut spread_mismatch_fallback: Option<(NodeIndex, TypeId, SelectedTypePredicate)> = None;
        // Mark arguments whose type comes from a type annotation (typed
        // identifier, `as`/`satisfies`/`as const`). The direct (non-overloaded)
        // call path computes the same markers so generic inference treats their
        // literals as non-fresh and does not re-widen them. Overload resolution
        // must thread the same markers; otherwise an inferred type parameter
        // (e.g. `Object.fromEntries`'s `T`) is widened `1 -> number` only
        // because the call happened to be overloaded.
        // Per-argument source markers (incl. the #17282 unannotated-callback mask).
        let arg_markers_owned = self.call_arg_source_markers(args, arg_types.len());
        let arg_markers = arg_markers_owned.as_borrowed();
        for (idx, original_sig) in signatures.iter().enumerate() {
            // An open-ended array/iterable spread can only land on an effective
            // rest parameter; a fixed-arity overload is not applicable (see
            // `skip_non_rest_for_spread`).
            if skip_non_rest_for_spread && !original_sig.params.iter().any(|param| param.rest) {
                continue;
            }
            let sig = self.overload_signature_for_inference(
                original_sig,
                idx,
                &arg_types,
                contextual_type,
            );
            let sig_shape = function_shape_from_call_signature_preserving_method(&sig, false);
            let sig_contextual_type = if contextual_type.is_some()
                && (self.suppress_generic_return_context_for_direct_arg_overlap(
                    &sig_shape,
                    args,
                    contextual_type,
                ) || self.suppress_generic_return_context_for_pinned_callback_return(
                    &sig_shape,
                    args,
                    contextual_type,
                )) {
                None
            } else {
                contextual_type
            };
            let func_type = function_type_from_shape(self.ctx.types, sig_shape.clone());
            tracing::debug!("Trying overload {} with {} args", idx, arg_types.len());
            self.ensure_callee_relation_inputs_ready(func_type);
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
            // Pass 1 (subtype pass): when the candidate fails under the
            // stricter any-source-not-related relation, roll back any
            // speculative state from the attempt and re-resolve with the
            // normal assignable relation. A success from that second attempt
            // is deferred (see `assignable_pass_fallback`) instead of
            // selected immediately.
            let mut subtype_pass_deferred = false;
            let (mut result, instantiated_predicate, instantiated_params) = if let Some(result) =
                self.overload_string_argument_array_parameter_mismatch(&sig, &arg_types)
            {
                (result, None, None)
            } else {
                let mut resolution = None;
                if overload_two_pass {
                    let subtype_pass_snap = self.snapshot_overload_retry_state();
                    let pass1 = self.resolve_call_with_checker_adapter_subtype_pass(
                        resolved_func_type,
                        &arg_types,
                        force_bivariant_callbacks,
                        sig_contextual_type,
                        None,
                        &arg_markers,
                    );
                    if matches!(pass1.0, CallResult::Success(_)) {
                        resolution = Some(pass1);
                        phase1_subtype_successes.push(idx);
                    } else {
                        self.rollback_overload_retry_state(&subtype_pass_snap);
                        subtype_pass_deferred = true;
                    }
                }
                resolution.unwrap_or_else(|| {
                    self.resolve_call_with_checker_adapter_maybe_arg_sources(
                        resolved_func_type,
                        &arg_types,
                        force_bivariant_callbacks,
                        sig_contextual_type,
                        None,
                        &arg_markers,
                    )
                })
            };
            if let CallResult::ArgumentTypeMismatch {
                expected,
                actual,
                fallback_return,
                ..
            } = result
                && self.type_is_or_constrained_to_top_rest_any_callable(expected)
                && crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    actual,
                )
                .is_some()
            {
                result = CallResult::Success(fallback_return);
            }
            let mut selected_type_predicate =
                Self::selected_overload_type_predicate(&sig, instantiated_predicate);
            if let Some(retry_result) = self.retry_overload_after_contextual_refresh_mismatch(
                super::contextual_retry::ContextualRetryInput {
                    result: &result,
                    sig: &sig,
                    instantiated_params: instantiated_params.as_ref(),
                    resolved_func_type,
                    args,
                    force_bivariant_callbacks,
                    contextual_type: sig_contextual_type,
                    actual_this_type,
                    overload_snap: &overload_snap,
                    has_contextual_refresh_args: !contextual_refresh_args.is_empty(),
                    caller_node_types: &original_node_types,
                },
                &mut selected_type_predicate,
            ) {
                result = retry_result;
            }

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
                    //
                    // A union of function-typed parameters yields an `any` callback
                    // parameter, so body errors specific to the selected signature
                    // (e.g. `o?.a.b` continuation typing, or a property that only exists
                    // under a different overload) never surface in the union pass.
                    // Speculatively re-type the callbacks against the selected
                    // signature so those cases are deferred too.
                    if has_multiple_arity_compatible_signatures
                        && (union_arg_collection_unreliable
                            || self.overload_candidate_has_callback_body_errors(
                                args,
                                &post_union_arg_diag_snap,
                            )
                            || self.selected_overload_callback_body_has_errors(args, func_type))
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
                    // When a non-generic overload succeeds but its return type contains
                    // `any` (e.g., T = `(a: any) => any` from `Array<(a: any) => any>`),
                    // defer it so that later generic overloads can be tried with return
                    // context substitution. A generic overload may bind its type parameter
                    // to the contextual return type and give a more precise result
                    // (e.g., `reduce<U>` returning U = Output instead of `(a: any) => any`).
                    // This matches TypeScript's behavior of preferring generic overloads
                    // when the non-generic return type is any-tainted and a contextual
                    // return type exists that could be satisfied by a generic binding.
                    if has_multiple_arity_compatible_signatures
                        && sig.type_params.is_empty()
                        && sig_contextual_type.is_some_and(|ct| {
                            !crate::query_boundaries::common::is_type_deeply_any(self.ctx.types, ct)
                        })
                        && crate::query_boundaries::assignability::contains_any_type(
                            self.ctx.types,
                            return_type,
                        )
                        && no_rcs_fallback.is_none()
                        && signatures[idx + 1..].iter().any(|later| {
                            if later.type_params.is_empty() {
                                return false;
                            }
                            let required = later.params.iter().filter(|p| !p.optional).count();
                            let has_rest = later.params.iter().any(|p| p.rest);
                            args.len() >= required && (has_rest || args.len() <= later.params.len())
                        })
                    {
                        no_rcs_fallback = Some((
                            arg_types.clone(),
                            return_type,
                            selected_type_predicate.clone(),
                            FullSpeculationSnapshot::new(&self.ctx),
                        ));
                        continue;
                    }
                    // When the matched overload is generic and has contextual refresh args,
                    // re-collect argument types with instantiated parameter types. The first
                    // pass used the union-contextual type which has unresolved type parameters,
                    // causing false diagnostics in callback bodies (e.g., TS2339 for `this.b`
                    // when `this` has type `TContext` instead of the inferred `{b: string}`).
                    let mut did_instantiated_retry = false;
                    let mut used_return_context_sub_outer = false;
                    let return_sub_for_retry = if sig_contextual_type.is_some() {
                        self.compute_return_context_substitution_from_shape(
                            &sig_shape,
                            sig_contextual_type,
                        )
                    } else {
                        crate::query_boundaries::common::TypeSubstitution::new()
                    };
                    let mut retry_substitution = None;
                    let retry_params = if !return_sub_for_retry.is_empty() {
                        // Compose round-1's argument-driven inference with the
                        // return-context substitution. Round-1 inferred bindings for
                        // type params it could see in the args (e.g. `T` in
                        // `from<T,U>(ArrayLike<T>, ...)` — inferred from `inputB:B[]`);
                        // the return-context substitution covers type params bound by
                        // the contextual return type (e.g. `U` from `A[]`). Both must
                        // contribute to the contextual types used to evaluate callback
                        // bodies. Return-context may refine overlap so that, for
                        // `Object.freeze<T>(o:T):Readonly<T>` with contextual
                        // `readonly [string,number][]`, the call uses the return-bound
                        // `T = [string,number][]` instead of round-1's widened
                        // `T = (string|number)[][]`, while incompatible argument
                        // inference still wins.
                        let mut combined_sub = if let Some(inst) = instantiated_params.as_ref() {
                            self.extract_arg_inference_substitution(
                                &sig.params,
                                inst,
                                &sig.type_params,
                            )
                        } else {
                            crate::query_boundaries::common::TypeSubstitution::new()
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
                                        crate::query_boundaries::common::instantiate_type(
                                            self.ctx.types,
                                            param.type_id,
                                            &combined_sub,
                                        );
                                    instantiated_param
                                })
                                .collect::<Vec<_>>(),
                        )
                    } else {
                        instantiated_params.clone()
                    };
                    let retry_params = retry_params.map(|params| {
                        self.resolve_signature_parameter_type_queries(&sig.params, &params)
                    });
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
                        // Rebuild on the caller's cached entries (see the snapshot
                        // comment above) rather than an empty map. `refresh_all_args`
                        // below clears this call's own argument nodes so they are
                        // recomputed under the instantiated parameter context, but
                        // the first-pass success return does not restore
                        // `original_node_types`, so a `Default::default()` reset here
                        // would permanently drop unrelated caller entries.
                        self.ctx.node_types = original_node_types.clone();
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
                            let instantiated_func = function_type_from_parts(
                                self.ctx.types,
                                Vec::new(),
                                instantiated_params.clone(),
                                sig.this_type,
                                return_type,
                                sig.type_predicate,
                                false,
                                sig.is_method,
                            );
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
                        // Keep the first-pass instantiated retry consistent with the
                        // signature-specific retry below. The retry may contextually
                        // type an object literal with the inferred parameter type
                        // (for example `Object.freeze<T>(o: T): Readonly<T>`). If
                        // literal preservation is dropped here, the retry overwrites
                        // the successful first pass with widened property values.
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
                            let mut progressive_sub = retry_substitution.clone().unwrap_or_else(
                                crate::query_boundaries::common::TypeSubstitution::new,
                            );
                            let mut progressive_args = Vec::with_capacity(args.len());
                            for (i, &arg_idx) in args.iter().enumerate() {
                                let contextual_type = self.instantiated_contextual_param_type_at(
                                    &sig.params,
                                    i,
                                    &progressive_sub,
                                );
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
                                if let Some(shape_param) =
                                    sig.params.get(i).map(|p| p.type_id).or_else(|| {
                                        let last = sig.params.last()?;
                                        last.rest.then_some(last.type_id)
                                    })
                                {
                                    let mut arg_substitution =
                                        crate::query_boundaries::common::TypeSubstitution::new();
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
                                                    || crate::query_boundaries::common::contains_type_parameters(
                                                        self.ctx.types,
                                                        existing,
                                                    )
                                                    || crate::query_boundaries::common::contains_infer_types(
                                                        self.ctx.types,
                                                        existing,
                                                    )
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
                                |i, _arg_count| {
                                    refreshed_contextual_types.get(i).copied().flatten()
                                },
                                false,
                                None,
                                sig_callable_ctx,
                            )
                        };
                        self.ctx.preserve_literal_types = prev_preserve_literals_retry;
                        self.ctx.in_const_assertion = prev_in_const_assertion_retry;
                        // When return-context substitution was used to provide better
                        // contextual types, re-resolve the call with the correctly-typed
                        // arguments to get the right return type. Without this, the return
                        // type would still reflect T inferred from the badly-typed first
                        // pass (e.g., Readonly<(string|number)[][]> instead of
                        // Readonly<[string,number][]>).
                        let final_return_type = if used_return_context_sub {
                            // Compose round-1's argument-driven inference with
                            // the return-context substitution and instantiate
                            // the signature's return type. Mirrors the
                            // composition logic used above to build
                            // `retry_params`. The re-resolve below then
                            // confirms argument compatibility, but the return
                            // type is taken from this substitution so that
                            // the contextual-return-type binding (e.g.
                            // `E = SVGRectElement` from
                            // `let r: SVGRectElement = qs(...)!`) is preserved
                            // even when the call's positional arguments don't
                            // pin down the type parameter and the re-resolve
                            // would default it to its bound.
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
                            let from_sub = crate::query_boundaries::common::instantiate_type(
                                self.ctx.types,
                                sig.return_type,
                                &combined_sub,
                            );
                            // Run the re-resolve for its side effects (argument
                            // compatibility diagnostics) but discard the return
                            // type — the substitution-derived value is
                            // authoritative for the type-parameter binding.
                            let _ = self.resolve_call_with_checker_adapter(
                                resolved_func_type,
                                &refreshed_arg_types,
                                force_bivariant_callbacks,
                                sig_contextual_type,
                                actual_this_type,
                            );
                            from_sub
                        } else {
                            return_type
                        };
                        did_instantiated_retry = true;
                        used_return_context_sub_outer = used_return_context_sub;
                        (refreshed_arg_types, final_return_type)
                    } else {
                        (arg_types.clone(), return_type)
                    };
                    let final_return_type = if args
                        .iter()
                        .any(|&arg_idx| self.is_callback_like_argument(arg_idx))
                    {
                        final_return_type
                    } else {
                        self.instantiate_overload_return_with_context(
                            &sig,
                            retry_params.as_deref().or(instantiated_params.as_deref()),
                            sig_contextual_type,
                            final_return_type,
                        )
                    };

                    if has_multiple_arity_compatible_signatures
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
                        && sig_contextual_type.is_some()
                        && no_rcs_fallback.is_none()
                    {
                        no_rcs_fallback = Some((
                            final_arg_types.clone(),
                            final_return_type,
                            selected_type_predicate.clone(),
                            FullSpeculationSnapshot::new(&self.ctx),
                        ));
                        continue;
                    }

                    // This candidate failed the subtype pass and only matched
                    // under the assignable relation. Mirror tsc: keep scanning
                    // for a later candidate that passes the subtype pass; the
                    // first assignable-only success is preserved (declaration
                    // order) as the pass-2 fallback accepted after the loop.
                    if subtype_pass_deferred {
                        if assignable_pass_fallback.is_none() {
                            assignable_pass_fallback = Some((
                                final_arg_types.clone(),
                                final_return_type,
                                selected_type_predicate.clone(),
                                FullSpeculationSnapshot::new(&self.ctx),
                            ));
                        }
                        continue;
                    }

                    // Merge the node types inferred during argument collection.
                    // If we did the instantiated retry, node_types already contains the
                    // refreshed entries; otherwise merge the first-pass temp_node_types.
                    if !did_instantiated_retry {
                        self.ctx.node_types.merge(&temp_node_types);
                    }
                    // A fixed-arity overload must not win solely by accepting a
                    // non-tuple spread; a later rest overload may still match.
                    if let Some(spread_idx) =
                        self.first_non_tuple_spread_rejected_by_signature(args, func_type)
                    {
                        if overload_two_pass {
                            if spread_mismatch_fallback.is_none() {
                                spread_mismatch_fallback = Some((
                                    spread_idx,
                                    sig.return_type,
                                    selected_type_predicate.clone(),
                                ));
                            }
                            continue;
                        }
                        self.error_spread_must_be_tuple_or_rest_at(spread_idx);
                    }

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

                    // Some expression-bodied callbacks emit their body diagnostics
                    // only while later validating the selected candidate. Re-check
                    // before accepting the first-pass match so an inner failing call
                    // like `accu.concat(el)` cannot be hidden by `U = never[]`.
                    if has_multiple_arity_compatible_signatures
                        && self.overload_candidate_has_callback_body_errors(
                            args,
                            &post_union_arg_diag_snap,
                        )
                    {
                        self.prune_callback_body_diagnostics(args, &overload_snap.diag);
                        continue;
                    }

                    self.prune_speculative_callback_body_diagnostics_for_accepted_overload(
                        args,
                        &overload_snap.diag,
                    );
                    return Some(OverloadResolution {
                        arg_types: final_arg_types,
                        result: CallResult::Success(final_return_type),
                        selected_type_predicate,
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
                            selected_type_predicate,
                        });
                    }
                }
                _ => {}
            }
        }

        // If the first pass deferred an overload without return context substitution
        // but no later overload succeeded, accept the deferred fallback.
        if let Some(fallback) = no_rcs_fallback {
            return Some(self.accept_first_pass_success_fallback(fallback, &temp_node_types));
        }

        // No candidate passed the subtype pass: accept the first candidate
        // (declaration order) that matched under the assignable relation,
        // mirroring tsc's second `chooseOverload` pass.
        if let Some(fallback) = assignable_pass_fallback {
            return Some(self.accept_first_pass_success_fallback(fallback, &temp_node_types));
        }

        // No overload accepted the non-tuple spread, so commit the deferred
        // TS2556 against the first fixed-arity rejection (#14319).
        if let Some((spread_idx, recovery_return, predicate)) = spread_mismatch_fallback {
            self.error_spread_must_be_tuple_or_rest_at(spread_idx);
            self.ctx.node_types.merge(&temp_node_types);
            return Some(OverloadResolution {
                arg_types: arg_types.clone(),
                result: CallResult::Success(recovery_return),
                selected_type_predicate: predicate,
            });
        }

        // Second pass: signature-specific contextual typing.
        // Some overload sets require contextual typing from a specific candidate to
        // type callback/object-literal arguments correctly. The union pass above can
        // miss those, producing false negatives and downstream false TS2345/TS2322.
        let mut failures = Vec::new();
        let mut all_arg_count_mismatches = true;
        let mut any_has_rest = false;
        let mut exact_expected_counts = std::collections::BTreeSet::new();
        let mut min_expected = usize::MAX;
        let mut max_expected = 0usize;
        let mut type_mismatch_count = 0usize;
        let mut has_non_count_non_type_failure = false;
        let mut best_type_mismatch: Option<BestTypeMismatch> = None;
        let mut mismatch_recovery_return: Option<TypeId> = None;
        let mut callback_body_failure_return: Option<TypeId> = None;
        let mut callback_body_overload_diagnostics = Vec::new();
        // The most recent overload that matched at the signature level (arguments
        // assignable) but produced errors *inside* a callback body. tsc selects the
        // overload from the discriminating arguments and then reports the callback
        // body errors against the resolved signature; it does not treat a body error
        // as an overload mismatch. When no overload resolves cleanly we commit to
        // this candidate and surface its diagnostics instead of silently recovering.
        //
        // Candidate order mirrors tsc's two passes: candidates whose pass-1
        // subtype resolution succeeded (see `phase1_subtype_successes`) are
        // tried first in declaration order, then the remainder in declaration
        // order. With no pass-1 successes this is exactly declaration order.
        let second_pass_order: Vec<usize> = phase1_subtype_successes
            .iter()
            .copied()
            .chain((0..signatures.len()).filter(|idx| !phase1_subtype_successes.contains(idx)))
            .collect();
        for idx in second_pass_order {
            let original_sig = &signatures[idx];
            // See the first-pass loop: an open-ended spread requires an effective
            // rest parameter, so fixed-arity overloads are skipped here too.
            if skip_non_rest_for_spread && !original_sig.params.iter().any(|param| param.rest) {
                continue;
            }
            let sig = self.overload_signature_for_inference(
                original_sig,
                idx,
                &arg_types,
                contextual_type,
            );
            let sig_shape = function_shape_from_call_signature_preserving_method(&sig, false);
            let sig_contextual_type = if contextual_type.is_some()
                && (self.suppress_generic_return_context_for_direct_arg_overlap(
                    &sig_shape,
                    args,
                    contextual_type,
                ) || self.suppress_generic_return_context_for_pinned_callback_return(
                    &sig_shape,
                    args,
                    contextual_type,
                )) {
                None
            } else {
                contextual_type
            };
            let func_type = function_type_from_shape(self.ctx.types, sig_shape.clone());
            // The candidate signature the reporter renders in tsc's per-overload
            // `TS2772` wrapper. Built from the *declared* signature (not the
            // inference-instantiated `sig`) so a generic overload elaborates with
            // its written type parameters, exactly as tsc's `signatureToString`.
            // Built lazily: only failing candidates push a diagnostic, so a
            // candidate that matches or is skipped never interns this.
            let overload_signature = || {
                function_type_from_parts(
                    self.ctx.types,
                    original_sig.type_params.clone(),
                    original_sig.params.clone(),
                    original_sig.this_type,
                    original_sig.return_type,
                    original_sig.type_predicate,
                    false,
                    original_sig.is_method,
                )
            };
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
            // Per-candidate scratch layer over the caller's cached types: see
            // the union-contextual collection above for the read-visibility
            // rationale.
            self.ctx.node_types = original_node_types.overlay();
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
            let mut active_contextual_types = candidate_param_types.clone();
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
            let has_const_type_params =
                Self::signature_const_type_params_require_readonly_argument_context(
                    self.ctx.types,
                    &sig.type_params,
                );
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
                    .resolve_call_with_checker_adapter_maybe_arg_sources(
                        resolved_func_type,
                        &round1_arg_types,
                        force_bivariant_callbacks,
                        sig_contextual_type,
                        actual_this_type,
                        &arg_markers,
                    )
                    .2;
                let return_sub_for_preinfer = if sig_contextual_type.is_some() {
                    self.compute_return_context_substitution_from_shape(
                        &sig_shape,
                        sig_contextual_type,
                    )
                } else {
                    crate::query_boundaries::common::TypeSubstitution::new()
                };
                let retry_params = if !return_sub_for_preinfer.is_empty() {
                    // See `extract_arg_inference_substitution` for the rationale:
                    // the contextual types used to evaluate callback bodies must
                    // honour both round-1's argument-driven inference AND the
                    // return-context substitution, with the latter winning on
                    // overlap.
                    let mut combined_sub = if let Some(inst) = instantiated_params.as_ref() {
                        self.extract_arg_inference_substitution(&sig.params, inst, &sig.type_params)
                    } else {
                        crate::query_boundaries::common::TypeSubstitution::new()
                    };
                    self.merge_return_context_substitution(
                        &mut combined_sub,
                        &sig.type_params,
                        &return_sub_for_preinfer,
                    );
                    Some(
                        sig.params
                            .iter()
                            .map(|param| {
                                let mut instantiated_param = *param;
                                instantiated_param.type_id =
                                    crate::query_boundaries::common::instantiate_type(
                                        self.ctx.types,
                                        param.type_id,
                                        &combined_sub,
                                    );
                                instantiated_param
                            })
                            .collect::<Vec<_>>(),
                    )
                } else {
                    instantiated_params.clone()
                };
                let retry_params = retry_params.map(|params| {
                    self.resolve_signature_parameter_type_queries(&sig.params, &params)
                });

                if let Some(instantiated_params) = retry_params.as_ref() {
                    self.ctx
                        .rollback_diagnostics_filtered(&candidate_snap, |diag| {
                            Self::should_preserve_speculative_call_diagnostic(diag)
                        });
                    self.ctx.restore_ts2454_state(&candidate_ts2454_errors);
                    self.clear_contextual_resolution_cache();
                    self.ctx.node_types = original_node_types.overlay();
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
                    if !return_sub_for_preinfer.is_empty() {
                        let tracked_type_params: rustc_hash::FxHashSet<_> =
                            sig.type_params.iter().map(|tp| tp.name).collect();
                        let mut progressive_sub = {
                            let mut sub = self.extract_arg_inference_substitution(
                                &sig.params,
                                instantiated_params,
                                &sig.type_params,
                            );
                            self.merge_return_context_substitution(
                                &mut sub,
                                &sig.type_params,
                                &return_sub_for_preinfer,
                            );
                            sub
                        };
                        let mut progressive_args = Vec::with_capacity(args.len());
                        for (i, &arg_idx) in args.iter().enumerate() {
                            let contextual_type = self
                                .instantiated_contextual_param_type_at(
                                    &sig.params,
                                    i,
                                    &progressive_sub,
                                )
                                .or_else(|| candidate_param_types.get(i).copied().flatten());
                            let arg_type = self.compute_single_call_argument_type(
                                arg_idx,
                                contextual_type,
                                false,
                                i,
                                args.len(),
                                true,
                                candidate_callable_ctx,
                            );
                            let arg_for_refinement = contextual_type
                                .map(|expected| {
                                    self.instantiate_generic_function_argument_against_target_params(
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
                                let mut arg_substitution =
                                    crate::query_boundaries::common::TypeSubstitution::new();
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
                                        || return_sub_for_preinfer.get(name).is_some()
                                    {
                                        continue;
                                    }
                                    let should_update = match progressive_sub.get(name) {
                                        None => true,
                                        Some(existing) if existing == ty => false,
                                        Some(existing) => {
                                            existing == TypeId::UNKNOWN
                                                || existing == TypeId::ERROR
                                                || crate::query_boundaries::common::contains_type_parameters(
                                                    self.ctx.types,
                                                    existing,
                                                )
                                                || crate::query_boundaries::common::contains_infer_types(
                                                    self.ctx.types,
                                                    existing,
                                                )
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
                    }
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

            self.ensure_callee_relation_inputs_ready(func_type);

            let (mut result, instantiated_predicate, instantiated_params) = self
                .resolve_call_with_checker_adapter_maybe_arg_sources(
                    resolved_func_type,
                    &sig_arg_types,
                    force_bivariant_callbacks,
                    sig_contextual_type,
                    actual_this_type,
                    &arg_markers,
                );
            if let CallResult::ArgumentTypeMismatch {
                expected,
                actual,
                fallback_return,
                ..
            } = result
                && self.type_is_or_constrained_to_top_rest_any_callable(expected)
                && crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    actual,
                )
                .is_some()
            {
                result = CallResult::Success(fallback_return);
            }
            let mut selected_type_predicate =
                Self::selected_overload_type_predicate(&sig, instantiated_predicate);
            let return_sub_for_retry = if sig_contextual_type.is_some() {
                self.compute_return_context_substitution_from_shape(&sig_shape, sig_contextual_type)
            } else {
                crate::query_boundaries::common::TypeSubstitution::new()
            };
            let retry_params = if !return_sub_for_retry.is_empty() {
                // Compose argument-driven inference with the return-context
                // substitution, allowing return context to refine but not replace
                // incompatible argument inference.
                let mut combined_sub = if let Some(inst) = instantiated_params.as_ref() {
                    self.extract_arg_inference_substitution(&sig.params, inst, &sig.type_params)
                } else {
                    crate::query_boundaries::common::TypeSubstitution::new()
                };
                self.merge_return_context_substitution(
                    &mut combined_sub,
                    &sig.type_params,
                    &return_sub_for_retry,
                );
                Some(
                    sig.params
                        .iter()
                        .map(|param| {
                            let mut instantiated_param = *param;
                            instantiated_param.type_id =
                                crate::query_boundaries::common::instantiate_type(
                                    self.ctx.types,
                                    param.type_id,
                                    &combined_sub,
                                );
                            instantiated_param
                        })
                        .collect::<Vec<_>>(),
                )
            } else {
                instantiated_params.clone()
            };
            let retry_params = retry_params
                .map(|params| self.resolve_signature_parameter_type_queries(&sig.params, &params));
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
                self.ctx.node_types = original_node_types.overlay();
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
                active_contextual_types.clone_from(&refreshed_contextual_types);
                let refreshed_arg_types = self.collect_call_argument_types_with_context(
                    args,
                    |i, _arg_count| refreshed_contextual_types.get(i).copied().flatten(),
                    false,
                    None,
                    retry_callable_ctx,
                );

                self.ctx.preserve_literal_types = prev_preserve_literals_retry;
                self.ctx.in_const_assertion = prev_in_const_assertion_retry;

                let (retry_result, retry_predicate, _retry_instantiated_params) = self
                    .resolve_call_with_checker_adapter_maybe_arg_sources(
                        resolved_func_type,
                        &refreshed_arg_types,
                        force_bivariant_callbacks,
                        sig_contextual_type,
                        actual_this_type,
                        &arg_markers,
                    );
                if retry_predicate.is_some() {
                    selected_type_predicate = retry_predicate;
                }
                match retry_result {
                    CallResult::Success(_) | CallResult::ArgumentTypeMismatch { .. } => {
                        sig_arg_types = refreshed_arg_types;
                        result = retry_result;
                    }
                    _ => {}
                }
            }

            match result {
                CallResult::Success(return_type) => {
                    let return_type = if args
                        .iter()
                        .any(|&arg_idx| self.is_callback_like_argument(arg_idx))
                    {
                        return_type
                    } else {
                        self.instantiate_overload_return_with_context(
                            &sig,
                            retry_params.as_deref().or(instantiated_params.as_deref()),
                            sig_contextual_type,
                            return_type,
                        )
                    };
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
                                    selected_type_predicate: None,
                                },
                                std::mem::take(&mut self.ctx.node_types),
                                Vec::new(),
                            ));
                        }
                        failures.push(
                            PendingDiagnosticBuilder::argument_not_assignable(actual, expected)
                                .with_optional_span(self.arg_source_span(args, index))
                                .with_argument_index(index)
                                .with_overload_signature(overload_signature()),
                        );
                        self.ctx
                            .rollback_diagnostics_filtered(&candidate_snap, |diag| {
                                Self::should_preserve_speculative_call_diagnostic(diag)
                            });
                        continue;
                    }
                    // Reject candidates with callback body errors (e.g., inner
                    // call failures) — same rationale as the first-pass check.
                    for (i, &arg_idx) in args.iter().enumerate() {
                        if !self.is_callback_like_argument(arg_idx) {
                            continue;
                        }
                        let Some(param_type) = active_contextual_types.get(i).copied().flatten()
                        else {
                            continue;
                        };
                        self.invalidate_expression_for_contextual_retry(arg_idx);
                        let request = TypingRequest::with_contextual_type(param_type);
                        let _ = self.get_type_of_node_with_request(arg_idx, &request);
                    }
                    let retained_generic_rest_any_body_errors = self
                        .overload_candidate_has_only_retained_generic_rest_any_callback_body_errors(
                            args,
                            &sig.params,
                            &candidate_snap,
                        );
                    if has_multiple_arity_compatible_signatures
                        && self.overload_candidate_has_callback_body_errors(args, &candidate_snap)
                        && !retained_generic_rest_any_body_errors
                    {
                        all_arg_count_mismatches = false;
                        has_non_count_non_type_failure = true;
                        // A non-generic overload matched at the signature level, so its
                        // callback body diagnostics are real. tsc's chooseOverload
                        // commits to the FIRST such candidate — callback-body
                        // diagnostics never disqualify a signature-level match, so
                        // later overloads are not consulted (they would retype the
                        // callback under a different parameter type and report the
                        // wrong body errors). Generic overloads are left to the
                        // instantiation machinery, whose transient body errors must
                        // not leak.
                        if sig.type_params.is_empty() {
                            let candidate = self.build_callback_body_only_candidate(
                                args,
                                &overload_snap.diag,
                                &candidate_snap,
                                sig_arg_types.clone(),
                                return_type,
                                selected_type_predicate,
                            );
                            return Some(self.commit_callback_body_only_candidate(
                                candidate,
                                &overload_snap,
                                &mut original_node_types,
                            ));
                        }
                        let nested_no_overload_diags =
                            self.callback_body_no_overload_diagnostics_since(args, &candidate_snap);
                        self.extend_unique_diagnostics(
                            &mut callback_body_overload_diagnostics,
                            nested_no_overload_diags,
                        );
                        if let Some((index, span)) =
                            self.callback_body_failure_span(args, &candidate_snap)
                        {
                            let recovery_return =
                                if crate::query_boundaries::common::contains_type_parameters(
                                    self.ctx.types,
                                    sig.return_type,
                                ) {
                                    signatures
                                        .first()
                                        .map(|first| first.return_type)
                                        .unwrap_or(sig.return_type)
                                } else {
                                    sig.return_type
                                };
                            callback_body_failure_return.get_or_insert(recovery_return);
                            failures.clear();
                            failures.push(
                                PendingDiagnosticBuilder::argument_not_assignable(
                                    return_type,
                                    sig.return_type,
                                )
                                .with_span(span),
                            );
                            type_mismatch_count = type_mismatch_count.max(1);
                            best_type_mismatch = Some((
                                OverloadResolution {
                                    arg_types: sig_arg_types.clone(),
                                    result: CallResult::ArgumentTypeMismatch {
                                        index,
                                        expected: sig.return_type,
                                        actual: return_type,
                                        fallback_return: return_type,
                                    },
                                    selected_type_predicate: None,
                                },
                                std::mem::take(&mut self.ctx.node_types),
                                self.diagnostics_for_overload_mismatch_argument_between(
                                    args,
                                    index,
                                    &candidate_snap,
                                    &self.ctx.snapshot_diagnostics(),
                                ),
                            ));
                        }
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
                        let candidate_end = self.ctx.snapshot_diagnostics();
                        let kept_candidate_diags = self.collect_non_callback_diagnostics_between(
                            args,
                            &candidate_snap,
                            &candidate_end,
                        );
                        // Merge: preserve hard speculative call diagnostics
                        // (e.g. TS2302/TS2708), then append first-pass and
                        // non-callback candidate diagnostics without duplication.
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
                            selected_type_predicate,
                        });
                    }
                    let preserved_first_pass_diags = self.collect_non_callback_diagnostics_between(
                        args,
                        &overload_snap.diag,
                        &candidate_snap,
                    );
                    let candidate_end = self.ctx.snapshot_diagnostics();
                    let kept_candidate_diags = self.collect_non_callback_diagnostics_between(
                        args,
                        &candidate_snap,
                        &candidate_end,
                    );
                    // Merge: preserve hard speculative call diagnostics
                    // (e.g. TS2302/TS2708), then append first-pass and
                    // non-callback candidate diagnostics without duplication.
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
                        selected_type_predicate,
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
                            selected_type_predicate,
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
                                    selected_type_predicate: None,
                                },
                                std::mem::take(&mut self.ctx.node_types),
                                self.diagnostics_for_overload_mismatch_argument_between(
                                    args,
                                    index,
                                    &candidate_snap,
                                    &self.ctx.snapshot_diagnostics(),
                                ),
                            ));
                        }
                        failures.push(
                            PendingDiagnosticBuilder::argument_not_assignable(actual, expected)
                                .with_optional_span(self.arg_source_span(args, index))
                                .with_argument_index(index)
                                .with_overload_signature(overload_signature()),
                        );
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
                    failures.push(
                        PendingDiagnosticBuilder::argument_count_mismatch(
                            expected_min,
                            max,
                            actual,
                        )
                        .with_overload_signature(overload_signature()),
                    );
                }
                CallResult::ThisTypeMismatch {
                    expected_this,
                    actual_this,
                    emit_not_callable,
                } => {
                    all_arg_count_mismatches = false;
                    type_mismatch_count += 1;
                    if type_mismatch_count == 1 {
                        best_type_mismatch = Some(best_this_type_mismatch(
                            sig_arg_types.clone(),
                            expected_this,
                            actual_this,
                            emit_not_callable,
                            std::mem::take(&mut self.ctx.node_types),
                        ));
                    }
                    failures.push(
                        PendingDiagnosticBuilder::this_type_mismatch(expected_this, actual_this)
                            .with_overload_signature(overload_signature()),
                    );
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
            && let Some((best_type_mismatch, sig_node_types, preserved_arg_diags)) =
                best_type_mismatch
        {
            self.ctx
                .rollback_diagnostics_filtered(&overload_snap.diag, |diag| {
                    Self::should_preserve_speculative_call_diagnostic(diag)
                });
            self.ctx
                .restore_ts2454_state(&overload_snap.emitted_ts2454_errors);
            if !preserved_arg_diags.is_empty() {
                let mut diagnostics = std::mem::take(&mut self.ctx.diagnostics);
                self.extend_unique_diagnostics(&mut diagnostics, preserved_arg_diags);
                self.ctx.diagnostics = diagnostics;
                self.ctx.rebuild_emitted_diagnostics_from_current();
            }
            if let CallResult::ArgumentTypeMismatch { index, .. } = &best_type_mismatch.result {
                self.recheck_overload_args_after_mismatch_without_context(args, *index);
            }
            self.ctx.node_types = std::mem::take(&mut original_node_types);
            self.ctx.node_types.merge_owned(sig_node_types);
            return Some(best_type_mismatch);
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
        if !callback_body_overload_diagnostics.is_empty() {
            let mut diagnostics = std::mem::take(&mut self.ctx.diagnostics);
            self.extend_unique_diagnostics(&mut diagnostics, callback_body_overload_diagnostics);
            self.ctx.diagnostics = diagnostics;
        }

        // Restore original state if no overload matched
        self.ctx.node_types = original_node_types;
        if all_arg_count_mismatches && !failures.is_empty() {
            // Expanded count, not `args.len()`: a spread expands to multiple
            // arguments (`f(1, ...[2, 3])` has 2 `args` nodes but 3 arguments).
            let actual_count = arg_types.len();
            if !any_has_rest
                && !exact_expected_counts.is_empty()
                && !exact_expected_counts.contains(&actual_count)
            {
                let mut lower = None;
                let mut upper = None;
                for &count in &exact_expected_counts {
                    if count < actual_count {
                        lower = Some(lower.map_or(count, |prev: usize| prev.max(count)));
                    } else if count > actual_count {
                        upper = Some(upper.map_or(count, |prev: usize| prev.min(count)));
                    }
                }
                if let (Some(expected_low), Some(expected_high)) = (lower, upper) {
                    return Some(OverloadResolution {
                        arg_types,
                        result: CallResult::OverloadArgumentCountMismatch {
                            actual: actual_count,
                            expected_low,
                            expected_high,
                        },
                        selected_type_predicate: None,
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
                    actual: actual_count,
                },
                selected_type_predicate: None,
            });
        }

        // When no overload matched, the recovery result type is the intersection
        // of every candidate signature's return type (tsc's
        // `createUnionOfSignaturesForOverloadFailure`). A near-match recovery
        // return (argument or callback-body mismatch on a single applicable
        // overload) takes precedence when present.
        let fallback_return = mismatch_recovery_return.unwrap_or_else(|| {
            callback_body_failure_return.unwrap_or_else(|| {
                tsz_solver::operations::overload_failure_return_type(self.ctx.types, signatures)
            })
        });
        Some(OverloadResolution {
            arg_types: arg_types.clone(),
            result: CallResult::NoOverloadMatch {
                func_type: TypeId::ANY,
                arg_types,
                failures,
                fallback_return,
            },
            selected_type_predicate: None,
        })
    }
}
