//! Tagged template expression type computation for `CheckerState`.
//!
//! Resolves the type of tagged template expressions (e.g., `` tag`hello ${x}` ``)
//! by extracting the tag function type, collecting substitution expressions,
//! and performing two-pass generic inference when needed.

use super::complex::is_contextually_sensitive;
use crate::context::TypingRequest;
use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::common::ContextualTypeContext;
use crate::query_boundaries::common::instantiate_type;
use crate::query_boundaries::construct_signatures as signature_construction;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// Parameters for finalizing a tagged template call after argument collection.
struct TaggedTemplateCallCtx {
    /// The full tagged-template expression index.
    idx: NodeIndex,
    /// The resolved callee type used for the primary call resolution.
    callee_type: TypeId,
    /// The collected substitution argument types.
    arg_types: Vec<TypeId>,
    /// Whether callbacks should be checked bivariantly.
    force_bivariant_callbacks: bool,
    /// Contextual return type for generic inference, if any.
    contextual_type: Option<TypeId>,
    /// Explicit `this` type for the call, if any.
    actual_this_type: Option<TypeId>,
    /// The selected (overload-resolved) callee type, may equal `callee_type`.
    selected_callee_type: TypeId,
}

impl<'a> CheckerState<'a> {
    /// Get the type of a tagged template expression (e.g., tag`hello ${x}`).
    ///
    /// Tagged templates are function calls where:
    /// - First argument is `TemplateStringsArray`
    /// - Remaining arguments are the template substitution expressions
    ///
    /// This computes the return type of the tag function and ensures
    /// the template substitution expressions are type-checked.
    #[expect(dead_code)]
    pub(crate) fn get_type_of_tagged_template_expression(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_tagged_template_expression_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_tagged_template_expression_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        use crate::query_boundaries::checkers::iterable::function_shape_for_type;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(tagged) = self.ctx.arena.get_tagged_template(node).cloned() else {
            return TypeId::ERROR;
        };

        // Check for missing comma between template expressions in array literals
        let parent_idx = self
            .ctx
            .arena
            .get_extended(idx)
            .map_or(NodeIndex::NONE, |ext| ext.parent);
        let parent_kind = self.ctx.arena.get(parent_idx).map(|p| p.kind);
        if parent_kind == Some(syntax_kind_ext::ARRAY_LITERAL_EXPRESSION) {
            let tag_kind = self.ctx.arena.get(tagged.tag).map(|t| t.kind);
            if tag_kind == Some(syntax_kind_ext::TEMPLATE_EXPRESSION)
                || tag_kind == Some(tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16)
            {
                use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    tagged.tag,
                    diagnostic_messages::IT_IS_LIKELY_THAT_YOU_ARE_MISSING_A_COMMA_TO_SEPARATE_THESE_TWO_TEMPLATE_EXPRESS,
                    diagnostic_codes::IT_IS_LIKELY_THAT_YOU_ARE_MISSING_A_COMMA_TO_SEPARATE_THESE_TWO_TEMPLATE_EXPRESS,
                );
                return TypeId::ERROR;
            }
        }

        // Get the type of the tag function
        let tag_request = request.read().contextual_opt(None);
        let tag_type = self.get_type_of_node_with_request(tagged.tag, &tag_request);

        // If tag type is `any`, type-check substitutions without context and return `any`
        if tag_type == TypeId::ANY || tag_type == TypeId::ERROR {
            self.type_check_template_substitutions_no_context(&tagged, request);
            return tag_type;
        }

        // Collect substitution expression NodeIndex values from the template
        let substitution_exprs: Vec<NodeIndex> = self.collect_template_substitution_exprs(&tagged);

        // Resolve the tag function type for signature extraction
        let resolved_tag_type = self.resolve_ref_type(tag_type);
        let resolved_tag_type = self.resolve_lazy_type(resolved_tag_type);

        // Extract function shape from the tag function type. Tagged templates
        // pass `TemplateStringsArray` as the first argument followed by the
        // substitution expressions, so the effective arg count is
        // `1 + substitution_exprs.len()`. Threading this arity into signature
        // selection lets overload-aware contextual typing pick the matching
        // overload (mirrors the regular call expression path) instead of
        // returning `None` for mixed-arity overload sets and falling back to a
        // signature-less single pass.
        let total_arg_count = 1 + substitution_exprs.len();
        let callee_shape =
            call_checker::get_call_signature(self.ctx.types, resolved_tag_type, total_arg_count)
                .or_else(|| {
                    call_checker::get_contextual_signature_for_arity(
                        self.ctx.types,
                        resolved_tag_type,
                        total_arg_count,
                    )
                })
                .or_else(|| {
                    call_checker::get_contextual_signature(self.ctx.types, resolved_tag_type)
                });

