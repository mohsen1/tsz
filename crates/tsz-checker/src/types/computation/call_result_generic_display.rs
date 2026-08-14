//! Display-target selection for a generic-callable argument mismatch,
//! split out of `call_result.rs` to keep it under the file-size guard.

use super::call_result::CallResultContext;
use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::common;
use crate::query_boundaries::common::CallResult;
use crate::query_boundaries::type_computation::core as expr_ops;
use crate::state::CheckerState;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn generic_callable_mismatch_display_target(
        &self,
        actual: TypeId,
        expected: TypeId,
    ) -> Option<TypeId> {
        let source_fn = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            actual,
        )?;
        let target_fn = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            expected,
        )?;
        // Only applies when the source is generic and the target is concrete.
        if source_fn.type_params.is_empty() || !target_fn.type_params.is_empty() {
            return None;
        }

        // Check that at least one source type parameter can be mapped from
        // the target's parameter types, confirming these are comparable
        // callable signatures worth building a concrete display target for.
        let has_mappable_param = source_fn.params.iter().zip(target_fn.params.iter()).any(
            |(source_param, target_param)| {
                let target_type = target_param.type_id;
                if target_type.is_any_unknown_or_error() {
                    return false;
                }
                common::collect_all_types(self.ctx.types, source_param.type_id)
                    .into_iter()
                    .any(|ty| {
                        common::type_param_info(self.ctx.types, ty).is_some_and(|tp| {
                            source_fn
                                .type_params
                                .iter()
                                .any(|source_tp| source_tp.is_same_binder(tp))
                        })
                    })
            },
        );
        if !has_mappable_param {
            return None;
        }

        // Build a concrete display target using the target's return type.
        // Previously this used the source's return type instantiated with
        // the target's param types, but that produced a target that was
        // trivially assignable from the source (e.g., `(v:string) => string`
        // for `identity<T>(v:T):T` vs `(v:string) => boolean`), suppressing
        // the TS2345 error that tsc emits.
        Some(call_checker::call_result_generic_callable_display_target(
            self.ctx.types,
            &target_fn,
        ))
    }

    /// Handle the result of a call evaluation, emitting diagnostics for errors
    /// and applying this-substitution/mixin refinement for successes.
    pub(super) fn handle_call_result(
        &mut self,
        result: CallResult,
        context: CallResultContext<'_>,
    ) -> TypeId {
        let CallResultContext {
            callee_expr,
            call_idx,
            args,
            arg_types,
            callee_type,
            callee_has_declared_generic_signature,
            raw_callee_shape,
            is_super_call,
            is_optional_chain,
            allow_contextual_mismatch_deferral,
            ..
        } = context;
        match result {
            CallResult::Success(return_type) => {
                if is_super_call {
                    return TypeId::VOID;
                }
                self.report_polymorphic_this_indexed_conditional_arg(callee_type, args, arg_types);
                self.finalize_call_return_like_success(
                    callee_expr,
                    call_idx,
                    callee_type,
                    arg_types,
                    return_type,
                    is_optional_chain,
                )
            }
            CallResult::NonVoidFunctionCalledWithNew => {
                // TS2350 only fires when `noImplicitAny` is off; with it on the
                // implicit-`any` result is reported as TS7009 instead.
                if !self.ctx.no_implicit_any() {
                    self.error_non_void_function_called_with_new_at(callee_expr);
                }
                TypeId::ANY
            }
            CallResult::VoidFunctionCalledWithNew => TypeId::ANY,
            CallResult::NotCallable { .. } => {
                if is_super_call {
                    // Emit TS2346 when the super() call target has no signatures
                    // (e.g., when the base class is used with invalid type arguments).
                    // Suppress TS2346 when:
                    // - callee type is ERROR (cascading diagnostic)
                    // - callee type is NULL (class extends null; TS17005 covers this)
                    // - callee is a completely empty callable (no sigs, no props) which
                    //   indicates a forward-reference resolution failure (TS2449 covers this)
                    // - the enclosing class extends a forward-referenced class in the
                    //   same file (TS2449 already reported on the heritage clause; tsc
                    //   suppresses the secondary TS2346 in this case).
                    let should_suppress = callee_type == TypeId::ERROR
                        || callee_type == TypeId::NULL
                        || crate::query_boundaries::common::get_callable_shape_for_type(
                            self.ctx.types,
                            callee_type,
                        )
                        .is_some_and(|shape| {
                            shape.call_signatures.is_empty()
                                && shape.construct_signatures.is_empty()
                                && shape.properties.is_empty()
                                && shape.string_index.is_none()
                                && shape.number_index.is_none()
                        })
                        || self.is_super_call_in_forward_referenced_extends(callee_expr);
                    if !should_suppress {
                        self.error_at_node(
                            callee_expr,
                            "Call target does not contain any signatures.",
                            diagnostic_codes::CALL_TARGET_DOES_NOT_CONTAIN_ANY_SIGNATURES,
                        );
                    }
                    return TypeId::VOID;
                }
                if self.is_constructor_type(callee_type)
                    && !self.is_intersection_with_conditional_application(callee_type)
                {
                    self.error_class_constructor_without_new_at(callee_type, callee_expr);
                } else if self.is_get_accessor_call(callee_expr) {
                    self.error_get_accessor_not_callable_at(callee_expr);
                } else if callee_type != TypeId::VOID
                    && (self.ctx.compiler_options.strict_null_checks
                        || crate::query_boundaries::type_predicates::has_ts_nullable_flag(
                            callee_type,
                        ))
                {
                    // tsc routes the callee through the same
                    // `checkNonNullTypeWithReporter` as every other non-null
                    // check, with `reportCannotInvokePossiblyNullOrUndefinedError`
                    // as the reporter. Without `strictNullChecks` the trigger
                    // narrows to `type.flags & TypeFlags.Nullable`, so a callee
                    // that *is* `null`/`undefined` still reports TS2721/2722 —
                    // it is not "not callable".
                    let (_non_nullish, nullish_cause) = self.split_nullish_type(callee_type);
                    if let Some(cause) = nullish_cause {
                        self.error_cannot_invoke_possibly_nullish_at(cause, callee_expr);
                        self.report_nullish_callee_declaration_companion(
                            call_idx,
                            callee_expr,
                            cause,
                        );
                    } else if !self.is_in_decorator_expression(callee_expr) {
                        // Don't emit TS2349 for calls inside decorators - decorators
                        // are resolved at runtime and should not be checked for callability.
                        self.error_not_callable_at(callee_type, callee_expr);
                    }
                } else if !self.is_in_decorator_expression(callee_expr) {
                    // Don't emit TS2349 for calls inside decorators - decorators
                    // are resolved at runtime and should not be checked for callability.
                    self.error_not_callable_at(callee_type, callee_expr);
                }
                TypeId::ERROR
            }
            CallResult::ArgumentCountMismatch {
                expected_min,
                expected_max,
                actual,
            } => {
                // Suppress TS2554/TS2555 for super calls where the parser already
                // emitted TS2754 ("super may not use type arguments") and stripped
                // the type arguments. The resulting `super(args)` call may have the
                // wrong arity because the type-arg stripping changed the resolved
                // constructor shape. TSC's checker handles TS2754 itself and
                // short-circuits before argument checking.
                let suppress_for_super_parse_error =
                    is_super_call && self.node_span_contains_parse_error(call_idx);
                if !self.ctx.has_parse_errors && !suppress_for_super_parse_error {
                    if actual < expected_min {
                        let is_iife = self.is_callee_function_expression(callee_expr);
                        if is_iife {
                            return TypeId::ERROR;
                        }
                    }

                    let has_non_tuple_spread = self.call_has_indeterminate_length_spread(args);
                    if has_non_tuple_spread {
                        // TS2556 was already emitted; don't cascade with TS2555/TS2554.
                    } else if actual < expected_min && expected_max.is_none() {
                        self.error_expected_at_least_arguments_at(expected_min, actual, call_idx);
                    } else {
                        let max = expected_max.unwrap_or(expected_min);
                        let expanded_args = self.build_expanded_args_for_error(args);
                        let args_for_error = if expanded_args.len() > args.len() {
                            &expanded_args
                        } else {
                            args
                        };
                        self.error_argument_count_mismatch_at(
                            expected_min,
                            max,
                            actual,
                            call_idx,
                            args_for_error,
                        );
                    }
                }
                if is_super_call {
                    TypeId::VOID
                } else if let Some(return_type) =
                    self.stable_call_recovery_return_type_with_default_type_args(callee_type)
                {
                    self.finalize_call_return_like_success(
                        callee_expr,
                        call_idx,
                        callee_type,
                        arg_types,
                        return_type,
                        is_optional_chain,
                    )
                } else {
                    TypeId::ERROR
                }
            }
            CallResult::OverloadArgumentCountMismatch {
                actual,
                expected_low,
                expected_high,
            } => {
                if !self.ctx.has_parse_errors {
                    self.error_at_node(
                        call_idx,
                        &format!(
                            "No overload expects {actual} arguments, but overloads do exist that expect either {expected_low} or {expected_high} arguments."
                        ),
                        diagnostic_codes::NO_OVERLOAD_EXPECTS_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_OR_ARGUM,
                    );
                }
                TypeId::ERROR
            }
            CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
                fallback_return,
            } => {
                if actual == TypeId::ERROR
                    || actual == TypeId::UNKNOWN
                    || expected == TypeId::ERROR
                    || expected == TypeId::UNKNOWN
                {
                    return TypeId::ERROR;
                }
                let arg_idx = self.map_expanded_arg_index_to_original(args, index);
                let arg_idx = arg_idx.map(|i| self.ctx.arena.skip_parenthesized(i));
                if self
                    .this_argument_satisfies_polymorphic_this_rest_target(arg_idx, actual, expected)
                {
                    return fallback_return;
                }
                if expected == TypeId::NEVER
                    && let Some(return_type) =
                        self.correlated_union_call_recovery_return(callee_type, index, actual)
                {
                    return if fallback_return != TypeId::ERROR {
                        fallback_return
                    } else {
                        return_type
                    };
                }
                let mismatch_is_spread_arg = arg_idx.is_some_and(|arg_idx| {
                    self.ctx
                        .arena
                        .get(arg_idx)
                        .is_some_and(|node| node.kind == syntax_kind_ext::SPREAD_ELEMENT)
                });
                if mismatch_is_spread_arg {
                    let normalized_rest_expected =
                        self.rest_argument_element_type_with_env(expected);
                    if normalized_rest_expected != expected
                        && self
                            .call_arg_relation_outcome_with_env(actual, normalized_rest_expected)
                            .related
                    {
                        return if fallback_return != TypeId::ERROR {
                            fallback_return
                        } else {
                            TypeId::ERROR
                        };
                    }
                }
                let aggregate_literal_actual = if expr_ops::contains_application_unknown_arg(
                    self.ctx.types.as_type_database(),
                    expected,
                ) {
                    None
                } else {
                    self.literalized_aggregate_actual_for_call_args(args, index, actual, expected)
                };
                let original_is_spread_marker = arg_types.get(index).is_some_and(|&ty| {
                    common::is_spread_marker_tuple(self.ctx.types.as_type_database(), ty)
                });
                let aggregate_rest_mismatch = (common::tuple_elements(self.ctx.types, actual)
                    .is_some()
                    || original_is_spread_marker)
                    && arg_types
                        .get(index)
                        .copied()
                        .is_none_or(|original| original != actual);
                let mut reported_actual = match arg_types.get(index).copied() {
                    Some(TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) | None => actual,
                    Some(original) if self.is_spread_argument_marker_type(original) => actual,
                    Some(original)
                        if original != actual
                            && common::tuple_elements(self.ctx.types, actual).is_some() =>
                    {
                        aggregate_literal_actual.unwrap_or(actual)
                    }
                    Some(original) => original,
                };
                let aggregate_anchor_override = if aggregate_rest_mismatch {
                    self.declared_rest_parameter_index_for_call(callee_expr)
                        .and_then(|rest_index| {
                            self.aggregate_actual_after_declared_rest_start(
                                reported_actual,
                                index,
                                rest_index,
                            )
                            .map(|adjusted| {
                                reported_actual = adjusted;
                                args.get(rest_index).copied().unwrap_or(call_idx)
                            })
                        })
                } else {
                    None
                };
                let polymorphic_this_expected = self.polymorphic_this_indexed_conditional_target(
                    callee_type,
                    args,
                    arg_types,
                    index,
                );
                // Preserve the parameter's type-parameter display (and emit the
                // bare TS2345 head that keeps the written `T`/`T[]` name) only
                // when the target genuinely carries a FREE type parameter — one
                // unbound in the current context. A concrete target that merely
                // contains a *bound* signature type parameter (a generic method
                // `m<S>(x: S): S` or a generic call/construct signature) is a
                // fully-formed structural type: tsc still promotes the missing
                // required-property failure to TS2739/TS2740/TS2741 there. The
                // broad `contains_type_parameters` walk descends into signature
                // bodies and counts that bound `S` (an index signature on the
                // interface leaves it as a raw `TypeParameter` rather than a
                // canonical `BoundParameter`), which wrongly suppressed the
                // promotion; `contains_free_type_parameters` skips generic
                // signature bodies and answers the question we actually mean.
                let preserve_type_parameter_expected_display =
                    common::contains_free_type_parameters(self.ctx.types, expected);
                let reported_expected = if let Some(expected) = polymorphic_this_expected {
                    expected
                } else if common::contains_this_type(self.ctx.types, expected) {
                    expected
                } else {
                    let reported_expected = self
                        .generic_callable_mismatch_display_target(actual, expected)
                        .unwrap_or(expected);
                    self.preferred_literal_expected_for_mismatch(
                        callee_has_declared_generic_signature,
                        raw_callee_shape,
                        arg_types,
                        args,
                        reported_actual,
                        index,
                        reported_expected,
                    )
                };
                let mut elaborated = false;
                let should_try_deferred_elaboration = self
                    .should_attempt_deferred_literal_elaboration(expected)
                    || arg_idx
                        .is_some_and(|arg_idx| self.argument_supports_literal_elaboration(arg_idx));
                if let Some(arg_idx) = arg_idx {
                    self.suppress_later_call_excess_property_diagnostics(args, arg_idx);
                    let arg_is_object_literal = self.ctx.arena.get(arg_idx).is_some_and(|node| {
                        node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    });
                    let evaluated_expected = self.evaluate_type_with_env(expected);
                    if arg_is_object_literal
                        && (common::type_is_conditional_type_result_with_unresolved_inference(
                            self.ctx.types,
                            expected,
                        ) || common::type_is_conditional_type_result_with_unresolved_inference(
                            self.ctx.types,
                            evaluated_expected,
                        ))
                    {
                        return if fallback_return != TypeId::ERROR {
                            fallback_return
                        } else {
                            TypeId::ERROR
                        };
                    }
                    // When a callback has a block body, TSC reports TS2345 at the
                    // argument level rather than elaborating with an inner TS2322
                    // on return statements. Compute this BEFORE the elaboration
                    // call so we can skip callback return elaboration entirely.
                    let prefer_argument_level_return_mismatch =
                        self.callback_prefers_argument_level_return_mismatch(arg_idx);
                    let suppress_inner_elaboration =
                        self.callback_has_explicit_param_type_conflict(arg_idx, expected);
                    // Skip elaboration when the original parameter type was a type parameter
                    // (excess properties are allowed for generic calls with type param targets).
                    let skip_for_generic = self
                        .ctx
                        .generic_excess_skip
                        .as_ref()
                        .is_some_and(|skip| index < skip.len() && skip[index]);
                    if should_try_deferred_elaboration
                        && !prefer_argument_level_return_mismatch
                        && !skip_for_generic
                        && !self.should_suppress_weak_key_arg_mismatch(
                            callee_expr,
                            args,
                            index,
                            actual,
                        )
                    {
                        elaborated = self.try_elaborate_object_literal_arg_error_with_source(
                            arg_idx,
                            expected,
                            Some(actual),
                        );
                    }
                    // When a callback has explicitly-typed parameters that conflict with the
                    // expected parameter types, TSC reports TS2345 at the argument level
                    // rather than elaborating with an inner TS2322. Only suppress inner
                    // elaboration when the *parameter* types are the source of the mismatch.
                    if !elaborated
                        && !suppress_inner_elaboration
                        && !prefer_argument_level_return_mismatch
                        && self
                            .callback_body_spans(arg_idx)
                            .iter()
                            .any(|(start, end)| {
                                self.has_diagnostic_code_within_span(
                                    *start,
                                    *end,
                                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                                )
                            })
                    {
                        elaborated = true;
                    }
                    // Check stored return-type errors that were pruned by the
                    // arg collection filter. If found, restore the diagnostic
                    // and suppress the outer TS2345.
                    if !elaborated
                        && !suppress_inner_elaboration
                        && !prefer_argument_level_return_mismatch
                    {
                        let stored: Vec<_> = self
                            .ctx
                            .callback_return_type_errors
                            .iter()
                            .filter(|d| {
                                self.callback_body_spans(arg_idx).iter().any(
                                    |(body_start, body_end)| {
                                        d.start >= *body_start && d.start < *body_end
                                    },
                                )
                            })
                            .cloned()
                            .collect();
                        if !stored.is_empty() {
                            self.ctx.diagnostics.extend(stored);
                            elaborated = true;
                        }
                    }
                    // When suppressing inner elaboration, remove any TS2322 inside the
                    // callback body that was left from the arg collection pass, so the
                    // outer TS2345 is the only diagnostic at the argument site.
                    if suppress_inner_elaboration || prefer_argument_level_return_mismatch {
                        let body_spans = self.callback_body_spans(arg_idx);
                        let arg_span = self.callback_argument_span(arg_idx);
                        self.ctx.diagnostics.retain(|d| {
                            !(d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                                && (body_spans.iter().any(|(body_start, body_end)| {
                                    d.start >= *body_start && d.start < *body_end
                                }) || (prefer_argument_level_return_mismatch
                                    && arg_span.is_some_and(|(arg_start, arg_end)| {
                                        d.start >= arg_start && d.start < arg_end
                                    }))))
                        });
                        self.ctx.rebuild_emitted_diagnostics_from_current();
                    }
                    if !elaborated
                        && !suppress_inner_elaboration
                        && allow_contextual_mismatch_deferral
                        && self.should_defer_contextual_argument_mismatch(actual, expected)
                    {
                        return if fallback_return != TypeId::ERROR {
                            fallback_return
                        } else {
                            TypeId::ERROR
                        };
                    }
                    let suppress_weak = self.should_suppress_weak_key_arg_mismatch(
                        callee_expr,
                        args,
                        index,
                        actual,
                    );
                    let suppress_cascading_constraint_mismatch = self
                        .callable_mismatch_cascades_from_constraint_diagnostic(
                            reported_actual,
                            reported_expected,
                        );
                    let resolved_reported_actual = self.resolve_lazy_type(reported_actual);
                    let evaluated_reported_expected =
                        self.evaluate_type_with_env(reported_expected);
                    let suppress_correlated_index_access_never_mismatch = (reported_expected
                        == TypeId::NEVER
                        || evaluated_reported_expected == TypeId::NEVER)
                        && common::index_access_parts(self.ctx.types, reported_actual)
                            .or_else(|| {
                                common::index_access_parts(self.ctx.types, resolved_reported_actual)
                            })
                            .is_some_and(|(_, index)| {
                                common::contains_type_parameters(self.ctx.types, index)
                                    || common::is_type_parameter_like(self.ctx.types, index)
                                    || common::type_param_info(self.ctx.types, index).is_some()
                            });
                    if !suppress_weak
                        && !elaborated
                        && !suppress_cascading_constraint_mismatch
                        && !suppress_correlated_index_access_never_mismatch
                    {
                        let spread_rest_tuple_display = (!aggregate_rest_mismatch)
                            .then(|| {
                                self.spread_rest_tuple_diagnostic_types(arg_idx, reported_expected)
                            })
                            .flatten();
                        if let Some(polymorphic_this_expected) = polymorphic_this_expected {
                            self.error_argument_not_assignable_preserving_param_display(
                                reported_actual,
                                polymorphic_this_expected,
                                arg_idx,
                            );
                        } else if let Some((spread_actual, spread_expected)) =
                            spread_rest_tuple_display
                        {
                            self.error_argument_not_assignable_at(
                                spread_actual,
                                spread_expected,
                                arg_idx,
                            );
                        } else if aggregate_rest_mismatch {
                            self.error_argument_not_assignable_structural_tuple_at(
                                reported_actual,
                                reported_expected,
                                aggregate_anchor_override.unwrap_or(arg_idx),
                            );
                        } else if prefer_argument_level_return_mismatch {
                            self.error_argument_not_assignable_at(
                                reported_actual,
                                reported_expected,
                                arg_idx,
                            );
                        } else if preserve_type_parameter_expected_display {
                            self.error_argument_not_assignable_preserving_param_display(
                                reported_actual,
                                reported_expected,
                                arg_idx,
                            );
                        } else {
                            let _ = self.check_argument_assignable_or_report(
                                reported_actual,
                                reported_expected,
                                arg_idx,
                            );
                        }
                    }
                } else if index >= arg_types.len() {
                    if should_try_deferred_elaboration
                        && !self.should_suppress_weak_key_arg_mismatch(
                            callee_expr,
                            args,
                            index,
                            actual,
                        )
                        && let Some(last_arg) = args.last().copied()
                    {
                        elaborated = self.try_elaborate_object_literal_arg_error_with_source(
                            last_arg,
                            expected,
                            Some(actual),
                        );
                    }
                    if !elaborated
                        && allow_contextual_mismatch_deferral
                        && self.should_defer_contextual_argument_mismatch(actual, expected)
                    {
                        return if fallback_return != TypeId::ERROR {
                            fallback_return
                        } else {
                            TypeId::ERROR
                        };
                    }
                    if !self.should_suppress_weak_key_arg_mismatch(callee_expr, args, index, actual)
                        && !elaborated
                    {
                        if aggregate_rest_mismatch {
                            self.error_argument_not_assignable_structural_tuple_at(
                                reported_actual,
                                reported_expected,
                                aggregate_anchor_override.unwrap_or(call_idx),
                            );
                        } else {
                            let _ = self.check_argument_assignable_or_report(
                                reported_actual,
                                reported_expected,
                                call_idx,
                            );
                        }
                    }
                } else if !args.is_empty() {
                    let last_arg = args[args.len() - 1];
                    if should_try_deferred_elaboration
                        && !self.should_suppress_weak_key_arg_mismatch(
                            callee_expr,
                            args,
                            index,
                            actual,
                        )
                    {
                        elaborated = self.try_elaborate_object_literal_arg_error_with_source(
                            last_arg,
                            expected,
                            Some(actual),
                        );
                    }
                    if !elaborated
                        && allow_contextual_mismatch_deferral
                        && self.should_defer_contextual_argument_mismatch(actual, expected)
                    {
                        return if fallback_return != TypeId::ERROR {
                            fallback_return
                        } else {
                            TypeId::ERROR
                        };
                    }
                    if !self.should_suppress_weak_key_arg_mismatch(callee_expr, args, index, actual)
                        && !elaborated
                    {
                        if aggregate_rest_mismatch {
                            self.error_argument_not_assignable_structural_tuple_at(
                                reported_actual,
                                reported_expected,
                                aggregate_anchor_override.unwrap_or(last_arg),
                            );
                        } else {
                            let _ = self.check_argument_assignable_or_report(
                                reported_actual,
                                reported_expected,
                                last_arg,
                            );
                        }
                    }
                } else {
                    if allow_contextual_mismatch_deferral
                        && self.should_defer_contextual_argument_mismatch(actual, expected)
                    {
                        return if fallback_return != TypeId::ERROR {
                            fallback_return
                        } else {
                            TypeId::ERROR
                        };
                    }
                    if aggregate_rest_mismatch {
                        self.error_argument_not_assignable_structural_tuple_at(
                            reported_actual,
                            reported_expected,
                            aggregate_anchor_override.unwrap_or(call_idx),
                        );
                    } else {
                        let _ = self.check_argument_assignable_or_report(
                            reported_actual,
                            reported_expected,
                            call_idx,
                        );
                    }
                }

                if self.is_generic_callable_against_nongeneric_target(actual, expected) {
                    TypeId::UNKNOWN
                } else if fallback_return != TypeId::ERROR {
                    fallback_return
                } else if let Some(return_type) =
                    crate::query_boundaries::assignability::get_function_return_type(
                        self.ctx.types,
                        callee_type,
                    )
                {
                    self.apply_this_substitution_to_call_return(return_type, callee_expr)
                } else {
                    TypeId::ERROR
                }
            }
            CallResult::NoOverloadMatch {
                failures,
                fallback_return,
                ..
            } => {
                self.ctx.no_overload_call_nodes.insert(call_idx.0);
                let has_error_surface = callee_type == TypeId::ERROR
                    || args
                        .iter()
                        .copied()
                        .any(|arg_idx| self.get_type_of_node(arg_idx) == TypeId::ERROR);
                if has_error_surface {
                    return TypeId::ERROR;
                }

                // A genuine no-overload-match always reports TS2769, matching tsc,
                // even when the callee's class/interface/namespace carries its own
                // structural error (TS2420/TS2430/TS2694). tsc treats the structural
                // error and the call-site overload failure as independent and emits
                // both; error-typed callees/arguments are already short-circuited
                // above via `has_error_surface`.
                let suppress_due_to_callback_body_errors =
                    self.should_suppress_no_overload_due_to_callback_body_errors(args);

                let should_emit_no_overload_error = !suppress_due_to_callback_body_errors
                    && !self.should_suppress_weak_key_no_overload(callee_expr, args);

                if should_emit_no_overload_error {
                    self.error_no_overload_matches_at(call_idx, &failures);
                }
                // `fallback_return` is the intersection of the candidate
                // signatures' return types (see `overload_failure_return_type`):
                // it suppresses spurious cascades after TS2769 yet keeps real
                // downstream errors, matching tsc.
                fallback_return
            }
            CallResult::ThisTypeMismatch {
                expected_this,
                actual_this,
                emit_not_callable,
            } => {
                if emit_not_callable {
                    self.error_not_callable_at(callee_type, callee_expr);
                }
                self.error_call_this_type_mismatch_at(
                    expected_this,
                    actual_this,
                    call_idx,
                    callee_expr,
                );
                TypeId::ERROR
            }
        }
    }
}
