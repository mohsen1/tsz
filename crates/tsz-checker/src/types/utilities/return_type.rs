//! Return type inference utilities for `CheckerState`.
//!
//! Functions for inferring return types from function bodies by collecting
//! return expressions, analyzing control flow (fall-through detection),
//! and checking for explicit `any` assertion returns.

use crate::context::TypingRequest;
use crate::query_boundaries::function_returns as return_type_queries;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// One block-body return contribution, collected *unwidened* so that a union of
/// distinct fresh literals (`return "a"; return "b"` → `"a" | "b"`) is preserved
/// rather than widened per branch. `widen_expr` is `Some(expr)` when this
/// contribution is a fresh-literal return that would be widened on its own (no
/// contextual / `satisfies` / `preserve_literal` / `const`-assertion /
/// conditional carve-out); it is the expression node so the AST-aware widener
/// (`widen_return_contribution_preserving_const`) can run if the union collapses
/// to a single literal. See `infer_return_type_from_body_inner` (#14530).
struct ReturnContribution {
    type_id: TypeId,
    widen_expr: Option<NodeIndex>,
}

impl<'a> CheckerState<'a> {
    fn inference_context_for_block_return_expression(
        &mut self,
        expr_idx: NodeIndex,
        return_context: Option<TypeId>,
    ) -> Option<TypeId> {
        let return_context = return_context?;
        // Only suppress the contextual return type when it is still genuinely
        // uninstantiated, i.e. it carries *free* type parameters or `infer`
        // placeholders (e.g. the bare `T` of `<T>() => T`, where contextual
        // typing carries no useful information). A type parameter *bound* by a
        // generic member inside an otherwise concrete context — such as the `K`
        // of `addEventListener<K extends keyof M>(...)` reachable through a
        // concrete type argument like `MessagePort` — is fully resolved and must
        // not block contextual typing. The broad `contains_type_parameters`
        // predicate counts those bound members and wrongly widened a concrete
        // contextual return such as `[MessagePort, number]` to a plain array; the
        // free-variable predicate keeps it flowing into block-body return
        // literals so they type as tuples.
        if return_context == TypeId::ANY
            || return_context == TypeId::UNKNOWN
            || return_type_queries::contains_free_type_parameters(self.ctx.types, return_context)
            || return_type_queries::contains_infer_types(self.ctx.types, return_context)
        {
            return None;
        }

        crate::computation::contextual::expression_needs_contextual_return_type(self, expr_idx)
            .then(|| self.contextual_type_for_expression(return_context))
    }

