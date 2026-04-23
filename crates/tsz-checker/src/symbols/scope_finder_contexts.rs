//! Context and enclosure helpers split out of `scope_finder.rs`.

use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

#[derive(Clone, Copy, Debug)]
struct SuperInitFlowState {
    super_called: bool,
    reachable: bool,
}

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Namespace Context Detection
    // =========================================================================

    /// Check if a `this` expression appears inside an enum member initializer where
    /// TypeScript reports TS2332 ("current location").
    ///
    /// Walks up the AST from the `this` node:
    /// - Arrow functions are transparent because they capture the outer `this`
    /// - Regular functions/methods/constructors create a new `this` binding and stop
    ///   the search
    /// - Reaching an enum member before a function boundary means `this` is invalid
    ///   in the enum initializer
    ///
    /// Returns true if `this` at `idx` is inside an enum member initializer
    /// and there is an arrow function between `this` and the enum member.
    /// Used to suppress the TS2683 companion diagnostic in arrow captures.
    pub(crate) fn has_enclosing_arrow_before_enum(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CONSTRUCTOR, ENUM_MEMBER, FUNCTION_DECLARATION, FUNCTION_EXPRESSION,
            GET_ACCESSOR, METHOD_DECLARATION, SET_ACCESSOR,
        };
        let mut current = idx;
        let mut found_arrow = false;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            match node.kind {
                k if k == ARROW_FUNCTION => {
                    found_arrow = true;
                    continue;
                }
                k if k == FUNCTION_DECLARATION
                    || k == FUNCTION_EXPRESSION
                    || k == METHOD_DECLARATION
                    || k == CONSTRUCTOR
                    || k == GET_ACCESSOR
                    || k == SET_ACCESSOR =>
                {
                    return false;
                }
                k if k == ENUM_MEMBER => return found_arrow,
                _ => continue,
            }
        }
    }

    pub(crate) fn is_this_in_enum_member_initializer(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CONSTRUCTOR, ENUM_MEMBER, FUNCTION_DECLARATION, FUNCTION_EXPRESSION,
            GET_ACCESSOR, METHOD_DECLARATION, SET_ACCESSOR,
        };

        let mut current = idx;
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            match node.kind {
                k if k == ARROW_FUNCTION => continue,
                k if k == FUNCTION_DECLARATION
                    || k == FUNCTION_EXPRESSION
                    || k == METHOD_DECLARATION
                    || k == CONSTRUCTOR
                    || k == GET_ACCESSOR
                    || k == SET_ACCESSOR =>
                {
                    return false;
                }
                k if k == ENUM_MEMBER => return true,
                _ => continue,
            }
        }
    }

    /// Check if a `this` expression is in a module/namespace body context
    /// where it cannot be referenced (TS2331).
    ///
    /// Walks up the AST from the `this` node:
    /// - Arrow functions are transparent (they inherit `this` from outer scope)
    /// - Regular functions/methods/constructors create their own `this` scope,
    ///   so `this` inside them is valid (stops the search)
    /// - For methods/constructors, only the body creates a `this` scope —
    ///   decorator expressions and computed property names execute in the outer scope
    /// - If we reach a `MODULE_DECLARATION` without hitting a function boundary,
    ///   `this` is in the namespace body → return true
    pub(crate) fn is_this_in_namespace_body(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CONSTRUCTOR, DECORATOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION,
            GET_ACCESSOR, METHOD_DECLARATION, MODULE_DECLARATION, SET_ACCESSOR,
        };
        let mut current = idx;
        let mut in_decorator = false;
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            // Track decorator context — decorators execute in the outer scope,
            // not inside the method they decorate
            if node.kind == DECORATOR {
                in_decorator = true;
            }

            match node.kind {
                // Arrow functions don't create their own `this` scope
                k if k == ARROW_FUNCTION => continue,

                // Regular functions always create their own `this` scope
                k if k == FUNCTION_DECLARATION || k == FUNCTION_EXPRESSION => return false,

                // Methods/constructors create `this` scope for their body,
                // but NOT for decorators applied to them
                k if k == METHOD_DECLARATION
                    || k == CONSTRUCTOR
                    || k == GET_ACCESSOR
                    || k == SET_ACCESSOR =>
                {
                    if in_decorator {
                        // `this` is in a decorator on this method — not inside
                        // the method body. Continue searching upward.
                        in_decorator = false;
                        continue;
                    }
                    // `this` is inside the method body → has its own scope
                    return false;
                }

                // Reached a namespace/module declaration → TS2331
                k if k == MODULE_DECLARATION => return true,

                _ => continue,
            }
        }
    }

    // =========================================================================
    // Super/This Ordering Detection
    // =========================================================================

    /// Check if a `this` expression is used before `super()` has been called
    /// in a derived class constructor (TS17009).
    ///
    /// Detects two patterns:
    /// 1. `super(this)` — `this` is an argument to the `super()` call itself
    /// 2. `constructor(x = this.prop)` — `this` in a parameter default of
    ///    a derived class constructor (evaluated before `super()` can run)
    /// 3. `this.prop; super();` — direct constructor-body access before first super call
    pub(crate) fn is_this_before_super_in_derived_constructor(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CALL_EXPRESSION, CONSTRUCTOR, FUNCTION_DECLARATION,
            FUNCTION_EXPRESSION, GET_ACCESSOR, METHOD_DECLARATION, PARAMETER, SET_ACCESSOR,
        };
        let mut current = idx;
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            match node.kind {
                // Pattern 1: this is inside super(...) call arguments
                k if k == CALL_EXPRESSION => {
                    if let Some(call_data) = self.ctx.arena.get_call_expr(node)
                        && let Some(callee) = self.ctx.arena.get(call_data.expression)
                        && callee.kind == SyntaxKind::SuperKeyword as u16
                    {
                        // Verify we're in a derived class constructor
                        return self.is_in_derived_class_constructor(current);
                    }
                }

                // Pattern 2: this is in a constructor parameter default
                k if k == PARAMETER => {
                    // Check if this parameter belongs to a constructor
                    if let Some(param_ext) = self.ctx.arena.get_extended(current) {
                        let param_parent = param_ext.parent;
                        if let Some(parent_node) = self.ctx.arena.get(param_parent)
                            && parent_node.kind == CONSTRUCTOR
                        {
                            return self.is_in_derived_class_constructor(param_parent);
                        }
                    }
                }

                // Stop at any function boundary — this is scoped to the function
                k if k == FUNCTION_DECLARATION
                    || k == FUNCTION_EXPRESSION
                    || k == ARROW_FUNCTION
                    || k == METHOD_DECLARATION
                    || k == GET_ACCESSOR
                    || k == SET_ACCESSOR =>
                {
                    return false;
                }

                // Pattern 3: direct constructor body access before first super() statement
                k if k == CONSTRUCTOR => {
                    return self.is_this_before_super_in_constructor(current, idx);
                }

                _ => continue,
            }
        }
    }

    fn is_this_before_super_in_constructor(
        &self,
        ctor_idx: NodeIndex,
        this_idx: NodeIndex,
    ) -> bool {
        self.is_before_definite_super_call_in_constructor_body(ctor_idx, this_idx)
    }

    pub(crate) fn is_before_definite_super_call_in_constructor_body(
        &self,
        ctor_idx: NodeIndex,
        target_idx: NodeIndex,
    ) -> bool {
        let Some(ctor_node) = self.ctx.arena.get(ctor_idx) else {
            return false;
        };
        let Some(ctor) = self.ctx.arena.get_constructor(ctor_node) else {
            return false;
        };

        // Only classes that actually require super() are subject to TS17009.
        let Some(ext) = self.ctx.arena.get_extended(ctor_idx) else {
            return false;
        };
        let class_idx = ext.parent;
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let Some(class_data) = self.ctx.arena.get_class(class_node) else {
            return false;
        };
        if !self.class_requires_super_call(class_data) {
            return false;
        }

        if ctor.body.is_none() {
            return false;
        }
        let Some(body_node) = self.ctx.arena.get(ctor.body) else {
            return false;
        };
        let Some(body_block) = self.ctx.arena.get_block(body_node) else {
            return false;
        };

        let mut state = SuperInitFlowState {
            super_called: false,
            reachable: true,
        };
        for &stmt_idx in &body_block.statements.nodes {
            if let Some(super_called_on_all_paths) =
                self.super_called_on_all_paths_to_target_in_statement(stmt_idx, target_idx, state)
            {
                return !super_called_on_all_paths;
            }

            state = self.super_flow_after_statement(stmt_idx, state);
            if !state.reachable {
                break;
            }
        }

        false
    }

    fn super_called_on_all_paths_to_target_in_statement(
        &self,
        stmt_idx: NodeIndex,
        target_idx: NodeIndex,
        state: SuperInitFlowState,
    ) -> Option<bool> {
        if !self.node_contains_target(stmt_idx, target_idx) {
            return None;
        }

        let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
            return Some(state.super_called);
        };

        match stmt_node.kind {
            syntax_kind_ext::BLOCK => {
                let block = self.ctx.arena.get_block(stmt_node)?;
                let mut block_state = state;
                for &child_stmt_idx in &block.statements.nodes {
                    if let Some(super_called_on_all_paths) = self
                        .super_called_on_all_paths_to_target_in_statement(
                            child_stmt_idx,
                            target_idx,
                            block_state,
                        )
                    {
                        return Some(super_called_on_all_paths);
                    }

                    // Target wasn't in this statement; advance the control-flow state.
                    block_state = self.super_flow_after_statement(child_stmt_idx, block_state);
                    if !block_state.reachable {
                        return None;
                    }
                }
                None
            }
            syntax_kind_ext::IF_STATEMENT => {
                let if_stmt = self.ctx.arena.get_if_statement(stmt_node)?;

                if self.node_contains_target(if_stmt.expression, target_idx) {
                    return Some(state.super_called);
                }

                let then_has_target = self.node_contains_target(if_stmt.then_statement, target_idx);
                let else_has_target = if_stmt.else_statement.is_some()
                    && self.node_contains_target(if_stmt.else_statement, target_idx);

                match (then_has_target, else_has_target) {
                    (true, false) => self.super_called_on_all_paths_to_target_in_statement(
                        if_stmt.then_statement,
                        target_idx,
                        state,
                    ),
                    (false, true) => self.super_called_on_all_paths_to_target_in_statement(
                        if_stmt.else_statement,
                        target_idx,
                        state,
                    ),
                    (true, true) => {
                        let then_state = self.super_called_on_all_paths_to_target_in_statement(
                            if_stmt.then_statement,
                            target_idx,
                            state,
                        )?;
                        let else_state = self.super_called_on_all_paths_to_target_in_statement(
                            if_stmt.else_statement,
                            target_idx,
                            state,
                        )?;
                        Some(then_state && else_state)
                    }
                    (false, false) => None,
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                let switch_stmt = self.ctx.arena.get_switch(stmt_node)?;
                if self.node_contains_target(switch_stmt.expression, target_idx) {
                    return Some(state.super_called);
                }

                let case_block_node = self.ctx.arena.get(switch_stmt.case_block)?;
                let case_block = self.ctx.arena.get_block(case_block_node)?;
                let clauses = &case_block.statements.nodes;
                if clauses.is_empty() {
                    return None;
                }

                let mut saw_target = false;
                let mut super_called_on_all_paths = true;

                for start_idx in 0..clauses.len() {
                    let Some(path_super_called) = self
                        .super_called_on_path_to_target_from_switch_entry(
                            clauses, start_idx, target_idx, state,
                        )
                    else {
                        continue;
                    };

                    saw_target = true;
                    if !path_super_called {
                        return Some(false);
                    }
                    super_called_on_all_paths &= path_super_called;
                }

                if saw_target {
                    Some(super_called_on_all_paths)
                } else {
                    None
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                let labeled = self.ctx.arena.get_labeled_statement(stmt_node)?;
                self.super_called_on_all_paths_to_target_in_statement(
                    labeled.statement,
                    target_idx,
                    state,
                )
            }
            _ => Some(state.super_called),
        }
    }

    fn super_called_on_path_to_target_from_switch_entry(
        &self,
        clauses: &[NodeIndex],
        start_idx: usize,
        target_idx: NodeIndex,
        entry_state: SuperInitFlowState,
    ) -> Option<bool> {
        let mut clause_state = entry_state;

        for &clause_idx in &clauses[start_idx..] {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let clause = self.ctx.arena.get_case_clause(clause_node)?;

            for &stmt_idx in &clause.statements.nodes {
                if self.is_break_statement(stmt_idx) {
                    return None;
                }

                if let Some(super_called_on_all_paths) = self
                    .super_called_on_all_paths_to_target_in_statement(
                        stmt_idx,
                        target_idx,
                        clause_state,
                    )
                {
                    return Some(super_called_on_all_paths);
                }

                clause_state = self.super_flow_after_statement(stmt_idx, clause_state);
                if !clause_state.reachable {
                    return None;
                }
            }
        }

        None
    }

    fn super_flow_after_statement(
        &self,
        stmt_idx: NodeIndex,
        state: SuperInitFlowState,
    ) -> SuperInitFlowState {
        if !state.reachable {
            return state;
        }

        let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
            return state;
        };

        match stmt_node.kind {
            syntax_kind_ext::BLOCK => {
                let Some(block) = self.ctx.arena.get_block(stmt_node) else {
                    return state;
                };
                let mut block_state = state;
                for &child_stmt_idx in &block.statements.nodes {
                    if self.is_break_statement(child_stmt_idx)
                        || self.is_continue_statement(child_stmt_idx)
                    {
                        return SuperInitFlowState {
                            super_called: block_state.super_called,
                            reachable: false,
                        };
                    }

                    block_state = self.super_flow_after_statement(child_stmt_idx, block_state);
                    if !block_state.reachable {
                        break;
                    }
                }
                block_state
            }
            syntax_kind_ext::IF_STATEMENT => {
                let Some(if_stmt) = self.ctx.arena.get_if_statement(stmt_node) else {
                    return state;
                };
                let then_state = self.super_flow_after_statement(if_stmt.then_statement, state);
                let else_state = if if_stmt.else_statement.is_some() {
                    self.super_flow_after_statement(if_stmt.else_statement, state)
                } else {
                    state
                };
                self.merge_super_flow_states(then_state, else_state)
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                self.super_flow_after_switch_statement(stmt_idx, state)
            }
            syntax_kind_ext::TRY_STATEMENT => {
                let Some(try_stmt) = self.ctx.arena.get_try(stmt_node) else {
                    return state;
                };

                let try_state = self.super_flow_after_statement(try_stmt.try_block, state);
                let catch_state = if try_stmt.catch_clause.is_some() {
                    self.super_flow_after_statement(try_stmt.catch_clause, state)
                } else {
                    SuperInitFlowState {
                        super_called: true,
                        reachable: false,
                    }
                };

                let merged = self.merge_super_flow_states(try_state, catch_state);
                if try_stmt.finally_block.is_some() {
                    self.super_flow_after_statement(try_stmt.finally_block, merged)
                } else {
                    merged
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                let Some(catch_clause) = self.ctx.arena.get_catch_clause(stmt_node) else {
                    return state;
                };
                self.super_flow_after_statement(catch_clause.block, state)
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                let Some(labeled) = self.ctx.arena.get_labeled_statement(stmt_node) else {
                    return state;
                };
                self.super_flow_after_statement(labeled.statement, state)
            }
            syntax_kind_ext::RETURN_STATEMENT | syntax_kind_ext::THROW_STATEMENT => {
                SuperInitFlowState {
                    super_called: state.super_called,
                    reachable: false,
                }
            }
            _ => {
                if self.statement_guarantees_super_call(stmt_idx) {
                    SuperInitFlowState {
                        super_called: true,
                        reachable: true,
                    }
                } else {
                    state
                }
            }
        }
    }

    fn super_flow_after_switch_statement(
        &self,
        switch_stmt_idx: NodeIndex,
        entry_state: SuperInitFlowState,
    ) -> SuperInitFlowState {
        let Some(switch_node) = self.ctx.arena.get(switch_stmt_idx) else {
            return entry_state;
        };
        let Some(switch_stmt) = self.ctx.arena.get_switch(switch_node) else {
            return entry_state;
        };
        let Some(case_block_node) = self.ctx.arena.get(switch_stmt.case_block) else {
            return entry_state;
        };
        let Some(case_block) = self.ctx.arena.get_block(case_block_node) else {
            return entry_state;
        };
        let clauses = &case_block.statements.nodes;
        if clauses.is_empty() {
            return entry_state;
        }

        let mut merged_exit_state = SuperInitFlowState {
            super_called: true,
            reachable: false,
        };
        for start_idx in 0..clauses.len() {
            let mut clause_state = entry_state;
            let mut exited_switch = false;

            'clause_walk: for &clause_idx in &clauses[start_idx..] {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(clause) = self.ctx.arena.get_case_clause(clause_node) else {
                    continue;
                };

                for &stmt_idx in &clause.statements.nodes {
                    if self.is_break_statement(stmt_idx) {
                        exited_switch = true;
                        break 'clause_walk;
                    }
                    clause_state = self.super_flow_after_statement(stmt_idx, clause_state);
                    if !clause_state.reachable {
                        exited_switch = true;
                        break 'clause_walk;
                    }
                }
            }

            // Falling out of the last clause is also a switch exit.
            if !exited_switch {
                exited_switch = true;
            }

            if exited_switch {
                merged_exit_state = self.merge_super_flow_states(merged_exit_state, clause_state);
            }
        }

        merged_exit_state
    }

    fn statement_guarantees_super_call(&self, stmt_idx: NodeIndex) -> bool {
        if self.is_super_call_statement(stmt_idx) {
            return true;
        }

        let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        if stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
            return self
                .ctx
                .arena
                .get_expression_statement(stmt_node)
                .is_some_and(|expr_stmt| {
                    self.expression_guarantees_super_call(expr_stmt.expression)
                });
        }

        self.ctx.arena.get_variable(stmt_node).is_some_and(|vars| {
            vars.declarations
                .nodes
                .iter()
                .copied()
                .any(|decl_idx| self.variable_declaration_guarantees_super_call(decl_idx))
        })
    }

    fn variable_declaration_guarantees_super_call(&self, decl_idx: NodeIndex) -> bool {
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        if let Some(var_group) = self.ctx.arena.get_variable(decl_node) {
            return var_group
                .declarations
                .nodes
                .iter()
                .copied()
                .any(|nested_decl_idx| {
                    self.variable_declaration_guarantees_super_call(nested_decl_idx)
                });
        }

        if let Some(binding_elem) = self.ctx.arena.get_binding_element(decl_node) {
            return self.expression_guarantees_super_call(binding_elem.initializer);
        }

        let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };

        if self.expression_guarantees_super_call(decl.initializer) {
            return true;
        }

        let Some(name_node) = self.ctx.arena.get(decl.name) else {
            return false;
        };
        let Some(pattern) = self.ctx.arena.get_binding_pattern(name_node) else {
            return false;
        };

        pattern.elements.nodes.iter().copied().any(|elem_idx| {
            self.ctx
                .arena
                .get(elem_idx)
                .and_then(|elem_node| self.ctx.arena.get_binding_element(elem_node))
                .is_some_and(|elem| self.expression_guarantees_super_call(elem.initializer))
        })
    }

    fn expression_guarantees_super_call(&self, expr_idx: NodeIndex) -> bool {
        if expr_idx.is_none() {
            return false;
        }

        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        match expr_node.kind {
            syntax_kind_ext::CALL_EXPRESSION | syntax_kind_ext::NEW_EXPRESSION => {
                let Some(call) = self.ctx.arena.get_call_expr(expr_node) else {
                    return false;
                };

                if self
                    .ctx
                    .arena
                    .get(call.expression)
                    .is_some_and(|callee| callee.kind == SyntaxKind::SuperKeyword as u16)
                {
                    return true;
                }

                if self.expression_guarantees_super_call(call.expression) {
                    return true;
                }

                call.arguments.as_ref().is_some_and(|args| {
                    args.nodes
                        .iter()
                        .copied()
                        .any(|arg_idx| self.expression_guarantees_super_call(arg_idx))
                })
            }
            syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                let Some(cond) = self.ctx.arena.get_conditional_expr(expr_node) else {
                    return false;
                };
                self.expression_guarantees_super_call(cond.condition)
                    || (self.expression_guarantees_super_call(cond.when_true)
                        && self.expression_guarantees_super_call(cond.when_false))
            }
            syntax_kind_ext::BINARY_EXPRESSION => {
                let Some(binary) = self.ctx.arena.get_binary_expr(expr_node) else {
                    return false;
                };
                let left = self.expression_guarantees_super_call(binary.left);
                let right = self.expression_guarantees_super_call(binary.right);

                match binary.operator_token {
                    k if k == SyntaxKind::AmpersandAmpersandToken as u16
                        || k == SyntaxKind::BarBarToken as u16
                        || k == SyntaxKind::QuestionQuestionToken as u16 =>
                    {
                        left
                    }
                    _ => left || right,
                }
            }
            syntax_kind_ext::PARENTHESIZED_EXPRESSION => self
                .ctx
                .arena
                .get_parenthesized(expr_node)
                .is_some_and(|paren| self.expression_guarantees_super_call(paren.expression)),
            syntax_kind_ext::AS_EXPRESSION
            | syntax_kind_ext::SATISFIES_EXPRESSION
            | syntax_kind_ext::TYPE_ASSERTION => self
                .ctx
                .arena
                .get_type_assertion(expr_node)
                .is_some_and(|assertion| {
                    self.expression_guarantees_super_call(assertion.expression)
                }),
            syntax_kind_ext::NON_NULL_EXPRESSION => self
                .ctx
                .arena
                .get_unary_expr_ex(expr_node)
                .is_some_and(|unary| self.expression_guarantees_super_call(unary.expression)),
            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.ctx
                    .arena
                    .get_literal_expr(expr_node)
                    .is_some_and(|literal| {
                        literal.elements.nodes.iter().copied().any(|elem_idx| {
                            self.object_literal_element_guarantees_super_call(elem_idx)
                        })
                    })
            }
            syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => self
                .ctx
                .arena
                .get_literal_expr(expr_node)
                .is_some_and(|literal| {
                    literal
                        .elements
                        .nodes
                        .iter()
                        .copied()
                        .any(|elem_idx| self.expression_guarantees_super_call(elem_idx))
                }),
            syntax_kind_ext::SPREAD_ELEMENT | syntax_kind_ext::SPREAD_ASSIGNMENT => self
                .ctx
                .arena
                .get_spread(expr_node)
                .is_some_and(|spread| self.expression_guarantees_super_call(spread.expression)),
            syntax_kind_ext::TEMPLATE_EXPRESSION => {
                let Some(template) = self.ctx.arena.get_template_expr(expr_node) else {
                    return false;
                };
                template
                    .template_spans
                    .nodes
                    .iter()
                    .copied()
                    .any(|span_idx| {
                        self.ctx
                            .arena
                            .get(span_idx)
                            .and_then(|span_node| self.ctx.arena.get_template_span(span_node))
                            .is_some_and(|span| {
                                self.expression_guarantees_super_call(span.expression)
                            })
                    })
            }
            _ => false,
        }
    }

    fn object_literal_element_guarantees_super_call(&self, elem_idx: NodeIndex) -> bool {
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return false;
        };

        match elem_node.kind {
            syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                .ctx
                .arena
                .get_property_assignment(elem_node)
                .is_some_and(|prop| self.expression_guarantees_super_call(prop.initializer)),
            syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                .ctx
                .arena
                .get_shorthand_property(elem_node)
                .is_some_and(|shorthand| {
                    self.expression_guarantees_super_call(shorthand.object_assignment_initializer)
                }),
            syntax_kind_ext::SPREAD_ASSIGNMENT | syntax_kind_ext::SPREAD_ELEMENT => self
                .ctx
                .arena
                .get_spread(elem_node)
                .is_some_and(|spread| self.expression_guarantees_super_call(spread.expression)),
            syntax_kind_ext::METHOD_DECLARATION
            | syntax_kind_ext::GET_ACCESSOR
            | syntax_kind_ext::SET_ACCESSOR => false,
            _ => self.expression_guarantees_super_call(elem_idx),
        }
    }

    const fn merge_super_flow_states(
        &self,
        left: SuperInitFlowState,
        right: SuperInitFlowState,
    ) -> SuperInitFlowState {
        if !left.reachable && !right.reachable {
            return SuperInitFlowState {
                super_called: true,
                reachable: false,
            };
        }

        SuperInitFlowState {
            super_called: (!left.reachable || left.super_called)
                && (!right.reachable || right.super_called),
            reachable: left.reachable || right.reachable,
        }
    }

    fn node_contains_target(&self, candidate_ancestor: NodeIndex, target_idx: NodeIndex) -> bool {
        candidate_ancestor == target_idx
            || self.is_descendant_of_node(target_idx, candidate_ancestor)
    }

    fn is_break_statement(&self, stmt_idx: NodeIndex) -> bool {
        self.ctx
            .arena
            .get(stmt_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::BREAK_STATEMENT)
    }

    fn is_continue_statement(&self, stmt_idx: NodeIndex) -> bool {
        self.ctx
            .arena
            .get(stmt_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::CONTINUE_STATEMENT)
    }

    /// Check if a node is inside a constructor of a derived class.
    fn is_in_derived_class_constructor(&self, from_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION,
            METHOD_DECLARATION,
        };
        let mut current = from_idx;
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            if node.kind == CONSTRUCTOR {
                // Walk up to find the class
                let Some(ext) = self.ctx.arena.get_extended(current) else {
                    return false;
                };
                let class_idx = ext.parent;
                return self.class_node_requires_super_call(class_idx);
            }

            // Stop at other function boundaries
            if node.kind == FUNCTION_DECLARATION
                || node.kind == FUNCTION_EXPRESSION
                || node.kind == ARROW_FUNCTION
                || node.kind == METHOD_DECLARATION
            {
                return false;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
        }
    }

    /// Check if a class node (or its parent class) has an extends clause.
    fn class_node_requires_super_call(&self, class_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let Some(class_data) = self.ctx.arena.get_class(node) else {
            return false;
        };
        self.class_requires_super_call(class_data)
    }

    // =========================================================================
    // Static Block Enclosure
    // =========================================================================

    /// Find the enclosing static block for a given node.
    ///
    /// Traverses up the AST to find a `CLASS_STATIC_BLOCK_DECLARATION`.
    /// Stops at function boundaries to avoid considering outer static blocks.
    ///
    /// Returns Some(NodeIndex) if inside a static block, None otherwise.
    pub(crate) fn find_enclosing_static_block(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                    return Some(current);
                }
                // Stop at function boundaries (don't consider outer static blocks)
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                    || node.kind == syntax_kind_ext::GET_ACCESSOR
                    || node.kind == syntax_kind_ext::SET_ACCESSOR
                {
                    return None;
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    // =========================================================================
    // Class Field / Static Block Arguments Check (TS2815)
    // =========================================================================

    /// Check if `arguments` at `idx` is inside a class property initializer
    /// or static block, without a regular function boundary in between.
    ///
    /// Arrow functions are transparent (they don't create their own `arguments`),
    /// so `() => arguments` in a field initializer is still TS2815.
    /// Regular functions (function expressions, methods, constructors, accessors)
    /// create their own `arguments` binding, so the check stops there.
    pub(crate) fn is_arguments_in_class_initializer_or_static_block(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                match node.kind {
                    // Regular function boundaries create their own `arguments` — stop
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION
                        || k == syntax_kind_ext::FUNCTION_EXPRESSION
                        || k == syntax_kind_ext::METHOD_DECLARATION
                        || k == syntax_kind_ext::CONSTRUCTOR
                        || k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        return false;
                    }
                    // Arrow functions are transparent — continue walking
                    k if k == syntax_kind_ext::ARROW_FUNCTION => {}
                    // Class field initializer — TS2815
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        return true;
                    }
                    // Static block — TS2815
                    k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                        return true;
                    }
                    // Source file — stop
                    k if k == syntax_kind_ext::SOURCE_FILE => {
                        return false;
                    }
                    _ => {}
                }
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
        }
        false
    }

    // =========================================================================
    // Computed Property Enclosure
    // =========================================================================

    /// Find the enclosing computed property name for a given node.
    ///
    /// Traverses up the AST to find a `COMPUTED_PROPERTY_NAME`.
    /// Stops at function boundaries (computed properties inside functions are evaluated at call time).
    ///
    /// Returns Some(NodeIndex) if inside a computed property name, None otherwise.
    pub(crate) fn find_enclosing_computed_property(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        while current.is_some() {
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    return Some(current);
                }
                // Stop at function boundaries
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                {
                    return None;
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Check if `this` is inside a class member's computed property name (TS2465).
    ///
    /// Walks up the parent chain without crossing function boundaries (including
    /// arrow functions). When a `ComputedPropertyName` is found:
    /// - If its owner's parent is a class (`ClassDeclaration`/`ClassExpression`) → return true
    /// - Otherwise (object literal computed property) → keep walking
    ///
    /// This correctly handles nested cases like `class C { [{ [this.x]: 1 }[0]]() {} }`
    /// where `this` is in an object-literal computed property that is itself inside a
    /// class member's computed property.
    pub(crate) fn is_this_in_class_member_computed_property_name(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CLASS_DECLARATION, CLASS_EXPRESSION, COMPUTED_PROPERTY_NAME,
            CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION, METHOD_DECLARATION,
        };
        let mut current = idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            // Stop at all function boundaries (arrow functions ARE boundaries for `this`)
            if parent_node.kind == FUNCTION_DECLARATION
                || parent_node.kind == FUNCTION_EXPRESSION
                || parent_node.kind == ARROW_FUNCTION
                || parent_node.kind == METHOD_DECLARATION
                || parent_node.kind == CONSTRUCTOR
                || parent_node.is_accessor()
            {
                return false;
            }
            if parent_node.kind == COMPUTED_PROPERTY_NAME {
                // Check if this computed property's owner's parent is a class
                if let Some(cpn_ext) = self.ctx.arena.get_extended(parent_idx) {
                    let owner_idx = cpn_ext.parent; // MethodDeclaration, PropertyDeclaration, etc.
                    if let Some(owner_ext) = self.ctx.arena.get_extended(owner_idx)
                        && let Some(class_node) = self.ctx.arena.get(owner_ext.parent)
                        && (class_node.kind == CLASS_DECLARATION
                            || class_node.kind == CLASS_EXPRESSION)
                    {
                        return true;
                    }
                }
                // Not a class member computed property; keep walking to find an outer one
            }
            current = parent_idx;
        }
    }

    /// Check if `super` is inside a computed property name in an illegal context (TS2466).
    ///
    /// Mirrors TSC's `getSuperContainer(node, stopOnFunctions=true)` skip semantics:
    ///
    /// When `getSuperContainer` encounters a `ComputedPropertyName`, it performs a
    /// double-advance (skips to the CPN's parent, then advances again), meaning the
    /// direct owner of the computed property name does NOT become the super container.
    /// We simulate this by skipping to the CPN's parent when we encounter one and
    /// continuing the walk from there.
    ///
    /// Legal super containers (reached without skipping through a CPN): methods,
    /// constructors, accessors, static blocks. When found, `super` has a valid context
    /// and we return `false` (not a 2466 error).
    ///
    /// Arrow function handling depends on whether `super` is a call:
    /// - `super()` call: arrow functions ARE boundaries (become the container).
    ///   If the arrow function is the container and we found a CPN → return true.
    /// - `super.x` access: arrow functions are transparent (walked through).
    ///
    /// Correctly handles:
    /// - `class C { [super.bar()]() {} }` → true (class member CPN, no legal container)
    /// - `class C { foo() { var obj = { [super.bar()]() {} }; } }` → false
    ///   (obj-lit CPN inside method `foo()` which IS a legal container)
    /// - `class B { bar() { return class { [super.foo()]() {} } } }` → false
    ///   (nested-class CPN; super's actual container is outer `bar()`)
    /// - `class C { [{ [super.bar()]: 1 }[0]]() {} }` → true
    ///   (inner obj-lit CPN nested inside outer class-member CPN; no legal container)
    /// - `ctor() { super(); () => { var obj = { [(super(), "prop")]() {} } } }` → true
    ///   (`super()` call; arrow fn is boundary; CPN found before boundary)
    pub(crate) fn is_super_in_computed_property_name(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CALL_EXPRESSION, CLASS_STATIC_BLOCK_DECLARATION,
            COMPUTED_PROPERTY_NAME, CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION,
            METHOD_DECLARATION, PROPERTY_DECLARATION,
        };

        // Determine whether this `super` is used as a call (`super()`).
        // For super() calls, TSC does not walk through arrow functions when searching
        // for the super container. For super property accesses, arrow functions are
        // transparent (walked through to find the outer container).
        let is_super_call = self
            .ctx
            .arena
            .get_extended(idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent).map(|n| (ext.parent, n.kind)))
            .is_some_and(|(parent_idx, parent_kind)| {
                if parent_kind != CALL_EXPRESSION {
                    return false;
                }
                // `super` must be the callee of the call expression
                self.ctx
                    .arena
                    .get_call_expr(
                        self.ctx
                            .arena
                            .get(parent_idx)
                            .expect("parent_idx obtained from valid extended node"),
                    )
                    .is_some_and(|call| call.expression == idx)
            });

        let mut current = idx;
        let mut found_computed_property = false;

        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                // Walked off the top of the tree.
                return found_computed_property;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return found_computed_property;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return found_computed_property;
            };

            if parent_node.kind == COMPUTED_PROPERTY_NAME {
                // TSC's getSuperContainer skips ComputedPropertyName by advancing to
                // CPN.parent (the member owner), then the loop advances once more to
                // the member owner's parent. We simulate this: mark that we've found
                // a CPN, then jump to CPN.parent so the next iteration processes
                // CPN.parent.parent.
                found_computed_property = true;
                let Some(cpn_ext) = self.ctx.arena.get_extended(parent_idx) else {
                    return found_computed_property;
                };
                let cpn_owner = cpn_ext.parent;
                if cpn_owner.is_none() {
                    return found_computed_property;
                }
                current = cpn_owner;
                continue;
            }

            // Arrow functions:
            // - For super() calls (isCallExpression=true in TSC): ArrowFunction stops
            //   the getSuperContainer walk and becomes the immediate container. Since
            //   ArrowFunction is never a legal super container (isLegalUsageOfSuperExpression
            //   returns false for it), if we've seen a CPN by now we return true.
            // - For super property accesses: arrow functions are transparent; TSC's
            //   post-container while loop continues through them.
            if parent_node.kind == ARROW_FUNCTION {
                if is_super_call {
                    // Arrow function is the container for this super() call.
                    // isLegalUsageOfSuperExpression(ArrowFunction) = false, so if we
                    // found a CPN between super and this arrow fn, emit TS2466.
                    return found_computed_property;
                }
                // Not a call: transparent, keep walking.
                current = parent_idx;
                continue;
            }

            // Regular function boundaries (stopOnFunctions=true): these become the
            // container. They are not legal super-property-access containers (their
            // parent is not class-like), but this is a different error — not TS2466.
            if parent_node.kind == FUNCTION_DECLARATION || parent_node.kind == FUNCTION_EXPRESSION {
                return false;
            }

            // Legal super container kinds. When reached directly (not via a CPN skip),
            // super is inside a valid class member body and TS2466 does not apply.
            if parent_node.kind == METHOD_DECLARATION
                || parent_node.kind == CONSTRUCTOR
                || parent_node.is_accessor()
                || parent_node.kind == CLASS_STATIC_BLOCK_DECLARATION
                || parent_node.kind == PROPERTY_DECLARATION
            {
                return false;
            }

            current = parent_idx;
        }
    }

    // =========================================================================
    // Heritage Clause Enclosure
    // =========================================================================

    /// Find the enclosing heritage clause (extends/implements) for a node.
    ///
    /// Returns the `NodeIndex` of the `HERITAGE_CLAUSE` if the node is inside one.
    /// Stops at function/class/interface boundaries.
    ///
    /// Returns Some(NodeIndex) if inside a heritage clause, None otherwise.
    pub(crate) fn find_enclosing_heritage_clause(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext::HERITAGE_CLAUSE;

        let mut current = idx;
        while current.is_some() {
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == HERITAGE_CLAUSE {
                    return Some(current);
                }
                // Stop at function/class/interface boundaries
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                    || node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || node.kind == syntax_kind_ext::CLASS_EXPRESSION
                    || node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                {
                    return None;
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Check if an identifier is the direct expression of an `ExpressionWithTypeArguments`
    /// in a heritage clause (e.g., `extends A` or `implements B`), as opposed to
    /// being nested deeper (e.g., as a function argument in `extends factory(A)`).
    ///
    /// Returns true ONLY when the identifier is the direct type reference.
    pub(crate) fn is_direct_heritage_type_reference(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::HERITAGE_CLAUSE;

        // Walk up from the identifier to the heritage clause.
        // If we encounter a CALL_EXPRESSION on the way, the identifier is
        // nested inside a call (e.g., `factory(A)`) — NOT a direct reference.
        let mut current = idx;
        for _ in 0..20 {
            let ext = match self.ctx.arena.get_extended(current) {
                Some(ext) if ext.parent.is_some() => ext,
                _ => return false,
            };
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent_node.kind == HERITAGE_CLAUSE {
                // Suppress TS2693 in ALL heritage clause contexts.
                // The heritage checker emits more specific errors:
                //   - TS2689 for class extending an interface
                //   - TS2507 for non-constructable base expression
                // tsc never emits TS2693 for heritage clause expressions.
                return true;
            }

            // If we pass through a call expression, the identifier might be:
            // (a) the callee (e.g., `color` in `extends color()`) — continue
            //     walking up because `color()` itself might be inside a heritage clause,
            //     and tsc doesn't emit TS2693 for the callee in that context.
            // (b) an argument (e.g., `A` in `extends factory(A)`) — stop, TS2693 applies.
            if parent_node.kind == syntax_kind_ext::CALL_EXPRESSION
                || parent_node.kind == syntax_kind_ext::NEW_EXPRESSION
            {
                if let Some(call) = self.ctx.arena.get_call_expr(parent_node)
                    && call.expression == current
                {
                    // The identifier is the callee — continue walking up
                    current = parent_idx;
                    continue;
                }
                return false;
            }

            // Stop at function/class/interface boundaries
            if parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                || parent_node.kind == syntax_kind_ext::SOURCE_FILE
            {
                return false;
            }

            current = parent_idx;
        }
        false
    }

    /// Like [`is_direct_heritage_type_reference`] but returns `true` ONLY when
    /// the heritage clause is in a **type-only context** — `interface extends`,
    /// `class implements`, or `declare class extends`.
    ///
    /// For non-ambient `class extends`, this returns `false` because the extends
    /// clause is a **value context** — it needs a constructable runtime value.
    /// When a type-only import is used in `class extends`, TS1361 must fire.
    ///
    /// This is used specifically for the `alias_resolves_to_type_only` path
    /// (TS1361/TS1362 emission).  The broader `is_direct_heritage_type_reference`
    /// is still used for TS2693/TS2708 suppression where ALL heritage clauses
    /// should suppress the generic error.
    pub(crate) fn is_heritage_type_only_context(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::HERITAGE_CLAUSE;

        let mut current = idx;
        for _ in 0..20 {
            let ext = match self.ctx.arena.get_extended(current) {
                Some(ext) if ext.parent.is_some() => ext,
                _ => return false,
            };
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent_node.kind == HERITAGE_CLAUSE {
                // Found the heritage clause. Check its parent to determine context.
                let hc_ext = match self.ctx.arena.get_extended(parent_idx) {
                    Some(ext) if ext.parent.is_some() => ext,
                    _ => return true, // fallback: suppress
                };
                let hc_parent_idx = hc_ext.parent;
                let Some(hc_parent) = self.ctx.arena.get(hc_parent_idx) else {
                    return true; // fallback: suppress
                };

                // Interface: always type-only context
                if hc_parent.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                    return true;
                }

                // Class: check extends vs implements, and declare modifier
                if hc_parent.kind == syntax_kind_ext::CLASS_DECLARATION
                    || hc_parent.kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    // `implements` is always a type-only context
                    if let Some(heritage) = self.ctx.arena.get_heritage_clause(parent_node)
                        && heritage.token == SyntaxKind::ImplementsKeyword as u16
                    {
                        return true;
                    }

                    // Ambient class extends (declare class, or class inside
                    // declare namespace/module) → suppress TS1361
                    if self.ctx.arena.is_in_ambient_context(hc_parent_idx) {
                        return true;
                    }

                    // Regular class extends: value context → DON'T suppress
                    return false;
                }

                return true; // other parent: suppress (fallback)
            }

            // Nested inside a call/new: not a direct reference
            if parent_node.kind == syntax_kind_ext::CALL_EXPRESSION
                || parent_node.kind == syntax_kind_ext::NEW_EXPRESSION
            {
                return false;
            }

            // Stop at function/class/interface boundaries
            if parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                || parent_node.kind == syntax_kind_ext::SOURCE_FILE
            {
                return false;
            }

            current = parent_idx;
        }
        false
    }

    /// Returns `true` when an identifier is inside a type annotation context
    /// (e.g., as a child of `TypeReference`, `TupleType`, `FunctionType`, etc.).
    ///
    /// In multi-file mode the checker may dispatch type-position identifiers
    /// through `get_type_of_identifier`.  This guard prevents false TS2693 for
    /// type parameters and interfaces used inside type annotations.
    pub(crate) fn is_identifier_in_type_position(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..20 {
            let ext = match self.ctx.arena.get_extended(current) {
                Some(ext) if ext.parent.is_some() => ext,
                _ => return false,
            };
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            // Computed property names contain expression positions, not type positions.
            // `[K]: number` should emit TS2693 if K is a type, not suppress it.
            if parent_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                return false;
            }
            match parent_node.kind {
                // Type nodes: identifier is in a type position
                syntax_kind_ext::TYPE_REFERENCE
                | syntax_kind_ext::TUPLE_TYPE
                | syntax_kind_ext::ARRAY_TYPE
                | syntax_kind_ext::UNION_TYPE
                | syntax_kind_ext::INTERSECTION_TYPE
                | syntax_kind_ext::FUNCTION_TYPE
                | syntax_kind_ext::CONSTRUCTOR_TYPE
                | syntax_kind_ext::TYPE_LITERAL
                | syntax_kind_ext::MAPPED_TYPE
                | syntax_kind_ext::INDEXED_ACCESS_TYPE
                | syntax_kind_ext::CONDITIONAL_TYPE
                | syntax_kind_ext::PARENTHESIZED_TYPE
                | syntax_kind_ext::TYPE_PREDICATE
                | syntax_kind_ext::TYPE_QUERY
                | syntax_kind_ext::TYPE_PARAMETER
                | syntax_kind_ext::PROPERTY_SIGNATURE
                | syntax_kind_ext::METHOD_SIGNATURE
                | syntax_kind_ext::INDEX_SIGNATURE
                | syntax_kind_ext::CALL_SIGNATURE
                | syntax_kind_ext::CONSTRUCT_SIGNATURE => return true,
                // Expression/statement boundaries: stop walking
                syntax_kind_ext::CALL_EXPRESSION
                | syntax_kind_ext::NEW_EXPRESSION
                | syntax_kind_ext::BINARY_EXPRESSION
                | syntax_kind_ext::VARIABLE_DECLARATION
                | syntax_kind_ext::RETURN_STATEMENT
                | syntax_kind_ext::EXPRESSION_STATEMENT
                | syntax_kind_ext::SOURCE_FILE => return false,
                _ => {
                    current = parent_idx;
                }
            }
        }
        false
    }

    /// Returns `true` when the identifier is inside a `typeof` type query
    /// (e.g., `type T = typeof X`).  In type positions, `typeof` is a type
    /// query, not a runtime value usage, so TS1361/TS1362 should be suppressed.
    pub(crate) fn is_in_type_query_context(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..10 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_QUERY {
                return true;
            }
            // Stop walking if we leave a type context into a statement or expression
            if parent_node.kind == tsz_parser::parser::syntax_kind_ext::EXPRESSION_STATEMENT
                || parent_node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION
                || parent_node.kind == tsz_parser::parser::syntax_kind_ext::RETURN_STATEMENT
                || parent_node.kind == tsz_parser::parser::syntax_kind_ext::SOURCE_FILE
            {
                return false;
            }
            current = parent_idx;
        }
        false
    }

    /// Returns `true` when the identifier is part of an import-equals
    /// declaration's entity name (e.g., `M` in `import r = M.X;`).
    /// In this context, namespace references are not value usages and
    /// should not trigger TS2708.
    pub(crate) fn is_in_import_equals_entity_name(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..10 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                return true;
            }
            // Keep walking through qualified names
            if parent_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                current = parent_idx;
                continue;
            }
            // Any other node kind means we're not in an import-equals entity name
            return false;
        }
        false
    }

    /// Returns `true` when the identifier is being evaluated inside a computed
    /// property name (`[expr]`) that belongs to a type-only or ambient context
    /// (interface member, type literal member, abstract member, `declare`
    /// member, or ambient class).  In these positions the expression is never
    /// emitted as runtime code, so TS1361/TS1362 should be suppressed.
    pub(crate) fn is_in_ambient_computed_property_context(&self) -> bool {
        let Some(cpn_idx) = self.ctx.checking_computed_property_name else {
            return false;
        };

        // Walk from the computed property name node upward to the member
        // declaration, then to its parent (class/interface/type literal).
        let mut current = cpn_idx;
        for _ in 0..8 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            match parent_node.kind {
                // Interface and type literal members are always type-only
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => return true,
                k if k == syntax_kind_ext::TYPE_LITERAL => return true,

                // Ambient class: `declare class C { [x]: any; }`
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    if let Some(class) = self.ctx.arena.get_class(parent_node)
                        && self.has_declare_modifier(&class.modifiers)
                    {
                        return true;
                    }
                    return false;
                }

                // Property/method declarations may have abstract or declare modifiers
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.ctx.arena.get_property_decl(parent_node)
                        && (self.has_abstract_modifier(&prop.modifiers)
                            || self.has_declare_modifier(&prop.modifiers))
                    {
                        return true;
                    }
                    current = parent_idx;
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(parent_node)
                        && self.has_abstract_modifier(&method.modifiers)
                    {
                        return true;
                    }
                    current = parent_idx;
                }

                _ => {
                    current = parent_idx;
                }
            }
        }
        false
    }

    /// Check if a function's body contains `this.property = value` assignments,
    /// which in JS files indicates a constructor function pattern. When a function
    /// has such assignments, tsc types `this` as the constructed instance and
    /// does not emit TS2683.
    pub(crate) fn function_body_has_this_property_assignments(&self, func_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            BINARY_EXPRESSION, EXPRESSION_STATEMENT, PROPERTY_ACCESS_EXPRESSION,
        };

        let Some(fn_node) = self.ctx.arena.get(func_idx) else {
            return false;
        };
        let Some(func) = self.ctx.arena.get_function(fn_node) else {
            return false;
        };
        let body_idx = func.body;
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };
        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return false;
        };

        for &stmt_idx in &block.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(expr_node) = self.ctx.arena.get(expr_stmt.expression) else {
                continue;
            };
            if expr_node.kind != BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(expr_node) else {
                continue;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }
            let Some(lhs_node) = self.ctx.arena.get(binary.left) else {
                continue;
            };
            if lhs_node.kind != PROPERTY_ACCESS_EXPRESSION {
                continue;
            }
            let Some(access) = self.ctx.arena.get_access_expr(lhs_node) else {
                continue;
            };
            let Some(base_node) = self.ctx.arena.get(access.expression) else {
                continue;
            };
            if base_node.kind == SyntaxKind::ThisKeyword as u16 {
                return true;
            }
        }

        false
    }
}