        // Detect constructor-only callable types (classes, interfaces with only `new` sigs).
        // `get_contextual_signature` falls back to construct signatures when call
        // signatures are absent, so we must check the callable shape directly.
        // Tagged templates are function calls — constructor-only types are not callable.
        if let Some(callable) = crate::query_boundaries::common::callable_shape_for_type(
            self.ctx.types,
            resolved_tag_type,
        ) && callable.call_signatures.is_empty()
            && !callable.construct_signatures.is_empty()
        {
            self.type_check_template_substitutions_no_context(&tagged, request);
            self.error_not_callable_at(tag_type, tagged.tag);
            return TypeId::ERROR;
        }

        // If `get_contextual_signature` found no signatures (not even construct), check
        // if the type is truly non-callable.  Types like `Function` or interfaces with
        // no concrete signatures should still fall through to return `any`.
        // Only emit TS2349 for types that are definitely non-callable (primitives, literals).
        if callee_shape.is_none()
            && function_shape_for_type(self.ctx.types, resolved_tag_type).is_none()
        {
            // Check if the type is a primitive/literal/intrinsic that cannot be called.
            let is_definitely_not_callable = matches!(
                resolved_tag_type,
                TypeId::STRING
                    | TypeId::NUMBER
                    | TypeId::BOOLEAN
                    | TypeId::VOID
                    | TypeId::NULL
                    | TypeId::UNDEFINED
                    | TypeId::NEVER
                    | TypeId::SYMBOL
                    | TypeId::BIGINT
                    | TypeId::OBJECT
            ) || crate::query_boundaries::common::is_literal_type(
                self.ctx.types,
                resolved_tag_type,
            );
            if is_definitely_not_callable {
                self.type_check_template_substitutions_no_context(&tagged, request);
                self.error_not_callable_at(tag_type, tagged.tag);
                return TypeId::ERROR;
            }
        }

        let is_generic_call = callee_shape
            .as_ref()
            .is_some_and(|s| !s.type_params.is_empty())
            && tagged.type_arguments.is_none();

        // Apply explicit type arguments to the tag type (e.g., tag<Stuff>`...`).
        // This instantiates type parameters in the function signature so that
        // contextual typing of substitution expressions and the return type
        // reflect the concrete type arguments instead of the raw type parameters.
        let resolved_tag_type = if tagged.type_arguments.is_some() {
            self.apply_type_arguments_to_callable_type(
                resolved_tag_type,
                tagged.type_arguments.as_ref(),
            )
        } else {
            resolved_tag_type
        };

        let callee_type_for_context = self.evaluate_application_type(resolved_tag_type);
        let callee_type_for_context = self.resolve_lazy_type(callee_type_for_context);
        let callee_type_for_context = self.evaluate_contextual_type(callee_type_for_context);
        let mut call_target_type = self.resolve_lazy_members_in_union(callee_type_for_context);
        call_target_type = self.replace_function_type_for_call(tag_type, call_target_type);
        if call_target_type == TypeId::ANY {
            self.type_check_template_substitutions_no_context(&tagged, request);
            return TypeId::ANY;
        }

        let unwrapped_tag = self.ctx.arena.skip_outer_expressions(tagged.tag);
        let force_bivariant_callbacks = matches!(
            self.ctx.arena.kind_at(unwrapped_tag),
            Some(
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            )
        );
        let actual_this_type = self.call_site_receiver_type(call_target_type, unwrapped_tag);

        // For tagged templates, the tag function parameters are:
        //   param[0] = TemplateStringsArray (always)
        //   param[1..] = substitution expressions
        // So substitution expression at index `i` corresponds to param at index `i + 1`.

