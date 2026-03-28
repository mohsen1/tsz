//! Tagged template expression type computation for `CheckerState`.
//!
//! Resolves the type of tagged template expressions (e.g., `` tag`hello ${x}` ``)
//! by extracting the tag function type, collecting substitution expressions,
//! and performing two-pass generic inference when needed.

use super::complex::is_contextually_sensitive;
use crate::context::TypingRequest;
use crate::query_boundaries::assignability::contains_type_parameters;
use crate::query_boundaries::checkers::call as call_checker;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{ContextualTypeContext, TypeId};

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
        use crate::query_boundaries::checkers::iterable::{
            call_signatures_for_type, function_shape_for_type,
        };
        use crate::query_boundaries::common::instantiate_type;

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

        // Extract function shape from the tag function type
        let callee_shape =
            call_checker::get_contextual_signature(self.ctx.types, resolved_tag_type);

        // Detect constructor-only callable types (classes, interfaces with only `new` sigs).
        // `get_contextual_signature` falls back to construct signatures when call
        // signatures are absent, so we must check the callable shape directly.
        // Tagged templates are function calls — constructor-only types are not callable.
        if let Some(callable) =
            tsz_solver::type_queries::get_callable_shape(self.ctx.types, resolved_tag_type)
            && callable.call_signatures.is_empty()
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
            let is_definitely_not_callable =
                matches!(
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
                ) || tsz_solver::type_queries::is_literal_type(self.ctx.types, resolved_tag_type);
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

        // Determine whether to check argument assignability (TS2345).
        // For overloaded functions, tsc performs full overload resolution and reports
        // TS2769 ("No overload matches this call") instead of TS2345 per argument.
        // We only check arguments when the tag has a single call signature.
        let is_overloaded =
            tsz_solver::type_queries::get_callable_shape(self.ctx.types, resolved_tag_type)
                .is_some_and(|callable| callable.call_signatures.len() > 1);
        let should_check_args = !is_overloaded;

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

        // For tagged templates, the tag function parameters are:
        //   param[0] = TemplateStringsArray (always)
        //   param[1..] = substitution expressions
        // So substitution expression at index `i` corresponds to param at index `i + 1`.

        // Create contextual context from tag function type
        let ctx_helper = ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            resolved_tag_type,
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
                let template_strings_type = TypeId::ANY;
                let mut round1_arg_types: Vec<TypeId> =
                    Vec::with_capacity(1 + substitution_exprs.len());
                round1_arg_types.push(template_strings_type);

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
                let mut reported_arg_error = false;
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

                    // Check argument assignability against expected parameter type (TS2345).
                    // tsc reports only the first argument mismatch per tagged template call.
                    // Skip for overloaded functions (handled via overload resolution / TS2769).
                    // Skip when expected type still contains unresolved type parameters
                    // (generic inference may not have fully instantiated the signature).
                    if should_check_args
                        && !reported_arg_error
                        && let Some(expected) = ctx_type
                        && actual_type != TypeId::ERROR
                        && actual_type != TypeId::UNKNOWN
                        && expected != TypeId::ERROR
                        && expected != TypeId::UNKNOWN
                        && !contains_type_parameters(self.ctx.types, expected)
                        && !self.should_defer_contextual_argument_mismatch(actual_type, expected)
                        && !self.check_argument_assignable_or_report(
                            actual_type,
                            expected,
                            expr_idx,
                        )
                    {
                        reported_arg_error = true;
                    }
                }

                // Return instantiated return type
                let return_type =
                    instantiate_type(self.ctx.types, shape.return_type, &substitution);
                return self.evaluate_type_with_env(return_type);
            }
        }

        // Single-pass: type-check substitutions with contextual types from tag signature
        let total_args = 1 + substitution_exprs.len();
        let mut reported_arg_error = false;
        for (i, &expr_idx) in substitution_exprs.iter().enumerate() {
            let ctx_type = ctx_helper.get_parameter_type_for_call(i + 1, total_args);
            let arg_request = request.read().contextual_opt(ctx_type);
            let actual_type = self.get_type_of_node_with_request(expr_idx, &arg_request);

            // Check argument assignability against expected parameter type (TS2345).
            // tsc reports only the first argument mismatch per tagged template call,
            // so stop checking after the first error.
            // Skip for overloaded functions (handled via overload resolution / TS2769).
            // Skip when expected type still contains unresolved type parameters
            // (generic inference may not have fully instantiated the signature).
            if should_check_args
                && !reported_arg_error
                && let Some(expected) = ctx_type
                && actual_type != TypeId::ERROR
                && actual_type != TypeId::UNKNOWN
                && expected != TypeId::ERROR
                && expected != TypeId::UNKNOWN
                && !contains_type_parameters(self.ctx.types, expected)
                && !self.should_defer_contextual_argument_mismatch(actual_type, expected)
                && !self.check_argument_assignable_or_report(actual_type, expected, expr_idx)
            {
                reported_arg_error = true;
            }
        }

        // Get the return type from the tag function's call signature.
        // Use resolved_tag_type which has explicit type arguments applied.
        if let Some(sig) = function_shape_for_type(self.ctx.types, resolved_tag_type) {
            return sig.return_type;
        }
        if let Some(call_sigs) = call_signatures_for_type(self.ctx.types, resolved_tag_type)
            && let Some(first_sig) = call_sigs.first()
        {
            return first_sig.return_type;
        }

        // If tag is Function type, return any
        TypeId::ANY
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