    fn should_preserve_tuple_literals_for_generic_return(
        &self,
        expr_idx: NodeIndex,
        return_context: Option<TypeId>,
        effective_return_context: Option<TypeId>,
    ) -> bool {
        let preserve_for_bare_generic_return =
            return_context.is_some() && effective_return_context.is_none();
        let preserve_for_async_array_return =
            return_context.is_some_and(|ctx| self.return_context_is_async_array_union_context(ctx));
        if !preserve_for_bare_generic_return && !preserve_for_async_array_return {
            return false;
        }

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => true,
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                self.ctx
                    .arena
                    .get_conditional_expr(node)
                    .is_some_and(|cond| {
                        self.ctx
                            .arena
                            .get(cond.when_true)
                            .is_some_and(|n| n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
                            && self.ctx.arena.get(cond.when_false).is_some_and(|n| {
                                n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            })
                    })
            }
            _ => false,
        }
    }

    fn return_context_is_async_array_union_context(&self, return_context: TypeId) -> bool {
        let Some(members) = return_type_queries::union_members(self.ctx.types, return_context)
        else {
            return false;
        };

        let mut saw_array = false;
        let mut saw_promise_wrapped_array = false;
        for member in members {
            if return_type_queries::array_element_type(self.ctx.types, member).is_some() {
                saw_array = true;
                continue;
            }

            if let Some((base, args)) =
                return_type_queries::application_info(self.ctx.types, member)
                && args.len() == 1
                && self.return_context_application_base_is_lib_promise_like(base)
                && return_type_queries::array_element_type(self.ctx.types, args[0]).is_some()
            {
                saw_promise_wrapped_array = true;
            }
        }

        saw_array && saw_promise_wrapped_array
    }

    /// Check if a function body falls through (doesn't always return).
    ///
    /// This function determines whether a function body might fall through
    /// without an explicit return statement. This is important for return type
    /// inference and validating function return annotations.
    ///
    /// ## Returns:
    /// - `true`: The function might fall through (no guaranteed return)
    /// - `false`: The function always returns (has return in all code paths)
    ///
    /// ## Examples:
    /// ```typescript
    /// // Falls through:
    /// function foo() {  // No return statement
    /// }
    ///
    /// function bar() {
    ///     if (cond) { return 1; }  // Might not return
    /// }
    ///
    /// // Doesn't fall through:
    /// function baz() {
    ///     return 1;
    /// }
    /// ```
    /// Lightweight AST scan: does the function body contain any `throw` statements?
    /// This is used as a pre-check before the more expensive `function_body_falls_through`
    /// to avoid triggering type evaluation in simple function bodies that obviously fall through.
    fn body_contains_throw_or_never_call(&self, body_idx: NodeIndex) -> bool {
        fn scan_stmts(arena: &tsz_parser::parser::NodeArena, stmts: &[NodeIndex]) -> bool {
            use tsz_parser::parser::syntax_kind_ext;
            for &idx in stmts {
                let Some(node) = arena.get(idx) else {
                    continue;
                };
                match node.kind {
                    syntax_kind_ext::THROW_STATEMENT => return true,
                    syntax_kind_ext::BLOCK => {
                        if let Some(block) = arena.get_block(node)
                            && scan_stmts(arena, &block.statements.nodes)
                        {
                            return true;
                        }
                    }
                    syntax_kind_ext::IF_STATEMENT => {
                        if let Some(if_data) = arena.get_if_statement(node) {
                            if scan_stmts(arena, &[if_data.then_statement]) {
                                return true;
                            }
                            if if_data.else_statement.is_some()
                                && scan_stmts(arena, &[if_data.else_statement])
                            {
                                return true;
                            }
                        }
                    }
                    syntax_kind_ext::TRY_STATEMENT => {
                        if let Some(try_data) = arena.get_try(node)
                            && scan_stmts(arena, &[try_data.try_block])
                        {
                            return true;
                        }
                    }
                    syntax_kind_ext::SWITCH_STATEMENT => {
                        if let Some(switch_data) = arena.get_switch(node)
                            && let Some(cb_node) = arena.get(switch_data.case_block)
                            && let Some(cb) = arena.get_block(cb_node)
                        {
                            for &clause_idx in &cb.statements.nodes {
                                if let Some(cn) = arena.get(clause_idx)
                                    && let Some(clause) = arena.get_case_clause(cn)
                                    && scan_stmts(arena, &clause.statements.nodes)
                                {
                                    return true;
                                }
                            }
                        }
                    }
                    // Expression statements could contain never-returning calls,
                    // but detecting those requires type checking. We conservatively
                    // return false here; the full falls_through check will catch them.
                    _ => {}
                }
            }
            false
        }

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };
        if body_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.ctx.arena.get_block(body_node)
        {
            return scan_stmts(self.ctx.arena, &block.statements.nodes);
        }
        false
    }

    pub fn function_body_falls_through(&mut self, body_idx: NodeIndex) -> bool {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return true;
        };
        if body_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.ctx.arena.get_block(body_node)
        {
            return self.block_falls_through(&block.statements.nodes);
        }
        false
    }

    /// Infer the return type of a function body by collecting return expressions.
    ///
    /// This function walks through all statements in a function body, collecting
    /// the types of all return expressions. It then infers the return type as:
    /// - `void`: If there are no return expressions
    /// - `union` of all return types: If there are multiple return expressions
    /// - The single return type: If there's only one return expression
    ///
    /// ## Parameters:
    /// - `body_idx`: The function body node index
    /// - `return_context`: Optional contextual type for return expressions
    ///
    /// ## Examples:
    /// ```typescript
    /// // No returns → void
    /// function foo() {}
    ///
    /// // Single return → string
    /// function bar() { return "hello"; }
    ///
    /// // Multiple returns → string | number
    /// function baz() {
    ///     if (cond) return "hello";
    ///     return 42;
    /// }
    ///
    /// // Empty return included → string | number | void
    /// function qux() {
    ///     if (cond) return;
    ///     return "hello";
    /// }
    /// ```
    pub(crate) fn has_only_explicit_any_assertion_returns(&mut self, body_idx: NodeIndex) -> bool {
        if body_idx.is_none() {
            return false;
        }
        let mut saw_value_return = false;
        let mut all_value_returns_explicit_any = true;
        self.collect_explicit_any_assertion_returns(
            body_idx,
            &mut saw_value_return,
            &mut all_value_returns_explicit_any,
        );
        saw_value_return && all_value_returns_explicit_any
    }

    fn collect_explicit_any_assertion_returns(
        &mut self,
        stmt_idx: NodeIndex,
        saw_value_return: &mut bool,
        all_value_returns_explicit_any: &mut bool,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(return_data) = self.ctx.arena.get_return_statement(node)
                    && return_data.expression.is_some()
                {
                    *saw_value_return = true;
                    if !self.is_explicit_any_assertion_expression(return_data.expression) {
                        *all_value_returns_explicit_any = false;
                    }
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_explicit_any_assertion_returns(
                            stmt,
                            saw_value_return,
                            all_value_returns_explicit_any,
                        );
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    self.collect_explicit_any_assertion_returns(
                        if_data.then_statement,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                    if if_data.else_statement.is_some() {
                        self.collect_explicit_any_assertion_returns(
                            if_data.else_statement,
                            saw_value_return,
                            all_value_returns_explicit_any,
                        );
                    }
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node)
                    && let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block)
                    && let Some(case_block) = self.ctx.arena.get_block(case_block_node)
                {
                    for &clause_idx in &case_block.statements.nodes {
                        if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                            && let Some(clause) = self.ctx.arena.get_case_clause(clause_node)
                        {
                            for &stmt in &clause.statements.nodes {
                                self.collect_explicit_any_assertion_returns(
                                    stmt,
                                    saw_value_return,
                                    all_value_returns_explicit_any,
                                );
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_explicit_any_assertion_returns(
                        try_data.try_block,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                    if try_data.catch_clause.is_some() {
                        self.collect_explicit_any_assertion_returns(
                            try_data.catch_clause,
                            saw_value_return,
                            all_value_returns_explicit_any,
                        );
                    }
                    if try_data.finally_block.is_some() {
                        self.collect_explicit_any_assertion_returns(
                            try_data.finally_block,
                            saw_value_return,
                            all_value_returns_explicit_any,
                        );
                    }
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_explicit_any_assertion_returns(
                        catch_data.block,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_explicit_any_assertion_returns(
                        loop_data.statement,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(for_in_of_data) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_explicit_any_assertion_returns(
                        for_in_of_data.statement,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled_data) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_explicit_any_assertion_returns(
                        labeled_data.statement,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                }
            }
            _ => {}
        }
    }

    /// Apply literal widening to a single return expression's inferred type,
    /// matching tsc's `getReturnTypeFromBody` widening rules per-contribution:
    ///
    /// - When the function has a contextual return type, do not widen — except
    ///   inside a `satisfies` operand. A `satisfies` type only *validates* the
    ///   operand; it does not pin the body literal unless it actually contains
    ///   that literal (`isLiteralOfContextualType`). So in a `satisfies` operand
    ///   a non-pinning contextual return (`unknown`, `any`, a base primitive, an
    ///   object/function type — as in `() => 1 satisfies () => unknown`) widens
    ///   the fresh literal just like the no-context case, per tsc's
    ///   `getWidenedLiteralLikeTypeForContextualType`, while a literal/literal
    ///   union (`satisfies () => 1`) keeps it. Outside `satisfies` the contextual
    ///   return is a genuine contextual position that already shaped the literal,
    ///   so it is preserved unchanged.
    /// - When the outer scope requested literal preservation
    ///   (`preserve_literal_types`), do not widen.
    /// - When the return expression is wrapped in a const assertion
    ///   (`return x as const` or `return <const>x`), preserve the asserted
    ///   literal type even without a contextual return type. tsc keeps the
    ///   const-asserted literal as the inferred return type.
    /// - Otherwise widen literal types only when the return expression is fresh
    ///   (`return "a"` → return type `string`). Non-fresh references such as
    ///   parameters or annotated locals keep their declared literal-union type.
    fn maybe_widen_return_contribution(
        &mut self,
        expr_idx: NodeIndex,
        type_id: TypeId,
        return_context: Option<TypeId>,
    ) -> TypeId {
        if self.return_contribution_is_widenable(expr_idx, type_id, return_context) {
            return self.widen_return_contribution_preserving_const(expr_idx, type_id);
        }
        type_id
    }

    /// Whether a single return-expression contribution would be widened by
    /// `maybe_widen_return_contribution` — i.e. it is a fresh literal expression
    /// with none of the per-expression carve-outs (contextual return type,
    /// `satisfies`, `preserve_literal_types`, a `const` assertion, or a
    /// conditional expression).
    ///
    /// Block-body inference collects the *unwidened* contributions, unions them,
    /// and widens the union only when it collapses to a single literal (tsc's
    /// `getWidenedType(getUnionType(unwidenedReturnTypes))`). Two distinct fresh
    /// literals (`return "a"; return "b"`) must stay a literal union, so the
    /// per-branch widen is deferred to that single-literal check. This predicate
    /// records, per branch, whether that survivor would have been widenable.
    fn return_contribution_is_widenable(
        &mut self,
        expr_idx: NodeIndex,
        type_id: TypeId,
        return_context: Option<TypeId>,
    ) -> bool {
        if let Some(ctx_type) = return_context
            && (!self.ctx.in_satisfies_operand
                || self.contextual_type_allows_literal(ctx_type, type_id))
        {
            return false;
        }
        if self.ctx.preserve_literal_types {
            return false;
        }
        if self.return_expression_is_const_assertion(expr_idx) {
            return false;
        }
        if self.ctx.arena.get(expr_idx).is_some_and(|node| {
            node.kind == tsz_parser::parser::syntax_kind_ext::CONDITIONAL_EXPRESSION
        }) {
            return false;
        }
        self.is_fresh_literal_expression(expr_idx)
    }

    /// Widen a fresh return-expression contribution while preserving literal
    /// property types whose object-literal initializer is a const assertion.
    ///
    /// tsc's `getWidenedType` only widens types carrying the widening flag. A
    /// per-property const assertion such as `{ type: "tracked" as const }`
    /// produces a *regular* (non-widening) literal, so the inferred return type
    /// keeps `type: "tracked"` while still widening its non-asserted siblings
    /// (`store: "x"` → `store: string`). This matters for discriminated-union
    /// narrowing on the inferred return type: widening the discriminant to
    /// `string` collapses the union and produces false `TS2339`/`TS2322`.
    ///
    /// The plain `widen_literal_type` widens every literal leaf unconditionally,
    /// so this AST-driven walk recurses through object-literal initializers and
    /// preserves the const-asserted subtrees, mirroring the const-assertion
    /// carve-out already applied to whole-expression `return x as const`.
    fn widen_return_contribution_preserving_const(
        &mut self,
        expr_idx: NodeIndex,
        type_id: TypeId,
    ) -> TypeId {
        let expr_idx = self.unwrap_parenthesized_expression(expr_idx);

        // A const-asserted subtree is preserved wholesale.
        if self.return_expression_is_const_assertion(expr_idx) {
            return type_id;
        }

        // Only object literals need per-property preservation. Other fresh
        // expressions (bare literals, array literals, template/conditional)
        // keep the existing blanket widening.
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return self.widen_literal_type(type_id);
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return self.widen_literal_type(type_id);
        }
        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return self.widen_literal_type(type_id);
        };
        let Some(shape) =
            crate::query_boundaries::widening::object_shape_for_type(self.ctx.types, type_id)
        else {
            return self.widen_literal_type(type_id);
        };

        // Map declared property names to their initializer expression so each
        // shape property can consult its own AST node. Spread/shorthand members
        // are not recorded; their properties fall back to plain widening (a
        // no-op for the annotated/non-fresh types that spreads contribute).
        let element_nodes: Vec<NodeIndex> = obj.elements.nodes.clone();
        let mut initializer_for: rustc_hash::FxHashMap<String, NodeIndex> =
            rustc_hash::FxHashMap::default();
        for element_idx in element_nodes {
            let Some(element) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                && let Some(prop) = self.ctx.arena.get_property_assignment(element)
                && let Some(name) = self.get_property_name(prop.name)
            {
                initializer_for.insert(name, prop.initializer);
            }
        }

        let mut new_props = Vec::with_capacity(shape.properties.len());
        let mut changed = false;
        for prop in &shape.properties {
            let name = self.ctx.types.resolve_atom(prop.name);
            let widened_type = match initializer_for.get(name.as_str()) {
                // Recurse so nested object-literal const assertions
                // (`{ outer: { type: "x" as const } }`) are preserved too.
                Some(&init_idx) => {
                    self.widen_return_contribution_preserving_const(init_idx, prop.type_id)
                }
                // Spread/shorthand-sourced property: widen as before.
                None => self.widen_literal_type(prop.type_id),
            };
            if widened_type != prop.type_id {
                changed = true;
            }
            let mut new_prop = prop.clone();
            new_prop.type_id = widened_type;
            new_prop.write_type = widened_type;
            new_props.push(new_prop);
        }

        if !changed {
            return type_id;
        }

        crate::query_boundaries::widening::rebuild_object_with_shape_metadata(
            self.ctx.types,
            type_id,
            &shape,
            new_props,
        )
    }

    fn unwrap_parenthesized_expression(&self, expr_idx: NodeIndex) -> NodeIndex {
        let mut current = expr_idx;
        while let Some(node) = self.ctx.arena.get(current) {
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            break;
        }
        current
    }

    pub(crate) fn maybe_evaluate_inferred_return_contribution(
        &mut self,
        type_id: TypeId,
        return_context: Option<TypeId>,
    ) -> TypeId {
        if return_context.is_some() || self.ctx.emit_declarations() {
            return type_id;
        }

        if return_type_queries::lazy_def_id(self.ctx.types, type_id).is_some() {
            return type_id;
        }

        self.record_index_access_value_type(type_id)
            .map(|value| self.maybe_evaluate_inferred_return_contribution(value, None))
            .unwrap_or(type_id)
    }

    fn record_index_access_value_type(&self, type_id: TypeId) -> Option<TypeId> {
        let (object_type, _index_type) =
            return_type_queries::index_access_types(self.ctx.types, type_id)?;
        let app_type = self
            .ctx
            .types
            .get_display_alias(object_type)
            .unwrap_or(object_type);
        let app = return_type_queries::type_application(self.ctx.types, app_type)?;
        if !self.application_alias_maps_keys_to_second_type_arg(app.base) || app.args.len() != 2 {
            return None;
        }
        Some(app.args[1])
    }

    fn application_alias_maps_keys_to_second_type_arg(&self, base: TypeId) -> bool {
        let Some(def_id) = return_type_queries::lazy_def_id(self.ctx.types, base) else {
            return false;
        };
        let Some(def) = self.ctx.definition_store.get(def_id) else {
            return false;
        };
        if def.kind != tsz_solver::def::DefKind::TypeAlias || def.type_params.len() != 2 {
            return false;
        }
        let Some(body) = def.body else {
            return false;
        };
        let Some(mapped) = return_type_queries::mapped_type_info(self.ctx.types, body) else {
            return false;
        };
        let Some(template_param) =
            return_type_queries::type_param_info(self.ctx.types, mapped.template)
        else {
            return false;
        };

        mapped.name_type.is_none()
            && mapped.type_param.name == def.type_params[0].name
            && template_param.name == def.type_params[1].name
    }

    /// Structurally detect whether a return expression is a const assertion
    /// (`expr as const` or `<const>expr`), skipping any wrapping parentheses.
    /// Mirrors the detection in `dispatch.rs` that toggles `in_const_assertion`
    /// for type-assertion nodes.
    fn return_expression_is_const_assertion(&self, expr_idx: NodeIndex) -> bool {
        let mut current = expr_idx;
        while let Some(node) = self.ctx.arena.get(current) {
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if (node.kind == syntax_kind_ext::AS_EXPRESSION
                || node.kind == syntax_kind_ext::TYPE_ASSERTION)
                && let Some(assertion) = self.ctx.arena.get_type_assertion(node)
            {
                return self.is_const_assertion_type_node(assertion.type_node);
            }
            return false;
        }
        false
    }

    fn is_explicit_any_assertion_expression(&mut self, expr_idx: NodeIndex) -> bool {
        let mut current = expr_idx;
        while let Some(node) = self.ctx.arena.get(current) {
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if (node.kind == syntax_kind_ext::AS_EXPRESSION
                || node.kind == syntax_kind_ext::TYPE_ASSERTION)
                && let Some(assertion) = self.ctx.arena.get_type_assertion(node)
            {
                return self.get_type_from_type_node(assertion.type_node) == TypeId::ANY;
            }
            return false;
        }
        false
    }

    pub(crate) fn infer_return_type_from_body(
        &mut self,
        function_idx: NodeIndex,
        body_idx: NodeIndex,
        return_context: Option<TypeId>,
    ) -> TypeId {
        // `resolvedReturnType` analog: a stable function body has exactly one
        // pure return-type inference per (node, contextual type, literal-mode,
        // type-parameter-scope) tuple. Generic-builder DAGs (effect/zod/kysely)
        // re-enter the same body through many parents via contextual return
        // requests; without this memo each re-entry re-runs full body flow
        // analysis, which is combinatorial. The memo records ONLY the inferred
        // `TypeId` (never diagnostics), so the separate body-check pass still
        // runs once and emits its diagnostics unchanged.
        let memo_key = self.inferred_return_type_memo_key(function_idx, body_idx, return_context);
        if let Some(key) = memo_key
            && let Some(&cached) = self.ctx.inferred_return_type_memo.get(&key)
        {
            return cached;
        }

        // The inference pass evaluates return expressions WITHOUT narrowing
        // context, which can produce false errors (e.g. TS2339 for discriminated
        // union property accesses) and cache wrong types.  Snapshot diagnostic,
        // node-type, and flow-analysis-cache state, then restore after inference
        // so that the subsequent check_statement pass recomputes everything with
        // proper narrowing context.
        let snap = self.ctx.snapshot_return_type();

        if self.ctx.is_checking_statements
            && function_idx.is_some()
            && (!self.contextual_return_suppresses_circularity(return_context)
                || self.return_body_has_resolving_var_in_call_like(body_idx))
            && let Some(function_node) = self.ctx.arena.get(function_idx)
        {
            let should_record = matches!(
                function_node.kind,
                syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION
            ) || (self.ctx.non_closure_circular_return_tracking_depth > 0
                && matches!(
                    function_node.kind,
                    syntax_kind_ext::METHOD_DECLARATION
                        | syntax_kind_ext::GET_ACCESSOR
                        | syntax_kind_ext::SET_ACCESSOR
                ));
            if should_record {
                self.record_pending_circular_return_sites(
                    function_idx,
                    body_idx,
                    return_context.is_none(),
                );
            }
        }

        // Function bodies compute their own return type and widen inferred
        // literal results (tsc's `getReturnTypeFromBody`). Clear the const
        // initializer's logical-literal preservation so a body like
        // `() => a && "yes"` infers `0 | string`, not `0 | "yes"`, even when
        // the function itself is a `const` initializer.
        let prev_preserve_logical = self.ctx.preserve_logical_operand_literals;
        self.ctx.preserve_logical_operand_literals = false;
        let result = self.infer_return_type_from_body_inner(body_idx, return_context);
        self.ctx.preserve_logical_operand_literals = prev_preserve_logical;

        // Direct self-recursive functions with no base case return `never`.
        // Example: `function fn2(n: number) { return fn2(n); }` → return type `never`.
        // When the inferred return type is `any` (from the circular provisional type)
        // and every return expression is a direct (non-wrapped) self-call, the function
        // never terminates. tsc handles this the same way.
        // Wrapped self-calls (e.g., `return [fn][0]()`) are handled separately via
        // TS7023 and keep `any` as their return type.
        if result == TypeId::ANY
            && return_context.is_none()
            && let Some(sym_id) = self.ctx.binder.get_node_symbol(function_idx)
            && self.ctx.symbol_resolution_set.contains(&sym_id)
            && self.all_returns_are_direct_self_calls(body_idx, sym_id)
        {
            snap.rollback(&mut self.ctx.speculation_state());
            return TypeId::NEVER;
        }

        // Fix Lazy class return types: when a method body returns a class reference
        // (e.g., `static getClass() { return A; }`) and the class is still being
        // constructed, the return type is captured as Lazy(DefId). But Lazy types
        // for classes resolve to the INSTANCE type in the solver (for type-position
        // semantics), whereas value-position class references should resolve to the
        // CONSTRUCTOR type (typeof A). Replace Lazy(DefId) for class symbols with
        // TypeQuery(SymbolRef), which correctly resolves to the constructor type.
        let result = self.resolve_lazy_class_to_constructor(result);

        // Closures newly added to `implicit_any_checked_closures` during this
        // speculation. The rollback below restores the set to its pre-speculation
        // snapshot, so each of these marks must be handled explicitly across the
        // rollback boundary. The membership in `implicit_any_contextual_closures`
        // (which is not speculation-scoped and survives the rollback) partitions
        // them:
        //   - NOT contextually typed -> recorded so
        //     `recheck_deferred_implicit_any_closures` can re-emit the TS7006 that
        //     the rollback discards, but only if they truly lack contextual types.
        //   - contextually typed -> their "already checked" mark is re-applied
        //     after the rollback, so an authoritative re-check that re-enters the
        //     closure WITHOUT re-deriving its contextual signature (e.g. the inner
        //     method of a curried `(a) => (): T => ({ m: (x, y) => ... })`, whose
        //     contextual type comes from the inner arrow's annotation but is not
        //     re-established on the outer arrow's authoritative body pass) does not
        //     spuriously re-emit TS7006. A closure's contextual typing is
        //     independent of the speculative return context, so preserving the mark
        //     is sound and mirrors tsc, which resolves a node's contextual type
        //     once and caches it.
        let newly_checked_closures: Vec<_> = self
            .ctx
            .implicit_any_checked_closures
            .difference(&snap.full.implicit_any_checked_closures)
            .copied()
            .collect();
        {
            use crate::diagnostics::diagnostic_codes;
            let speculative_diags = self
                .ctx
                .speculative_diagnostics_since(snap.diagnostic_snapshot());
            let has_implicit_any_diags = speculative_diags.iter().any(|d| {
                d.code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                    || d.code == diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE
                    || d.code == diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
                    || d.code == diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN
            });
            if has_implicit_any_diags {
                self.ctx.speculative_implicit_any_closures.extend(
                    newly_checked_closures
                        .iter()
                        .copied()
                        .filter(|idx| !self.ctx.implicit_any_contextual_closures.contains(idx)),
                );
            }
        }
        snap.rollback(&mut self.ctx.speculation_state());
        for idx in newly_checked_closures {
            if self.ctx.implicit_any_contextual_closures.contains(&idx) {
                self.ctx.implicit_any_checked_closures.insert(idx);
            }
        }

        // Widening of inferred return types is performed per-return-expression
        // during collection (`maybe_widen_return_contribution`), so that
        // contributions from `return ... as const` (or `<const>...`) preserve
        // their literal types while plain literal returns still widen. The only
        // remaining case here is when the caller explicitly requested literal
        // preservation (e.g. `preserve_literal_types`) and per-expression
        // widening already deferred to that flag.

        // Publish the stable inference. `inferred_return_type_memo_key` returned
        // `Some` only for stable (non-circular, non-provisional) calls, so this
        // never caches an in-progress placeholder. The only early return between
        // key computation and here is the direct-self-call `any`→`never` rewrite
        // above, which both rolls back and returns before this point AND would
        // already have forced `memo_key` to `None`, so the cached value is always
        // the final stable inference.
        if let Some(key) = memo_key {
            self.ctx.inferred_return_type_memo.insert(key, result);
        }
        result
    }

    /// Build the memo key for `infer_return_type_from_body`, or `None` when the
    /// inference must not be cached because it is participating in circular
    /// return resolution (and so may be provisional / dependent on the active
    /// resolution stack). A `Some` key captures every ambient input to pure
    /// return-type inference; see [`crate::context::InferredReturnTypeKey`].
    fn inferred_return_type_memo_key(
        &mut self,
        function_idx: NodeIndex,
        body_idx: NodeIndex,
        return_context: Option<TypeId>,
    ) -> Option<crate::context::InferredReturnTypeKey> {
        if !self.inferred_return_type_is_memoizable(function_idx, body_idx, return_context) {
            return None;
        }
        Some(crate::context::InferredReturnTypeKey {
            function_node: function_idx,
            return_context,
            in_const_assertion: self.ctx.in_const_assertion,
            preserve_literal_types: self.ctx.preserve_literal_types,
            this_type: self.current_this_type(),
            scope_fingerprint: self.inferred_return_type_scope_fingerprint(),
        })
    }

    /// True when the pure return-type inference of `function_idx`'s body is a
    /// stable function of its ambient inputs (so it may be memoized).
    ///
    /// Only **contextual** inference (`return_context.is_some()`) is memoized.
    /// The non-contextual inference of a declaration runs exactly once, in the
    /// declaration's own check pass, and that pass relies on the body evaluation
    /// to warm shared solver caches (e.g. indexed-access / large-union
    /// evaluations) that the immediately-following body diagnostic check then
    /// reuses — short-circuiting it changes downstream complexity accounting
    /// (witnessed by a spurious TS2590 on `intersectionsOfLargeUnions.ts`). The
    /// combinatorial blow-up this memo targets is the **contextual** return
    /// requests that re-enter a shared generic-builder DAG through many parents;
    /// those are exactly the `return_context.is_some()` calls.
    ///
    /// A function's OWN symbol is always in `symbol_resolution_set` while its
    /// type is computed — that is normal, not circular, and does NOT disqualify
    /// memoization. The inference becomes resolution-state-dependent, and unsafe
    /// to cache, only in these cases:
    ///
    /// - the function node is *genuinely* re-entrant on the resolution stack
    ///   (appears more than once): its result is a provisional placeholder.
    ///   A single occurrence is the node's OWN in-progress resolution frame and
    ///   is NOT re-entrancy — see the inline note below;
    /// - the body returns a still-resolving variable through a call-like return,
    ///   which `tsc` resolves on demand with `any` and refines later;
    /// - the body is directly self-recursive (`all_returns_are_direct_self_calls`):
    ///   the `any`→`never` rewrite above keys off the live resolution set, so the
    ///   inferred value depends on whether this is the resolving pass.
    fn inferred_return_type_is_memoizable(
        &mut self,
        function_idx: NodeIndex,
        body_idx: NodeIndex,
        return_context: Option<TypeId>,
    ) -> bool {
        if function_idx.is_none() || body_idx.is_none() {
            return false;
        }
        if return_context.is_none() {
            return false;
        }
        // A function node is pushed onto `node_resolution_stack` by its own
        // resolution frame (`get_type_of_node_with_request`) *before* this body
        // inference runs, so a single occurrence is the node's OWN in-progress
        // frame — the normal, complete inference, which is safe to memoize. Only
        // a *second* occurrence means a genuine re-entrant request whose result
        // is a provisional placeholder; reject memoization just for that. A plain
        // `contains` test (`>= 1`) would reject every contextual inference,
        // because the own frame is always present, leaving the memo permanently
        // inert. (In practice the circular guard in `get_type_of_node_with_request`
        // returns `ERROR` before re-entering this body, so the count rarely
        // exceeds one, but the `> 1` test keeps the guard correct if that path
        // changes.)
        let mut seen_own_frame = false;
        for &node in &self.ctx.node_resolution_stack {
            if node == function_idx {
                if seen_own_frame {
                    return false;
                }
                seen_own_frame = true;
            }
        }
        if self.return_body_has_resolving_var_in_call_like(body_idx) {
            return false;
        }
        if let Some(sym_id) = self.ctx.binder.get_node_symbol(function_idx)
            && self.ctx.symbol_resolution_set.contains(&sym_id)
            && self.all_returns_are_direct_self_calls(body_idx, sym_id)
        {
            return false;
        }
        true
    }

    /// Stable hash of the active type-parameter scope: the bindings a generic
    /// body resolves its free type parameters through. Two scopes with identical
    /// `(name, TypeId)` bindings produce identical inference, so this isolates
    /// the same body inferred under different ambient type-parameter bindings.
    fn inferred_return_type_scope_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        if self.ctx.type_parameter_scope.is_empty() {
            return 0;
        }
        let mut entries: Vec<(&str, u32)> = self
            .ctx
            .type_parameter_scope
            .iter()
            .map(|(name, type_id)| (name.as_str(), type_id.0))
            .collect();
        entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
        let mut hasher = rustc_hash::FxHasher::default();
        entries.hash(&mut hasher);
        hasher.finish()
    }

    /// Inner implementation of return type inference (no diagnostic/cache cleanup).
    fn infer_return_type_from_body_inner(
        &mut self,
        body_idx: NodeIndex,
        return_context: Option<TypeId>,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        if body_idx.is_none() {
            return TypeId::VOID; // No body - function returns void
        }

        let Some(node) = self.ctx.arena.get(body_idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if node.kind != syntax_kind_ext::BLOCK {
            let raw = self.return_expression_type(body_idx, return_context);
            let raw = self.maybe_evaluate_inferred_return_contribution(raw, return_context);
            return self.maybe_widen_return_contribution(body_idx, raw, return_context);
        }

        let mut return_types = Vec::new();
        let mut saw_empty = false;

        if let Some(block) = self.ctx.arena.get_block(node) {
            for &stmt_idx in &block.statements.nodes {
                self.collect_return_types_in_statement(
                    stmt_idx,
                    &mut return_types,
                    &mut saw_empty,
                    return_context,
                );
            }
        }

        if return_types.is_empty() {
            // No return statements found. Check if the body falls through:
            // - If it does (normal implicit return), the return type is `void`
            // - If it doesn't (all paths throw or call never), the return type is `never`
            // Only call the (potentially expensive) fallthrough checker when the body
            // could plausibly be non-falling-through, i.e. it contains throw statements.
            // This avoids triggering unnecessary type evaluation in simple function bodies.
            let may_not_fall_through = self.body_contains_throw_or_never_call(body_idx);

            // Check if function has a return type annotation
            let has_return_type_annotation = if let Some(func_node) = self.ctx.arena.get(body_idx)
                && let Some(func) = self.ctx.arena.get_function(func_node)
            {
                func.type_annotation.is_some()
            } else {
                false
            };

            if has_return_type_annotation
                && may_not_fall_through
                && !self.function_body_falls_through(body_idx)
            {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    body_idx,
                    "Function lacks ending return statement and return type does not include undefined",
                    diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                );
                return TypeId::ERROR; // Return error to avoid further issues
            }

            // `return;` statements set saw_empty but push nothing to return_types.
            // When contextually typed as `() => undefined | T`, use `undefined` so the
            // function matches. When the contextual return type doesn't accept `undefined`
            // (e.g., `number`), fall through to the void/contextual check below.
            if saw_empty {
                if let Some(ctx) = return_context {
                    // Only narrow to `undefined` when the contextual return type is a
                    // specific undefined-compatible type (not `any`/`unknown`/`void`).
                    // `any`/`unknown` accept everything — don't change inference for them.
                    if ctx != TypeId::VOID
                        && ctx != TypeId::ANY
                        && ctx != TypeId::UNKNOWN
                        && self.return_relation_outcome(TypeId::UNDEFINED, ctx).related
                    {
                        return TypeId::UNDEFINED;
                    }
                } else {
                    return TypeId::UNDEFINED;
                }
            }

            return if !may_not_fall_through || self.function_body_falls_through(body_idx) {
                // When contextual return type expects `undefined` (not void/any/unknown),
                // use `undefined` so `const f: () => undefined = () => {}` doesn't produce TS2322.
                // tsc applies contextual typing to infer the return type of such lambdas.
                // Exclude `any`/`unknown` — they accept everything and shouldn't change inference.
                if let Some(ctx) = return_context
                    && ctx != TypeId::VOID
                    && ctx != TypeId::ANY
                    && ctx != TypeId::UNKNOWN
                    && self.return_relation_outcome(TypeId::UNDEFINED, ctx).related
                {
                    TypeId::UNDEFINED
                } else {
                    TypeId::VOID
                }
            } else {
                TypeId::NEVER
            };
        }

        if saw_empty || self.function_body_falls_through(body_idx) {
            // When a function has value-returning paths AND also falls through
            // (or has empty `return;`), the non-returning paths contribute
            // `undefined` to the union, not `void`. tsc behaves the same way:
            // `function f(x) { if (x) return 1; }` → `number | undefined`.
            // `undefined` is never a widenable literal contribution, so it keeps
            // a literal+undefined union (`"a" | undefined`) from collapsing to a
            // single-literal widen.
            return_types.push(ReturnContribution {
                type_id: TypeId::UNDEFINED,
                widen_expr: None,
            });
        }

        // Filter out ERROR types from return type inference when there are
        // non-ERROR alternatives. This handles recursive self-referencing functions
        // like `const fn1 = () => { if (...) return fn1(); return 0; }`.
        // The recursive call resolves to ERROR during type computation (circular
        // reference), but the base case `return 0` provides a concrete `number` type.
        // tsc filters out circular contributions and infers the return type from
        // non-circular branches only, so `fn1` gets return type `number`.
        let has_non_error = return_types.iter().any(|c| c.type_id != TypeId::ERROR);
        if has_non_error {
            return_types.retain(|c| c.type_id != TypeId::ERROR);
        }

        // Union the contributions and apply the remaining primitive-collapse
        // widen (tsc's `getWidenedType(getUnionType(...))`). Fresh object/array
        // contributions were already structure-widened during collection (so
        // `{ a: 1 } | { a: 2 }` arrives as `{ a: number }`); the only widen left
        // here is the bare primitive literal that was deferred to preserve a
        // multi-branch literal union. `factory.union` dedups and flattens a
        // single surviving member to a scalar, so a multi-member literal union
        // (`"a" | "b"`, `1 | 2`, `"a" | undefined`) is NOT a literal type and is
        // returned unwidened, while a single fresh literal (`return "x"`, or
        // `return "a"; return "a"` deduped to one) is widened via its originating
        // expression's AST-aware widener — preserving per-property `const`
        // subtrees (#14530).
        let widen_expr = return_types.iter().find_map(|c| c.widen_expr);
        let union = factory.union(return_types.iter().map(|c| c.type_id).collect());
        if let Some(expr_idx) = widen_expr
            && crate::query_boundaries::common::is_literal_type(self.ctx.types, union)
        {
            return self.widen_return_contribution_preserving_const(expr_idx, union);
        }
        union
    }

    /// Resolve a Lazy class type to a `TypeQuery` (constructor/value-position type).
    ///
    /// When a class references itself during construction (e.g., `return A`
    /// inside class A, or `static s = C.#method()`), the type is captured as
    /// `Lazy(DefId)`. The solver's `resolve_lazy` resolves this to the INSTANCE
    /// type, but value-position class references should be `typeof A` (the
    /// constructor type). This method replaces `Lazy(DefId)` for CLASS symbols
    /// with `TypeQuery(SymbolRef)`, which correctly resolves to the constructor
    /// type in both relation checks and property access resolution.
    ///
    /// IMPORTANT: Only converts to `TypeQuery` when the class symbol is currently
    /// being resolved (i.e., in `class_instance_resolution_set` or
    /// `class_constructor_resolution_set`). If the class is NOT being resolved,
    /// the `Lazy(DefId)` came from contextual parameter/return typing (e.g., a
    /// parameter `p: Point` typed as `Lazy(DefId_of_Point)`) and should remain
    /// as the instance type, not be converted to the constructor type.
    pub(crate) fn resolve_lazy_class_to_constructor(&self, type_id: TypeId) -> TypeId {
        use tsz_solver::SymbolRef;

        let Some(def_id) = return_type_queries::lazy_def_id(self.ctx.types, type_id) else {
            return type_id;
        };

        // Use stable-identity fallback to resolve DefId→SymbolId.
        // def_to_symbol_id_with_fallback handles cross-context DefIds by
        // falling back to the DefinitionStore's symbol_id backreference.
        let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id) else {
            return type_id;
        };

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return type_id;
        };

        if !symbol.has_any_flags(tsz_binder::symbol_flags::CLASS) {
            return type_id;
        }

        // Only convert to TypeQuery when we're actively building the type for
        // this class symbol (circular resolution). If the class is not currently
        // in a resolution set, the Lazy(DefId) came from contextual typing of an
        // instance (e.g., `p: Point` typed as Lazy during class body construction),
        // and converting it to TypeQuery would incorrectly make instance types
        // appear as constructor types (causing false TS2741 "prototype missing" errors).
        let in_instance_resolution = self.ctx.class_instance_resolution_set.contains(&sym_id);
        let in_constructor_resolution = self.ctx.class_constructor_resolution_set.contains(&sym_id);

        if !in_instance_resolution && !in_constructor_resolution {
            return type_id;
        }

        // Replace Lazy(DefId) with TypeQuery(SymbolRef) for value-position semantics
        self.ctx.types.factory().type_query(SymbolRef(sym_id.0))
    }

    /// Get the type of a return expression with optional contextual typing.
    ///
    /// This function temporarily sets the contextual type (if provided) before
    /// computing the type of the return expression, then restores the previous
    /// contextual type. This enables contextual typing for return expressions.
    ///
    /// ## Parameters:
    /// - `expr_idx`: The return expression node index
    /// - `return_context`: Optional contextual type for the return
    fn return_expression_type(
        &mut self,
        expr_idx: NodeIndex,
        return_context: Option<TypeId>,
    ) -> TypeId {
        // Expression-bodied arrows returning `void expr` are always `void`.
        // During inference this avoids unnecessary recursive type computation
        // (which can create self-referential cycles and spuriously degrade to `any`).
        if let Some(expr_node) = self.ctx.arena.get(expr_idx)
            && expr_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && let Some(unary) = self.ctx.arena.get_unary_expr(expr_node)
            && unary.operator == SyntaxKind::VoidKeyword as u16
        {
            return TypeId::VOID;
        }

        let prev_preserve_literals = self.ctx.preserve_literal_types;

        // When the return expression is a function/arrow, do NOT set
        // preserve_literal_types.  Function types compute their own return types
        // via infer_return_type_from_body, which checks this flag to decide
        // whether to widen.  Setting it here leaks into nested function
        // inference, blocking return-type widening for patterns like
        // `() => () => 0` (inner `0` should widen to `number`).
        //
        // For non-function expressions (literals, identifiers, calls, etc.),
        // preserve literal types: tsc's checkExpression always returns literal
        // types for literals (e.g., "1" not string); widening happens later in
        // getReturnTypeFromBody.  Without this, `return "1"` with contextual
        // type `string` widens to `string` too early.
        let is_function_expr = self.ctx.arena.get(expr_idx).is_some_and(|node| {
            matches!(
                node.kind,
                syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
            )
        });
        // When the return context is a bare type parameter (e.g., `B` from an outer
        // generic signature like `compose<A, B, C>`), do NOT pass it as the contextual
        // type for the body expression. Type parameters carry no useful inference
        // information for inner generic calls, and passing them causes the solver to
        // seed return-type inference from the type parameter, producing incorrect
        // results (e.g., `unbox(a)` resolving W=B instead of W=T[]).
        // This matches tsc's behavior where type parameter contextual return types
        // do not flow into inner call expression inference.
        let effective_return_context = return_context.filter(|&ctx_type| {
            return_type_queries::type_param_info(self.ctx.types, ctx_type).is_none()
        });
        let request = match effective_return_context {
            Some(ctx_type) => TypingRequest::with_contextual_type(ctx_type),
            None => TypingRequest::NONE,
        };
        if is_function_expr {
            // Function expressions compute their own return types via
            // infer_return_type_from_body.  Clear preserve_literal_types so
            // nested function inference makes its own widening decision rather
            // than inheriting a flag from an outer return_expression_type call.
            self.ctx.preserve_literal_types = false;
        } else {
            self.ctx.preserve_literal_types = true;
        }
        let prev_const_assertion = self.ctx.in_const_assertion;
        if !prev_const_assertion
            && self.should_preserve_tuple_literals_for_generic_return(
                expr_idx,
                return_context,
                effective_return_context,
            )
        {
            self.ctx.in_const_assertion = true;
        }
        let mut return_type = self.get_type_of_node_with_request(expr_idx, &request);
        if let Some(contextual_type) = effective_return_context
            && self
                .ctx
                .arena
                .get(expr_idx)
                .is_some_and(|expr_node| expr_node.kind == syntax_kind_ext::NEW_EXPRESSION)
            && (self.contextual_application_recovers_unknown_result(return_type, contextual_type)
                || self.contextual_application_recovers_type_param_result(
                    return_type,
                    contextual_type,
                )
                || (return_type_queries::contains_type_parameters(self.ctx.types, return_type)
                    && self
                        .ctx
                        .arena
                        .get_call_expr_at(expr_idx)
                        .is_some_and(|new_expr| {
                            self.contextual_application_matches_new_target(
                                new_expr.expression,
                                contextual_type,
                            )
                        })))
        {
            return_type = contextual_type;
        }
        self.ctx.in_const_assertion = prev_const_assertion;
        self.ctx.preserve_literal_types = prev_preserve_literals;
        return_type
    }

    /// Collect return types from a statement and its nested statements.
    ///
    /// This function recursively walks through statements, collecting the types
    /// of all return expressions. It handles:
    /// - Direct return statements
    /// - Nested blocks
    /// - If/else statements (both branches)
    /// - Switch statements (all cases)
    /// - Try/catch/finally statements (all blocks)
    /// - Loops (nested statements)
    fn collect_return_types_in_statement(
        &mut self,
        stmt_idx: NodeIndex,
        return_types: &mut Vec<ReturnContribution>,
        saw_empty: &mut bool,
        return_context: Option<TypeId>,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(return_data) = self.ctx.arena.get_return_statement(node) {
                    if return_data.expression.is_none() {
                        *saw_empty = true;
                    } else {
                        // Keep block-body inference mostly raw, but allow concrete
                        // contextual return types to shape literals and other
                        // context-sensitive return expressions that would otherwise
                        // lose tuple/object precision.
                        let infer_context = self.inference_context_for_block_return_expression(
                            return_data.expression,
                            return_context,
                        );
                        let return_type =
                            self.return_expression_type(return_data.expression, infer_context);
                        let return_type = self.maybe_evaluate_inferred_return_contribution(
                            return_type,
                            return_context,
                        );
                        // Apply tsc's `getWidenedType(getUnionType(returns))` per
                        // contribution, splitting on freshness kind:
                        //
                        // - A bare **primitive** literal (`return 1`, `return "a"`)
                        //   is collected UNWIDENED, recording the originating
                        //   expression. tsc's `getUnionType` de-freshes primitive
                        //   literal members, so a multi-branch literal union
                        //   (`"a" | "b"`, `1 | 2`) must survive unwidened; only a
                        //   union that collapses to a single literal is widened, in
                        //   `infer_return_type_from_body_inner` (#14530).
                        // - A fresh **object/array** literal (`return { a: 1 }`,
                        //   `return [1, 2]`) is widened EAGERLY here. tsc widens
                        //   fresh object/array literal *structure* regardless of
                        //   union membership (`{ a: 1 } | { a: 2 }` → `{ a: number }`),
                        //   because `getUnionType` does not de-fresh those leaves —
                        //   `getWidenedType` still reaches them. The widen is
                        //   freshness/`as const`-respecting, so per-property const
                        //   subtrees (`{ k: "x" as const, n: 1 }` → `{ k: "x"; n: number }`)
                        //   are preserved exactly as the single-collapse path does.
                        //   (Conditional-expression returns keep `widenable == false`
                        //   via the carve-out in `return_contribution_is_widenable`,
                        //   so a discriminant-preserving `return c ? a : b` is left
                        //   to its existing path and not eagerly collapsed here.)
                        let widenable = self.return_contribution_is_widenable(
                            return_data.expression,
                            return_type,
                            return_context,
                        );
                        let (contribution_type, widen_expr) = if widenable
                            && !crate::query_boundaries::common::is_literal_type(
                                self.ctx.types,
                                return_type,
                            ) {
                            (
                                crate::query_boundaries::widening::widen_const_initializer(
                                    self.ctx.types,
                                    return_type,
                                ),
                                None,
                            )
                        } else {
                            (return_type, widenable.then_some(return_data.expression))
                        };
                        return_types.push(ReturnContribution {
                            type_id: contribution_type,
                            widen_expr,
                        });
                    }
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_return_types_in_statement(
                            stmt,
                            return_types,
                            saw_empty,
                            return_context,
                        );
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    // Evaluate the condition expression so that call-expression type
                    // guards (e.g. `isFunction(item)`) get their callee types cached
                    // in `node_types` and their predicates stored in
                    // `call_type_predicates`. Without this, flow narrowing for
                    // identifiers in the then/else branches cannot find the type
                    // predicate and falls back to the declared (un-narrowed) type.
                    if if_data.expression.is_some() {
                        self.get_type_of_node(if_data.expression);
                    }
                    self.collect_return_types_in_statement(
                        if_data.then_statement,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                    if if_data.else_statement.is_some() {
                        self.collect_return_types_in_statement(
                            if_data.else_statement,
                            return_types,
                            saw_empty,
                            return_context,
                        );
                    }
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node)
                    && let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block)
                    && let Some(case_block) = self.ctx.arena.get_block(case_block_node)
                {
                    for &clause_idx in &case_block.statements.nodes {
                        if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                            && let Some(clause) = self.ctx.arena.get_case_clause(clause_node)
                        {
                            for &stmt_idx in &clause.statements.nodes {
                                self.collect_return_types_in_statement(
                                    stmt_idx,
                                    return_types,
                                    saw_empty,
                                    return_context,
                                );
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_return_types_in_statement(
                        try_data.try_block,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                    if try_data.catch_clause.is_some() {
                        self.collect_return_types_in_statement(
                            try_data.catch_clause,
                            return_types,
                            saw_empty,
                            return_context,
                        );
                    }
                    if try_data.finally_block.is_some() {
                        self.collect_return_types_in_statement(
                            try_data.finally_block,
                            return_types,
                            saw_empty,
                            return_context,
                        );
                    }
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_return_types_in_statement(
                        catch_data.block,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_return_types_in_statement(
                        loop_data.statement,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(for_in_of_data) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_return_types_in_statement(
                        for_in_of_data.statement,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled_data) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_return_types_in_statement(
                        labeled_data.statement,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                }
            }
            _ => {}
        }
    }

    /// Check if a function body has at least one return statement with a value.
    ///
    /// This is a simplified check that doesn't do full control flow analysis.
    /// It's used to determine if a function needs an explicit return type
    /// annotation or if implicit any should be inferred.
    ///
    /// ## Returns:
    /// - `true`: At least one return statement with a value exists
    /// - `false`: No return statements or only empty returns
    ///
    /// ## Examples:
    /// ```typescript
    /// // Returns true:
    /// function foo() { return 42; }
    /// function bar() { if (x) return "hello"; else return 42; }
    ///
    /// // Returns false:
    /// function baz() {}  // No returns
    /// function qux() { return; }  // Only empty return
    /// ```
    pub(crate) fn body_has_return_with_value(&self, body_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(body_idx) else {
            return false;
        };

        // For block bodies, check all statements
        if node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.ctx.arena.get_block(node)
        {
            return self.statements_have_return_with_value(&block.statements.nodes);
        }

        false
    }

    /// Check if any statement in the list contains a return with a value.
    fn statements_have_return_with_value(&self, statements: &[NodeIndex]) -> bool {
        for &stmt_idx in statements {
            if self.statement_has_return_with_value(stmt_idx) {
                return true;
            }
        }
        false
    }

    /// Check if a statement contains a return with a value.
    ///
    /// This function recursively checks a statement (and its nested statements)
    /// for any return statement with a value. It handles all statement types
    /// including blocks, conditionals, loops, and try/catch.
    fn statement_has_return_with_value(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(return_data) = self.ctx.arena.get_return_statement(node) {
                    // Return with expression
                    return return_data.expression.is_some();
                }
                false
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    return self.statements_have_return_with_value(&block.statements.nodes);
                }
                false
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    // Check both then and else branches
                    let then_has = self.statement_has_return_with_value(if_data.then_statement);
                    let else_has = if if_data.else_statement.is_some() {
                        self.statement_has_return_with_value(if_data.else_statement)
                    } else {
                        false
                    };
                    return then_has || else_has;
                }
                false
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node)
                    && let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block)
                {
                    // Case block is stored as a Block containing case clauses
                    if let Some(case_block) = self.ctx.arena.get_block(case_block_node) {
                        for &clause_idx in &case_block.statements.nodes {
                            if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                                && let Some(clause) = self.ctx.arena.get_case_clause(clause_node)
                                && self.statements_have_return_with_value(&clause.statements.nodes)
                            {
                                return true;
                            }
                        }
                    }
                }
                false
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    let try_has = self.statement_has_return_with_value(try_data.try_block);
                    let catch_has = if try_data.catch_clause.is_some() {
                        self.statement_has_return_with_value(try_data.catch_clause)
                    } else {
                        false
                    };
                    let finally_has = if try_data.finally_block.is_some() {
                        self.statement_has_return_with_value(try_data.finally_block)
                    } else {
                        false
                    };
                    return try_has || catch_has || finally_has;
                }
                false
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    return self.statement_has_return_with_value(catch_data.block);
                }
                false
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    return self.statement_has_return_with_value(loop_data.statement);
                }
                false
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(for_in_of_data) = self.ctx.arena.get_for_in_of(node) {
                    return self.statement_has_return_with_value(for_in_of_data.statement);
                }
                false
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled_data) = self.ctx.arena.get_labeled_statement(node) {
                    return self.statement_has_return_with_value(labeled_data.statement);
                }
                false
            }
            _ => false,
        }
    }
}
