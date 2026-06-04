impl<'a> CheckerState<'a> {
    fn direct_const_type_param_requires_readonly_argument_context(
        db: &dyn tsz_solver::construction::TypeDatabase,
        type_id: TypeId,
    ) -> bool {
        use crate::query_boundaries::common;

        let Some(info) = common::type_param_info(db, type_id) else {
            return false;
        };
        if !info.is_const {
            return false;
        }
        !info
            .constraint
            .is_some_and(|constraint| Self::constraint_allows_mutable_array_like(db, constraint))
    }

    pub(super) fn constraint_allows_mutable_array_like(
        db: &dyn tsz_solver::construction::TypeDatabase,
        type_id: TypeId,
    ) -> bool {
        crate::query_boundaries::common::constraint_allows_mutable_array_like(db, type_id)
    }

    /// Check excess properties on call arguments that are object literals.
    pub(super) fn check_call_argument_excess_properties<F>(
        &mut self,
        args: &[NodeIndex],
        arg_types: &[TypeId],
        mut expected_for_index: F,
    ) where
        F: FnMut(usize, usize) -> Option<TypeId>,
    {
        let arg_count = args.len();
        for (i, &arg_idx) in args.iter().enumerate() {
            let expected = expected_for_index(i, arg_count);
            if let Some(expected) = expected {
                self.try_emit_polymorphic_this_object_literal_arg_errors(arg_idx, expected);
            }
            if let Some(expected) = expected
                && expected != TypeId::ANY
                && expected != TypeId::UNKNOWN
                && let Some(arg_node) = self.ctx.arena.get(arg_idx)
                && arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                // Skip excess property checking for type parameters - the type parameter
                // captures the full object type, so extra properties are allowed.
                && !is_type_parameter_type(self.ctx.types, expected)
                // Also skip when the original parameter type contains a type parameter
                // (set via generic_excess_skip for generic call paths).
                && !self.ctx.generic_excess_skip.as_ref().is_some_and(|skip| {
                    i < skip.len() && skip[i]
                })
                && !self.contextual_type_is_unresolved_for_argument_refresh(expected)
            {
                let arg_type = arg_types.get(i).copied().unwrap_or(TypeId::UNKNOWN);
                self.check_object_literal_excess_properties(arg_type, expected, arg_idx);
            }
        }
    }

    pub(super) fn validate_non_tuple_spreads_for_signature(
        &mut self,
        args: &[NodeIndex],
        func_type: TypeId,
    ) {
        let ctx = ContextualTypeContext::with_expected(self.ctx.types, func_type);
        let mut expanded_count = 0usize;
        for &arg_idx in args {
            if let Some(arg_node) = self.ctx.arena.get(arg_idx)
                && arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                && let Some(spread_data) = self.ctx.arena.get_spread(arg_node)
            {
                let spread_type = self.normalized_spread_argument_type(spread_data.expression);
                if let Some(elems) = tuple_elements_for_type(self.ctx.types, spread_type) {
                    expanded_count += elems.len();
                    continue;
                }
                if array_element_type_for_type(self.ctx.types, spread_type).is_some()
                    && let Some(expr_node) = self.ctx.arena.get(spread_data.expression)
                    && let Some(literal) = self.ctx.arena.get_literal_expr(expr_node)
                {
                    expanded_count += literal.elements.nodes.len();
                    continue;
                }
            }
            expanded_count += 1;
        }

        let mut effective_index = 0usize;
        for &arg_idx in args {
            let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                effective_index += 1;
                continue;
            };
            if arg_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
                effective_index += 1;
                continue;
            }
            let Some(spread_data) = self.ctx.arena.get_spread(arg_node) else {
                effective_index += 1;
                continue;
            };
            let spread_type = self.normalized_spread_argument_type(spread_data.expression);
            if let Some(elems) = tuple_elements_for_type(self.ctx.types, spread_type) {
                effective_index += elems.len();
                continue;
            }
            // An array literal spread (e.g. `...['a', 'x']`) is expanded element-by-element
            // during argument collection, so each element is checked individually against
            // the corresponding parameter. Treat it like a tuple-like spread here: advance
            // by the literal's element count and skip the TS2556 emission. tsc behaves the
            // same way — TS2556 is only reported for spreads of opaque arrays/iterables
            // whose runtime length is unknown at the call site.
            if array_element_type_for_type(self.ctx.types, spread_type).is_some()
                && let Some(expr_node) = self
                    .ctx
                    .arena
                    .get(self.ctx.arena.skip_parenthesized(spread_data.expression))
                && let Some(literal) = self.ctx.arena.get_literal_expr(expr_node)
            {
                effective_index += literal.elements.nodes.len();
                continue;
            }
            if is_type_parameter_type(self.ctx.types, spread_type)
                && let Some(constraint) = crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    spread_type,
                )
                && (array_element_type_for_type(self.ctx.types, constraint).is_some()
                    || tuple_elements_for_type(self.ctx.types, constraint).is_some())
            {
                effective_index += 1;
                continue;
            }
            let is_non_tuple_spread = array_element_type_for_type(self.ctx.types, spread_type)
                .is_some()
                || self.is_iterable_type(spread_type);
            if is_non_tuple_spread
                && !ctx.allows_non_tuple_spread_position(effective_index, expanded_count)
            {
                self.error_spread_must_be_tuple_or_rest_at(arg_idx);
                return;
            }
            effective_index += 1;
        }
    }

    pub(super) fn find_prior_non_tuple_spread_for_mismatch(
        &mut self,
        args: &[NodeIndex],
        mismatch_index: usize,
    ) -> Option<NodeIndex> {
        let mut effective_index = 0usize;
        let mut prior_non_tuple_spread = None;

        for &arg_idx in args {
            if effective_index > mismatch_index {
                break;
            }
            let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                effective_index += 1;
                continue;
            };
            if arg_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
                if effective_index == mismatch_index {
                    return prior_non_tuple_spread;
                }
                effective_index += 1;
                continue;
            }
            let Some(spread_data) = self.ctx.arena.get_spread(arg_node) else {
                effective_index += 1;
                continue;
            };
            let spread_type = self.normalized_spread_argument_type(spread_data.expression);
            if let Some(elems) = tuple_elements_for_type(self.ctx.types, spread_type) {
                if mismatch_index < effective_index + elems.len() {
                    return prior_non_tuple_spread;
                }
                effective_index += elems.len();
                continue;
            }
            // An array literal spread (e.g. `...['a', 'x']`) is expanded element-by-element
            // during argument collection. A mismatch at one of those expanded indices is a
            // per-element type error (TS2345/TS2322), not a TS2556. Skip past the literal's
            // elements without setting `prior_non_tuple_spread`.
            if array_element_type_for_type(self.ctx.types, spread_type).is_some()
                && let Some(expr_node) = self
                    .ctx
                    .arena
                    .get(self.ctx.arena.skip_parenthesized(spread_data.expression))
                && let Some(literal) = self.ctx.arena.get_literal_expr(expr_node)
            {
                let count = literal.elements.nodes.len();
                if mismatch_index < effective_index + count {
                    return prior_non_tuple_spread;
                }
                effective_index += count;
                continue;
            }
            let is_non_tuple_spread = array_element_type_for_type(self.ctx.types, spread_type)
                .is_some()
                || self.is_iterable_type(spread_type);
            if effective_index == mismatch_index {
                return prior_non_tuple_spread;
            }
            if is_non_tuple_spread {
                prior_non_tuple_spread = Some(arg_idx);
            }
            effective_index += 1;
        }

        prior_non_tuple_spread
    }
}
