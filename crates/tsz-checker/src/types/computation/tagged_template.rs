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
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the type of a tagged template expression (e.g., tag`hello ${x}`).
    ///
    /// Tagged templates are function calls where:
    /// - First argument is `TemplateStringsArray`
    /// - Remaining arguments are the template substitution expressions
    ///
    /// This computes the return type of the tag function and ensures
    /// the template substitution expressions are type-checked.
    #[allow(dead_code)]
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
        let callee_shape = call_checker::get_contextual_signature_for_arity(
            self.ctx.types,
            resolved_tag_type,
            total_arg_count,
        )
        .or_else(|| call_checker::get_contextual_signature(self.ctx.types, resolved_tag_type));

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

        let unwrapped_tag = self.ctx.arena.skip_parenthesized_and_assertions(tagged.tag);
        let force_bivariant_callbacks = matches!(
            self.ctx.arena.kind_at(unwrapped_tag),
            Some(
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            )
        );
        let actual_this_type = self.actual_this_type_for_tagged_template_call(unwrapped_tag);

        // For tagged templates, the tag function parameters are:
        //   param[0] = TemplateStringsArray (always)
        //   param[1..] = substitution expressions
        // So substitution expression at index `i` corresponds to param at index `i + 1`.

        // Create contextual context from tag function type
        let ctx_helper = ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            callee_type_for_context,
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
                    let arg_request = request.read().contextual_opt(ctx_type);
                    let arg_type = self.get_type_of_node_with_request(expr_idx, &arg_request);
                    arg_types.push(arg_type);
                }

                return self.finish_tagged_template_call(
                    idx,
                    &tagged,
                    &substitution_exprs,
                    call_target_type,
                    arg_types,
                    force_bivariant_callbacks,
                    request.contextual_type,
                    actual_this_type,
                );
            }

            if needs_two_pass {
                // === Round 1: Collect non-contextual substitution types ===
                let factory = self.ctx.types.factory();
                let placeholder = {
                    let fshape = tsz_solver::FunctionShape {
                        params: vec![],
                        return_type: TypeId::ANY,
                        this_type: None,
                        type_params: vec![],
                        type_predicate: None,
                        is_constructor: false,
                        is_method: false,
                    };
                    factory.function(fshape)
                };

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
                        let arg_request = request.read().contextual_opt(ctx_type);
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
                let total_args = 1 + substitution_exprs.len();
                let mut arg_types = Vec::with_capacity(total_args);
                arg_types.push(TypeId::ANY);
                for (i, &expr_idx) in substitution_exprs.iter().enumerate() {
                    let ctx_type = ctx_helper
                        .get_parameter_type_for_call(i + 1, total_args)
                        .map(|pt| {
                            let instantiated = instantiate_type(self.ctx.types, pt, &substitution);
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
                    idx,
                    &tagged,
                    &substitution_exprs,
                    call_target_type,
                    arg_types,
                    force_bivariant_callbacks,
                    request.contextual_type,
                    actual_this_type,
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
            idx,
            &tagged,
            &substitution_exprs,
            call_target_type,
            arg_types,
            force_bivariant_callbacks,
            request.contextual_type,
            actual_this_type,
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

    #[allow(clippy::too_many_arguments)]
    fn finish_tagged_template_call(
        &mut self,
        idx: NodeIndex,
        tagged: &tsz_parser::parser::node::TaggedTemplateData,
        substitution_exprs: &[NodeIndex],
        callee_type: TypeId,
        arg_types: Vec<TypeId>,
        force_bivariant_callbacks: bool,
        contextual_type: Option<TypeId>,
        actual_this_type: Option<TypeId>,
    ) -> TypeId {
        let mut args = Vec::with_capacity(1 + substitution_exprs.len());
        args.push(tagged.template);
        args.extend_from_slice(substitution_exprs);

        let (result, _instantiated_predicate, _instantiated_params) = self
            .resolve_call_with_checker_adapter(
                callee_type,
                &arg_types,
                force_bivariant_callbacks,
                contextual_type,
                actual_this_type,
            );

        self.handle_call_result(
            result,
            super::call_result::CallResultContext {
                callee_expr: tagged.tag,
                call_idx: idx,
                args: &args,
                arg_types: &arg_types,
                callee_type,
                is_super_call: false,
                is_optional_chain: false,
                allow_contextual_mismatch_deferral: true,
            },
        )
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