        // Create contextual context from tag function type
        let contextual_callee_type = if tagged.type_arguments.is_some() {
            call_checker::get_call_signature(self.ctx.types, call_target_type, total_arg_count)
                .or_else(|| {
                    call_checker::get_contextual_signature_for_arity(
                        self.ctx.types,
                        call_target_type,
                        total_arg_count,
                    )
                })
                .or_else(|| {
                    call_checker::get_contextual_signature(self.ctx.types, call_target_type)
                })
                .map(|shape| {
                    signature_construction::function_type_from_shape(self.ctx.types, shape)
                })
                .unwrap_or(call_target_type)
        } else {
            callee_shape
                .as_ref()
                .map(|shape| {
                    signature_construction::function_type_from_shape(self.ctx.types, shape.clone())
                })
                .unwrap_or(callee_type_for_context)
        };
        let selected_callee_type = if tagged.type_arguments.is_some() {
            call_target_type
        } else {
            contextual_callee_type
        };
        let ctx_helper = ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            contextual_callee_type,
            self.ctx.compiler_options.no_implicit_any,
        );

        if is_generic_call
            && !substitution_exprs.is_empty()
            && let Some(shape) = callee_shape.as_ref()
        {
            // Pre-compute contextual sensitivity
            let sensitive_args: Vec<bool> = substitution_exprs
                .iter()
                .map(|&arg| is_contextually_sensitive(self, arg))
                .collect();
            let needs_two_pass = sensitive_args.iter().copied().any(std::convert::identity);

            if !needs_two_pass {
                // === Single-pass inference: no contextually-sensitive args ===
                // All arguments are concrete, so we can infer type parameters directly.
                let total_args = 1 + substitution_exprs.len();
                let mut arg_types: Vec<TypeId> = Vec::with_capacity(total_args);
                arg_types.push(TypeId::ANY);

                for (i, &expr_idx) in substitution_exprs.iter().enumerate() {
                    let ctx_type = ctx_helper.get_parameter_type_for_call(i + 1, total_args);
                    let is_nullish_literal = self
                        .literal_type_from_initializer(expr_idx)
                        .is_some_and(|ty| ty == TypeId::UNDEFINED || ty == TypeId::NULL);
                    let arg_request = if is_nullish_literal {
                        request.read().contextual_opt(None)
                    } else {
                        request.read().contextual_opt(ctx_type)
                    };
                    let arg_type = self.get_type_of_node_with_request(expr_idx, &arg_request);
                    arg_types.push(arg_type);
                }

                return self.finish_tagged_template_call(
                    TaggedTemplateCallCtx {
                        idx,
                        callee_type: call_target_type,
                        arg_types,
                        force_bivariant_callbacks,
                        contextual_type: request.contextual_type,
                        actual_this_type,
                        selected_callee_type,
                    },
                    &tagged,
                    &substitution_exprs,
                );
            }

            if needs_two_pass {
                // === Round 1: Collect non-contextual substitution types ===
                let placeholder = signature_construction::function_type_from_params_and_return(
                    self.ctx.types,
                    vec![],
                    TypeId::ANY,
                );

                // Build argument types for Round 1: TemplateStringsArray + substitutions
                // Use ANY as stand-in for TemplateStringsArray since it's a fixed
                // non-generic type that doesn't affect type parameter inference.
                let mut round1_arg_types: Vec<TypeId> =
                    Vec::with_capacity(1 + substitution_exprs.len());
                round1_arg_types.push(TypeId::ANY);

                for (i, &expr_idx) in substitution_exprs.iter().enumerate() {
                    if sensitive_args[i] {
                        round1_arg_types.push(placeholder);
                    } else {
                        let ctx_type = ctx_helper
                            .get_parameter_type_for_call(i + 1, 1 + substitution_exprs.len());
                        let is_nullish_literal = self
                            .literal_type_from_initializer(expr_idx)
                            .is_some_and(|ty| ty == TypeId::UNDEFINED || ty == TypeId::NULL);
                        let arg_request = if is_nullish_literal {
                            request.read().contextual_opt(None)
                        } else {
                            request.read().contextual_opt(ctx_type)
                        };
                        let arg_type = self.get_type_of_node_with_request(expr_idx, &arg_request);
                        round1_arg_types.push(arg_type);
                    }
                }

                // Perform Round 1 inference
                let evaluated_shape = {
                    let new_params: Vec<_> = shape
                        .params
                        .iter()
                        .map(|p| tsz_solver::ParamInfo {
                            suppress_display_optional: false,
                            name: p.name,
                            type_id: self.evaluate_type_with_env(p.type_id),
                            optional: p.optional,
                            rest: p.rest,
                        })
                        .collect();
                    tsz_solver::FunctionShape {
                        params: new_params,
                        return_type: shape.return_type,
                        this_type: shape.this_type,
                        type_params: shape.type_params.clone(),
                        type_predicate: shape.type_predicate,
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    }
                };
                let substitution = {
                    let env = self.ctx.type_env.borrow();
                    call_checker::compute_contextual_types_with_context(
                        self.ctx.types,
                        &self.ctx,
                        &env,
                        &evaluated_shape,
                        &round1_arg_types,
                        request.contextual_type,
                    )
                };

                // === Round 2: Type-check all substitutions with contextual types ===
                // Widen literal types inferred from round 1 (e.g. a numeric
                // literal argument binding `T := 10`) before using the
                // substitution to seed a sensitive argument's contextual
                // type — mirrors the regular call-argument two-pass path
                // (`widen_round2_contextual_substitution`, used by
                // `argument_collection.rs`). Without this, `T` stays pinned
                // to the literal while the tag's final call resolution
                // (a separate, later inference pass) widens it normally,
                // producing a self-contradictory diagnostic where a
                // callback's actual and expected types render identically
                // but their nested literal-vs-widened members disagree.
                let contextual_substitution =
                    self.widen_round2_contextual_substitution(&evaluated_shape, &substitution);
                let total_args = 1 + substitution_exprs.len();
                let mut arg_types = Vec::with_capacity(total_args);
                arg_types.push(TypeId::ANY);
                for (i, &expr_idx) in substitution_exprs.iter().enumerate() {
                    let ctx_type = ctx_helper
                        .get_parameter_type_for_call(i + 1, total_args)
                        .map(|pt| {
                            let instantiated =
                                instantiate_type(self.ctx.types, pt, &contextual_substitution);
                            self.evaluate_type_with_env(instantiated)
                        });
                    let arg_request = if is_contextually_sensitive(self, expr_idx) {
                        request.read().contextual_opt(ctx_type)
                    } else {
                        request.read().contextual_opt(None)
                    };
                    let actual_type = self.get_type_of_node_with_request(expr_idx, &arg_request);
                    arg_types.push(actual_type);
                }

                return self.finish_tagged_template_call(
                    TaggedTemplateCallCtx {
                        idx,
                        callee_type: call_target_type,
                        arg_types,
                        force_bivariant_callbacks,
                        contextual_type: request.contextual_type,
                        actual_this_type,
                        selected_callee_type,
                    },
                    &tagged,
                    &substitution_exprs,
                );
            }
        }

        // Single-pass: type-check substitutions with contextual types from tag signature
        let total_args = 1 + substitution_exprs.len();
        let mut arg_types = Vec::with_capacity(total_args);
        arg_types.push(TypeId::ANY);
        for (i, &expr_idx) in substitution_exprs.iter().enumerate() {
            let ctx_type = ctx_helper.get_parameter_type_for_call(i + 1, total_args);
            let arg_request = request.read().contextual_opt(ctx_type);
            let actual_type = self.get_type_of_node_with_request(expr_idx, &arg_request);
            arg_types.push(actual_type);
        }

        self.finish_tagged_template_call(
            TaggedTemplateCallCtx {
                idx,
                callee_type: call_target_type,
                arg_types,
                force_bivariant_callbacks,
                contextual_type: request.contextual_type,
                actual_this_type,
                selected_callee_type,
            },
            &tagged,
            &substitution_exprs,
        )
    }

    fn actual_this_type_for_tagged_template_call(
        &mut self,
        unwrapped_tag: NodeIndex,
    ) -> Option<TypeId> {
        let tag_node = self.ctx.arena.get(unwrapped_tag)?;
        if tag_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && tag_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(tag_node)?;
        Some(self.get_type_of_node(access.expression))
    }

    fn finish_tagged_template_call(
        &mut self,
        call_ctx: TaggedTemplateCallCtx,
        tagged: &tsz_parser::parser::node::TaggedTemplateData,
        substitution_exprs: &[NodeIndex],
    ) -> TypeId {
        let TaggedTemplateCallCtx {
            idx,
            callee_type,
            arg_types,
            force_bivariant_callbacks,
            contextual_type,
            actual_this_type,
            selected_callee_type,
        } = call_ctx;
        let mut args = Vec::with_capacity(1 + substitution_exprs.len());
        args.push(tagged.template);
        args.extend_from_slice(substitution_exprs);

        let selected_return = if selected_callee_type != callee_type {
            let (selected_result, _, _) = self.resolve_call_with_checker_adapter(
                selected_callee_type,
                &arg_types,
                force_bivariant_callbacks,
                contextual_type,
                actual_this_type,
            );
            match selected_result {
                tsz_solver::operations::CallResult::Success(return_type) => Some(return_type),
                _ => None,
            }
        } else {
            None
        };

        let (result, _instantiated_predicate, _instantiated_params) = self
            .resolve_call_with_checker_adapter(
                callee_type,
                &arg_types,
                force_bivariant_callbacks,
                contextual_type,
                actual_this_type,
            );
        let full_resolution_succeeded =
            matches!(result, tsz_solver::operations::CallResult::Success(_));
        if let tsz_solver::operations::CallResult::NoOverloadMatch { failures, .. } = &result
            && selected_callee_type != callee_type
            && let Some(selected_return) = self
                .selected_tagged_template_nullish_overload_return(selected_callee_type, &arg_types)
        {
            self.error_no_overload_matches_at(idx, failures);
            return selected_return;
        }

        let return_type = self.handle_call_result(
            result,
            super::call_result::CallResultContext {
                callee_expr: tagged.tag,
                call_idx: idx,
                args: &args,
                arg_types: &arg_types,
                callee_type,
                callee_has_declared_generic_signature:
                    crate::query_boundaries::common::function_shape_for_type(
                        self.ctx.types,
                        callee_type,
                    )
                    .is_some_and(|shape| !shape.type_params.is_empty())
                        || crate::query_boundaries::common::callable_shape_for_type(
                            self.ctx.types,
                            callee_type,
                        )
                        .is_some_and(|shape| {
                            shape
                                .call_signatures
                                .iter()
                                .any(|sig| !sig.type_params.is_empty())
                        }),
                // Tagged-template arguments map positionally to the tag's
                // declared parameters; the literal-display heuristic that
                // consumes `raw_callee_shape` does not apply here.
                raw_callee_shape: None,
                is_super_call: false,
                is_optional_chain: false,
                allow_contextual_mismatch_deferral: true,
            },
        );

        if full_resolution_succeeded && let Some(selected_return) = selected_return {
            return selected_return;
        }

        return_type
    }

    fn selected_tagged_template_nullish_overload_return(
        &self,
        selected_callee_type: TypeId,
        arg_types: &[TypeId],
    ) -> Option<TypeId> {
        let shape = crate::query_boundaries::common::function_shape_for_type(
            self.ctx.types,
            selected_callee_type,
        )?;
        if !shape.type_params.is_empty()
            || arg_types.len() != 2
            || !matches!(
                arg_types.get(1).copied(),
                Some(TypeId::NULL | TypeId::UNDEFINED)
            )
        {
            return None;
        }
        Some(shape.return_type)
    }

    /// Collect template substitution expression `NodeIndex` values from a tagged template.
    fn collect_template_substitution_exprs(
        &self,
        tagged: &tsz_parser::parser::node::TaggedTemplateData,
    ) -> Vec<NodeIndex> {
        let mut exprs = Vec::new();
        if let Some(template_node) = self.ctx.arena.get(tagged.template)
            && template_node.kind == syntax_kind_ext::TEMPLATE_EXPRESSION
            && let Some(templ_data) = self.ctx.arena.get_template_expr(template_node)
        {
            for &span_idx in &templ_data.template_spans.nodes {
                if let Some(span_node) = self.ctx.arena.get(span_idx)
                    && let Some(span_data) = self.ctx.arena.get_template_span(span_node)
                {
                    exprs.push(span_data.expression);
                }
            }
        }
        exprs
    }

    /// Type-check template substitution expressions without contextual types.
    fn type_check_template_substitutions_no_context(
        &mut self,
        tagged: &tsz_parser::parser::node::TaggedTemplateData,
        request: &TypingRequest,
    ) {
        if let Some(template_node) = self.ctx.arena.get(tagged.template)
            && template_node.kind == syntax_kind_ext::TEMPLATE_EXPRESSION
            && let Some(templ_data) = self.ctx.arena.get_template_expr(template_node).cloned()
        {
            for &span_idx in &templ_data.template_spans.nodes {
                if let Some(span_node) = self.ctx.arena.get(span_idx)
                    && let Some(span_data) = self.ctx.arena.get_template_span(span_node).cloned()
                {
                    let expr_request = request.read().contextual_opt(None);
                    self.get_type_of_node_with_request(span_data.expression, &expr_request);
                }
            }
        }
    }
}

