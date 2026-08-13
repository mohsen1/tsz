//! Code reachability and fall-through analysis.

use crate::query_boundaries::flow_analysis as query;
use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use crate::statements::StatementCheckCallbacks;
use crate::statements::StatementChecker;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

// =============================================================================
// Reachability Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    pub(crate) fn call_expression_terminates_control_flow(&mut self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        match expr_node.kind {
            syntax_kind_ext::CALL_EXPRESSION => {
                let Some(call) = self.ctx.arena.get_call_expr(expr_node) else {
                    return false;
                };

                let callee = self
                    .ctx
                    .arena
                    .skip_parenthesized_and_assertions(call.expression);
                self.callee_explicitly_returns_never(callee)
                    || self.assertion_call_with_false_condition_terminates(expr_idx, callee)
            }
            syntax_kind_ext::NEW_EXPRESSION => self.get_type_of_node(expr_idx).is_never(),
            _ => false,
        }
    }

    fn assertion_call_with_false_condition_terminates(
        &mut self,
        call_idx: NodeIndex,
        callee_idx: NodeIndex,
    ) -> bool {
        let Some((predicate, params)) = self.assertion_predicate_for_call(call_idx) else {
            return false;
        };
        if predicate.type_id.is_some() {
            return false;
        }
        if !self.validate_assertion_call_target(call_idx, callee_idx) {
            return false;
        }
        let Some(asserted_expr) =
            self.assertion_call_asserted_expression(call_idx, predicate, &params)
        else {
            return false;
        };
        self.is_false_condition(asserted_expr)
    }

    pub(crate) fn terminating_iife_unreachable_anchor(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let expr_node = self.ctx.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.ctx.arena.get_call_expr(expr_node)?;
        let callee = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(call.expression);
        let callee_node = self.ctx.arena.get(callee)?;
        if callee_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            && callee_node.kind != syntax_kind_ext::ARROW_FUNCTION
        {
            return None;
        }

        let func = self.ctx.arena.get_function(callee_node)?;
        let body_idx = func.body;
        let body_node = self.ctx.arena.get(body_idx)?;
        let block = self.ctx.arena.get_block(body_node)?;
        let statement_count = block.statements.nodes.len();

        for statement_index in 0..statement_count {
            let stmt_idx = {
                let body_node = self.ctx.arena.get(body_idx)?;
                let block = self.ctx.arena.get_block(body_node)?;
                *block.statements.nodes.get(statement_index)?
            };
            if self.statement_always_throws(stmt_idx) {
                return Some(stmt_idx);
            }
        }

        None
    }

    /// Check if a callee expression explicitly returns `never` based on its
    /// declaration's return type annotation. This avoids fully type-checking the
    /// call expression, which would cache a potentially stale result in
    /// `node_types` during early phases (e.g., type environment building) when
    /// `this` hasn't been resolved yet.
    ///
    /// tsc's `isNeverReturningCall` similarly examines the callee's signature
    /// rather than evaluating the full call expression.
    fn callee_explicitly_returns_never(&mut self, callee_idx: NodeIndex) -> bool {
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return false;
        };

        match callee_node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .resolve_identifier_symbol(callee_idx)
                .is_some_and(|sym_id| self.symbol_explicitly_returns_never(sym_id)),
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.property_access_callee_explicitly_returns_never(callee_idx)
            }
            syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION => {
                // Direct IIFE callee: safe to check body for throws since the
                // function expression is the literal callee, not resolved through
                // a symbol that could be self-referential.
                self.declaration_explicitly_returns_never(callee_idx, true)
            }
            _ => false,
        }
    }

    /// Check if a property access callee (e.g., `this.fail`, `obj.bail`)
    /// explicitly returns `never` by resolving the property's symbol and
    /// checking its declaration's return type annotation.
    ///
    /// For `this.method()` calls, we resolve the method through the enclosing
    /// class symbol's member table (available from the binder) rather than
    /// fully type-checking the receiver, which would cache stale types during
    /// early phases like type environment building.
    fn property_access_callee_explicitly_returns_never(&mut self, callee_idx: NodeIndex) -> bool {
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return false;
        };
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        let property_name = &ident.escaped_text;

        // For `this.method()` or `obj.method()`, try to resolve the callee's
        // symbol through the binder's node_symbols (which is available without
        // full type-checking). The binder resolves property access names in some
        // cases.
        if let Some(&sym_id) = self.ctx.binder.node_symbols.get(&access.name_or_argument.0)
            && self.symbol_explicitly_returns_never(sym_id)
        {
            return true;
        }

        // For `this.method()` calls, try the enclosing class's member table.
        let Some(expr_node) = self.ctx.arena.get(access.expression) else {
            return false;
        };
        if expr_node.kind == SyntaxKind::ThisKeyword as u16 {
            // Directly search class member nodes for a method with matching name
            // and check its return type annotation. This avoids reliance on the
            // binder's class symbol members map which may not be available in all
            // checking paths.
            let matching_member = self.ctx.enclosing_class.as_ref().and_then(|class_info| {
                class_info.member_nodes.iter().copied().find(|&member_idx| {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        return false;
                    };
                    if member_node.kind != syntax_kind_ext::METHOD_DECLARATION {
                        return false;
                    }
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        return false;
                    };
                    self.get_property_name(method.name)
                        .is_some_and(|method_name| method_name == *property_name)
                })
            });
            if let Some(member_idx) = matching_member {
                return self.declaration_explicitly_returns_never(member_idx, false);
            }
        }

        // For namespace-qualified calls (e.g., `Debug.fail()`), resolve the
        // receiver identifier to its namespace symbol and look up the member
        // in its exports table.
        if expr_node.kind == SyntaxKind::Identifier as u16
            && let Some(ns_sym_id) = self.resolve_identifier_symbol(access.expression)
            && let Some(ns_symbol) = self.ctx.binder.get_symbol(ns_sym_id)
            && let Some(ref exports) = ns_symbol.exports
            && let Some(member_sym_id) = exports.get(property_name)
        {
            return self.symbol_explicitly_returns_never(member_sym_id);
        }

        // Fallback: resolve the receiver type and check the property.
        // This may produce stale results during early phases, but covers
        // non-`this` receivers like `services.panic()`.
        let object_type = self.get_type_of_node(access.expression);
        if object_type == TypeId::ANY || object_type == TypeId::ERROR {
            return false;
        }
        let resolved = self.resolve_type_for_property_access(object_type);
        query::property_access_function_returns_never(self.ctx.types, resolved, property_name)
    }

    fn symbol_explicitly_returns_never(&mut self, sym_id: tsz_binder::SymbolId) -> bool {
        let (is_alias, decl_idx) = {
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                return false;
            };
            (
                symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS),
                symbol.primary_declaration(),
            )
        };

        // An imported function's symbol is an alias whose primary declaration is
        // the import specifier (no return-type annotation) and whose real
        // declaration lives in another file's arena, so neither can be read
        // through `self.ctx.arena`. Inspect the alias's computed function-type
        // return instead — arena-independent and already resolved by the type
        // system through import and `export *` re-export chains — matching tsc's
        // `isNeverReturningCall`, which examines the resolved signature. Covers a
        // direct `import { die }` and transitive `export *` barrel re-exports.
        if is_alias {
            let ty = self.get_type_of_symbol(sym_id);
            return query::function_return_type(self.ctx.types, ty) == Some(TypeId::NEVER);
        }

        let Some(decl_idx) = decl_idx else {
            return false;
        };

        self.declaration_explicitly_returns_never(decl_idx, false)
    }

    fn declaration_explicitly_returns_never(
        &mut self,
        decl_idx: NodeIndex,
        check_body_for_throws: bool,
    ) -> bool {
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        if let Some(func) = self.ctx.arena.get_function(decl_node) {
            if func.type_annotation.is_some() {
                return self.get_type_from_type_node(func.type_annotation) == TypeId::NEVER;
            }
            // JS files express the return type via `@returns {never}` JSDoc
            // instead of a TypeScript return-type annotation. Honor that as an
            // explicit never-return so TS7027 fires for code after the call.
            if self.is_js_file()
                && self.ctx.compiler_options.check_js
                && let Some(jsdoc) = self.find_jsdoc_for_function(decl_idx)
                && {
                    let comment_start = self.get_jsdoc_comment_pos_for_function(decl_idx);
                    self.resolve_jsdoc_return_type(&jsdoc, comment_start) == Some(TypeId::NEVER)
                }
            {
                return true;
            }
            // For function/arrow expressions without an explicit return type annotation,
            // check if the body always throws (never completes normally). This handles
            // IIFEs like `(function() { throw "x" })()` which tsc recognizes as
            // never-returning calls.
            //
            // IMPORTANT: We only check for "always throws," NOT "doesn't fall through."
            // A function that always returns (e.g., `(() => { return 1; })()`) completes
            // normally from the caller's perspective - only throw/never-call terminates
            // the caller's control flow.
            //
            // CRITICAL: Only perform body analysis when `check_body_for_throws` is
            // true (i.e., the function is a direct IIFE callee).  When resolving
            // through a symbol (e.g., named function expression `self` calling
            // itself), body analysis would recurse infinitely because the body
            // contains calls to the same function.
            if check_body_for_throws && (decl_node.is_function_expression_or_arrow()) {
                let body_idx = func.body;
                if let Some(body_node) = self.ctx.arena.get(body_idx)
                    && let Some(block) = self.ctx.arena.get_block(body_node)
                {
                    return !block.statements.nodes.is_empty()
                        && self.block_always_throws(&block.statements.nodes);
                }
            }
            return false;
        }

        if let Some(method) = self.ctx.arena.get_method_decl(decl_node) {
            return method.type_annotation.is_some()
                && self.get_type_from_type_node(method.type_annotation) == TypeId::NEVER;
        }

        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) {
            if var_decl.type_annotation.is_none() {
                return false;
            }

            let declared_type = self.get_type_from_type_node(var_decl.type_annotation);
            return query::function_return_type(self.ctx.types, declared_type)
                == Some(TypeId::NEVER);
        }

        if let Some(param) = self.ctx.arena.get_parameter(decl_node) {
            if param.type_annotation.is_none() {
                return false;
            }

            let declared_type = self.get_type_from_type_node(param.type_annotation);
            return query::function_return_type(self.ctx.types, declared_type)
                == Some(TypeId::NEVER);
        }

        false
    }

    fn nullish_coalescing_switch_type(&mut self, switch_expr: NodeIndex) -> Option<TypeId> {
        let switch_expr = self.ctx.arena.skip_parenthesized(switch_expr);
        let node = self.ctx.arena.get(switch_expr)?;
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let bin = self.ctx.arena.get_binary_expr(node)?;
        if bin.operator_token != SyntaxKind::QuestionQuestionToken as u16 {
            return None;
        }

        let left_type = self
            .literal_type_from_initializer(bin.left)
            .unwrap_or_else(|| self.get_type_of_node(bin.left));
        let right_type = self
            .literal_type_from_initializer(bin.right)
            .unwrap_or_else(|| self.get_type_of_node(bin.right));
        if left_type == TypeId::ERROR || right_type == TypeId::ERROR {
            return None;
        }

        query::nullish_coalescing_switch_domain(self.ctx.types, left_type, right_type)
    }

    fn normalize_enum_union_members(&self, type_id: TypeId) -> TypeId {
        query::enum_member_union_domain(self.ctx.types, type_id)
    }

    fn typeof_switch_operand(&self, switch_expr: NodeIndex) -> Option<NodeIndex> {
        let switch_expr = self.ctx.arena.skip_parenthesized(switch_expr);
        let node = self.ctx.arena.get(switch_expr)?;
        if node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return None;
        }
        let unary = self.ctx.arena.get_unary_expr(node)?;
        if unary.operator != SyntaxKind::TypeOfKeyword as u16 {
            return None;
        }
        Some(self.ctx.arena.skip_parenthesized(unary.operand))
    }

    fn typeof_switch_domain_from_operand_type(&self, operand_type: TypeId) -> Option<TypeId> {
        let env = self.ctx.type_environment.borrow();
        query::typeof_switch_domain(self.ctx.types, Some(&env), operand_type)
    }

    fn switch_exhaustive_with_types(&self, switch_type: TypeId, case_types: &[TypeId]) -> bool {
        let env = self.ctx.type_environment.borrow();
        query::cases_exhaust_type(self.ctx.types, Some(&env), switch_type, case_types)
    }

    /// Cache-backed exhaustiveness probe used from immutable analysis paths.
    pub(crate) fn switch_has_exhaustive_coverage_cached(
        &self,
        switch_data: &tsz_parser::parser::node::SwitchData,
    ) -> bool {
        let switch_type =
            if let Some(typeof_operand) = self.typeof_switch_operand(switch_data.expression) {
                let operand_type = self
                    .literal_type_from_initializer(typeof_operand)
                    .or_else(|| self.ctx.node_types.get(&typeof_operand.0).copied())
                    .unwrap_or(TypeId::ERROR);
                self.typeof_switch_domain_from_operand_type(operand_type)
                    .unwrap_or(TypeId::ERROR)
            } else {
                self.literal_type_from_initializer(switch_data.expression)
                    .or_else(|| self.ctx.node_types.get(&switch_data.expression.0).copied())
                    .unwrap_or(TypeId::ERROR)
            };

        let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block) else {
            return false;
        };
        let Some(case_block) = self.ctx.arena.get_block(case_block_node) else {
            return false;
        };

        let mut case_types = Vec::new();
        for &clause_idx in &case_block.statements.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(clause) = self.ctx.arena.get_case_clause(clause_node) else {
                continue;
            };
            if clause.expression.is_none() {
                continue;
            }
            let case_type = self
                .literal_type_from_initializer(clause.expression)
                .or_else(|| self.ctx.node_types.get(&clause.expression.0).copied())
                .unwrap_or(TypeId::ERROR);
            case_types.push(case_type);
        }

        self.switch_exhaustive_with_types(switch_type, &case_types)
    }

    /// Check if a switch statement without a default clause is still exhaustive.
    ///
    /// This is true when excluding all case expression types from the switch
    /// discriminant leaves `never`.
    pub(crate) fn switch_has_exhaustive_coverage(
        &mut self,
        switch_data: &tsz_parser::parser::node::SwitchData,
    ) -> bool {
        let switch_type = if let Some(typeof_operand) =
            self.typeof_switch_operand(switch_data.expression)
        {
            let operand_type = self
                .literal_type_from_initializer(typeof_operand)
                .unwrap_or_else(|| self.get_type_of_node(typeof_operand));
            self.typeof_switch_domain_from_operand_type(operand_type)
                .unwrap_or(TypeId::ERROR)
        } else if let Some(coalesced) = self.nullish_coalescing_switch_type(switch_data.expression)
        {
            coalesced
        } else {
            self.literal_type_from_initializer(switch_data.expression)
                .unwrap_or_else(|| self.get_type_of_node(switch_data.expression))
        };

        let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block) else {
            return false;
        };
        let Some(case_block) = self.ctx.arena.get_block(case_block_node) else {
            return false;
        };

        let mut case_types = Vec::new();
        for &clause_idx in &case_block.statements.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(clause) = self.ctx.arena.get_case_clause(clause_node) else {
                continue;
            };
            if clause.expression.is_none() {
                continue;
            }
            let case_type = self
                .literal_type_from_initializer(clause.expression)
                .unwrap_or_else(|| self.get_type_of_node(clause.expression));
            case_types.push(case_type);
        }

        if self.switch_exhaustive_with_types(switch_type, &case_types) {
            return true;
        }

        let normalized_switch = self.normalize_enum_union_members(switch_type);
        let normalized_cases: Vec<TypeId> = case_types
            .iter()
            .copied()
            .map(|ty| self.normalize_enum_union_members(ty))
            .collect();
        let cases_union = query::union_types(self.ctx.types, normalized_cases);
        let env = self.ctx.type_environment.borrow();
        query::flow_assignability_outcome(
            self.ctx.types,
            Some(&env),
            None,
            normalized_switch,
            cases_union,
            self.ctx.strict_null_checks(),
        )
        .related
    }

    // =========================================================================
    // Block Analysis
    // =========================================================================

    /// Check if execution can fall through a block of statements.
    ///
    /// Returns true if execution can continue after the block, false if it always exits.
    pub(crate) fn block_falls_through(&mut self, statements: &[NodeIndex]) -> bool {
        for &stmt_idx in statements {
            if !self.statement_falls_through(stmt_idx) {
                return false;
            }
        }
        true
    }

    /// Check if a block of statements always terminates via `throw` or a
    /// call to a never-returning function. Unlike `block_falls_through`,
    /// this returns `false` for blocks that terminate via `return` - because
    /// a `return` inside a function body means the call completes normally.
    ///
    /// Used for IIFE body analysis: `(function() { throw "x" })()` terminates
    /// control flow, but `(() => { return 1; })()` does NOT.
    fn block_always_throws(&mut self, statements: &[NodeIndex]) -> bool {
        for &stmt_idx in statements {
            if self.statement_always_throws(stmt_idx) {
                return true;
            }
            if !self.statement_falls_through(stmt_idx) {
                // Block terminates but not via throw - e.g., `return`
                return false;
            }
        }
        false
    }

    /// Check if a statement always terminates via `throw` or a call to a
    /// never-returning function (not via `return`).
    fn statement_always_throws(&mut self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };
        match node.kind {
            syntax_kind_ext::THROW_STATEMENT => true,
            syntax_kind_ext::BLOCK => self
                .ctx
                .arena
                .get_block(node)
                .is_some_and(|block| self.block_always_throws(&block.statements.nodes)),
            syntax_kind_ext::EXPRESSION_STATEMENT => {
                let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) else {
                    return false;
                };
                self.call_expression_terminates_control_flow(expr_stmt.expression)
            }
            syntax_kind_ext::IF_STATEMENT => {
                let Some(if_data) = self.ctx.arena.get_if_statement(node) else {
                    return false;
                };
                if if_data.else_statement.is_none() {
                    return false;
                }
                self.statement_always_throws(if_data.then_statement)
                    && self.statement_always_throws(if_data.else_statement)
            }
            _ => false,
        }
    }

    // =========================================================================
    // Statement Analysis
    // =========================================================================

    /// Check if execution can fall through a statement.
    ///
    /// Returns true if execution can continue after the statement.
    pub(crate) fn statement_falls_through(&mut self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return true;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT | syntax_kind_ext::THROW_STATEMENT => false,
            syntax_kind_ext::BREAK_STATEMENT | syntax_kind_ext::CONTINUE_STATEMENT => {
                // An illegal jump (TS1104/TS1105/TS1107/TS1115/TS1116 — no
                // enclosing loop/switch/label, or one on the far side of a
                // function boundary) does not transfer control anywhere, so it
                // does not terminate flow either: tsc's binder only marks a
                // break/continue as unreachable-after when it has a resolved
                // flow target (`bindBreakOrContinueFlow`, which no-ops when
                // `breakTarget`/`continueTarget` is unset). Only a jump with a
                // legal target makes the following code unreachable.
                !self.jump_statement_has_legal_target(stmt_idx, node)
            }
            syntax_kind_ext::BLOCK => self
                .ctx
                .arena
                .get_block(node)
                .is_none_or(|block| self.block_falls_through(&block.statements.nodes)),
            syntax_kind_ext::EXPRESSION_STATEMENT => {
                let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) else {
                    return true;
                };
                !self.call_expression_terminates_control_flow(expr_stmt.expression)
            }
            // VARIABLE_STATEMENT falls through (handled by the wildcard arm
            // below). TypeScript only treats expression-statement-level never
            // calls as terminators of control flow; `const x = fail()` still
            // leaves the function falling off the end and must surface TS2355.
            syntax_kind_ext::IF_STATEMENT => {
                let Some(if_data) = self.ctx.arena.get_if_statement(node) else {
                    return true;
                };
                let then_falls = self.statement_falls_through(if_data.then_statement);
                if if_data.else_statement.is_none() {
                    // A missing else branch normally means execution can skip
                    // the `then` branch entirely, so the statement as a whole
                    // falls through regardless of `then_falls`. But when the
                    // condition is a compile-time-true constant there is no
                    // implicit else path — the `then` branch always runs, so
                    // completion follows it exactly (mirrors
                    // `loop_falls_through`'s `is_true_condition` handling for
                    // `while (true)` with no reachable `break`).
                    return if self.is_true_condition(if_data.expression) {
                        then_falls
                    } else {
                        true
                    };
                }
                let else_falls = self.statement_falls_through(if_data.else_statement);
                then_falls || else_falls
            }
            syntax_kind_ext::SWITCH_STATEMENT => self.switch_falls_through(stmt_idx),
            syntax_kind_ext::TRY_STATEMENT => self.try_falls_through(stmt_idx),
            syntax_kind_ext::CATCH_CLAUSE => self
                .ctx
                .arena
                .get_catch_clause(node)
                .is_none_or(|catch_data| self.statement_falls_through(catch_data.block)),
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => self.loop_falls_through(stmt_idx, node),
            syntax_kind_ext::LABELED_STATEMENT => {
                let Some(labeled) = self.ctx.arena.get_labeled_statement(node) else {
                    return true;
                };
                if self.statement_falls_through(labeled.statement) {
                    return true;
                }
                // The labeled statement's own body never completes normally
                // (e.g. it's a `try` whose every path returns/throws/loops
                // forever), but a `break <label>` reachable anywhere inside —
                // even several loops or switches deep — still exits the WHOLE
                // labeled statement and resumes right after it: tsc's
                // `bindBreakOrContinueFlow` attaches that break to the
                // label's own break-target flow node no matter what
                // non-iteration construct (`try`, `if`, a bare block, ...)
                // the label wraps.
                self.contains_break_targeting(labeled.statement, stmt_idx)
            }
            _ => true,
        }
    }

    /// Whether a `break`/`continue` statement has a resolvable target: an
    /// enclosing iteration statement (both) or switch statement (break only)
    /// for an unlabeled jump, or a matching enclosing label for a labeled one
    /// (further requiring that label to wrap an iteration statement, for
    /// `continue`) — walked structurally from the jump node itself, the same
    /// way tsc's own grammar check does, rather than from ambient
    /// depth/label-stack state that is only valid while that node is
    /// currently being visited. A function-like boundary (or class static
    /// block, which is its own jump boundary) crossed before any match means
    /// the jump is illegal (TS1104/TS1105/TS1107/TS1115/TS1116) and has no
    /// target.
    fn jump_statement_has_legal_target(
        &self,
        stmt_idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let is_break = node.kind == syntax_kind_ext::BREAK_STATEMENT;
        let label_name = self.ctx.arena.get_jump_data(node).and_then(|jump_data| {
            if jump_data.label.is_none() {
                None
            } else {
                self.get_node_text(jump_data.label)
            }
        });

        let mut current = stmt_idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return true;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return true;
            };
            if ext.parent.is_none() {
                return false;
            }
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return true;
            };

            if parent_node.is_function_like()
                || parent_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
            {
                return false;
            }

            match &label_name {
                None => {
                    let is_iteration = matches!(
                        parent_node.kind,
                        syntax_kind_ext::WHILE_STATEMENT
                            | syntax_kind_ext::DO_STATEMENT
                            | syntax_kind_ext::FOR_STATEMENT
                            | syntax_kind_ext::FOR_IN_STATEMENT
                            | syntax_kind_ext::FOR_OF_STATEMENT
                    );
                    if is_iteration
                        || (is_break && parent_node.kind == syntax_kind_ext::SWITCH_STATEMENT)
                    {
                        return true;
                    }
                }
                Some(name) => {
                    if parent_node.kind == syntax_kind_ext::LABELED_STATEMENT
                        && let Some(labeled) = self.ctx.arena.get_labeled_statement(parent_node)
                        && self.get_node_text(labeled.label).as_deref() == Some(name.as_str())
                    {
                        return is_break
                            || StatementChecker::is_iteration_or_nested_iteration(
                                self.ctx.arena,
                                labeled.statement,
                            );
                    }
                }
            }
            current = parent_idx;
        }
    }

    // =========================================================================
    // Control Flow Analysis
    // =========================================================================

    /// Check if a switch statement falls through.
    ///
    /// Returns true if execution can continue after the switch.
    pub(crate) fn switch_falls_through(&mut self, switch_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(switch_idx) else {
            return true;
        };
        let Some(switch_data) = self.ctx.arena.get_switch(node) else {
            return true;
        };
        let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block) else {
            return true;
        };
        let Some(case_block) = self.ctx.arena.get_block(case_block_node) else {
            return true;
        };

        let has_default = case_block.statements.nodes.iter().any(|&clause_idx| {
            self.ctx
                .arena
                .get(clause_idx)
                .is_some_and(|clause_node| clause_node.kind == syntax_kind_ext::DEFAULT_CLAUSE)
        });

        // Without a default clause, unmatched discriminants can skip the switch
        // body unless case coverage is exhaustive.
        if !has_default && !self.switch_has_exhaustive_coverage(switch_data) {
            return true;
        }

        // Analyze from bottom to top so empty/grouped clauses inherit the
        // fall-through behavior of the next clause in the chain.
        let mut falls_from_next = true;
        let mut any_entry_falls_through = false;

        for &clause_idx in case_block.statements.nodes.iter().rev() {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(clause) = self.ctx.arena.get_case_clause(clause_node) else {
                continue;
            };

            let clause_falls_through = if clause.statements.nodes.is_empty() {
                // Empty case labels fall through to the next clause.
                falls_from_next
            } else if clause
                .statements
                .nodes
                .iter()
                .any(|&stmt| self.contains_break_targeting(stmt, switch_idx))
            {
                // A break that specifically targets this switch can complete it
                // normally, even if later clauses would not fall through. A
                // labeled break reachable from this clause but resolving to some
                // *other* construct (an outer loop, or a label on a statement
                // that merely wraps this switch) does not complete the switch —
                // it skips past it entirely, the same way `contains_break_targeting`
                // already distinguishes targets for `loop_falls_through` and the
                // `LABELED_STATEMENT` fall-through check above.
                true
            } else if self.block_falls_through(&clause.statements.nodes) {
                // Non-terminating clauses continue into the next clause.
                falls_from_next
            } else {
                // Clause exits function/control flow (e.g. return/throw).
                false
            };

            any_entry_falls_through |= clause_falls_through;
            falls_from_next = clause_falls_through;
        }

        any_entry_falls_through
    }

    /// Check if a try statement falls through.
    ///
    /// Returns true if execution can continue after the try statement.
    pub(crate) fn try_falls_through(&mut self, try_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(try_idx) else {
            return true;
        };
        let Some(try_data) = self.ctx.arena.get_try(node) else {
            return true;
        };

        let try_falls = self.statement_falls_through(try_data.try_block);
        let catch_falls = if try_data.catch_clause.is_some() {
            self.statement_falls_through(try_data.catch_clause)
        } else {
            false
        };

        if try_data.finally_block.is_some() {
            let finally_falls = self.statement_falls_through(try_data.finally_block);
            if !finally_falls {
                return false;
            }
        }

        try_falls || catch_falls
    }

    /// Check if a loop statement falls through.
    ///
    /// Returns true if execution can continue after the loop.
    pub(crate) fn loop_falls_through(
        &mut self,
        stmt_idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let Some(loop_data) = self.ctx.arena.get_loop(node) else {
            return true;
        };

        let condition_always_true = if loop_data.condition.is_none() {
            true
        } else {
            self.is_true_condition(loop_data.condition)
        };

        if condition_always_true && !self.contains_break_targeting(loop_data.statement, stmt_idx) {
            return false;
        }

        true
    }

    /// Check if a condition is always true.
    ///
    /// Mirrors `tsc`'s reachability rule, which is narrower than general
    /// constant folding. `binder.ts`'s `createFlowCondition` tests
    /// `expression.kind === SyntaxKind.TrueKeyword` on the condition node
    /// **as written** — it does not skip parentheses and does not fold a
    /// prefix `!`. So `if (true)` marks the implicit else unreachable while
    /// `if ((true))` and `if (!false)` do not.
    ///
    /// `&&`/`||` still compose, but for a different reason than folding:
    /// `bindCondition` recurses into a logical expression and binds each
    /// operand against its own branch targets, so the literal `true` inside
    /// `true && true` is what reaches the kind check. Reproducing that as
    /// recursion here gives the same answer, and stopping at parentheses and
    /// `!` reproduces where `tsc` stops.
    pub(crate) fn is_true_condition(&self, condition_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(condition_idx) else {
            return false;
        };
        if node.kind == SyntaxKind::TrueKeyword as u16 {
            return true;
        }
        if let Some(bin) = self.ctx.arena.get_binary_expr(node) {
            if bin.operator_token == SyntaxKind::AmpersandAmpersandToken as u16 {
                return self.is_true_condition(bin.left) && self.is_true_condition(bin.right);
            }
            if bin.operator_token == SyntaxKind::BarBarToken as u16 {
                return self.is_true_condition(bin.left) || self.is_true_condition(bin.right);
            }
        }
        false
    }

    /// Check if a condition is always false.
    ///
    /// The `FalseKeyword` counterpart of [`Self::is_true_condition`], with the
    /// same stop conditions: no parenthesis skipping, no prefix-`!` folding.
    pub(crate) fn is_false_condition(&self, condition_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(condition_idx) else {
            return false;
        };
        if node.kind == SyntaxKind::FalseKeyword as u16 {
            return true;
        }
        if let Some(bin) = self.ctx.arena.get_binary_expr(node) {
            if bin.operator_token == SyntaxKind::AmpersandAmpersandToken as u16 {
                return self.is_false_condition(bin.left)
                    || (self.is_true_condition(bin.left) && self.is_false_condition(bin.right));
            }
            if bin.operator_token == SyntaxKind::BarBarToken as u16 {
                return self.is_false_condition(bin.left) && self.is_false_condition(bin.right);
            }
        }
        false
    }

    /// Whether `stmt_idx`'s subtree contains a `break` statement whose
    /// legal jump target — resolved the same structural way as
    /// [`Self::jump_statement_has_legal_target`] (nearest enclosing
    /// loop/switch for an unlabeled break, nearest same-named
    /// `LABELED_STATEMENT` for a labeled one) — is `target_idx`, or a label
    /// stacked directly around/inside `target_idx` with nothing else
    /// between (so the two exit to the identical source position).
    ///
    /// Recurses into nested loops and switches too: a *labeled* break
    /// several loops deep can still target an outer construct, and
    /// resolving each break's actual
    /// target — rather than just noting a break exists — is what tells that
    /// apart from a break that targets the inner loop/switch itself.
    pub(crate) fn contains_break_targeting(
        &self,
        stmt_idx: NodeIndex,
        target_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            syntax_kind_ext::BREAK_STATEMENT => self
                .resolve_break_target(stmt_idx, node)
                .is_some_and(|resolved| {
                    self.innermost_labeled_target(resolved)
                        == self.innermost_labeled_target(target_idx)
                }),
            syntax_kind_ext::BLOCK => self.ctx.arena.get_block(node).is_some_and(|block| {
                block
                    .statements
                    .nodes
                    .iter()
                    .any(|&stmt| self.contains_break_targeting(stmt, target_idx))
            }),
            syntax_kind_ext::IF_STATEMENT => {
                self.ctx
                    .arena
                    .get_if_statement(node)
                    .is_some_and(|if_data| {
                        self.contains_break_targeting(if_data.then_statement, target_idx)
                            || (if_data.else_statement.is_some()
                                && self
                                    .contains_break_targeting(if_data.else_statement, target_idx))
                    })
            }
            syntax_kind_ext::TRY_STATEMENT => {
                self.ctx.arena.get_try(node).is_some_and(|try_data| {
                    self.contains_break_targeting(try_data.try_block, target_idx)
                        || (try_data.catch_clause.is_some()
                            && self.contains_break_targeting(try_data.catch_clause, target_idx))
                        || (try_data.finally_block.is_some()
                            && self.contains_break_targeting(try_data.finally_block, target_idx))
                })
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                self.ctx
                    .arena
                    .get_catch_clause(node)
                    .is_some_and(|catch_data| {
                        self.contains_break_targeting(catch_data.block, target_idx)
                    })
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => self
                .ctx
                .arena
                .get_loop(node)
                .is_some_and(|d| self.contains_break_targeting(d.statement, target_idx)),
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => self
                .ctx
                .arena
                .get_for_in_of(node)
                .is_some_and(|d| self.contains_break_targeting(d.statement, target_idx)),
            syntax_kind_ext::SWITCH_STATEMENT => {
                self.ctx.arena.get_switch(node).is_some_and(|switch_data| {
                    self.ctx
                        .arena
                        .get(switch_data.case_block)
                        .and_then(|case_block_node| self.ctx.arena.get_block(case_block_node))
                        .is_some_and(|case_block| {
                            case_block.statements.nodes.iter().any(|&clause_idx| {
                                self.ctx
                                    .arena
                                    .get(clause_idx)
                                    .and_then(|clause_node| {
                                        self.ctx.arena.get_case_clause(clause_node)
                                    })
                                    .is_some_and(|clause| {
                                        clause.statements.nodes.iter().any(|&stmt| {
                                            self.contains_break_targeting(stmt, target_idx)
                                        })
                                    })
                            })
                        })
                })
            }
            syntax_kind_ext::LABELED_STATEMENT => self
                .ctx
                .arena
                .get_labeled_statement(node)
                .is_some_and(|labeled| {
                    self.contains_break_targeting(labeled.statement, target_idx)
                }),
            _ => false,
        }
    }

    /// Resolve a `break` statement's legal jump target: the nearest
    /// enclosing loop/switch for an unlabeled break, or the nearest
    /// `LABELED_STATEMENT` whose label matches for a labeled one. Mirrors
    /// [`Self::jump_statement_has_legal_target`]'s walk but returns the
    /// target node instead of a bool; `None` means the break has no legal
    /// target (illegal jump, already diagnosed elsewhere as TS1105/TS1116) —
    /// callers only compare against a specific candidate, so an illegal
    /// break simply never matches anything.
    fn resolve_break_target(
        &self,
        stmt_idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<NodeIndex> {
        let label_name = self.ctx.arena.get_jump_data(node).and_then(|jump_data| {
            if jump_data.label.is_none() {
                None
            } else {
                self.get_node_text(jump_data.label)
            }
        });

        let mut current = stmt_idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            let parent_idx = ext.parent;
            let parent_node = self.ctx.arena.get(parent_idx)?;

            if parent_node.is_function_like()
                || parent_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
            {
                return None;
            }

            match &label_name {
                None => {
                    let is_iteration = matches!(
                        parent_node.kind,
                        syntax_kind_ext::WHILE_STATEMENT
                            | syntax_kind_ext::DO_STATEMENT
                            | syntax_kind_ext::FOR_STATEMENT
                            | syntax_kind_ext::FOR_IN_STATEMENT
                            | syntax_kind_ext::FOR_OF_STATEMENT
                    );
                    if is_iteration || parent_node.kind == syntax_kind_ext::SWITCH_STATEMENT {
                        return Some(parent_idx);
                    }
                }
                Some(name) => {
                    if parent_node.kind == syntax_kind_ext::LABELED_STATEMENT
                        && let Some(labeled) = self.ctx.arena.get_labeled_statement(parent_node)
                        && self.get_node_text(labeled.label).as_deref() == Some(name.as_str())
                    {
                        return Some(parent_idx);
                    }
                }
            }
            current = parent_idx;
        }
    }

    /// Fully unwrap a chain of `LABELED_STATEMENT`s (`a: b: while (true) {}`)
    /// down to the core statement they wrap, so two labels stacked around —
    /// or a label directly around — the same loop/switch/try compare equal.
    /// Non-labeled input is returned unchanged.
    fn innermost_labeled_target(&self, mut idx: NodeIndex) -> NodeIndex {
        loop {
            let Some(node) = self.ctx.arena.get(idx) else {
                return idx;
            };
            if node.kind != syntax_kind_ext::LABELED_STATEMENT {
                return idx;
            }
            let Some(labeled) = self.ctx.arena.get_labeled_statement(node) else {
                return idx;
            };
            idx = labeled.statement;
        }
    }

    /// Check if a statement is a `var` declaration without any initializers.
    /// `var t;` after a throw/return is hoisted and has no runtime effect,
    /// so TypeScript doesn't report TS7027 for it.
    pub(crate) fn is_var_without_initializer(
        &self,
        _stmt_idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        use tsz_parser::parser::flags::node_flags;

        if node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return false;
        }
        let Some(var_data) = self.ctx.arena.get_variable(node) else {
            return false;
        };
        // var_data.declarations.nodes contains VARIABLE_DECLARATION_LIST nodes
        // (typically one). Walk each list, then each list's individual
        // VARIABLE_DECLARATIONs, to check both `var`-ness and initializer
        // presence. Iterating only the outer list previously missed
        // initializers on declarations like `var x = 10;`, so unreachable
        // `var x = 10;` after `return` was wrongly skipped instead of
        // emitting TS7027.
        for &list_idx in &var_data.declarations.nodes {
            // Check let/const flag at the list level.
            let list_flags = self.ctx.arena.get_variable_declaration_flags(list_idx);
            if node_flags::is_let_or_const(list_flags) {
                return false;
            }
            let Some(list_node) = self.ctx.arena.get(list_idx) else {
                continue;
            };
            let Some(var_list) = self.ctx.arena.get_variable(list_node) else {
                continue;
            };
            for &decl_idx in &var_list.declarations.nodes {
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
                    && var_decl.initializer.is_some()
                {
                    return false;
                }
            }
        }
        true
    }

    // =========================================================================
    // Bare-Return Collection (TS7030)
    // =========================================================================

    /// Collect every bare `return;` (no expression) syntactically inside a
    /// function body, for the `noImplicitReturns` (TS7030) check.
    ///
    /// `tsc` reports TS7030 from two independent sources: fall-off-the-end
    /// (already covered by [`Self::function_body_falls_through`] and its
    /// callers) and each bare `return;` in the function's own scope. The
    /// bare-return half is **not** gated by flow reachability — oracle-verified
    /// (`typescript` 6.0.2) against `if (false) { return; }`, a bare return
    /// after an unconditional `return`/`throw`, and one after
    /// `while (true) { return 1; }`: `tsc` reports TS7030 at the bare return
    /// in every one of these dead-code cases, alongside TS7027 when
    /// `allowUnreachableCode` is off. So this only needs a plain structural
    /// descent through every statement kind that can contain a nested
    /// statement (block, if/else, switch, try/catch/finally, loop, labeled
    /// statement) — no `statement_falls_through`/`is_true_condition` pruning —
    /// and must not descend into nested function-like bodies (a class
    /// method, function, or arrow declared inside this one has its own
    /// independent return-type context); no arm below matches their syntax
    /// kinds, so recursion stops there on its own.
    pub(crate) fn collect_bare_returns(&mut self, body_idx: NodeIndex) -> Vec<NodeIndex> {
        let mut out = Vec::new();
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return out;
        };
        if body_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.ctx.arena.get_block(body_node)
        {
            self.collect_block_bare_returns(&block.statements.nodes, &mut out);
        }
        out
    }

    fn collect_block_bare_returns(&mut self, statements: &[NodeIndex], out: &mut Vec<NodeIndex>) {
        for &stmt_idx in statements {
            self.collect_statement_bare_returns(stmt_idx, out);
        }
    }

    fn collect_statement_bare_returns(&mut self, stmt_idx: NodeIndex, out: &mut Vec<NodeIndex>) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if self
                    .ctx
                    .arena
                    .get_return_statement(node)
                    .is_some_and(|ret| ret.expression.is_none())
                {
                    out.push(stmt_idx);
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    self.collect_block_bare_returns(&block.statements.nodes, out);
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                let Some(if_data) = self.ctx.arena.get_if_statement(node) else {
                    return;
                };
                self.collect_statement_bare_returns(if_data.then_statement, out);
                if if_data.else_statement.is_some() {
                    self.collect_statement_bare_returns(if_data.else_statement, out);
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                let Some(switch_data) = self.ctx.arena.get_switch(node) else {
                    return;
                };
                let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block) else {
                    return;
                };
                let Some(case_block) = self.ctx.arena.get_block(case_block_node) else {
                    return;
                };
                for &clause_idx in &case_block.statements.nodes {
                    let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                        continue;
                    };
                    let Some(clause) = self.ctx.arena.get_case_clause(clause_node) else {
                        continue;
                    };
                    self.collect_block_bare_returns(&clause.statements.nodes, out);
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                let Some(try_data) = self.ctx.arena.get_try(node) else {
                    return;
                };
                self.collect_statement_bare_returns(try_data.try_block, out);
                if try_data.catch_clause.is_some() {
                    self.collect_statement_bare_returns(try_data.catch_clause, out);
                }
                if try_data.finally_block.is_some() {
                    self.collect_statement_bare_returns(try_data.finally_block, out);
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_statement_bare_returns(catch_data.block, out);
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_statement_bare_returns(loop_data.statement, out);
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(for_in_of) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_statement_bare_returns(for_in_of.statement, out);
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_statement_bare_returns(labeled.statement, out);
                }
            }
            _ => {}
        }
    }

    /// Emit TS7030 for every bare `return;` in `body_idx`.
    ///
    /// `tsc` anchors this at the `return` keyword itself (a fixed 6-column
    /// span), not the statement's full range — which in tsz's parser
    /// (`parse_return_statement`) extends through the trailing semicolon.
    /// `node.pos` is exactly where the `ReturnKeyword` token starts (recorded
    /// before `parse_expected(SyntaxKind::ReturnKeyword)` consumes it), so a
    /// raw 6-length span from there reproduces tsc's anchor without going
    /// through `error_at_node`'s full-node span.
    pub(crate) fn report_ts7030_for_bare_returns(&mut self, body_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        let bare_returns = self.collect_bare_returns(body_idx);
        for ret_idx in bare_returns {
            let Some(node) = self.ctx.arena.get(ret_idx) else {
                continue;
            };
            self.error_at_position(
                node.pos,
                "return".len() as u32,
                diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
            );
        }
    }
}
