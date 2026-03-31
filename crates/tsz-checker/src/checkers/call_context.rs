//! Contextual typing analysis for call expressions.
//!
//! Helpers that determine whether and how contextual types from callee signatures
//! should be applied to call arguments. Covers callback sensitivity analysis,
//! generic return context suppression, and argument-level contextual typing
//! decisions.

use crate::computation::complex::is_contextually_sensitive;
use crate::context::TypingRequest;
use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::common;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn contextual_type_is_unresolved_for_argument_refresh(
        &self,
        type_id: TypeId,
    ) -> bool {
        type_id == TypeId::UNKNOWN
            || type_id == TypeId::ERROR
            || common::contains_infer_types(self.ctx.types, type_id)
            || common::contains_type_parameters(self.ctx.types, type_id)
    }

    pub(crate) fn is_immediate_call_or_new_callee(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..100 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let parent_idx = ext.parent;
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            match parent.kind {
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    || k == syntax_kind_ext::NON_NULL_EXPRESSION
                    || k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    current = parent_idx;
                }
                k if k == syntax_kind_ext::CALL_EXPRESSION
                    || k == syntax_kind_ext::NEW_EXPRESSION =>
                {
                    return self
                        .ctx
                        .arena
                        .get_call_expr(parent)
                        .is_some_and(|call| call.expression == current);
                }
                _ => return false,
            }
        }
        false
    }

    pub(crate) fn is_immediate_call_or_new_argument(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..100 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let parent_idx = ext.parent;
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            match parent.kind {
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    || k == syntax_kind_ext::NON_NULL_EXPRESSION
                    || k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    current = parent_idx;
                }
                k if k == syntax_kind_ext::CALL_EXPRESSION
                    || k == syntax_kind_ext::NEW_EXPRESSION =>
                {
                    return self
                        .ctx
                        .arena
                        .get_call_expr(parent)
                        .and_then(|call| call.arguments.as_ref())
                        .is_some_and(|args| args.nodes.contains(&current));
                }
                _ => return false,
            }
        }
        false
    }

    pub(crate) fn object_literal_function_like_param_spans(
        &self,
        arg_idx: NodeIndex,
    ) -> Vec<(u32, u32)> {
        fn collect<'a>(checker: &CheckerState<'a>, idx: NodeIndex, spans: &mut Vec<(u32, u32)>) {
            let Some(node) = checker.ctx.arena.get(idx) else {
                return;
            };

            match node.kind {
                k if k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR =>
                {
                    spans.extend(checker.function_like_param_spans_for_node(idx));
                }
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(paren) = checker.ctx.arena.get_parenthesized(node) {
                        collect(checker, paren.expression, spans);
                    }
                }
                k if k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION
                    || k == syntax_kind_ext::TYPE_ASSERTION =>
                {
                    if let Some(assertion) = checker.ctx.arena.get_type_assertion(node) {
                        collect(checker, assertion.expression, spans);
                    }
                }
                k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION =>
                {
                    let Some(literal) = checker.ctx.arena.get_literal_expr(node) else {
                        return;
                    };

                    for &element_idx in &literal.elements.nodes {
                        let Some(element) = checker.ctx.arena.get(element_idx) else {
                            continue;
                        };

                        match element.kind {
                            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                                let Some(prop) = checker.ctx.arena.get_property_assignment(element)
                                else {
                                    continue;
                                };
                                collect(checker, prop.initializer, spans);
                            }
                            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                                if let Some(spread) = checker.ctx.arena.get_spread(element) {
                                    collect(checker, spread.expression, spans);
                                }
                            }
                            _ => collect(checker, element_idx, spans),
                        }
                    }
                }
                _ => {}
            }
        }

        let mut spans = Vec::new();
        collect(self, arg_idx, &mut spans);
        spans
    }

    pub(crate) fn object_literal_noncontextual_function_param_spans(
        &self,
        arg_idx: NodeIndex,
    ) -> Vec<(u32, u32)> {
        fn collect<'a>(checker: &CheckerState<'a>, idx: NodeIndex, spans: &mut Vec<(u32, u32)>) {
            let Some(node) = checker.ctx.arena.get(idx) else {
                return;
            };
            let Some(obj) = checker.ctx.arena.get_literal_expr(node) else {
                return;
            };

            for &element_idx in &obj.elements.nodes {
                let Some(element) = checker.ctx.arena.get(element_idx) else {
                    continue;
                };

                let collect_function_spans =
                    |checker: &CheckerState<'a>,
                     function_idx: NodeIndex,
                     spans: &mut Vec<(u32, u32)>| {
                        if checker
                            .ctx
                            .implicit_any_contextual_closures
                            .contains(&function_idx)
                        {
                            return;
                        }
                        spans.extend(checker.function_like_param_spans_for_node(function_idx));
                    };

                match element.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION
                        || k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        collect_function_spans(checker, element_idx, spans);
                    }
                    k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                        let Some(prop) = checker.ctx.arena.get_property_assignment(element) else {
                            continue;
                        };
                        let init_idx = prop.initializer;
                        if checker.ctx.arena.get(init_idx).is_some_and(|init_node| {
                            init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                                || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                        }) {
                            collect_function_spans(checker, init_idx, spans);
                        } else {
                            collect(checker, init_idx, spans);
                        }
                    }
                    _ => {}
                }
            }
        }

        let mut spans = Vec::new();
        collect(self, arg_idx, &mut spans);
        spans
    }

    /// Nested calls/new expressions should infer from their own callee shapes during
    /// Round 1 generic inference. Applying the outer call's contextual parameter type
    /// at this stage can erase useful inference from the inner call.
    pub(crate) fn round1_should_skip_outer_contextual_type(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        loop {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::CALL_EXPRESSION
                    || k == syntax_kind_ext::NEW_EXPRESSION =>
                {
                    return true;
                }
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    let Some(paren) = self.ctx.arena.get_parenthesized(node) else {
                        return false;
                    };
                    current = paren.expression;
                }
                _ => return false,
            }
        }
    }

    pub(crate) fn callback_body_span(&self, arg_idx: NodeIndex) -> Option<(u32, u32)> {
        self.ctx
            .arena
            .get(arg_idx)
            .and_then(|node| self.ctx.arena.get_function(node))
            .and_then(|func| self.ctx.arena.get(func.body))
            .map(|body_node| (body_node.pos, body_node.end))
    }

    /// Returns true when the callback argument has at least one explicitly-typed parameter
    /// whose type is incompatible with the corresponding expected parameter type.
    ///
    /// TSC reports TS2345 at the call site when the *parameter* types conflict
    /// (e.g. `(n: string) => n` where `(b: number) => number` is expected). In that case
    /// an inner TS2322 from the callback body should not suppress TS2345 --- the outer
    /// diagnostic is more informative. When only the return type differs (e.g.
    /// `(x: number) => ''` where `(x: number) => number` is expected), TSC still reports
    /// the inner TS2322.
    ///
    /// This uses the inferred `actual` type of the callback (not AST node resolution)
    /// to get reliable parameter types.
    pub(crate) fn callback_has_explicit_param_type_conflict(
        &mut self,
        arg_idx: NodeIndex,
        expected: tsz_solver::TypeId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };
        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };

        // Check if ANY parameter has an explicit type annotation.
        // If none do, this is a fully-contextual callback and we keep the inner error.
        let has_any_explicit_annotation = func.parameters.nodes.iter().any(|param_idx| {
            self.ctx
                .arena
                .get(*param_idx)
                .and_then(|n| self.ctx.arena.get_parameter(n))
                .is_some_and(|p| p.type_annotation.is_some())
        });
        if !has_any_explicit_annotation {
            return false;
        }

        // Resolve the expected function type to extract expected parameter types.
        let resolved_expected = self.evaluate_type_with_env(expected);
        let resolved_expected = self.resolve_type_for_property_access(resolved_expected);
        let resolved_expected = self.resolve_lazy_type(resolved_expected);
        let expected_params =
            call_checker::get_function_parameter_types(self.ctx.types, resolved_expected);
        if expected_params.is_empty() {
            return false;
        }

        // Get the inferred type of the callback to extract its parameter types.
        // Using the inferred type is more reliable than walking the AST param nodes.
        let actual_type = self.get_type_of_node(arg_idx);
        let resolved_actual = self.evaluate_type_with_env(actual_type);
        let resolved_actual = self.resolve_type_for_property_access(resolved_actual);
        let resolved_actual = self.resolve_lazy_type(resolved_actual);
        let actual_params =
            call_checker::get_function_parameter_types(self.ctx.types, resolved_actual);

        // For each explicitly-annotated parameter, check if its type conflicts with
        // the expected parameter type.
        func.parameters
            .nodes
            .iter()
            .enumerate()
            .any(|(i, param_idx)| {
                let has_annotation = self
                    .ctx
                    .arena
                    .get(*param_idx)
                    .and_then(|n| self.ctx.arena.get_parameter(n))
                    .is_some_and(|p| p.type_annotation.is_some());
                if !has_annotation {
                    return false;
                }
                let actual_param_type = actual_params
                    .get(i)
                    .copied()
                    .unwrap_or(tsz_solver::TypeId::UNKNOWN);
                let expected_param_type = expected_params.get(i).copied().unwrap_or_else(|| {
                    *expected_params
                        .last()
                        .expect("expected_params should not be empty")
                });
                // Parameter types conflict if the actual is NOT assignable to expected.
                !self.is_assignable_to(actual_param_type, expected_param_type)
            })
    }

    pub(crate) fn suppress_generic_return_context_for_arg(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::ARROW_FUNCTION
            && node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return false;
        }
        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };
        // Do not suppress for generator functions: a generator with no parameters
        // has no parameter-based sensitivity. The outer contextual type should flow
        // into the generator's return type to correctly constrain generic inference.
        func.parameters.nodes.is_empty()
            && func.type_annotation.is_none()
            && !func.asterisk_token
            && is_contextually_sensitive(self, idx)
    }

    fn object_literal_contains_function_member(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return false;
        };

        obj.elements.nodes.iter().any(|&element_idx| {
            let Some(element) = self.ctx.arena.get(element_idx) else {
                return false;
            };
            if element.kind == syntax_kind_ext::METHOD_DECLARATION
                || element.kind == syntax_kind_ext::GET_ACCESSOR
                || element.kind == syntax_kind_ext::SET_ACCESSOR
            {
                return true;
            }
            let Some(prop) = self.ctx.arena.get_property_assignment(element) else {
                return false;
            };
            self.ctx.arena.get(prop.initializer).is_some_and(|init| {
                init.kind == syntax_kind_ext::ARROW_FUNCTION
                    || init.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            })
        })
    }

    pub(crate) fn suppress_generic_return_context_for_direct_arg_overlap(
        &mut self,
        shape: &tsz_solver::FunctionShape,
        args: &[NodeIndex],
    ) -> bool {
        let return_type_params: FxHashSet<_> =
            common::collect_referenced_types(self.ctx.types, shape.return_type)
                .into_iter()
                .filter_map(|ty| common::type_param_info(self.ctx.types, ty).map(|info| info.name))
                .collect();

        if return_type_params.is_empty() {
            return false;
        }

        for (i, &arg_idx) in args.iter().enumerate() {
            let Some(param_type) = shape.params.get(i).map(|p| p.type_id).or_else(|| {
                shape
                    .params
                    .last()
                    .and_then(|p| p.rest.then_some(p.type_id))
            }) else {
                break;
            };

            if self.expression_needs_contextual_signature_instantiation(arg_idx, Some(param_type)) {
                continue;
            }

            // Skip function expressions: they are deferred arguments whose return
            // types contribute to inference in Round 2. Including them here would
            // incorrectly suppress the contextual return type even when the callback
            // is the sole inference source for the return type's type params.
            // Example: `invoke(() => 1)` where `invoke<T>(f: () => T): T` — the
            // callback `() => 1` references T in its return position, but T has no
            // other inference source; suppressing the contextual type loses the
            // literal preservation.
            if self.ctx.arena.get(arg_idx).is_some_and(|n| {
                n.kind == syntax_kind_ext::ARROW_FUNCTION
                    || n.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            }) {
                continue;
            }

            if is_contextually_sensitive(self, arg_idx)
                || self.object_literal_contains_function_member(arg_idx)
            {
                continue;
            }

            let param_type_params = common::collect_referenced_types(self.ctx.types, param_type);
            if param_type_params.into_iter().any(|ty| {
                common::type_param_info(self.ctx.types, ty)
                    .is_some_and(|info| return_type_params.contains(&info.name))
            }) {
                return true;
            }
        }

        false
    }

    pub(crate) fn sensitive_callback_placeholder_should_skip_round1_inference(
        &mut self,
        callee_shape: &tsz_solver::FunctionShape,
        callback_param_type: TypeId,
    ) -> bool {
        !self
            .sensitive_callback_return_only_type_params(callee_shape, callback_param_type)
            .is_empty()
    }

    pub(crate) fn sensitive_callback_return_only_type_params(
        &mut self,
        callee_shape: &tsz_solver::FunctionShape,
        callback_param_type: TypeId,
    ) -> Vec<tsz_common::interner::Atom> {
        let tracked_type_params: FxHashSet<_> =
            callee_shape.type_params.iter().map(|tp| tp.name).collect();
        if tracked_type_params.is_empty() {
            return Vec::new();
        }

        let callback_shape =
            call_checker::get_contextual_signature(self.ctx.types, callback_param_type)
                .or_else(|| {
                    let evaluated = self.evaluate_type_with_env(callback_param_type);
                    (evaluated != callback_param_type).then(|| {
                        call_checker::get_contextual_signature(self.ctx.types, evaluated)
                    })?
                })
                .or_else(|| {
                    let evaluated = self.evaluate_application_type(callback_param_type);
                    (evaluated != callback_param_type).then(|| {
                        call_checker::get_contextual_signature(self.ctx.types, evaluated)
                    })?
                });
        let Some(callback_shape) = callback_shape else {
            return Vec::new();
        };

        let return_mentions: FxHashSet<_> =
            common::collect_referenced_types(self.ctx.types, callback_shape.return_type)
                .into_iter()
                .filter_map(|ty| {
                    common::type_param_info(self.ctx.types, ty).and_then(|info| {
                        tracked_type_params
                            .contains(&info.name)
                            .then_some(info.name)
                    })
                })
                .collect();
        if return_mentions.is_empty() {
            return Vec::new();
        }

        let param_mentions: FxHashSet<_> = callback_shape
            .params
            .iter()
            .flat_map(|param| {
                common::collect_referenced_types(self.ctx.types, param.type_id).into_iter()
            })
            .filter_map(|ty| {
                common::type_param_info(self.ctx.types, ty).and_then(|info| {
                    tracked_type_params
                        .contains(&info.name)
                        .then_some(info.name)
                })
            })
            .collect();

        return_mentions
            .into_iter()
            .filter(|name| !param_mentions.contains(name))
            .collect()
    }

    pub(crate) fn should_strip_sensitive_placeholder_substitution(
        &mut self,
        callee_shape: &tsz_solver::FunctionShape,
        callback_param_type: TypeId,
        type_param_name: tsz_common::interner::Atom,
        inferred: TypeId,
    ) -> bool {
        if !self
            .sensitive_callback_return_only_type_params(callee_shape, callback_param_type)
            .into_iter()
            .any(|name| name == type_param_name)
        {
            return false;
        }

        inferred == TypeId::ANY
            || inferred == TypeId::UNKNOWN
            || inferred == TypeId::ERROR
            || common::contains_infer_types(self.ctx.types, inferred)
    }

    pub(crate) fn suppress_initializer_contextual_type_for_generic_call(
        &mut self,
        idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION
            && node.kind != syntax_kind_ext::NEW_EXPRESSION
        {
            return false;
        }

        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return false;
        };

        let mut callee_idx = call.expression;
        while let Some(callee_node) = self.ctx.arena.get(callee_idx) {
            if callee_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(callee_node)
            {
                callee_idx = paren.expression;
                continue;
            }
            break;
        }
        if let Some(callee_node) = self.ctx.arena.get(callee_idx)
            && (callee_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || callee_node.kind == syntax_kind_ext::ARROW_FUNCTION)
        {
            return false;
        }

        if call.type_arguments.is_some() {
            return false;
        }

        let callee_type = self.get_type_of_node(call.expression);
        if matches!(
            callee_type,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN | TypeId::NEVER
        ) {
            return false;
        }

        let callee_type_for_context = self.evaluate_application_type(callee_type);
        let callee_type_for_context = self.resolve_lazy_type(callee_type_for_context);
        let args = match call.arguments.as_ref() {
            Some(args) => args.nodes.as_slice(),
            None => &[],
        };
        let Some(shape) =
            crate::query_boundaries::checkers::call::get_contextual_signature_for_arity(
                self.ctx.types,
                callee_type_for_context,
                args.len(),
            )
        else {
            return false;
        };

        if shape.type_params.is_empty() {
            return false;
        }
        // When the return type is a bare type parameter that also appears in
        // parameter position, suppressing the contextual type loses the upper
        // bound that prevents literal widening. For `identity<T>(x: T): T`
        // called as `let v: DooDad = identity('ELSE')`, the contextual type
        // `DooDad` is needed so the solver preserves `"ELSE"` instead of
        // widening to `string`. The solver's `return_type_bare_var` logic
        // already handles priority correctly — it only adds the contextual type
        // as a candidate when no direct argument candidates exist.
        let return_is_bare_type_param = shape.type_params.iter().any(|tp| {
            tsz_solver::type_queries::get_type_parameter_info(self.ctx.types, shape.return_type)
                .is_some_and(|info| info.name == tp.name)
        });
        if return_is_bare_type_param {
            return false;
        }
        self.suppress_generic_return_context_for_direct_arg_overlap(&shape, args)
    }

    /// Whether an argument node needs contextual typing from the callee signature.
    ///
    /// Literal expressions need contextual typing to preserve literal types when
    /// the expected parameter type is a literal union (e.g., `"A"` should remain
    /// `"A"` when passed to a parameter of type `"A" | "B"`).
    ///
    /// Other expressions like arrow functions, object literals, etc. also need
    /// contextual typing for their internal structure.
    pub(crate) fn argument_needs_contextual_type(&self, idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        // Literal expressions need contextual typing to preserve literal types
        // when the expected type is a literal union or specific literal type.
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
            return true;
        }

        match node.kind {
            k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
                is_contextually_sensitive(self, idx)
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::CONDITIONAL_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
            {
                true
            }
            // Parenthesized expressions: recurse into inner expression.
            // A parenthesized identifier like `(identity)` should NOT get contextual
            // typing — identifiers have fixed declared types. Only parenthesized
            // context-sensitive expressions (arrows, object literals, etc.) need it.
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.argument_needs_contextual_type(paren.expression)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub(crate) fn expression_needs_contextual_signature_instantiation(
        &mut self,
        idx: NodeIndex,
        expected_type: Option<TypeId>,
    ) -> bool {
        let Some(expected_type) = expected_type else {
            return false;
        };

        let expected_type = self
            .contextual_type_option_for_expression(Some(expected_type))
            .unwrap_or(expected_type);
        let expected_eval = self.evaluate_type_with_env(expected_type);
        let expected_shape = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            expected_type,
        )
        .or_else(|| {
            crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                expected_eval,
            )
        });
        if expected_shape.is_none() {
            return false;
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        match node.kind {
            k if k == tsz_scanner::SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {}
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                if self
                    .ctx
                    .arena
                    .get_function(node)
                    .and_then(|func| func.type_parameters.as_ref())
                    .is_none_or(|params| params.nodes.is_empty())
                {
                    return false;
                }
            }
            _ => return false,
        }

        let source_type = self.get_type_of_node(expr_idx);
        let source_eval = self.evaluate_type_with_env(source_type);
        crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            source_type,
        )
        .or_else(|| {
            crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                source_eval,
            )
        })
        .is_some_and(|shape| !shape.type_params.is_empty())
    }

    pub(crate) fn argument_needs_refresh_for_contextual_call(
        &mut self,
        idx: NodeIndex,
        expected_type: Option<TypeId>,
    ) -> bool {
        self.argument_needs_contextual_type(idx)
            || self.expression_needs_contextual_signature_instantiation(idx, expected_type)
    }

    pub(crate) fn instantiate_callable_result_from_request(
        &mut self,
        idx: NodeIndex,
        result_type: TypeId,
        request: &TypingRequest,
    ) -> TypeId {
        let Some(expected_type) = request.contextual_type else {
            return result_type;
        };
        if matches!(result_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return result_type;
        }

        let result_eval = self.evaluate_type_with_env(result_type);
        let has_generic_signature =
            crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                result_type,
            )
            .or_else(|| {
                crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    result_eval,
                )
            })
            .is_some_and(|shape| !shape.type_params.is_empty());
        if !has_generic_signature {
            return result_type;
        }

        if self.is_immediate_call_or_new_callee(idx) || !self.is_immediate_call_or_new_argument(idx)
        {
            return result_type;
        }

        let expected_type = self.contextual_type_option_for_expression(Some(expected_type));
        let Some(expected_type) = expected_type else {
            return result_type;
        };

        let instantiated =
            if self.target_has_concrete_return_context_for_generic_refinement(expected_type) {
                self.instantiate_generic_function_argument_against_target_for_refinement(
                    result_type,
                    expected_type,
                )
            } else {
                self.instantiate_generic_function_argument_against_target_params(
                    result_type,
                    expected_type,
                )
            };

        if instantiated == TypeId::ERROR {
            result_type
        } else {
            instantiated
        }
    }
}