#[cfg(test)]
mod tagged_template_overload_literal_widening_tests {
    use crate::test_utils::check_source_diagnostics;

    // Oracle-pinned against `typescript@7.0.2` on
    // `conformance/expressions/contextualTyping/parenthesizedContexualTyping3.ts`.
    // Every case below is clean on tsc.

    fn assert_no_ts2345(source: &str) {
        let diags = check_source_diagnostics(source);
        let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
        assert!(
            ts2345.is_empty(),
            "expected no TS2345, got: {ts2345:?} (all diagnostics: {diags:?})"
        );
    }

    const SINGLE_ARITY_TAG: &str = "
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, x: T): T {
    return g(x);
}
";

    #[test]
    fn direct_pass_widens_round1_literal_before_seeding_sensitive_arg_context() {
        assert_no_ts2345(&format!(
            "{SINGLE_ARITY_TAG}\nvar a = tempFun`${{ x => x }}  ${{ 10 }}`;"
        ));
    }

    #[test]
    fn literal_widening_is_not_numeric_specific() {
        // The fix reuses the general-purpose `widen_round2_contextual_substitution`
        // helper, so a string-literal candidate must widen exactly like a
        // numeric one — this is not a numeric-literal special case.
        assert_no_ts2345(&format!(
            r#"{SINGLE_ARITY_TAG}
var s = tempFun`${{ x => x }} ${{ "s" }}`;"#
        ));
    }

    #[test]
    fn parenthesized_arrow_variants_are_unaffected() {
        assert_no_ts2345(&format!(
            "{SINGLE_ARITY_TAG}\nvar b = tempFun`${{ (x => x) }}  ${{ 10 }}`;"
        ));
        assert_no_ts2345(&format!(
            "{SINGLE_ARITY_TAG}\nvar c = tempFun`${{ ((x => x)) }} ${{ 10 }}`;"
        ));
    }

    #[test]
    fn renamed_binder_and_type_param_are_unaffected() {
        assert_no_ts2345(
            "
function stamp<Value>(strs: TemplateStringsArray, project: (received: Value) => Value, seed: Value): Value {
    return project(seed);
}
var out = stamp`${ received => received }  ${ 10 }`;
",
        );
    }

    #[test]
    fn overload_with_two_callback_params_widens_both() {
        // Second overload: two `(x: T) => T` callbacks before the literal.
        const TWO_CALLBACK_TAG: &str = "
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, x: T): T;
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, h: (y: T) => T, x: T): T;
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, x: T): T {
    return g(x);
}
";
        assert_no_ts2345(&format!(
            "{TWO_CALLBACK_TAG}\nvar d = tempFun`${{ x => x }} ${{ x => x }} ${{ 10 }}`;"
        ));
        assert_no_ts2345(&format!(
            "{TWO_CALLBACK_TAG}\nvar e = tempFun`${{ x => x }} ${{ (x => x) }} ${{ 10 }}`;"
        ));
        assert_no_ts2345(&format!(
            "{TWO_CALLBACK_TAG}\nvar f = tempFun`${{ x => x }} ${{ ((x => x)) }} ${{ 10 }}`;"
        ));
        assert_no_ts2345(&format!(
            "{TWO_CALLBACK_TAG}\nvar g = tempFun`${{ (x => x) }} ${{ (((x => x))) }} ${{ 10 }}`;"
        ));
    }

    #[test]
    fn nullish_literal_positional_argument_is_unaffected() {
        const TWO_CALLBACK_TAG: &str = "
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, x: T): T;
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, h: (y: T) => T, x: T): T;
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, x: T): T {
    return g(x);
}
";
        assert_no_ts2345(&format!(
            "{TWO_CALLBACK_TAG}\nvar h = tempFun`${{ (x => x) }} ${{ (((x => x))) }} ${{ undefined }}`;"
        ));
    }

    #[test]
    fn genuine_body_mismatch_still_reports_after_widening() {
        // Negative control: the fix must widen the *contextual parameter type*
        // fed to the callback, not silence real errors inside its body. Once
        // `x` is correctly widened to `number`, `.length` on it is a genuine
        // TS2339, proving the widened type actually reached the callback.
        let diags = check_source_diagnostics(&format!(
            "{SINGLE_ARITY_TAG}\nvar neg = tempFun`${{ x => x.length }} ${{ 10 }}`;"
        ));
        let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
        assert_eq!(
            ts2339.len(),
            1,
            "expected exactly one TS2339 from `.length` on the widened `number` \
             parameter, got: {ts2339:?} (all diagnostics: {diags:?})"
        );
    }
}
