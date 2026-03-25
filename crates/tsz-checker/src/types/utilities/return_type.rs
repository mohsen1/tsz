//! Return type inference utilities for `CheckerState`.
//!
//! Functions for inferring return types from function bodies by collecting
//! return expressions, analyzing control flow (fall-through detection),
//! and checking for explicit `any` assertion returns.

use crate::context::TypingRequest;
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
        function_idx: NodeIndex,
        body_idx: NodeIndex,
        return_context: Option<TypeId>,
    ) -> TypeId {
        // The inference pass evaluates return expressions WITHOUT narrowing
        // context, which can produce false errors (e.g. TS2339 for discriminated
        // union property accesses) and cache wrong types.  Snapshot diagnostic,
        // node-type, and flow-analysis-cache state, then restore after inference
        // so that the subsequent check_statement pass recomputes everything with
        // proper narrowing context.
        let snap = self.ctx.snapshot_return_type();

        if self.ctx.is_checking_statements
            && !function_idx.is_none()
            && !self.contextual_return_suppresses_circularity(return_context)
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
                self.record_pending_circular_return_sites(function_idx, body_idx);
            }
        }

        let result = self.infer_return_type_from_body_inner(body_idx, return_context);

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
            self.ctx.rollback_return_type(&snap);
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

        self.ctx.rollback_return_type(&snap);

        // Widen inferred return types when there is no contextual return type,
        // unless the caller explicitly requested literal preservation
        // (e.g. computed property name resolution or literal-sensitive inference).
        // `function f() { return "a"; }` → return type `string` (widened).
        // But `const g: () => "a" = () => "a"` → return type `"a"` (preserved
        // by contextual typing).
        if return_context.is_none() {
            if self.ctx.preserve_literal_types {
                return result;
            }
            self.widen_literal_type(result)
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
            // When a function has value-returning paths AND also falls through
            // (or has empty `return;`), the non-returning paths contribute
            // `undefined` to the union, not `void`. tsc behaves the same way:
            // `function f(x) { if (x) return 1; }` → `number | undefined`
            return_types.push(TypeId::UNDEFINED);
        }

        // Filter out ERROR types from return type inference when there are
        // non-ERROR alternatives. This handles recursive self-referencing functions
        // like `const fn1 = () => { if (...) return fn1(); return 0; }`.
        // The recursive call resolves to ERROR during type computation (circular
        // reference), but the base case `return 0` provides a concrete `number` type.
        // tsc filters out circular contributions and infers the return type from
        // non-circular branches only, so `fn1` gets return type `number`.
        let has_non_error = return_types.iter().any(|&t| t != TypeId::ERROR);
        if has_non_error {
            return_types.retain(|&t| t != TypeId::ERROR);
        }

        factory.union(return_types)
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
        use tsz_solver::lazy_def_id;

        let Some(def_id) = lazy_def_id(self.ctx.types, type_id) else {
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

        if symbol.flags & tsz_binder::symbol_flags::CLASS == 0 {
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
        use crate::query_boundaries::common::type_param_info;
        let effective_return_context =
            return_context.filter(|&ctx_type| type_param_info(self.ctx.types, ctx_type).is_none());
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
        let return_type = self.get_type_of_node_with_request(expr_idx, &request);
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
