//! Round-2 argument contextual typing helpers for generic call inference.

use super::*;

impl<'a> CheckerState<'a> {
    pub(crate) fn recheck_generic_call_arguments_with_real_types(
        &mut self,
        result: CallResult,
        instantiated_params: &[tsz_solver::ParamInfo],
        args: &[NodeIndex],
        arg_types: &[TypeId],
    ) -> CallResult {
        let success_return = if let CallResult::Success(return_type) = result {
            return_type
        } else {
            return result;
        };
        let expected_signature = (!instantiated_params.is_empty()).then(|| {
            self.ctx.types.factory().function(FunctionShape::new(
                instantiated_params.to_vec(),
                TypeId::UNKNOWN,
            ))
        });
        for (index, &cached_actual) in arg_types.iter().enumerate() {
            // Skip spread marker tuples [...T] created by the checker for generic
            // TypeParameter spreads. The solver already validated these against the
            // full rest parameter type; re-checking here would incorrectly compare
            // the spread marker against the rest element type (e.g., [...U] vs
            // `string | number | boolean` instead of `(string | number | boolean)[]`).
            if crate::query_boundaries::common::is_spread_marker_tuple(
                self.ctx.types.as_type_database(),
                cached_actual,
            ) {
                continue;
            }

            let expected = expected_signature.and_then(|signature| {
                self.contextual_parameter_type_for_call_with_env_from_expected(
                    signature,
                    index,
                    arg_types.len(),
                )
            });

            let Some(mut expected) = expected else {
                break;
            };

            // When the expected type has readonly members (from const type parameter
            // inference), skip the recheck for this argument. The argument was already
            // validated against the const-inferred type during the solver's
            // resolve_generic_call. Re-checking here would re-compute the argument
            // type without in_const_assertion, producing a mutable type that fails
            // assignability against the readonly expected type.
            if crate::query_boundaries::common::type_has_readonly_members(
                self.ctx.types.as_type_database(),
                expected,
            ) {
                continue;
            }

            // Skip rechecking arguments whose expected type still contains inference
            // placeholders. In those cases, the call solver's earlier check already
            // used the concrete placeholder-driven relationships, and re-checking with
            // concrete assignability tends to produce false positives (for example,
            // constraint signatures with `infer` branches).
            if crate::query_boundaries::common::contains_infer_types(self.ctx.types, expected) {
                continue;
            }

            // When the argument is a variadic tuple spread marker `[...U]`
            // (created by the call checker for generic type parameter spreads),
            // unwrap the marker to get U and compare it against the full rest
            // parameter array type.  The marker is synthetic — tsc never
            // produces this comparison — so we must undo the wrapping here.
            let spread_inner = common::tuple_elements(self.ctx.types, cached_actual)
                .filter(|elems| elems.len() == 1 && elems[0].rest)
                .map(|elems| elems[0].type_id);
            if let Some(inner_type) = spread_inner
                && let Some(param) = instantiated_params.get(index).or_else(|| {
                    let last = instantiated_params.last()?;
                    last.rest.then_some(last)
                })
                && param.rest
            {
                let rest_array_type = self.evaluate_type_with_env(param.type_id);
                let is_assignable = self.is_assignable_to_with_env(inner_type, rest_array_type);
                if is_assignable {
                    continue;
                }
                // If not directly assignable, fall through to normal check
                expected = rest_array_type;
            }

            let arg_idx = args.get(index).copied();
            let skip_unresolved_callable_recheck = arg_idx.is_some_and(|arg_idx| {
                self.is_callback_like_argument(arg_idx)
                    && (common::contains_type_parameters(self.ctx.types, expected)
                        || common::contains_infer_types(self.ctx.types, expected))
                    && crate::query_boundaries::checkers::call::get_contextual_signature(
                        self.ctx.types,
                        expected,
                    )
                    .or_else(|| {
                        let evaluated = self.evaluate_type_with_env(expected);
                        crate::query_boundaries::checkers::call::get_contextual_signature(
                            self.ctx.types,
                            evaluated,
                        )
                    })
                    .is_some()
                    && crate::query_boundaries::checkers::call::get_contextual_signature(
                        self.ctx.types,
                        cached_actual,
                    )
                    .or_else(|| {
                        let evaluated = self.evaluate_type_with_env(cached_actual);
                        crate::query_boundaries::checkers::call::get_contextual_signature(
                            self.ctx.types,
                            evaluated,
                        )
                    })
                    .is_some()
            });
            if skip_unresolved_callable_recheck {
                continue;
            }

            let object_literal_function_param_spans = arg_idx
                .filter(|&arg_idx| {
                    self.ctx.arena.get(arg_idx).is_some_and(|node| {
                        node.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    })
                })
                .map(|arg_idx| self.object_literal_function_like_param_spans(arg_idx))
                .unwrap_or_default();
            let refresh_snap = self.ctx.snapshot_diagnostics();
            let actual = args
                .get(index)
                .copied()
                .map(|arg_idx| {
                    self.refreshed_generic_call_arg_type_with_context(
                        arg_idx,
                        cached_actual,
                        Some(expected),
                    )
                })
                .unwrap_or(cached_actual);
            let actual = arg_idx
                .map(|arg_idx| self.sanitize_generic_inference_arg_type(arg_idx, actual))
                .unwrap_or(actual);
            let refreshed_object_literal_param_has_implicit_any = !object_literal_function_param_spans
                .is_empty()
                && self.ctx.speculative_diagnostics_since(&refresh_snap).iter().any(|diag| {
                    matches!(
                        diag.code,
                        crate::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                            | crate::diagnostics::diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE
                            | crate::diagnostics::diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
                            | crate::diagnostics::diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN
                    ) && object_literal_function_param_spans
                        .iter()
                        .any(|(start, end)| diag.start >= *start && diag.start < *end)
                });
            let expected_is_concrete = expected != TypeId::UNKNOWN
                && expected != TypeId::ERROR
                && !common::contains_infer_types(self.ctx.types, expected)
                && !common::contains_type_parameters(self.ctx.types, expected);
            if expected_is_concrete && !refreshed_object_literal_param_has_implicit_any {
                self.ctx.diagnostics.retain(|diag| {
                    !matches!(
                        diag.code,
                        crate::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                            | crate::diagnostics::diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE
                            | crate::diagnostics::diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
                            | crate::diagnostics::diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN
                    ) || !object_literal_function_param_spans
                        .iter()
                        .any(|(start, end)| diag.start >= *start && diag.start < *end)
                });
            }

            // Skip spread marker tuples [...T] created by the checker for generic
            // TypeParameter spreads. These are already validated by the solver's
            // check_argument_types_with which has spread-aware logic. The recheck
            // here compares against the element type of the rest param, which would
            // incorrectly reject `[...U]` against `ElementType`.
            if is_spread_marker_tuple(self.ctx.types, actual) {
                continue;
            }

            let actual_is_object_literal = arg_idx.is_some_and(|arg_idx| {
                self.ctx.arena.get(arg_idx).is_some_and(|node| {
                    node.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                })
            });
            if actual_is_object_literal
                && common::type_is_conditional_type_result_with_unresolved_inference(
                    self.ctx.types,
                    expected,
                )
            {
                continue;
            }

            let is_assignable = self.is_assignable_to_with_env(actual, expected)
                || self.is_assignable_via_contextual_signatures(actual, expected);

            if !is_assignable {
                return CallResult::ArgumentTypeMismatch {
                    index,
                    expected,
                    actual,
                    fallback_return: success_return,
                };
            }
        }

        CallResult::Success(success_return)
    }

