//! Code reachability and fall-through analysis.

use crate::query_boundaries::flow_analysis as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{NarrowingContext, TypeId};

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
            }
            syntax_kind_ext::NEW_EXPRESSION => self.get_type_of_node(expr_idx).is_never(),
            _ => false,
        }
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
        if expr_node.kind == SyntaxKind::ThisKeyword as u16
            && let Some(ref class_info) = self.ctx.enclosing_class.clone()
        {
            // Directly search class member nodes for a method with matching name
            // and check its return type annotation. This avoids reliance on the
            // binder's class symbol members map which may not be available in all
            // checking paths.
            for &member_idx in &class_info.member_nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::METHOD_DECLARATION {
                    continue;
                }
                let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                    continue;
                };
                let Some(method_name) = self.get_property_name(method.name) else {
                    continue;
                };
                if method_name == *property_name {
                    return self.declaration_explicitly_returns_never(member_idx, false);
                }
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
        // Use the solver's property access to find the method type and check
        // if it has a never return type.
        if let tsz_solver::operations::property::PropertyAccessResult::Success { type_id, .. } =
            self.ctx
                .types
                .resolve_property_access(resolved, property_name)
        {
            return query::function_return_type(self.ctx.types, type_id) == Some(TypeId::NEVER);
        }

        false
    }

    fn symbol_explicitly_returns_never(&mut self, sym_id: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
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
            if check_body_for_throws
                && (decl_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || decl_node.kind == syntax_kind_ext::ARROW_FUNCTION)
            {
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

        let left_non_nullish =
            crate::query_boundaries::flow::narrow_optional_chain(self.ctx.types, left_type);
        if left_non_nullish == TypeId::ERROR {
            return None;
        }
        if left_non_nullish == TypeId::NEVER {
            return Some(right_type);
        }
        Some(self.ctx.types.union2(left_non_nullish, right_type))
    }

    fn normalize_enum_union_members(&self, type_id: TypeId) -> TypeId {
        if let Some(members) = query::union_members_for_type(self.ctx.types, type_id) {
            let normalized: Vec<TypeId> = members
                .into_iter()
                .map(|member| query::enum_member_domain(self.ctx.types, member))
                .collect();
            query::union_types(self.ctx.types, normalized)
        } else {
            query::enum_member_domain(self.ctx.types, type_id)
        }
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
        if operand_type == TypeId::ERROR {
            return None;
        }

        const TYPEOF_RESULTS: [&str; 8] = [
            "string",
            "number",
            "bigint",
            "boolean",
            "symbol",
            "undefined",
            "object",
            "function",
        ];

        let env = self.ctx.type_environment.borrow();
        let narrowing = NarrowingContext::new(self.ctx.types).with_resolver(&*env);

        let mut possible = Vec::with_capacity(TYPEOF_RESULTS.len());
        for typeof_result in TYPEOF_RESULTS {
            if narrowing.narrow_by_typeof(operand_type, typeof_result) != TypeId::NEVER {
                possible.push(self.ctx.types.literal_string(typeof_result));
            }
        }

        match possible.len() {
            0 => None,
            1 => possible.first().copied(),
            _ => Some(self.ctx.types.union(possible)),
        }
    }

    fn switch_exhaustive_with_types(&self, switch_type: TypeId, case_types: &[TypeId]) -> bool {
        let switch_type = query::enum_member_domain(self.ctx.types, switch_type);
        if matches!(switch_type, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN)
            || case_types.is_empty()
        {
            return false;
        }
        if case_types
            .iter()
            .any(|&ty| matches!(ty, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN))
        {
            return false;
        }

        let env = self.ctx.type_environment.borrow();
        let narrowing = NarrowingContext::new(self.ctx.types).with_resolver(&*env);
        narrowing.narrow_excluding_types(switch_type, case_types) == TypeId::NEVER
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
        query::is_assignable_with_env(
            self.ctx.types,
            &env,
            normalized_switch,
            cases_union,
            self.ctx.strict_null_checks(),
        )
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
            syntax_kind_ext::RETURN_STATEMENT
            | syntax_kind_ext::THROW_STATEMENT
            | syntax_kind_ext::BREAK_STATEMENT
            | syntax_kind_ext::CONTINUE_STATEMENT => false,
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
            syntax_kind_ext::VARIABLE_STATEMENT => {
                let Some(var_stmt) = self.ctx.arena.get_variable(node) else {
                    return true;
                };
                for &decl_idx in &var_stmt.declarations.nodes {
                    let Some(list_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(var_list) = self.ctx.arena.get_variable(list_node) else {
                        continue;
                    };
                    for &list_decl_idx in &var_list.declarations.nodes {
                        let Some(list_decl_node) = self.ctx.arena.get(list_decl_idx) else {
                            continue;
                        };
                        let Some(decl) = self.ctx.arena.get_variable_declaration(list_decl_node)
                        else {
                            continue;
                        };
                        if decl.initializer.is_none() {
                            continue;
                        }
                        // Only treat call/new expressions as non-falling-through when
                        // they return never. Type assertions like `null as never` still
                        // complete normally at runtime.
                        if self.call_expression_terminates_control_flow(decl.initializer) {
                            return false;
                        }
                    }
                }
                true
            }
            syntax_kind_ext::IF_STATEMENT => {
                let Some(if_data) = self.ctx.arena.get_if_statement(node) else {
                    return true;
                };
                let then_falls = self.statement_falls_through(if_data.then_statement);
                if if_data.else_statement.is_none() {
                    return true;
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
            | syntax_kind_ext::FOR_STATEMENT => self.loop_falls_through(node),
            syntax_kind_ext::LABELED_STATEMENT => self
                .ctx
                .arena
                .get_labeled_statement(node)
                .is_none_or(|labeled| self.statement_falls_through(labeled.statement)),
            _ => true,
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

        let mut has_default = false;
        let mut clause_indices = Vec::new();
        for &clause_idx in &case_block.statements.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            if clause_node.kind == syntax_kind_ext::DEFAULT_CLAUSE {
                has_default = true;
            }
            clause_indices.push(clause_idx);
        }

        // Without a default clause, unmatched discriminants can skip the switch
        // body unless case coverage is exhaustive.
        if !has_default && !self.switch_has_exhaustive_coverage(switch_data) {
            return true;
        }

        // Analyze from bottom to top so empty/grouped clauses inherit the
        // fall-through behavior of the next clause in the chain.
        let mut falls_from_next = true;
        let mut any_entry_falls_through = false;

        for &clause_idx in clause_indices.iter().rev() {
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
                .any(|&stmt| self.contains_break_statement(stmt))
            {
                // A break can complete the switch normally, even if later clauses
                // would not fall through.
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
    pub(crate) fn loop_falls_through(&mut self, node: &tsz_parser::parser::node::Node) -> bool {
        let Some(loop_data) = self.ctx.arena.get_loop(node) else {
            return true;
        };

        let condition_always_true = if loop_data.condition.is_none() {
            true
        } else {
            self.is_true_condition(loop_data.condition)
        };

        if condition_always_true && !self.contains_break_statement(loop_data.statement) {
            return false;
        }

        true
    }

    /// Check if a condition is always true.
    pub(crate) fn is_true_condition(&self, condition_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(condition_idx) else {
            return false;
        };
        node.kind == SyntaxKind::TrueKeyword as u16
    }

    /// Check if a condition is always false.
    pub(crate) fn is_false_condition(&self, condition_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(condition_idx) else {
            return false;
        };
        node.kind == SyntaxKind::FalseKeyword as u16
    }

    /// Check if a statement contains a break statement.
    pub(crate) fn contains_break_statement(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            syntax_kind_ext::BREAK_STATEMENT => true,
            syntax_kind_ext::BLOCK => self.ctx.arena.get_block(node).is_some_and(|block| {
                block
                    .statements
                    .nodes
                    .iter()
                    .any(|&stmt| self.contains_break_statement(stmt))
            }),
            syntax_kind_ext::IF_STATEMENT => {
                self.ctx
                    .arena
                    .get_if_statement(node)
                    .is_some_and(|if_data| {
                        self.contains_break_statement(if_data.then_statement)
                            || (if_data.else_statement.is_some()
                                && self.contains_break_statement(if_data.else_statement))
                    })
            }
            syntax_kind_ext::TRY_STATEMENT => {
                self.ctx.arena.get_try(node).is_some_and(|try_data| {
                    self.contains_break_statement(try_data.try_block)
                        || (try_data.catch_clause.is_some()
                            && self.contains_break_statement(try_data.catch_clause))
                        || (try_data.finally_block.is_some()
                            && self.contains_break_statement(try_data.finally_block))
                })
            }
            syntax_kind_ext::LABELED_STATEMENT => self
                .ctx
                .arena
                .get_labeled_statement(node)
                .is_some_and(|labeled| self.contains_break_statement(labeled.statement)),
            _ => false,
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
        // Check if it's `var` (not let/const) by examining declaration list flags
        // The flags are on the VariableDeclarationList child node
        for &decl_idx in &var_data.declarations.nodes {
            if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                // Check if it's let/const (not var) using combined node+parent flags
                let flags = self.ctx.arena.get_variable_declaration_flags(decl_idx);
                if (flags & (node_flags::LET | node_flags::CONST)) != 0 {
                    return false;
                }
                // Check that declaration has no initializer
                if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
                    && var_decl.initializer.is_some()
                {
                    return false;
                }
            }
        }
        true
    }
}
