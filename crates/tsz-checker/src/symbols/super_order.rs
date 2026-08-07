//! Super/this ordering detection for TS17009/TS17011.
//!
//! Split out of `scope_finder_contexts.rs` to keep that file under the size
//! cap. Implements the syntactic analogue of `tsc`'s `isPostSuperFlowNode`:
//! a `this` (or `super.x`) use in a derived-class constructor is an error
//! only when no `super()` call definitely executes before it, tracked in
//! evaluation order both across statements and within a single statement.

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
    // Super/This Ordering Detection
    // =========================================================================

    /// Check if a `this` expression is used before `super()` has been called
    /// in a derived class constructor (TS17009).
    ///
    /// Detects two patterns:
    /// 1. `constructor(x = this.prop)` — `this` in a parameter default of
    ///    a derived class constructor (evaluated before `super()` can run)
    /// 2. `this.prop; super();` — constructor-body access with no `super()`
    ///    call definitely executed before it in evaluation order (this also
    ///    covers `super(this)`: arguments evaluate before the call fires)
    pub(crate) fn is_this_before_super_in_derived_constructor(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION, GET_ACCESSOR,
            METHOD_DECLARATION, PARAMETER, SET_ACCESSOR,
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
                // Pattern 1: this is in a constructor parameter default
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

                // Pattern 2: constructor body access before a definite super() call
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
                    return Some(self.super_state_at_target_in_expression(
                        if_stmt.expression,
                        target_idx,
                        state.super_called,
                    ));
                }

                // Either branch only runs after the condition fully evaluated.
                let state = SuperInitFlowState {
                    super_called: state.super_called
                        || self.expression_guarantees_super_call(if_stmt.expression),
                    reachable: state.reachable,
                };

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
                    return Some(self.super_state_at_target_in_expression(
                        switch_stmt.expression,
                        target_idx,
                        state.super_called,
                    ));
                }

                // Every clause runs after the switch expression fully evaluated.
                let state = SuperInitFlowState {
                    super_called: state.super_called
                        || self.expression_guarantees_super_call(switch_stmt.expression),
                    reachable: state.reachable,
                };

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
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                self.super_state_at_target_in_loop(stmt_idx, target_idx, state)
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                let for_in_of = self.ctx.arena.get_for_in_of(stmt_node)?;
                if self.node_contains_target(for_in_of.expression, target_idx) {
                    return Some(self.super_state_at_target_in_expression(
                        for_in_of.expression,
                        target_idx,
                        state.super_called,
                    ));
                }
                // The initializer pattern and the body only run after the
                // iterated expression fully evaluated.
                let state = SuperInitFlowState {
                    super_called: state.super_called
                        || self.expression_guarantees_super_call(for_in_of.expression),
                    reachable: state.reachable,
                };
                if self.node_contains_target(for_in_of.initializer, target_idx) {
                    return Some(state.super_called);
                }
                self.super_called_on_all_paths_to_target_in_statement(
                    for_in_of.statement,
                    target_idx,
                    state,
                )
            }
            _ => Some(self.super_state_at_target_in_leaf_statement(
                stmt_idx,
                target_idx,
                state.super_called,
            )),
        }
    }

    /// Target-side handling for `while`/`do`/`for` statements, mirroring
    /// `tsc`'s first-iteration flow: the path that reaches the loop top is
    /// the pre-loop path, and evaluation order within one iteration counts.
    fn super_state_at_target_in_loop(
        &self,
        stmt_idx: NodeIndex,
        target_idx: NodeIndex,
        state: SuperInitFlowState,
    ) -> Option<bool> {
        let stmt_node = self.ctx.arena.get(stmt_idx)?;
        let loop_data = self.ctx.arena.get_loop(stmt_node)?;
        let is_do = stmt_node.kind == syntax_kind_ext::DO_STATEMENT;

        if is_do {
            // Evaluation order: body first, condition after.
            if let Some(result) = self.super_called_on_all_paths_to_target_in_statement(
                loop_data.statement,
                target_idx,
                state,
            ) {
                return Some(result);
            }
            if loop_data.condition.is_some()
                && self.node_contains_target(loop_data.condition, target_idx)
            {
                let after_body = self.super_flow_after_statement(loop_data.statement, state);
                return Some(self.super_state_at_target_in_expression(
                    loop_data.condition,
                    target_idx,
                    after_body.super_called,
                ));
            }
            return None;
        }

        // while/for evaluation order: initializer (for only), condition, body.
        let mut super_called = state.super_called;
        if loop_data.initializer.is_some() {
            if self.node_contains_target(loop_data.initializer, target_idx) {
                return Some(self.super_state_at_target_in_leaf_statement(
                    loop_data.initializer,
                    target_idx,
                    super_called,
                ));
            }
            super_called = super_called
                || self.statement_guarantees_super_call(loop_data.initializer)
                || self.expression_guarantees_super_call(loop_data.initializer);
        }
        if loop_data.condition.is_some() {
            if self.node_contains_target(loop_data.condition, target_idx) {
                return Some(self.super_state_at_target_in_expression(
                    loop_data.condition,
                    target_idx,
                    super_called,
                ));
            }
            super_called =
                super_called || self.expression_guarantees_super_call(loop_data.condition);
        }
        let body_entry = SuperInitFlowState {
            super_called,
            reachable: state.reachable,
        };
        if let Some(result) = self.super_called_on_all_paths_to_target_in_statement(
            loop_data.statement,
            target_idx,
            body_entry,
        ) {
            return Some(result);
        }
        if loop_data.incrementor.is_some()
            && self.node_contains_target(loop_data.incrementor, target_idx)
        {
            // The incrementor first runs after one full body execution.
            let after_body = self.super_flow_after_statement(loop_data.statement, body_entry);
            return Some(self.super_state_at_target_in_expression(
                loop_data.incrementor,
                target_idx,
                after_body.super_called,
            ));
        }
        None
    }

    /// Evaluation-order walk for the statement forms that directly carry
    /// expressions (expression statements, `return`/`throw`, variable
    /// declarations). Returns whether a `super()` call definitely executes
    /// before `target_idx` is reached. Falls back to the statement-entry
    /// state when the target sits in an unmodeled position.
    fn super_state_at_target_in_leaf_statement(
        &self,
        stmt_idx: NodeIndex,
        target_idx: NodeIndex,
        super_called: bool,
    ) -> bool {
        let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
            return super_called;
        };

        match stmt_node.kind {
            syntax_kind_ext::EXPRESSION_STATEMENT => {
                let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt_node) else {
                    return super_called;
                };
                self.super_state_at_target_in_expression(
                    expr_stmt.expression,
                    target_idx,
                    super_called,
                )
            }
            syntax_kind_ext::RETURN_STATEMENT | syntax_kind_ext::THROW_STATEMENT => {
                let Some(ret) = self.ctx.arena.get_return_statement(stmt_node) else {
                    return super_called;
                };
                if ret.expression.is_some() {
                    self.super_state_at_target_in_expression(
                        ret.expression,
                        target_idx,
                        super_called,
                    )
                } else {
                    super_called
                }
            }
            _ => {
                if self.ctx.arena.get_variable(stmt_node).is_some() {
                    self.super_state_at_target_in_variable_group(stmt_idx, target_idx, super_called)
                } else {
                    super_called
                }
            }
        }
    }

    /// Walk a variable statement / declaration list in declaration order.
    fn super_state_at_target_in_variable_group(
        &self,
        group_idx: NodeIndex,
        target_idx: NodeIndex,
        mut super_called: bool,
    ) -> bool {
        let Some(group_node) = self.ctx.arena.get(group_idx) else {
            return super_called;
        };
        let Some(group) = self.ctx.arena.get_variable(group_node) else {
            return super_called;
        };

        for &decl_idx in &group.declarations.nodes {
            if self.node_contains_target(decl_idx, target_idx) {
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    return super_called;
                };
                if self.ctx.arena.get_variable(decl_node).is_some() {
                    return self.super_state_at_target_in_variable_group(
                        decl_idx,
                        target_idx,
                        super_called,
                    );
                }
                if let Some(binding_elem) = self.ctx.arena.get_binding_element(decl_node) {
                    if binding_elem.initializer.is_some()
                        && self.node_contains_target(binding_elem.initializer, target_idx)
                    {
                        return self.super_state_at_target_in_expression(
                            binding_elem.initializer,
                            target_idx,
                            super_called,
                        );
                    }
                    return super_called;
                }
                if let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node)
                    && decl.initializer.is_some()
                    && self.node_contains_target(decl.initializer, target_idx)
                {
                    return self.super_state_at_target_in_expression(
                        decl.initializer,
                        target_idx,
                        super_called,
                    );
                }
                return super_called;
            }
            super_called =
                super_called || self.variable_declaration_guarantees_super_call(decl_idx);
        }

        super_called
    }

    /// Evaluation-order walk of an expression subtree, the syntactic
    /// analogue of `tsc`'s flow-graph ordering: returns whether a `super()`
    /// call definitely executes before `target_idx` is reached. Earlier
    /// unconditionally-evaluated siblings contribute through
    /// `expression_guarantees_super_call`; a `super(...)` call itself only
    /// counts *after* its arguments (so `super(this)` stays an error).
    /// Unmodeled constructs conservatively return the entry state.
    fn super_state_at_target_in_expression(
        &self,
        expr_idx: NodeIndex,
        target_idx: NodeIndex,
        super_called: bool,
    ) -> bool {
        if expr_idx == target_idx {
            return super_called;
        }
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return super_called;
        };

        match expr_node.kind {
            syntax_kind_ext::CALL_EXPRESSION | syntax_kind_ext::NEW_EXPRESSION => {
                let Some(call) = self.ctx.arena.get_call_expr(expr_node) else {
                    return super_called;
                };
                if self.node_contains_target(call.expression, target_idx) {
                    return self.super_state_at_target_in_expression(
                        call.expression,
                        target_idx,
                        super_called,
                    );
                }
                let mut acc =
                    super_called || self.expression_guarantees_super_call(call.expression);
                if let Some(args) = call.arguments.as_ref() {
                    for &arg_idx in &args.nodes {
                        if self.node_contains_target(arg_idx, target_idx) {
                            return self
                                .super_state_at_target_in_expression(arg_idx, target_idx, acc);
                        }
                        acc = acc || self.expression_guarantees_super_call(arg_idx);
                    }
                }
                acc
            }
            syntax_kind_ext::BINARY_EXPRESSION => {
                let Some(binary) = self.ctx.arena.get_binary_expr(expr_node) else {
                    return super_called;
                };
                if self.node_contains_target(binary.left, target_idx) {
                    return self.super_state_at_target_in_expression(
                        binary.left,
                        target_idx,
                        super_called,
                    );
                }
                // Whenever the right operand evaluates, the left already
                // completed — this holds for logical operators too.
                let acc = super_called || self.expression_guarantees_super_call(binary.left);
                self.super_state_at_target_in_expression(binary.right, target_idx, acc)
            }
            syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                let Some(cond) = self.ctx.arena.get_conditional_expr(expr_node) else {
                    return super_called;
                };
                if self.node_contains_target(cond.condition, target_idx) {
                    return self.super_state_at_target_in_expression(
                        cond.condition,
                        target_idx,
                        super_called,
                    );
                }
                let acc = super_called || self.expression_guarantees_super_call(cond.condition);
                if self.node_contains_target(cond.when_true, target_idx) {
                    return self.super_state_at_target_in_expression(
                        cond.when_true,
                        target_idx,
                        acc,
                    );
                }
                self.super_state_at_target_in_expression(cond.when_false, target_idx, acc)
            }
            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                let Some(literal) = self.ctx.arena.get_literal_expr(expr_node) else {
                    return super_called;
                };
                let mut acc = super_called;
                for &elem_idx in &literal.elements.nodes {
                    if self.node_contains_target(elem_idx, target_idx) {
                        return self.super_state_at_target_in_object_literal_element(
                            elem_idx, target_idx, acc,
                        );
                    }
                    acc = acc || self.object_literal_element_guarantees_super_call(elem_idx);
                }
                acc
            }
            syntax_kind_ext::TEMPLATE_EXPRESSION => {
                let Some(template) = self.ctx.arena.get_template_expr(expr_node) else {
                    return super_called;
                };
                let mut acc = super_called;
                for &span_idx in &template.template_spans.nodes {
                    let span_expr = self
                        .ctx
                        .arena
                        .get(span_idx)
                        .and_then(|span_node| self.ctx.arena.get_template_span(span_node))
                        .map(|span| span.expression);
                    let Some(span_expr) = span_expr else {
                        continue;
                    };
                    if self.node_contains_target(span_expr, target_idx) {
                        return self
                            .super_state_at_target_in_expression(span_expr, target_idx, acc);
                    }
                    acc = acc || self.expression_guarantees_super_call(span_expr);
                }
                acc
            }
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
                    return super_called;
                };
                if self.node_contains_target(access.expression, target_idx) {
                    return self.super_state_at_target_in_expression(
                        access.expression,
                        target_idx,
                        super_called,
                    );
                }
                let acc = super_called || self.expression_guarantees_super_call(access.expression);
                self.super_state_at_target_in_expression(access.name_or_argument, target_idx, acc)
            }
            syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let Some(paren) = self.ctx.arena.get_parenthesized(expr_node) else {
                    return super_called;
                };
                self.super_state_at_target_in_expression(paren.expression, target_idx, super_called)
            }
            syntax_kind_ext::AS_EXPRESSION
            | syntax_kind_ext::SATISFIES_EXPRESSION
            | syntax_kind_ext::TYPE_ASSERTION => {
                let Some(assertion) = self.ctx.arena.get_type_assertion(expr_node) else {
                    return super_called;
                };
                self.super_state_at_target_in_expression(
                    assertion.expression,
                    target_idx,
                    super_called,
                )
            }
            syntax_kind_ext::NON_NULL_EXPRESSION
            | syntax_kind_ext::AWAIT_EXPRESSION
            | syntax_kind_ext::YIELD_EXPRESSION
            | syntax_kind_ext::SPREAD_ELEMENT => {
                let Some(unary) = self.ctx.arena.get_unary_expr_ex(expr_node) else {
                    return super_called;
                };
                self.super_state_at_target_in_expression(unary.expression, target_idx, super_called)
            }
            syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            | syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
            | syntax_kind_ext::VOID_EXPRESSION
            | syntax_kind_ext::TYPE_OF_EXPRESSION
            | syntax_kind_ext::DELETE_EXPRESSION => {
                let Some(unary) = self.ctx.arena.get_unary_expr(expr_node) else {
                    return super_called;
                };
                self.super_state_at_target_in_expression(unary.operand, target_idx, super_called)
            }
            syntax_kind_ext::SPREAD_ASSIGNMENT => {
                let Some(spread) = self.ctx.arena.get_spread(expr_node) else {
                    return super_called;
                };
                self.super_state_at_target_in_expression(
                    spread.expression,
                    target_idx,
                    super_called,
                )
            }
            _ => super_called,
        }
    }

    /// Evaluation-order walk into one object/array literal element that
    /// contains the target.
    fn super_state_at_target_in_object_literal_element(
        &self,
        elem_idx: NodeIndex,
        target_idx: NodeIndex,
        super_called: bool,
    ) -> bool {
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return super_called;
        };

        match elem_node.kind {
            syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                    return super_called;
                };
                if self.node_contains_target(prop.name, target_idx) {
                    return self.super_state_at_target_in_expression(
                        prop.name,
                        target_idx,
                        super_called,
                    );
                }
                self.super_state_at_target_in_expression(prop.initializer, target_idx, super_called)
            }
            syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node) else {
                    return super_called;
                };
                if shorthand.object_assignment_initializer.is_some() {
                    return self.super_state_at_target_in_expression(
                        shorthand.object_assignment_initializer,
                        target_idx,
                        super_called,
                    );
                }
                super_called
            }
            // Method/accessor bodies do not execute during literal
            // evaluation; leave the entry state untouched.
            syntax_kind_ext::METHOD_DECLARATION
            | syntax_kind_ext::GET_ACCESSOR
            | syntax_kind_ext::SET_ACCESSOR => super_called,
            _ => self.super_state_at_target_in_expression(elem_idx, target_idx, super_called),
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
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                let Some(loop_data) = self.ctx.arena.get_loop(stmt_node) else {
                    return state;
                };
                let mut super_called = state.super_called;
                if stmt_node.kind == syntax_kind_ext::DO_STATEMENT {
                    // A do-while body executes at least once.
                    super_called = super_called
                        || self
                            .super_flow_after_statement(loop_data.statement, state)
                            .super_called;
                } else if loop_data.initializer.is_some() {
                    // A for initializer executes exactly once.
                    super_called = super_called
                        || self.statement_guarantees_super_call(loop_data.initializer)
                        || self.expression_guarantees_super_call(loop_data.initializer);
                }
                // The condition evaluates at least once (before the first
                // body run for while/for, after it for do-while).
                if loop_data.condition.is_some() {
                    super_called =
                        super_called || self.expression_guarantees_super_call(loop_data.condition);
                }
                SuperInitFlowState {
                    super_called,
                    reachable: state.reachable,
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                let Some(for_in_of) = self.ctx.arena.get_for_in_of(stmt_node) else {
                    return state;
                };
                // The iterated expression evaluates exactly once.
                SuperInitFlowState {
                    super_called: state.super_called
                        || self.expression_guarantees_super_call(for_in_of.expression),
                    reachable: state.reachable,
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
}
