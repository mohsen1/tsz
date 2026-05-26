//! Suspension-discovery and await-presence analysis for the async ES5
//! IR transformer.
//!
//! Every method here takes `&self`, walks the AST, and returns a
//! predicate or a `NodeIndex`. None of them construct IR or mutate the
//! transformer. Walks stop at nested function/method bodies so an
//! inner async function never falsely flags its enclosing scope.
//! Lowering passes consume these predicates and then decide whether to
//! emit a switch case, a `yield`, or a passthrough.

use super::AsyncES5Transformer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;

impl AsyncES5Transformer<'_> {
    /// Syntax-kind atom that this transformer treats as a suspension
    /// point. Async mode suspends on `AwaitExpression`; generator mode
    /// suspends on `YieldExpression`. Async-generator mode treats both
    /// as suspensions, so callers that need the full set should reach
    /// for [`AsyncES5Transformer::is_suspension_expression`] instead.
    pub(crate) const fn suspension_kind(&self) -> u16 {
        if self.generator_mode {
            syntax_kind_ext::YIELD_EXPRESSION
        } else {
            syntax_kind_ext::AWAIT_EXPRESSION
        }
    }

    /// True when `idx` resolves to a node that suspends the lowered
    /// generator state machine.
    pub(crate) fn is_suspension_expression(&self, idx: NodeIndex) -> bool {
        self.arena.get(idx).is_some_and(|n| {
            n.kind == self.suspension_kind()
                || (self.async_generator_mode && n.kind == syntax_kind_ext::AWAIT_EXPRESSION)
        })
    }

    /// Check if a function body contains any await expressions
    pub fn body_contains_await(&self, body_idx: NodeIndex) -> bool {
        self.contains_await_recursive(body_idx)
    }

    /// Walk the sub-tree at `idx` looking for any node the current
    /// transformer mode considers a suspension. Stops at nested
    /// function/method bodies so that an inner `async function` does
    /// not falsely flag its enclosing scope.
    pub(super) fn contains_await_recursive(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        // Check if this is an await expression
        if node.kind == self.suspension_kind()
            || (self.async_generator_mode && node.kind == syntax_kind_ext::AWAIT_EXPRESSION)
        {
            return true;
        }

        if node.kind == syntax_kind_ext::SPREAD_ELEMENT {
            let spread_expr = self
                .arena
                .get_spread(node)
                .map(|spread| spread.expression)
                .or_else(|| {
                    self.arena
                        .get_unary_expr_ex(node)
                        .map(|spread| spread.expression)
                });
            return spread_expr.is_some_and(|expr| self.contains_await_recursive(expr));
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            && (node.flags as u32 & node_flags::USING) != 0
        {
            return true;
        }

        // Don't recurse into nested functions
        // This check must happen before recursing into any children
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.is_function_expression_or_arrow()
        {
            return false;
        }

        // Check block statements
        if node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.arena.get_block(node) {
                for &stmt_idx in &block.statements.nodes {
                    if self.contains_await_recursive(stmt_idx) {
                        return true;
                    }
                }
            }
            return false;
        }

        // Class method bodies are function-like scopes, but heritage clauses are
        // evaluated in the surrounding async function.
        if (node.kind == syntax_kind_ext::CLASS_DECLARATION
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION)
            && let Some(class_data) = self.arena.get_class(node)
        {
            if let Some(extends_expr) = crate::transforms::emit_utils::get_extends_expression_index(
                self.arena,
                &class_data.heritage_clauses,
            ) && self.contains_await_recursive(extends_expr)
            {
                return true;
            }
            return false;
        }

        // Check expression statements
        if node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
            && let Some(expr_stmt) = self.arena.get_expression_statement(node)
        {
            return self.contains_await_recursive(expr_stmt.expression);
        }

        // Check return statements
        if node.kind == syntax_kind_ext::RETURN_STATEMENT
            && let Some(ret) = self.arena.get_return_statement(node)
        {
            return self.contains_await_recursive(ret.expression);
        }

        // Check variable statements
        // Structure: VARIABLE_STATEMENT -> VARIABLE_DECLARATION_LIST -> VARIABLE_DECLARATION
        if node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            && let Some(var_stmt) = self.arena.get_variable(node)
        {
            for &decl_list_idx in &var_stmt.declarations.nodes {
                if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                    && let Some(decl_list) = self.arena.get_variable(decl_list_node)
                {
                    if (decl_list_node.flags as u32 & node_flags::USING) != 0 {
                        return true;
                    }
                    for &decl_idx in &decl_list.declarations.nodes {
                        if let Some(decl_node) = self.arena.get(decl_idx)
                            && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                            && self.contains_await_recursive(decl.initializer)
                        {
                            return true;
                        }
                    }
                }
            }
        }

        // Check call expressions
        if node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.arena.get_call_expr(node)
        {
            if self.contains_await_recursive(call.expression) {
                return true;
            }
            if let Some(args) = &call.arguments {
                for &arg_idx in &args.nodes {
                    if self.contains_await_recursive(arg_idx) {
                        return true;
                    }
                }
            }
        }

        // Check binary expressions
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(node)
        {
            return self.contains_await_recursive(bin.left)
                || self.contains_await_recursive(bin.right);
        }

        // Check if statements
        if node.kind == syntax_kind_ext::IF_STATEMENT
            && let Some(if_stmt) = self.arena.get_if_statement(node)
        {
            if self.contains_await_recursive(if_stmt.expression) {
                return true;
            }
            if self.contains_await_recursive(if_stmt.then_statement) {
                return true;
            }
            if self.contains_await_recursive(if_stmt.else_statement) {
                return true;
            }
        }

        // Check property/element access expressions
        if (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.arena.get_access_expr(node)
        {
            if self.contains_await_recursive(access.expression) {
                return true;
            }
            if self.contains_await_recursive(access.name_or_argument) {
                return true;
            }
        }

        // Check array/object literals
        if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            || node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            if let Some(literal) = self.arena.get_literal_expr(node) {
                for &elem_idx in &literal.elements.nodes {
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        continue;
                    };

                    match elem_node.kind {
                        syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                            if let Some(prop) = self.arena.get_property_assignment(elem_node) {
                                if self.computed_name_contains_await(prop.name) {
                                    return true;
                                }
                                if self.contains_await_recursive(prop.initializer) {
                                    return true;
                                }
                            }
                        }
                        syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                            if let Some(prop) = self.arena.get_shorthand_property(elem_node) {
                                if self.computed_name_contains_await(prop.name) {
                                    return true;
                                }
                                if self.contains_await_recursive(prop.object_assignment_initializer)
                                {
                                    return true;
                                }
                            }
                        }
                        syntax_kind_ext::SPREAD_ELEMENT => {
                            let spread_expr = self
                                .arena
                                .get_spread(elem_node)
                                .map(|spread| spread.expression)
                                .or_else(|| {
                                    self.arena
                                        .get_unary_expr_ex(elem_node)
                                        .map(|spread| spread.expression)
                                });
                            if spread_expr.is_some_and(|expr| self.contains_await_recursive(expr)) {
                                return true;
                            }
                        }
                        syntax_kind_ext::METHOD_DECLARATION => {
                            if let Some(method) = self.arena.get_method_decl(elem_node)
                                && self.computed_name_contains_await(method.name)
                            {
                                return true;
                            }
                        }
                        syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                            if let Some(accessor) = self.arena.get_accessor(elem_node)
                                && self.computed_name_contains_await(accessor.name)
                            {
                                return true;
                            }
                        }
                        _ => {
                            if self.contains_await_recursive(elem_idx) {
                                return true;
                            }
                        }
                    }
                }
            }
            return false;
        }

        // Check conditional expressions
        if node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            && let Some(cond) = self.arena.get_conditional_expr(node)
        {
            if self.contains_await_recursive(cond.condition) {
                return true;
            }
            if self.contains_await_recursive(cond.when_true) {
                return true;
            }
            if self.contains_await_recursive(cond.when_false) {
                return true;
            }
        }

        // Check prefix/postfix unary expressions
        if (node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
            && let Some(unary) = self.arena.get_unary_expr(node)
        {
            return self.contains_await_recursive(unary.operand);
        }

        // Check parenthesized expressions
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            return self.contains_await_recursive(paren.expression);
        }

        // Type-only expression wrappers (TS-only syntax stripped by
        // `expression_to_ir`). Analysis must look through them too so
        // that `(await foo()) as T` is detected as containing an await.
        if (node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && let Some(assertion) = self.arena.get_type_assertion(node)
        {
            return self.contains_await_recursive(assertion.expression);
        }
        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr_ex(node)
        {
            return self.contains_await_recursive(unary.expression);
        }

        // Check try/catch/finally statements
        if node.kind == syntax_kind_ext::TRY_STATEMENT
            && let Some(try_data) = self.arena.get_try(node)
        {
            if self.contains_await_recursive(try_data.try_block) {
                return true;
            }
            if self.contains_await_recursive(try_data.catch_clause) {
                return true;
            }
            if self.contains_await_recursive(try_data.finally_block) {
                return true;
            }
        }

        // Check catch clauses
        if node.kind == syntax_kind_ext::CATCH_CLAUSE
            && let Some(catch) = self.arena.get_catch_clause(node)
        {
            return self.contains_await_recursive(catch.block);
        }

        // Check loop statements
        if (node.kind == syntax_kind_ext::WHILE_STATEMENT
            || node.kind == syntax_kind_ext::DO_STATEMENT
            || node.kind == syntax_kind_ext::FOR_STATEMENT)
            && let Some(loop_data) = self.arena.get_loop(node)
        {
            if self.contains_await_recursive(loop_data.initializer) {
                return true;
            }
            if self.contains_await_recursive(loop_data.condition) {
                return true;
            }
            if self.contains_await_recursive(loop_data.incrementor) {
                return true;
            }
            if self.contains_await_recursive(loop_data.statement) {
                return true;
            }
        }

        // Check for-in/for-of statements
        if (node.kind == syntax_kind_ext::FOR_IN_STATEMENT
            || node.kind == syntax_kind_ext::FOR_OF_STATEMENT)
            && let Some(for_data) = self.arena.get_for_in_of(node)
        {
            if for_data.await_modifier {
                return true;
            }
            if node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                && crate::transforms::emit_utils::for_of_using_info(
                    self.arena,
                    for_data.initializer,
                )
                .is_some()
            {
                return true;
            }
            if self.contains_await_recursive(for_data.initializer) {
                return true;
            }
            if self.contains_await_recursive(for_data.expression) {
                return true;
            }
            if self.contains_await_recursive(for_data.statement) {
                return true;
            }
        }

        // Check switch statements
        if node.kind == syntax_kind_ext::SWITCH_STATEMENT
            && let Some(switch_data) = self.arena.get_switch(node)
        {
            if self.contains_await_recursive(switch_data.expression) {
                return true;
            }
            if self.contains_await_recursive(switch_data.case_block) {
                return true;
            }
        }

        // Check case blocks
        if node.kind == syntax_kind_ext::CASE_BLOCK
            && let Some(block_data) = self.arena.get_block(node)
        {
            for &stmt_idx in &block_data.statements.nodes {
                if self.contains_await_recursive(stmt_idx) {
                    return true;
                }
            }
        }

        // Check case/default clauses
        if (node.kind == syntax_kind_ext::CASE_CLAUSE
            || node.kind == syntax_kind_ext::DEFAULT_CLAUSE)
            && let Some(clause_data) = self.arena.get_case_clause(node)
        {
            if self.contains_await_recursive(clause_data.expression) {
                return true;
            }
            for &stmt_idx in &clause_data.statements.nodes {
                if self.contains_await_recursive(stmt_idx) {
                    return true;
                }
            }
        }

        // Check new expressions
        if node.kind == syntax_kind_ext::NEW_EXPRESSION
            && let Some(call) = self.arena.get_call_expr(node)
        {
            if self.contains_await_recursive(call.expression) {
                return true;
            }
            if let Some(args) = &call.arguments {
                for &arg_idx in &args.nodes {
                    if self.contains_await_recursive(arg_idx) {
                        return true;
                    }
                }
            }
        }

        // Check template expressions
        if node.kind == syntax_kind_ext::TEMPLATE_EXPRESSION
            && let Some(template) = self.arena.get_template_expr(node)
        {
            for &span_idx in &template.template_spans.nodes {
                if let Some(span_node) = self.arena.get(span_idx)
                    && let Some(span) = self.arena.get_template_span(span_node)
                    && self.contains_await_recursive(span.expression)
                {
                    return true;
                }
            }
        }

        // Check with statements (uses IfStatementData)
        if node.kind == syntax_kind_ext::WITH_STATEMENT
            && let Some(with_data) = self.arena.get_with_statement(node)
        {
            if self.contains_await_recursive(with_data.expression) {
                return true;
            }
            if self.contains_await_recursive(with_data.then_statement) {
                return true;
            }
        }

        // Check throw statements
        if node.kind == syntax_kind_ext::THROW_STATEMENT
            && let Some(throw_data) = self.arena.get_return_statement(node)
            && self.contains_await_recursive(throw_data.expression)
        {
            return true;
        }

        // Check labeled statements
        if node.kind == syntax_kind_ext::LABELED_STATEMENT
            && let Some(labeled_data) = self.arena.get_labeled_statement(node)
            && self.contains_await_recursive(labeled_data.statement)
        {
            return true;
        }

        false
    }

    /// Detect a suspension expression nested inside a computed property
    /// name (`[await k()]: ...`).
    pub(super) fn computed_name_contains_await(&self, idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(idx) else {
            return false;
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(name_node)
        {
            return self.contains_await_recursive(computed.expression);
        }

        false
    }

    /// True when a parameter's default initializer contains a top-level
    /// `await`. Delegates to the shared `emit_utils` helper.
    pub(super) fn param_initializer_has_top_level_await(&self, param_idx: NodeIndex) -> bool {
        crate::transforms::emit_utils::param_initializer_has_top_level_await(self.arena, param_idx)
    }

    /// Name of the first parameter whose default initializer contains a
    /// top-level `await`, used by the default-param await rewrite.
    pub(super) fn first_await_default_param_name(
        &self,
        params: &tsz_parser::parser::NodeList,
    ) -> Option<String> {
        crate::transforms::emit_utils::first_await_default_param_name(self.arena, &params.nodes)
    }

    /// Find the first suspension expression inside `idx`, walking
    /// through transparent expression wrappers (parentheses, casts,
    /// non-null assertions) but stopping at nested function bodies.
    pub(super) fn find_suspension_expression(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(idx)?;
        if node.kind == self.suspension_kind()
            || (self.async_generator_mode && node.kind == syntax_kind_ext::AWAIT_EXPRESSION)
        {
            return Some(idx);
        }
        if node.kind == syntax_kind_ext::SPREAD_ELEMENT {
            let spread_expr = self
                .arena
                .get_spread(node)
                .map(|spread| spread.expression)
                .or_else(|| {
                    self.arena
                        .get_unary_expr_ex(node)
                        .map(|spread| spread.expression)
                });
            return spread_expr.and_then(|expr| self.find_suspension_expression(expr));
        }
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.is_function_expression_or_arrow()
        {
            return None;
        }
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(node)
        {
            if let Some(found) = self.find_suspension_expression(bin.left) {
                return Some(found);
            }
            return self.find_suspension_expression(bin.right);
        }
        if node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.arena.get_call_expr(node)
        {
            if let Some(found) = self.find_suspension_expression(call.expression) {
                return Some(found);
            }
            if let Some(args) = &call.arguments {
                for &arg_idx in &args.nodes {
                    if let Some(found) = self.find_suspension_expression(arg_idx) {
                        return Some(found);
                    }
                }
            }
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(node)
        {
            return self.find_suspension_expression(access.expression);
        }
        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(node)
        {
            if let Some(found) = self.find_suspension_expression(access.expression) {
                return Some(found);
            }
            return self.find_suspension_expression(access.name_or_argument);
        }
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            return self.find_suspension_expression(paren.expression);
        }
        // Type-only wrappers: `(await foo()) as T`, `<T>await foo()`,
        // `await foo() satisfies T`, `(await foo())!`. These are stripped
        // by `expression_to_ir`, so the analysis must look through them
        // too — otherwise we treat `var x = (await foo()) as T;` as
        // "no await" and emit `_a.sent()` without a preceding yield.
        if (node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && let Some(assertion) = self.arena.get_type_assertion(node)
        {
            return self.find_suspension_expression(assertion.expression);
        }
        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr_ex(node)
        {
            return self.find_suspension_expression(unary.expression);
        }
        if node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            && let Some(cond) = self.arena.get_conditional_expr(node)
        {
            if let Some(found) = self.find_suspension_expression(cond.condition) {
                return Some(found);
            }
            if let Some(found) = self.find_suspension_expression(cond.when_true) {
                return Some(found);
            }
            return self.find_suspension_expression(cond.when_false);
        }
        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr(node)
        {
            return self.find_suspension_expression(unary.operand);
        }
        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(node)
        {
            return self.find_suspension_expression(computed.expression);
        }
        if (node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            || node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
            && let Some(literal) = self.arena.get_literal_expr(node)
        {
            for &elem_idx in &literal.elements.nodes {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    continue;
                };

                if elem_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    && let Some(prop) = self.arena.get_property_assignment(elem_node)
                {
                    if let Some(found) = self.find_suspension_expression(prop.name) {
                        return Some(found);
                    }
                    if let Some(found) = self.find_suspension_expression(prop.initializer) {
                        return Some(found);
                    }
                } else if let Some(found) = self.find_suspension_expression(elem_idx) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// True when the source text spanning `idx` contains the literal
    /// substring `yield`. Used by lowering passes that need to fall
    /// back to source-text inspection when no structured yield node is
    /// available.
    pub(super) fn node_text_contains_yield(&self, idx: NodeIndex) -> bool {
        self.node_text_contains(idx, "yield")
    }

    /// True when the source text spanning `idx` contains `needle`.
    /// Returns `false` if the transformer was not constructed with a
    /// source-text reference or if `idx` is out of range.
    pub(super) fn node_text_contains(&self, idx: NodeIndex, needle: &str) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        let start = (node.pos as usize).min(text.len());
        let end = (node.end as usize).min(text.len());
        start < end && text[start..end].contains(needle)
    }
}
