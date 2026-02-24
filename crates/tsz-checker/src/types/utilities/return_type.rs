//! Return type inference utilities for `CheckerState`.
//!
//! Functions for inferring return types from function bodies by collecting
//! return expressions, analyzing control flow (fall-through detection),
//! and checking for explicit `any` assertion returns.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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
        _function_idx: NodeIndex,
        body_idx: NodeIndex,
        return_context: Option<TypeId>,
    ) -> TypeId {
        // The inference pass evaluates return expressions WITHOUT narrowing
        // context, which can produce false errors (e.g. TS2339 for discriminated
        // union property accesses) and cache wrong types.  Snapshot diagnostic,
        // node-type, and flow-analysis-cache state, then restore after inference
        // so that the subsequent check_statement pass recomputes everything with
        // proper narrowing context.
        let diag_count = self.ctx.diagnostics.len();
        let emitted_before = self.ctx.emitted_diagnostics.clone();
        let emitted_ts2454_before = self.ctx.emitted_ts2454_errors.clone();
        let modules_ts2307_before = self.ctx.modules_with_ts2307_emitted.clone();
        let cached_before: std::collections::HashSet<u32> =
            self.ctx.node_types.keys().copied().collect();
        let flow_cache_before = self.ctx.flow_analysis_cache.borrow().clone();

        let result = self.infer_return_type_from_body_inner(body_idx, return_context);

        self.ctx.diagnostics.truncate(diag_count);
        self.ctx.emitted_diagnostics = emitted_before;
        self.ctx.emitted_ts2454_errors = emitted_ts2454_before;
        self.ctx.modules_with_ts2307_emitted = modules_ts2307_before;
        self.ctx.node_types.retain(|k, _| cached_before.contains(k));
        *self.ctx.flow_analysis_cache.borrow_mut() = flow_cache_before;

        // Widen inferred return types when there is no contextual return type.
        // `function f() { return "a"; }` → return type `string` (widened).
        // But `const g: () => "a" = () => "a"` → return type `"a"` (preserved
        // by contextual typing).
        if return_context.is_none() {
            let widened = self.widen_literal_type(result);
            if !self.ctx.strict_null_checks()
                && tsz_solver::type_queries::is_only_null_or_undefined(self.ctx.types, widened)
            {
                TypeId::ANY
            } else {
                widened
            }
        } else {
            result
        }
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
            return self.return_expression_type(body_idx, return_context);
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

            return if !may_not_fall_through || self.function_body_falls_through(body_idx) {
                TypeId::VOID
            } else {
                TypeId::NEVER
            };
        }

        if saw_empty || self.function_body_falls_through(body_idx) {
            return_types.push(TypeId::VOID);
        }

        factory.union(return_types)
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

        let prev_context = self.ctx.contextual_type;
        if let Some(ctx_type) = return_context {
            self.ctx.contextual_type = Some(ctx_type);
        }
        let return_type = self.get_type_of_node(expr_idx);
        self.ctx.contextual_type = prev_context;
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
        return_types: &mut Vec<TypeId>,
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
                        let return_type =
                            self.return_expression_type(return_data.expression, return_context);
                        return_types.push(return_type);
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