    pub(crate) fn compute_round2_contextual_types(
        &mut self,
        shape: &tsz_solver::FunctionShape,
        round1_instantiated_params: Option<&[tsz_solver::ParamInfo]>,
        sensitive_args: &[bool],
        current_substitution: &crate::query_boundaries::common::TypeSubstitution,
        arg_count: usize,
    ) -> Vec<Option<TypeId>> {
        let resolved_round1_instantiated_params = round1_instantiated_params
            .map(|params| self.resolve_signature_parameter_type_queries(&shape.params, params));
        let round1_instantiated_params = resolved_round1_instantiated_params
            .as_deref()
            .or(round1_instantiated_params);
        let mut round2_contextual_types: Vec<Option<TypeId>> = Vec::with_capacity(arg_count);
        for i in 0..arg_count {
            let shape_round2_param =
                shape
                    .params
                    .get(i)
                    .map(|p| (p.type_id, p.rest))
                    .or_else(|| {
                        let last = shape.params.last()?;
                        last.rest.then_some((last.type_id, true))
                    });
            let round2_param = round1_instantiated_params
                .and_then(|params| {
                    params.get(i).map(|p| (p.type_id, p.rest)).or_else(|| {
                        let last = params.last()?;
                        last.rest.then_some((last.type_id, true))
                    })
                })
                .or(shape_round2_param);
            let is_sensitive = i < sensitive_args.len() && sensitive_args[i];
            let round2_param = if is_sensitive {
                match (shape_round2_param, round2_param) {
                    (Some(shape_param), Some(instantiated_param)) => {
                        let shape_is_genericish =
                            common::contains_infer_types(self.ctx.types, shape_param.0)
                                || common::contains_type_parameters(self.ctx.types, shape_param.0)
                                || !common::collect_type_queries(self.ctx.types, shape_param.0)
                                    .is_empty();
                        let instantiated_is_concrete =
                            !common::contains_infer_types(self.ctx.types, instantiated_param.0)
                                && !common::contains_type_parameters(
                                    self.ctx.types,
                                    instantiated_param.0,
                                );
                        if shape_is_genericish && instantiated_is_concrete {
                            Some(instantiated_param)
                        } else {
                            Some(shape_param)
                        }
                    }
                    (shape_param, None) => shape_param,
                    (None, instantiated_param) => instantiated_param,
                }
            } else {
                round2_param
            };
            let ctx_type = if let Some((param_type, is_rest_param)) = round2_param {
                let contextual_substitution = if is_sensitive {
                    self.fill_unresolved_contextual_substitution_from_constraints(
                        shape,
                        current_substitution,
                    )
                } else {
                    current_substitution.clone()
                };
                let fresh_instantiated_from_shape =
                    shape_round2_param.map(|(shape_param_type, _)| {
                        crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            shape_param_type,
                            &contextual_substitution,
                        )
                    });
                let round1_has_unknown =
                    common::contains_type_by_id(self.ctx.types, param_type, TypeId::UNKNOWN);
                let round1_has_error =
                    common::contains_type_by_id(self.ctx.types, param_type, TypeId::ERROR);
                // For sensitive (callback) args, we normally re-instantiate from the
                // shape to pick up contextual-return-type inference. However, if
                // the substitution only contains widened primitives (string, number,
                // etc.), the solver likely widened literal inferences (K = "a" → string).
                // In that case the round1 result, which preserves literals, is more
                // specific and should be preferred.
                let sensitive_needs_fresh = is_sensitive
                    && (round1_has_unknown
                        || round1_has_error
                        || current_substitution.map().values().any(|&v| {
                            v != TypeId::STRING
                                && v != TypeId::NUMBER
                                && v != TypeId::BOOLEAN
                                && v != TypeId::BIGINT
                                && v != TypeId::UNKNOWN
                                && v != TypeId::ERROR
                        }));
                let prefer_fresh_instantiation = sensitive_needs_fresh
                    || round1_has_error
                    || common::contains_infer_types(self.ctx.types, param_type)
                    || common::contains_type_parameters(self.ctx.types, param_type)
                    || fresh_instantiated_from_shape.is_some_and(|fresh| {
                        (round1_has_unknown || round1_has_error)
                            && (common::contains_infer_types(self.ctx.types, fresh)
                                || common::contains_type_parameters(self.ctx.types, fresh))
                    });
                let instantiated = if round1_instantiated_params.is_some()
                    && !prefer_fresh_instantiation
                {
                    let original_param = shape_round2_param.map(|(type_id, _)| type_id);
                    if let Some(orig) = original_param
                        && let Some(tp_info) = common::type_param_info(self.ctx.types, orig)
                        && self
                            .unresolved_contextual_substitution_target(
                                &tp_info,
                                &contextual_substitution,
                            )
                            .is_some()
                    {
                        let instantiated_constraint = match self
                            .instantiate_contextual_constraint_without_unresolved_self(
                                orig,
                                &tp_info,
                                &contextual_substitution,
                            ) {
                            Some(instantiated_constraint) => instantiated_constraint,
                            None => param_type,
                        };
                        let evaluated_constraint =
                            self.evaluate_type_with_env(instantiated_constraint);
                        if !common::contains_type_parameters(self.ctx.types, evaluated_constraint) {
                            let constraint_specificity =
                                callable_param_specificity(self.ctx.types, evaluated_constraint);
                            let round1_specificity =
                                callable_param_specificity(self.ctx.types, param_type);
                            if constraint_specificity >= round1_specificity {
                                evaluated_constraint
                            } else {
                                param_type
                            }
                        } else {
                            param_type
                        }
                    } else {
                        param_type
                    }
                } else {
                    let base_param_type = if prefer_fresh_instantiation {
                        shape_round2_param
                            .map(|(type_id, _)| type_id)
                            .unwrap_or(param_type)
                    } else {
                        param_type
                    };
                    let inst = if let Some(tp_info) =
                        common::type_param_info(self.ctx.types, base_param_type)
                    {
                        if self
                            .unresolved_contextual_substitution_target(
                                &tp_info,
                                &contextual_substitution,
                            )
                            .is_some()
                        {
                            self.instantiate_contextual_constraint_without_unresolved_self(
                                base_param_type,
                                &tp_info,
                                &contextual_substitution,
                            )
                            .unwrap_or_else(|| {
                                crate::query_boundaries::common::instantiate_type(
                                    self.ctx.types,
                                    base_param_type,
                                    &contextual_substitution,
                                )
                            })
                        } else {
                            crate::query_boundaries::common::instantiate_type(
                                self.ctx.types,
                                base_param_type,
                                &contextual_substitution,
                            )
                        }
                    } else {
                        crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            base_param_type,
                            &contextual_substitution,
                        )
                    };
                    if let Some(tp_info) = common::type_param_info(self.ctx.types, inst) {
                        let instantiated_constraint = match self
                            .instantiate_contextual_constraint_without_unresolved_self(
                                inst,
                                &tp_info,
                                &contextual_substitution,
                            ) {
                            Some(instantiated_constraint) => instantiated_constraint,
                            None => inst,
                        };
                        let evaluated = self.evaluate_type_with_env(instantiated_constraint);
                        if !common::contains_type_parameters(self.ctx.types, evaluated) {
                            evaluated
                        } else {
                            inst
                        }
                    } else {
                        inst
                    }
                };
                let preserve_application_shape =
                    should_preserve_contextual_application_shape(self.ctx.types, instantiated);
                let evaluated = if common::contains_type_parameters(self.ctx.types, instantiated)
                    || common::contains_infer_types(self.ctx.types, instantiated)
                    || preserve_application_shape
                {
                    instantiated
                } else {
                    self.evaluate_type_with_env(instantiated)
                };
                let evaluated = if is_sensitive
                    && (common::contains_type_parameters(self.ctx.types, evaluated)
                        || common::contains_infer_types(self.ctx.types, evaluated))
                {
                    crate::query_boundaries::inference::instantiate_remaining_contextual_type_params(
                        self.ctx.types,
                        evaluated,
                        &shape.type_params,
                        &contextual_substitution,
                    )
                } else {
                    evaluated
                };
                Some(if is_rest_param {
                    self.rest_argument_element_type_with_env(evaluated)
                } else {
                    evaluated
                })
            } else {
                None
            };
            round2_contextual_types.push(ctx_type);
        }
        round2_contextual_types
    }

    pub(crate) fn compute_single_call_argument_type(
        &mut self,
        arg_idx: NodeIndex,
        expected_type: Option<TypeId>,
        check_excess_properties: bool,
        effective_index: usize,
        arg_count: usize,
        suppress_diagnostics: bool,
        callable_ctx: CallableContext,
    ) -> TypeId {
        use tsz_scanner::SyntaxKind;

        let syntax_needs_contextual = {
            let Some(node) = self.ctx.arena.get(arg_idx) else {
                return TypeId::ERROR;
            };
            let is_literal = matches!(
                node.kind,
                k if k == SyntaxKind::StringLiteral as u16
                    || k == SyntaxKind::NumericLiteral as u16
                    || k == SyntaxKind::BigIntLiteral as u16
                    || k == SyntaxKind::TrueKeyword as u16
                    || k == SyntaxKind::FalseKeyword as u16
                    || k == SyntaxKind::NullKeyword as u16
                    || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            );
            if is_literal {
                true
            } else {
                matches!(
                    node.kind,
                    k if k == syntax_kind_ext::ARROW_FUNCTION
                        || k == syntax_kind_ext::FUNCTION_EXPRESSION
                        || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                        || k == syntax_kind_ext::CONDITIONAL_EXPRESSION
                        || k == syntax_kind_ext::CALL_EXPRESSION
                        || k == syntax_kind_ext::NEW_EXPRESSION
                        || k == syntax_kind_ext::YIELD_EXPRESSION
                        || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                )
            }
        };
        let expected_is_unresolved = expected_type.is_some_and(|expected| {
            expected == TypeId::UNKNOWN
                || expected == TypeId::ERROR
                || common::contains_infer_types(self.ctx.types, expected)
        });
        let expected_context_type =
            self.contextual_type_option_for_call_argument(expected_type, arg_idx, callable_ctx);
        let expected_context_is_generic_callable =
            expected_context_type.or(expected_type).is_some_and(|ty| {
                let evaluated = self.evaluate_type_with_env(ty);
                call_checker::get_contextual_signature(self.ctx.types, ty)
                    .or_else(|| call_checker::get_contextual_signature(self.ctx.types, evaluated))
                    .is_some_and(|shape| !shape.type_params.is_empty())
            });
        let skip_generic_callable_context_for_annotated_generic_function =
            expected_context_is_generic_callable
                && self.explicit_generic_function_has_fully_annotated_signature(arg_idx);
        let needs_contextual_signature_instantiation =
            self.expression_needs_contextual_signature_instantiation(arg_idx, expected_type);
        let apply_contextual = (syntax_needs_contextual
            || needs_contextual_signature_instantiation)
            && !skip_generic_callable_context_for_annotated_generic_function;
        let suppress_unresolved_object_literal_context = self
            .ctx
            .arena
            .get(arg_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
            && expected_context_type.is_some_and(|ty| {
                let evaluated = self.evaluate_type_with_env(ty);
                common::contains_type_parameters(self.ctx.types, ty)
                    || common::contains_infer_types(self.ctx.types, ty)
                    || common::contains_type_parameters(self.ctx.types, evaluated)
                    || common::contains_infer_types(self.ctx.types, evaluated)
            });
        let _concrete_callback_context = expected_context_type.is_some_and(|ty| {
            ty != TypeId::ANY
                && ty != TypeId::UNKNOWN
                && ty != TypeId::ERROR
                && !common::contains_type_parameters(self.ctx.types, ty)
                && !common::contains_infer_types(self.ctx.types, ty)
                && crate::query_boundaries::common::function_shape_for_type(self.ctx.types, ty)
                    .is_some_and(|shape| shape.params.iter().all(|param| !param.rest))
        });
        let raw_context_requires_generic_epc_skip = expected_context_type.is_some_and(|ty| {
            common::contains_type_parameters(self.ctx.types, ty)
                || should_preserve_contextual_application_shape(self.ctx.types, ty)
        });
        let callable_context_requires_generic_epc_skip =
            callable_ctx.callable_type.is_some_and(|callable_type| {
                let ctx =
                    common::ContextualTypeContext::with_expected(self.ctx.types, callable_type);
                ctx.get_parameter_type_for_call(effective_index, arg_count)
                    .is_some_and(|param_type| {
                        common::contains_type_parameters(self.ctx.types, param_type)
                            || should_preserve_contextual_application_shape(
                                self.ctx.types,
                                param_type,
                            )
                    })
            });

        // Extract ThisType<T> marker from the unevaluated expected type BEFORE
        // contextual_type_for_expression evaluates it away. ThisType<T> is an empty
        // interface marker, so intersection simplification removes it. We need to
        // preserve it for object literal methods' `this` type.
        let is_object_literal_arg = self
            .ctx
            .arena
            .get(self.ctx.arena.skip_parenthesized_and_assertions(arg_idx))
            .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION);
        let pushed_this_type = if is_object_literal_arg && let Some(et) = expected_type {
            let ctx_helper = common::ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                et,
                self.ctx.compiler_options.no_implicit_any,
            );
            let _env = self.ctx.type_env.borrow();
            if let Some(this_type) = ctx_helper
                .get_this_type_from_marker()
                .or_else(|| ctx_helper.get_this_type_from_marker())
            {
                self.ctx.this_type_stack.push(this_type);
                true
            } else {
                false
            }
        } else {
            false
        };

        let skip_flow = if apply_contextual {
            false
        } else if let Some(node) = self.ctx.arena.get(arg_idx) {
            if node.kind != SyntaxKind::Identifier as u16 {
                false
            } else if let Some(sym_id) = self
                .ctx
                .binder
                .get_node_symbol(arg_idx)
                .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, arg_idx))
            {
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    let value_decl = symbol.value_declaration;
                    if value_decl.is_none() || !self.is_const_variable_declaration(value_decl) {
                        false
                    } else if let Some(decl_node) = self.ctx.arena.get(value_decl) {
                        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) {
                            if var_decl.type_annotation.is_some() || var_decl.initializer.is_none()
                            {
                                false
                            } else if let Some(init_node) = self.ctx.arena.get(var_decl.initializer)
                            {
                                init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                    || init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        let request = if apply_contextual {
            if suppress_unresolved_object_literal_context {
                TypingRequest::NONE
            } else {
                match expected_context_type {
                    Some(ty) => TypingRequest::with_contextual_type(ty),
                    None => TypingRequest::NONE,
                }
            }
        } else if skip_flow {
            TypingRequest::for_write_context()
        } else {
            TypingRequest::NONE
        };
        if skip_generic_callable_context_for_annotated_generic_function {
            self.invalidate_expression_for_contextual_retry(arg_idx);
            self.clear_contextual_resolution_cache();
        }

        // Snapshot diagnostic + closure state when in speculative round2.
        // Round2 marks closures as "already checked" even when their TS7006 diagnostics are later
        // dropped by the suppress filter. Without restoring, the final retry pass sees these
        // closures as already-checked and skips TS7006 — silencing real implicit-any errors
        // for parameters whose object-literal-property contextual type is never (e.g., an
        // extra key C in a negated-type-like constraint mapped type maps to never).
        let speculation_snap =
            suppress_diagnostics.then(|| DiagnosticSpeculationSnapshot::new(&self.ctx));
        let implicit_any_closure_snapshot =
            suppress_diagnostics.then(|| ImplicitAnyClosureSnapshot::new(&self.ctx));
        let provisional_context_snap =
            (!suppress_diagnostics && apply_contextual && expected_is_unresolved)
                .then(|| DiagnosticSpeculationSnapshot::new(&self.ctx));
        let arg_type = self.get_type_of_node_with_request(arg_idx, &request);

        if check_excess_properties
            && let Some(expected) = expected_type
            && expected != TypeId::ANY
            && expected != TypeId::UNKNOWN
            && let Some(arg_node) = self.ctx.arena.get(arg_idx)
            && arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && !is_type_parameter_type(self.ctx.types, expected)
            && !self
                .ctx
                .generic_excess_skip
                .as_ref()
                .is_some_and(|skip| effective_index < skip.len() && skip[effective_index])
            && !raw_context_requires_generic_epc_skip
            && !callable_context_requires_generic_epc_skip
            && !expected_is_unresolved
        {
            self.check_object_literal_excess_properties(arg_type, expected, arg_idx);
        }

        let arg_node = self.ctx.arena.get(arg_idx);
        let provisional_context_arg_span = arg_node.and_then(|node| {
            let is_context_sensitive_arg = self.is_callback_like_argument(arg_idx)
                || node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION;
            is_context_sensitive_arg.then_some((node.pos, node.end))
        });

        if let Some(snap) = speculation_snap {
            let object_literal_method_param_spans: Vec<(u32, u32)> = arg_node
                .filter(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
                .and_then(|node| self.ctx.arena.get_literal_expr(node))
                .map(|obj| {
                    obj.elements
                        .nodes
                        .iter()
                        .filter_map(|&element_idx| {
                            let element = self.ctx.arena.get(element_idx)?;
                            match element.kind {
                                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                                    .ctx
                                    .arena
                                    .get_method_decl(element)
                                    .map(|method| method.parameters.nodes.as_slice()),
                                k if k == syntax_kind_ext::GET_ACCESSOR
                                    || k == syntax_kind_ext::SET_ACCESSOR =>
                                {
                                    self.ctx
                                        .arena
                                        .get_accessor(element)
                                        .map(|accessor| accessor.parameters.nodes.as_slice())
                                }
                                _ => None,
                            }
                            .map(|params| {
                                params
                                    .iter()
                                    .filter_map(|&param_idx| {
                                        let param_node = self.ctx.arena.get(param_idx)?;
                                        Some((param_node.pos, param_node.end))
                                    })
                                    .collect::<Vec<_>>()
                            })
                        })
                        .flatten()
                        .collect()
                })
                .unwrap_or_default();
            let callback_body_spans: Vec<_> = self
                .callback_body_spans(arg_idx)
                .into_iter()
                .filter(|(start, end)| start < end)
                .collect();
            let callback_param_spans = self.callback_function_param_spans(arg_idx);
            let function_arg_span = self.callback_argument_span(arg_idx);
            let callback_has_block_body = self
                .callback_function_index(arg_idx)
                .and_then(|callback_idx| self.ctx.arena.get(callback_idx))
                .and_then(|callback_node| self.ctx.arena.get_function(callback_node))
                .and_then(|func| self.ctx.arena.get(func.body))
                .is_some_and(|body_node| body_node.kind == syntax_kind_ext::BLOCK);
            let diag_len = snap.checkpoint();
            // Build pre-existing diagnostic keys for exact dedup.
            let existing_diag_keys: Vec<_> = self
                .ctx
                .diagnostics
                .iter()
                .take(diag_len)
                .map(|d| (d.code, d.start, d.length, d.message_text.clone()))
                .collect();
            let mut seen_new_diags = FxHashSet::default();
            let mut seen_diag_keys = existing_diag_keys;
            let types = self.ctx.types;
            snap.rollback_filtered(&mut self.ctx.diagnostic_state(), |diag| {
                if Self::should_preserve_speculative_call_diagnostic(diag) {
                    return true;
                }
                // --- Phase 1: dedup by (code, start) against pre-existing + already-kept ---
                let key = (diag.code, diag.start);
                if !seen_new_diags.insert(key) {
                    return false;
                }
                // Duplicate of a pre-speculation diagnostic — drop.
                if seen_diag_keys.iter().any(|existing| existing.0 == diag.code && existing.1 == diag.start) {
                    return false;
                }
                // --- Phase 2: classify the diagnostic ---
                let is_provisional_implicit_any = matches!(
                    diag.code,
                    diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                        | diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE
                        | diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
                        | diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN
                );
                let is_assignability = diag.code
                    == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                    || diag.code
                        == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE;
                let is_object_literal_diag = arg_node.is_some_and(|node| {
                    node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        && diag.start >= node.pos
                        && diag.start < node.end
                });
                let is_function_arg_implicit_any_diag = is_provisional_implicit_any
                    && callback_param_spans
                        .iter()
                        .any(|(start, end)| diag.start >= *start && diag.start < *end);
                let is_function_arg_diag = function_arg_span
                    .is_some_and(|(start, end)| diag.start >= start && diag.start < end);
                let is_nullish_callback_body_diag = callback_body_spans.iter().any(|(start, _)| {
                    diag.start == *start
                        && matches!(
                            diag.code,
                            diagnostic_codes::IS_POSSIBLY_NULL
                                | diagnostic_codes::IS_POSSIBLY_UNDEFINED
                                | diagnostic_codes::IS_POSSIBLY_NULL_OR_UNDEFINED
                                | diagnostic_codes::OBJECT_IS_POSSIBLY_NULL
                                | diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED
                                | diagnostic_codes::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED
                                | diagnostic_codes::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_NULL
                                | diagnostic_codes::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_UNDEFINED
                                | diagnostic_codes::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_NULL_OR_UNDEFINED
                        )
                });
                let is_direct_callback_body_assignability = is_assignability
                    && callback_body_spans
                        .iter()
                        .any(|(start, end)| diag.start >= *start && diag.start < *end);
                // When the contextual type for a callback argument is fully
                // resolved (no type parameters, no infer types), assignability
                // errors from inside the callback body are real — the later
                // instantiated retry does not re-check the body, so these
                // diagnostics would be lost. Keep them.
                // Note: the diagnostic position may be at the arrow function
                // start rather than inside the body, so we also check for
                // assignability errors anywhere in the function arg when the
                // context is concrete.
                // Check if the expected type or its constraint is concrete.
                // For generic calls like `g6<T extends () => any>(x: T)`, the expected
                // type is `T` (has type params), but the constraint `() => any` is
                // concrete. The contextual callable comes from the constraint, so
                // TS7006 is definitive if the constraint is concrete.
                let has_concrete_expected_type = !expected_is_unresolved
                    && expected_type.is_some_and(|et| {
                        if et == TypeId::UNKNOWN || et == TypeId::ERROR || et == TypeId::ANY {
                            return false;
                        }
                        if !common::contains_type_parameters(types, et) {
                            return true;
                        }
                        // Expected type has type params — check if it's a single type
                        // parameter with a concrete constraint (the contextual callable
                        // source for callback args).
                        let constraint = common::type_parameter_constraint(types, et);
                        constraint.is_some_and(|c| {
                            c != TypeId::UNKNOWN
                                && c != TypeId::ERROR
                                && c != TypeId::ANY
                                && !common::contains_type_parameters(types, c)
                                && !common::contains_infer_types(types, c)
                        })
                    });
                let is_concrete_callback_assignability = is_function_arg_diag
                    && is_assignability
                    && has_concrete_expected_type;
                let is_concrete_expression_body_callback_assignability =
                    is_concrete_callback_assignability && !callback_has_block_body;
                // When the callback's contextual type is fully concrete (no type
                // parameters, no infer types), TS2339 (property does not exist)
                // errors from inside the callback body are definitive — the
                // parameter types are fully resolved and the later instantiated
                // retry will not change them. Keep these diagnostics so that e.g.
                // `make<A,B>(fn: (a:A)=>B): (s:A)=>B` with contextual type
                // `(s:number)=>string` correctly reports TS2339 for
                // `(x) => x.toUpperCase()` when x is inferred as number.
                //
                // Only apply this for expression-body arrows (where the body IS
                // the expression that fails) — block-body callbacks may get
                // stale TS2339 from speculative union-contextual passes that
                // will be refined in the instantiated retry.
                let is_concrete_callback_body_property_error = has_concrete_expected_type
                    && diag.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                    && !matches!(
                        request.contextual_type,
                        Some(ctx_type)
                            if common::contains_type_parameters(types, ctx_type)
                                || common::contains_infer_types(types, ctx_type)
                    )
                    && callback_body_spans
                        .iter()
                        .any(|(start, end)| diag.start >= *start && diag.start < *end);
                if expected_is_unresolved
                    && (is_function_arg_diag
                        || (is_object_literal_diag && is_provisional_implicit_any))
                {
                    return false;
                }
                // Round-2/single-arg recomputes are speculative for direct callback
                // arguments. Keep their diagnostics owned by the final contextual
                // recheck so stale wide-generic errors (for example TS2339 from
                // `ClientEvents[string]`) do not leak past the instantiated retry.
                // The narrow exceptions here are body-owned diagnostics that the
                // later instantiated retry does not recreate: nullish checks like
                // TS18048, direct callback-body assignability like nested JSX
                // TS2322, and assignability errors from callbacks with concrete
                // contextual types.
                // When the contextual type is concrete (no type parameters, no
                // infer types), TS7006 (parameter implicitly has 'any' type) is
                // definitive — if the contextual callable signature has fewer
                // parameters than the callback, there's no later refinement that
                // will provide a type for the excess parameters. Preserve these.
                let is_concrete_callback_implicit_any = is_provisional_implicit_any
                    && has_concrete_expected_type;
                if is_function_arg_diag
                    && !is_nullish_callback_body_diag
                    && !is_direct_callback_body_assignability
                    && !is_concrete_expression_body_callback_assignability
                    && !is_concrete_callback_body_property_error
                    && !is_concrete_callback_implicit_any
                {
                    return false;
                }
                // Keep implicit-any diagnostics (TS7006/TS7019/TS7031) from inside object
                // literals even in round2 speculative passes. Unlike assignability errors
                // (which get a definitive check in resolve_call_with_checker_adapter), TS7006
                // is determined by whether the contextual type is available in THIS pass.
                let implicit_any_in_object_literal =
                    is_provisional_implicit_any && is_object_literal_diag;
                let implicit_any_in_object_literal_method =
                    implicit_any_in_object_literal
                        && object_literal_method_param_spans
                            .iter()
                            .any(|(start, end)| diag.start >= *start && diag.start < *end);
                // TS2345 diagnostics from within object literals come from
                // nested call argument checking, not speculative property
                // assignment. They are definitive and should be preserved.
                let is_nested_call_arg_error = is_object_literal_diag
                    && diag.code
                        == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE;
                // TS18046/TS2571 ("is of type 'unknown'") from within object
                // literal arguments are provisional during speculative passes.
                // During generic inference, callback parameters in nested objects
                // may get their type from an unresolved type parameter constraint
                // (e.g., Record<string, unknown>) that will be replaced by the
                // actual inferred type in the final contextual pass.
                let is_provisional_unknown_in_object_literal = is_object_literal_diag
                    && matches!(
                        diag.code,
                        diagnostic_codes::IS_OF_TYPE_UNKNOWN
                            | diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN
                    );
                let keep = is_nested_call_arg_error
                    || (!is_assignability
                        && !is_provisional_implicit_any
                        && !is_provisional_unknown_in_object_literal)
                    || (implicit_any_in_object_literal
                        && !implicit_any_in_object_literal_method)
                    || is_nullish_callback_body_diag
                    || is_concrete_callback_implicit_any
                    || !(is_object_literal_diag || is_function_arg_implicit_any_diag);
                // --- Phase 3: exact-message dedup for kept diagnostics ---
                if keep {
                    let full_key = (
                        diag.code,
                        diag.start,
                        diag.length,
                        diag.message_text.clone(),
                    );
                    if seen_diag_keys.iter().any(|existing| existing == &full_key) {
                        return false;
                    }
                    seen_diag_keys.push(full_key);
                }
                keep
            });
            // Restore implicit-any closure tracking to the pre-round2 state so the final
            // retry pass can re-emit TS7006 for closures whose diagnostics were suppressed.
            if let Some(snapshot) = implicit_any_closure_snapshot {
                snapshot.restore_preserving_contextual(&mut self.ctx.speculation_state());
            }
        }
        if let Some(snap) = provisional_context_snap {
            snap.rollback_filtered(&mut self.ctx.diagnostic_state(), |diag| {
                Self::should_preserve_speculative_call_diagnostic(diag)
                    || !provisional_context_arg_span
                        .is_some_and(|(start, end)| diag.start >= start && diag.start < end)
            });
        }

        if pushed_this_type {
            self.ctx.this_type_stack.pop();
        }
        arg_type
    }
}
