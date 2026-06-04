impl<'a> CheckerState<'a> {
    pub(crate) fn should_defer_contextual_argument_mismatch(
        &mut self,
        actual: TypeId,
        expected: TypeId,
    ) -> bool {
        if self.call_target_generic_rest_requires_fixed_arity_error(actual, expected) {
            return false;
        }
        if common::contains_this_type(self.ctx.types, expected) {
            return false;
        }
        // Bare __infer_N expected + concrete actual: inference is done, mismatch is definitive.
        if common::is_bare_infer_placeholder(self.ctx.types, expected)
            && !assign_query::contains_infer_types(self.ctx.types, actual)
            && actual != expected
        {
            return false;
        }
        // When both types are Applications of the same base (e.g., F<CP> vs F<unknown>),
        // the mismatch comes from variance checking, not from contextual typing.
        // Don't defer — the variance rejection is definitive. This matches tsc which
        // reports TS2345 immediately for same-generic-type argument mismatches.
        if let Some(s_app_id) =
            crate::query_boundaries::common::application_id(self.ctx.types, actual)
            && let Some(t_app_id) =
                crate::query_boundaries::common::application_id(self.ctx.types, expected)
        {
            let s_app = self.ctx.types.type_application(s_app_id);
            let t_app = self.ctx.types.type_application(t_app_id);
            if s_app.base == t_app.base
                && !assign_query::contains_infer_types(self.ctx.types, actual)
                && !assign_query::contains_infer_types(self.ctx.types, expected)
                && !assign_query::contains_type_parameters(self.ctx.types, actual)
                && !assign_query::contains_type_parameters(self.ctx.types, expected)
            {
                return false;
            }
        }
        let has_callable_shape = |this: &mut Self, ty: TypeId| {
            if crate::query_boundaries::common::function_shape_for_type(this.ctx.types, ty)
                .is_some()
            {
                return true;
            }
            if common::callable_shape_for_type(this.ctx.types, ty).is_some() {
                return true;
            }
            let evaluated = this.evaluate_type_with_env(ty);
            crate::query_boundaries::common::function_shape_for_type(this.ctx.types, evaluated)
                .is_some()
                || common::callable_shape_for_type(this.ctx.types, evaluated).is_some()
        };
        let callable_mismatch =
            has_callable_shape(self, actual) && has_callable_shape(self, expected);
        let actual_has_generic_signatures = self.callable_has_own_generic_signatures(actual);
        let expected_has_generic_signatures = self.callable_has_own_generic_signatures(expected);
        let has_construct_signatures = |this: &mut Self, ty: TypeId| {
            common::callable_shape_for_type(this.ctx.types, ty)
                .or_else(|| {
                    let evaluated = this.evaluate_type_with_env(ty);
                    common::callable_shape_for_type(this.ctx.types, evaluated)
                })
                .is_some_and(|shape| !shape.construct_signatures.is_empty())
        };
        let constructor_mismatch =
            has_construct_signatures(self, actual) && has_construct_signatures(self, expected);
        let constructor_generic_mismatch = constructor_mismatch
            && (actual_has_generic_signatures || expected_has_generic_signatures);
        let actual_contains_infer = assign_query::contains_infer_types(self.ctx.types, actual);
        let expected_contains_infer = assign_query::contains_infer_types(self.ctx.types, expected);
        if actual_contains_infer || expected_contains_infer {
            let evaluated_actual = self.evaluate_type_with_env(actual);
            let evaluated_expected = self.evaluate_type_with_env(expected);
            let evaluated_still_has_holes =
                assign_query::contains_infer_types(self.ctx.types, evaluated_actual)
                    || assign_query::contains_infer_types(self.ctx.types, evaluated_expected)
                    || assign_query::contains_type_parameters(self.ctx.types, evaluated_actual)
                    || assign_query::contains_type_parameters(self.ctx.types, evaluated_expected);
            return evaluated_still_has_holes;
        }
        if callable_mismatch {
            let refined_actual = if self
                .target_has_concrete_return_context_for_generic_refinement(expected)
            {
                self.instantiate_generic_function_argument_against_target_for_refinement(
                    actual, expected,
                )
            } else {
                self.instantiate_generic_function_argument_against_target_params(actual, expected)
            };
            let refined_actual = self.normalize_contextual_signature_with_env(refined_actual);
            let refined_expected = self.normalize_contextual_signature_with_env(expected);
            let refined_still_has_holes =
                assign_query::contains_infer_types(self.ctx.types, refined_actual)
                    || assign_query::contains_infer_types(self.ctx.types, refined_expected)
                    || assign_query::contains_type_parameters(self.ctx.types, refined_actual)
                    || assign_query::contains_type_parameters(self.ctx.types, refined_expected);
            if constructor_generic_mismatch {
                return !self
                    .generic_constructor_mismatch_has_uncovered_required_arity(actual, expected);
            }
            if !refined_still_has_holes {
                return false;
            }
            // Defer only when holes are in expected (outer inference will resolve them),
            // not when holes are in actual (those are permanent outer-scope type params).
            if !actual_has_generic_signatures && !expected_has_generic_signatures {
                let actual_has_holes =
                    assign_query::contains_infer_types(self.ctx.types, refined_actual)
                        || assign_query::contains_type_parameters(self.ctx.types, refined_actual);
                if !actual_has_holes {
                    return true;
                }
                let actual_type_params: rustc_hash::FxHashSet<_> =
                    common::collect_referenced_types(self.ctx.types, refined_actual)
                        .into_iter()
                        .filter(|&ty| common::type_param_info(self.ctx.types, ty).is_some())
                        .collect();
                let expected_type_params: rustc_hash::FxHashSet<_> =
                    common::collect_referenced_types(self.ctx.types, refined_expected)
                        .into_iter()
                        .filter(|&ty| common::type_param_info(self.ctx.types, ty).is_some())
                        .collect();
                if !actual_type_params.is_empty()
                    && actual_type_params
                        .iter()
                        .all(|ty| expected_type_params.contains(ty))
                {
                    return true;
                }
            }
        }
        // Defer callable mismatches only when a callable has its own generic signatures
        // (higher-order inference may still resolve them), not for outer-scope type params.
        if callable_mismatch && (actual_has_generic_signatures || expected_has_generic_signatures) {
            // Do not defer when the actual is a same-arity generic function with all type
            // parameters constrained but the expected has none constrained. That is a
            // structural constraint-strictness mismatch — inference cannot resolve it.
            if assign_query::generic_arg_constraint_mismatch_is_structural(
                self.ctx.types,
                actual,
                expected,
            ) {
                return false;
            }
            return true;
        }
        if !callable_mismatch
            && assign_query::contains_type_parameters(self.ctx.types, expected)
            && assign_query::contains_any_type(self.ctx.types, actual)
        {
            return true;
        }
        if !callable_mismatch
            && assign_query::contains_type_parameters(self.ctx.types, actual)
            && assign_query::contains_type_parameters(self.ctx.types, expected)
        {
            // Don't defer when the base types of generic instantiations are different
            // classes. For example, B<T> vs A<T> where A has private members should
            // NOT be deferred — the mismatch is structural and type parameter resolution
            // cannot fix it. Only defer when the types could plausibly become compatible
            // once type parameters are resolved.
            if self.are_incompatible_generic_class_instances(actual, expected) {
                return false;
            }
            // When both sides are *bare* `TypeParameter` types with different
            // identities, neither side is in flight. Distinct enclosing-scope
            // type parameters never become equal under inference, so the
            // solver's rejection is permanent — deferring would silently drop
            // a real TS2345. Mirrors `(x: T) => void` vs `(x: U) => void` for
            // non-callable bare type parameters.
            //
            // `Infer` types are excluded so in-flight `infer T` placeholders
            // inside conditional inference still defer.
            if actual != expected
                && crate::query_boundaries::checkers::generic::is_bare_named_type_parameter(
                    self.ctx.types,
                    actual,
                )
                && crate::query_boundaries::checkers::generic::is_bare_named_type_parameter(
                    self.ctx.types,
                    expected,
                )
            {
                return false;
            }
            return true;
        }
        assign_query::is_any_type(self.ctx.types, expected)
    }

    /// Check if `actual` and `expected` are generic instantiations of different classes.
    ///
    /// When `B<T>` and `A<T>` are Applications of different class definitions,
    /// the mismatch is structural (e.g., different private brands) and cannot be
    /// resolved by type parameter instantiation. In this case, deferral is incorrect.
    fn are_incompatible_generic_class_instances(&self, actual: TypeId, expected: TypeId) -> bool {
        use crate::query_boundaries::common::{application_id, lazy_def_id};

        let db = self.ctx.types;

        // Extract the base DefId from a class Application type (e.g., A<T> -> DefId_A).
        // Type aliases such as Partial<T> remain transparent enough for deferred
        // assignability; only nominal class bases make the rejection permanent.
        let base_def = |ty: TypeId| -> Option<tsz_solver::DefId> {
            let app_id = application_id(db, ty)?;
            let app = db.type_application(app_id);
            let def_id = lazy_def_id(db, app.base)?;
            matches!(
                TypeResolver::get_def_kind(&self.ctx, def_id),
                Some(tsz_solver::def::DefKind::Class)
            )
            .then_some(def_id)
        };

        let actual_def = base_def(actual);
        let expected_def = base_def(expected);

        match (actual_def, expected_def) {
            (Some(a), Some(e)) => a != e,
            _ => false,
        }
    }

    fn call_target_generic_rest_requires_fixed_arity_error(
        &mut self,
        actual: TypeId,
        expected: TypeId,
    ) -> bool {
        let normalize = |shape: tsz_solver::FunctionShape| {
            let mut normalized = shape.clone();
            normalized.params = shape
                .params
                .iter()
                .flat_map(|param| common::unpack_tuple_rest_parameter(self.ctx.types, param))
                .collect();
            normalized
        };

        let actual = self.normalize_contextual_signature_with_env(actual);
        let expected = self.normalize_contextual_signature_with_env(expected);
        let Some(actual_shape) = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            actual,
        ) else {
            return false;
        };
        let Some(expected_shape) =
            crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                expected,
            )
        else {
            return false;
        };

        let actual_shape = normalize(actual_shape);
        let expected_shape = normalize(expected_shape);
        let Some(expected_rest) = expected_shape.params.last().filter(|param| param.rest) else {
            return false;
        };

        if !common::is_type_parameter_like(self.ctx.types, expected_rest.type_id)
            && !common::contains_type_parameters(self.ctx.types, expected_rest.type_id)
        {
            return false;
        }

        let actual_required = actual_shape
            .params
            .iter()
            .filter(|param| !param.optional && !param.rest)
            .count();
        let expected_fixed = expected_shape.params.len().saturating_sub(1);
        actual_required > expected_fixed
    }

    pub(crate) fn suppress_later_call_excess_property_diagnostics(
        &mut self,
        args: &[NodeIndex],
        primary_arg_idx: NodeIndex,
    ) {
        let Some(primary_pos) = args.iter().position(|&arg| arg == primary_arg_idx) else {
            return;
        };
        let later_spans: Vec<(u32, u32)> = args[primary_pos + 1..]
            .iter()
            .filter_map(|&arg_idx| {
                self.get_node_span(arg_idx)
                    .map(|(start, len)| (start, start.saturating_add(len)))
            })
            .collect();
        if later_spans.is_empty() {
            return;
        }
        self.ctx.diagnostics.retain(|diag| {
            if diag.code
                != diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
            {
                return true;
            }
            !later_spans
                .iter()
                .any(|&(start, end)| diag.start >= start && diag.start < end)
        });
        self.ctx.rebuild_emitted_diagnostics_from_current();
    }

    pub(crate) fn build_expanded_args_for_error(&mut self, args: &[NodeIndex]) -> Vec<NodeIndex> {
        let mut expanded = Vec::with_capacity(args.len());
        for &arg_idx in args {
            if let Some(n) = self.ctx.arena.get(arg_idx)
                && n.kind == syntax_kind_ext::SPREAD_ELEMENT
                && let Some(spread_expression) = self
                    .ctx
                    .arena
                    .get_spread(n)
                    .map(|spread| spread.expression)
                    .or_else(|| self.ctx.arena.get_children(arg_idx).first().copied())
            {
                let spread_type = self.get_type_of_node(spread_expression);
                let spread_type = self.resolve_type_for_property_access(spread_type);
                let spread_type = self.resolve_lazy_type(spread_type);
                if let Some(elems) =
                    crate::query_boundaries::common::tuple_elements(self.ctx.types, spread_type)
                {
                    expanded.extend(std::iter::repeat_n(arg_idx, elems.len()));
                    continue;
                }
                // Array literal spreads have known element count — expand them
                let inner_idx = self.ctx.arena.skip_parenthesized(spread_expression);
                if let Some(expr_node) = self.ctx.arena.get(inner_idx)
                    && expr_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    && let Some(literal) = self.ctx.arena.get_literal_expr(expr_node)
                {
                    expanded.extend(std::iter::repeat_n(arg_idx, literal.elements.nodes.len()));
                    continue;
                }
            }
            expanded.push(arg_idx);
        }
        expanded
    }

    /// Check if TS2769 (no overload matches) should be suppressed due to structural
    /// errors on the callee type. When a class/interface has structural errors
    /// (TS2420, TS2430, TS2694), we suppress "no overload matches" errors because
    /// the type is known to be broken and the primary errors should be shown instead.
    fn should_suppress_no_overload_due_to_structural_errors(
        &mut self,
        callee_expr: NodeIndex,
    ) -> bool {
        // Only check for property access expressions (e.g., Promise.try)
        let Some(callee_node) = self.ctx.arena.get(callee_expr) else {
            return false;
        };

        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };

        // Get the base expression (e.g., Promise in Promise.try)
        let base_expr = access.expression;

        // Resolve the base identifier to its symbol
        let Some(symbol_id) = self.resolve_identifier_symbol(base_expr) else {
            return false;
        };

        // Check if this symbol has structural error diagnostics
        self.symbol_has_structural_errors(symbol_id)
    }

    fn should_suppress_no_overload_due_to_callback_body_errors(&self, args: &[NodeIndex]) -> bool {
        const CALLBACK_BODY_DIAGNOSTIC_CODES: &[u32] = &[2322, 2339, 2345, 2347, 7006, 7019, 7031];

        args.iter().copied().any(|arg_idx| {
            self.is_callback_like_argument(arg_idx)
                && self
                    .callback_body_spans(arg_idx)
                    .iter()
                    .any(|(start, end)| {
                        self.ctx.diagnostics.iter().any(|diag| {
                            diag.start >= *start
                                && diag.start < *end
                                && CALLBACK_BODY_DIAGNOSTIC_CODES.contains(&diag.code)
                        })
                    })
        })
    }

    /// Check if a symbol has structural error diagnostics (TS2420, TS2430, TS2694).
    fn symbol_has_structural_errors(&self, symbol_id: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) else {
            return false;
        };

        let structural_error_codes = [
            diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
            diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
            diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER,
        ];

        // Check if any structural error diagnostics are within this symbol's declaration spans
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let decl_start = node.pos;
            let decl_end = node.end;

            for diag in &self.ctx.diagnostics {
                if structural_error_codes.contains(&diag.code)
                    && diag.start >= decl_start
                    && diag.start < decl_end
                {
                    return true;
                }
            }
        }

        false
    }
}
