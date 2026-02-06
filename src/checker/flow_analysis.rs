//! Flow Analysis Module
//!
//! This module contains flow analysis utilities for:
//!
//! ## Property Assignment Tracking
//! - Tracking property assignments in constructors and class bodies
//! - Detecting property used before assignment (TS2565)
//! - Tracking definite assignment of class properties
//! - Analyzing control flow in constructors
//!
//! ## Definite Assignment Analysis
//! - Checking variables are assigned before use (TS2454)
//! - TDZ (Temporal Dead Zone) checking for static blocks and computed properties
//! - Flow-based assignment tracking through control flow
//!
//! ## Type Narrowing
//! - typeof-based type narrowing
//! - Discriminated union narrowing
//! - Instance type narrowing
//!
//! The analysis is flow-sensitive and handles:
//! - If/else branches
//! - Switch statements
//! - Try/catch/finally blocks
//! - Loop statements
//! - Return/throw exits

use crate::binder::SymbolId;
use crate::checker::FlowAnalyzer;
use crate::checker::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use crate::checker::types::diagnostics::Diagnostic;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::{FlowTypeEvaluator, TypeId};
use rustc_hash::FxHashSet;
use std::rc::Rc;

// =============================================================================
// Property Key Types
// =============================================================================

/// Represents a property key for tracking property assignments.
///
/// Used to identify properties on `this` in constructor and class body analysis.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) enum PropertyKey {
    /// A simple identifier property: `this.foo`
    Ident(String),
    /// A private identifier property: `this.#foo`
    Private(String),
    /// A computed property: `this["foo"]`, `this[0]`, etc.
    Computed(ComputedKey),
}

/// Represents a computed property key.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) enum ComputedKey {
    /// A computed key that is an identifier: `this[foo]`
    Ident(String),
    /// A computed key that is a string literal: `this["foo"]`
    String(String),
    /// A computed key that is a numeric literal: `this[0]`
    Number(String),
    /// A computed key that is a qualified name: `this[Foo.Bar]`
    Qualified(String),
    /// Symbol call like Symbol("key") or Symbol() - stores optional description
    Symbol(Option<String>),
}

// =============================================================================
// Flow Result
// =============================================================================

/// Result of analyzing control flow for property assignments.
///
/// Tracks two sets of assigned properties:
/// - `normal`: Properties definitely assigned on normal control flow paths
/// - `exits`: Properties definitely assigned on paths that exit (return/throw)
#[derive(Clone, Debug)]
pub(crate) struct FlowResult {
    /// Properties assigned on paths that continue normally
    pub normal: Option<FxHashSet<PropertyKey>>,
    /// Properties assigned on paths that exit (return/throw)
    pub exits: Option<FxHashSet<PropertyKey>>,
}

// =============================================================================
// Property Assignment Flow Analysis Implementation
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Constructor Assignment Analysis
    // =========================================================================

    /// Analyze property assignments in a constructor body.
    ///
    /// This is the main entry point for analyzing which properties are
    /// definitely assigned by a constructor.
    pub(crate) fn analyze_constructor_assignments(
        &self,
        body_idx: NodeIndex,
        tracked: &FxHashSet<PropertyKey>,
        require_super: bool,
    ) -> FxHashSet<PropertyKey> {
        let result = if require_super {
            self.analyze_constructor_body_after_super(body_idx, tracked)
        } else {
            self.analyze_statement(body_idx, &FxHashSet::default(), tracked)
        };

        self.flow_result_to_assigned(result)
    }

    /// Analyze a constructor body starting after the super() call.
    ///
    /// In derived classes, properties can only be assigned after super() is called.
    fn analyze_constructor_body_after_super(
        &self,
        body_idx: NodeIndex,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return FlowResult {
                normal: Some(FxHashSet::default()),
                exits: None,
            };
        };

        if body_node.kind != syntax_kind_ext::BLOCK {
            return FlowResult {
                normal: Some(FxHashSet::default()),
                exits: None,
            };
        }

        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return FlowResult {
                normal: Some(FxHashSet::default()),
                exits: None,
            };
        };

        let Some(start_idx) = self.find_super_statement_start(&block.statements.nodes) else {
            return FlowResult {
                normal: Some(FxHashSet::default()),
                exits: None,
            };
        };

        self.analyze_block(
            &block.statements.nodes[start_idx..],
            &FxHashSet::default(),
            tracked,
        )
    }

    /// Find the index of the first statement after the super() call.
    pub(crate) fn find_super_statement_start(&self, statements: &[NodeIndex]) -> Option<usize> {
        for (idx, &stmt_idx) in statements.iter().enumerate() {
            if self.is_super_call_statement(stmt_idx) {
                return Some(idx + 1);
            }
        }
        None
    }

    /// Convert a FlowResult to a set of definitely assigned properties.
    fn flow_result_to_assigned(&self, result: FlowResult) -> FxHashSet<PropertyKey> {
        let mut assigned = None;
        if let Some(normal) = result.normal {
            assigned = Some(normal);
        }
        if let Some(exits) = result.exits {
            assigned = Some(match assigned {
                Some(current) => self.intersect_sets(&current, &exits),
                None => exits,
            });
        }

        assigned.unwrap_or_default()
    }

    // =========================================================================
    // Statement Analysis
    // =========================================================================

    /// Analyze a single statement for property assignments.
    pub(crate) fn analyze_statement(
        &self,
        stmt_idx: NodeIndex,
        assigned_in: &FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        if stmt_idx.is_none() {
            return FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            };
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            };
        };

        match node.kind {
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    return self.analyze_block(&block.statements.nodes, assigned_in, tracked);
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                    let mut assigned = assigned_in.clone();
                    self.collect_assignments_in_expression(
                        if_stmt.expression,
                        &mut assigned,
                        tracked,
                    );

                    let then_result =
                        self.analyze_statement(if_stmt.then_statement, &assigned, tracked);

                    let else_result = if !if_stmt.else_statement.is_none() {
                        self.analyze_statement(if_stmt.else_statement, &assigned, tracked)
                    } else {
                        FlowResult {
                            normal: Some(assigned),
                            exits: None,
                        }
                    };

                    return FlowResult {
                        normal: self.combine_flow_sets(then_result.normal, else_result.normal),
                        exits: self.combine_flow_sets(then_result.exits, else_result.exits),
                    };
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let mut assigned = assigned_in.clone();
                if let Some(ret) = self.ctx.arena.get_return_statement(node)
                    && !ret.expression.is_none()
                {
                    self.collect_assignments_in_expression(ret.expression, &mut assigned, tracked);
                }
                return FlowResult {
                    normal: None,
                    exits: Some(assigned),
                };
            }
            k if k == syntax_kind_ext::THROW_STATEMENT => {
                let mut assigned = assigned_in.clone();
                if let Some(ret) = self.ctx.arena.get_return_statement(node)
                    && !ret.expression.is_none()
                {
                    self.collect_assignments_in_expression(ret.expression, &mut assigned, tracked);
                }
                return FlowResult {
                    normal: None,
                    exits: Some(assigned),
                };
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                let mut assigned = assigned_in.clone();
                if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node)
                    && !expr_stmt.expression.is_none()
                {
                    self.collect_assignments_in_expression(
                        expr_stmt.expression,
                        &mut assigned,
                        tracked,
                    );
                }
                return FlowResult {
                    normal: Some(assigned),
                    exits: None,
                };
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                let mut assigned = assigned_in.clone();
                if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
                    // Variable statements have a declarations field, iterate through it
                    for &decl_idx in &var_stmt.declarations.nodes {
                        if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                            && let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node)
                            && !decl.initializer.is_none()
                        {
                            self.collect_assignments_in_expression(
                                decl.initializer,
                                &mut assigned,
                                tracked,
                            );
                        }
                    }
                }
                return FlowResult {
                    normal: Some(assigned),
                    exits: None,
                };
            }
            k if k == syntax_kind_ext::FOR_STATEMENT || k == syntax_kind_ext::WHILE_STATEMENT => {
                // For for/while loops: body might not execute
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    let mut assigned = assigned_in.clone();
                    if !loop_data.initializer.is_none() {
                        if let Some(init_node) = self.ctx.arena.get(loop_data.initializer)
                            && init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                        {
                            self.collect_assignments_in_variable_decl_list(
                                loop_data.initializer,
                                &mut assigned,
                                tracked,
                            );
                        } else {
                            self.collect_assignments_in_expression(
                                loop_data.initializer,
                                &mut assigned,
                                tracked,
                            );
                        }
                    }
                    if !loop_data.condition.is_none() {
                        self.collect_assignments_in_expression(
                            loop_data.condition,
                            &mut assigned,
                            tracked,
                        );
                    }
                    // Loop bodies may not execute
                    return FlowResult {
                        normal: Some(assigned),
                        exits: None,
                    };
                }
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                if let Some(_for_data) = self.ctx.arena.get_for_in_of(node) {
                    let assigned = assigned_in.clone();
                    // Loop bodies may not execute
                    return FlowResult {
                        normal: Some(assigned),
                        exits: None,
                    };
                }
            }
            k if k == syntax_kind_ext::DO_STATEMENT => {
                // Do-while body executes at least once
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    let body_result =
                        self.analyze_statement(loop_data.statement, assigned_in, tracked);
                    return FlowResult {
                        normal: body_result.normal,
                        exits: body_result.exits,
                    };
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    return self.analyze_try_statement(try_data, assigned_in, tracked);
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node) {
                    return self.analyze_switch_statement(switch_data, assigned_in, tracked);
                }
            }
            _ => {}
        }

        FlowResult {
            normal: Some(assigned_in.clone()),
            exits: None,
        }
    }

    /// Analyze a block of statements.
    fn analyze_block(
        &self,
        statements: &[NodeIndex],
        assigned_in: &FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        let mut assigned = assigned_in.clone();
        let mut normal = Some(assigned.clone());
        let mut exits: Option<FxHashSet<PropertyKey>> = None;

        for &stmt_idx in statements {
            if normal.is_none() {
                break;
            }
            let result = self.analyze_statement(stmt_idx, &assigned, tracked);
            exits = self.combine_flow_sets(exits, result.exits);
            match result.normal {
                Some(next) => {
                    assigned = next;
                    normal = Some(assigned.clone());
                }
                None => {
                    normal = None;
                }
            }
        }

        FlowResult { normal, exits }
    }

    /// Analyze a try/catch/finally statement.
    fn analyze_try_statement(
        &self,
        try_data: &crate::parser::node::TryData,
        assigned_in: &FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        let try_result = self.analyze_statement(try_data.try_block, assigned_in, tracked);
        let catch_result = if !try_data.catch_clause.is_none() {
            if let Some(catch_node) = self.ctx.arena.get(try_data.catch_clause) {
                if let Some(catch) = self.ctx.arena.get_catch_clause(catch_node) {
                    self.analyze_statement(catch.block, assigned_in, tracked)
                } else {
                    FlowResult {
                        normal: None,
                        exits: None,
                    }
                }
            } else {
                FlowResult {
                    normal: None,
                    exits: None,
                }
            }
        } else {
            FlowResult {
                normal: None,
                exits: None,
            }
        };

        let mut normal = if try_data.catch_clause.is_none() {
            try_result.normal
        } else {
            self.combine_flow_sets(try_result.normal, catch_result.normal)
        };
        let mut exits = if try_data.catch_clause.is_none() {
            try_result.exits
        } else {
            self.combine_flow_sets(try_result.exits, catch_result.exits)
        };

        if !try_data.finally_block.is_none() {
            let finally_result =
                self.analyze_statement(try_data.finally_block, &FxHashSet::default(), tracked);
            let finally_assigned = self
                .combine_flow_sets(finally_result.normal, finally_result.exits)
                .unwrap_or_default();

            if let Some(ref mut normal_set) = normal {
                normal_set.extend(finally_assigned.iter().cloned());
            }
            if let Some(ref mut exits_set) = exits {
                exits_set.extend(finally_assigned.iter().cloned());
            }
        }

        FlowResult { normal, exits }
    }

    /// Analyze a switch statement.
    fn analyze_switch_statement(
        &self,
        switch_data: &crate::parser::node::SwitchData,
        assigned_in: &FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        let mut assigned = assigned_in.clone();
        self.collect_assignments_in_expression(switch_data.expression, &mut assigned, tracked);

        let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block) else {
            return FlowResult {
                normal: Some(assigned),
                exits: None,
            };
        };
        let Some(case_block) = self.ctx.arena.get_block(case_block_node) else {
            return FlowResult {
                normal: Some(assigned),
                exits: None,
            };
        };

        let mut normal: Option<FxHashSet<PropertyKey>> = None;
        let mut exits: Option<FxHashSet<PropertyKey>> = None;

        let mut has_default_clause = false;

        for &clause_idx in &case_block.statements.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            if let Some(clause) = self.ctx.arena.get_case_clause(clause_node) {
                // Check if this is a default clause (no expression)
                if clause.expression.is_none() {
                    has_default_clause = true;
                }
                let result = self.analyze_block(&clause.statements.nodes, &assigned, tracked);
                normal = self.combine_flow_sets(normal, result.normal);
                exits = self.combine_flow_sets(exits, result.exits);
            }
        }

        // If there's no default clause, the switch might not execute any case
        // Properties are only definitely assigned if ALL cases assign them
        // AND the switch covers all possible values (has default)
        if !has_default_clause {
            // Without a default, we can't guarantee any case will execute
            // However, execution CAN continue after the switch (fall-through)
            // Return the incoming assignments to preserve the normal flow
            return FlowResult {
                normal: Some(assigned),
                exits,
            };
        }

        // With a default clause, use the combined assignments
        if normal.is_none() && exits.is_some() {
            normal = exits.clone();
        } else if normal.is_none() && exits.is_none() {
            normal = Some(assigned);
        }

        FlowResult { normal, exits }
    }

    // =========================================================================
    // Assignment Collection
    // =========================================================================

    /// Collect property assignments from a variable declaration list.
    pub(crate) fn collect_assignments_in_variable_decl_list(
        &self,
        decl_list_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        let Some(list_node) = self.ctx.arena.get(decl_list_idx) else {
            return;
        };
        let Some(list) = self.ctx.arena.get_variable(list_node) else {
            return;
        };
        for &decl_idx in &list.declarations.nodes {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if !decl.initializer.is_none() {
                self.collect_assignments_in_expression(decl.initializer, assigned, tracked);
            }
        }
    }

    /// Collect property assignments from an expression.
    ///
    /// This walks the expression tree and tracks assignments to `this.property`.
    pub(crate) fn collect_assignments_in_expression(
        &self,
        expr_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        if expr_idx.is_none() {
            return;
        }

        let mut stack = vec![expr_idx];
        while let Some(idx) = stack.pop() {
            if idx.is_none() {
                continue;
            }
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };

            match node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR =>
                {
                    continue;
                }
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    if let Some(bin) = self.ctx.arena.get_binary_expr(node) {
                        if self.is_assignment_operator(bin.operator_token) {
                            self.collect_assignment_target(bin.left, assigned, tracked);
                        }
                        if !bin.right.is_none() {
                            stack.push(bin.right);
                        }
                        if !bin.left.is_none() {
                            stack.push(bin.left);
                        }
                    }
                }
                k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                    || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
                {
                    if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
                        if unary.operator == SyntaxKind::PlusPlusToken as u16
                            || unary.operator == SyntaxKind::MinusMinusToken as u16
                        {
                            self.collect_assignment_target(unary.operand, assigned, tracked);
                        }
                        if !unary.operand.is_none() {
                            stack.push(unary.operand);
                        }
                    }
                }
                k if k == syntax_kind_ext::CALL_EXPRESSION
                    || k == syntax_kind_ext::NEW_EXPRESSION =>
                {
                    if let Some(call) = self.ctx.arena.get_call_expr(node) {
                        stack.push(call.expression);
                        if let Some(ref args) = call.arguments {
                            for &arg in &args.nodes {
                                stack.push(arg);
                            }
                        }
                    }
                }
                k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
                {
                    if let Some(access) = self.ctx.arena.get_access_expr(node) {
                        stack.push(access.expression);
                        stack.push(access.name_or_argument);
                    }
                }
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                        stack.push(paren.expression);
                    }
                }
                k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                    if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                        stack.push(cond.condition);
                        stack.push(cond.when_true);
                        stack.push(cond.when_false);
                    }
                }
                k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION =>
                {
                    if let Some(literal) = self.ctx.arena.get_literal_expr(node) {
                        for &elem in &literal.elements.nodes {
                            stack.push(elem);
                        }
                    }
                }
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    if let Some(prop) = self.ctx.arena.get_property_assignment(node) {
                        stack.push(prop.initializer);
                    }
                }
                k if k == syntax_kind_ext::SPREAD_ELEMENT
                    || k == syntax_kind_ext::SPREAD_ASSIGNMENT =>
                {
                    if let Some(spread) = self.ctx.arena.get_spread(node) {
                        stack.push(spread.expression);
                    }
                }
                k if k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION
                    || k == syntax_kind_ext::TYPE_ASSERTION =>
                {
                    if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                        stack.push(assertion.expression);
                    }
                }
                k if k == syntax_kind_ext::NON_NULL_EXPRESSION
                    || k == syntax_kind_ext::AWAIT_EXPRESSION
                    || k == syntax_kind_ext::YIELD_EXPRESSION =>
                {
                    if let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) {
                        stack.push(unary.expression);
                    }
                }
                _ => {}
            }
        }
    }

    /// Collect assignment target from an expression.
    fn collect_assignment_target(
        &self,
        target_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        if target_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(target_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(key) = self.property_key_from_access(target_idx) {
                    self.record_property_assignment(key, assigned, tracked);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.collect_assignment_target(paren.expression, assigned, tracked);
                }
            }
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION =>
            {
                if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                    self.collect_assignment_target(assertion.expression, assigned, tracked);
                }
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.collect_assignment_target(unary.expression, assigned, tracked);
                }
            }
            // Handle destructuring assignments: ({ a: this.a, b: this.b } = obj)
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.collect_destructuring_assignments(target_idx, assigned, tracked);
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.collect_array_destructuring_assignments(target_idx, assigned, tracked);
            }
            _ => {}
        }
    }

    /// Collect property assignments from object destructuring patterns.
    /// Handles: ({ a: this.a, b: this.b } = data)
    fn collect_destructuring_assignments(
        &self,
        literal_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        let Some(node) = self.ctx.arena.get(literal_idx) else {
            return;
        };
        let Some(literal) = self.ctx.arena.get_literal_expr(node) else {
            return;
        };

        for &elem_idx in &literal.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Handle property assignment: { a: this.a }
            if elem_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                    // Check if the value being assigned is a property access like this.a
                    if let Some(key) = self.property_key_from_access(prop.initializer) {
                        self.record_property_assignment(key, assigned, tracked);
                    }
                }
            }
            // Handle shorthand property assignment: { this.a }
            // (This is less common but syntactically valid in destructuring)
            else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                if let Some(prop) = self.ctx.arena.get_shorthand_property(elem_node)
                    && let Some(key) = self.property_key_from_access(prop.name)
                {
                    self.record_property_assignment(key, assigned, tracked);
                }
            }
            // Handle nested destructuring (recursively)
            else if elem_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                self.collect_destructuring_assignments(elem_idx, assigned, tracked);
            } else if elem_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                self.collect_array_destructuring_assignments(elem_idx, assigned, tracked);
            }
        }
    }

    /// Collect property assignments from array destructuring patterns.
    /// Handles: [this.a, this.b] = arr, [x = 1] = []
    fn collect_array_destructuring_assignments(
        &self,
        literal_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        let Some(node) = self.ctx.arena.get(literal_idx) else {
            return;
        };
        let Some(literal) = self.ctx.arena.get_literal_expr(node) else {
            return;
        };

        for &elem_idx in &literal.elements.nodes {
            // Skip holes in array destructuring: [a, , b]
            if elem_idx.is_none() {
                continue;
            }

            // Check if the element is a property access like this.a
            if let Some(key) = self.property_key_from_access(elem_idx) {
                self.record_property_assignment(key, assigned, tracked);
            }
            // Handle nested destructuring and other patterns
            else if let Some(elem_node) = self.ctx.arena.get(elem_idx) {
                if elem_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                    // Handle assignment patterns with defaults: [x = 1] = []
                    // The left side of the assignment is the target being assigned to
                    if let Some(bin) = self.ctx.arena.get_binary_expr(elem_node) {
                        if self.is_assignment_operator(bin.operator_token) {
                            self.collect_assignment_target(bin.left, assigned, tracked);
                        }
                    }
                } else if elem_node.kind == SyntaxKind::Identifier as u16 {
                    // Handle simple identifier: [x] = [1]
                    // This clears narrowing on x because x is being assigned to
                    if let Some(key) = self.property_key_from_name(elem_idx) {
                        self.record_property_assignment(key, assigned, tracked);
                    }
                } else if elem_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                    self.collect_destructuring_assignments(elem_idx, assigned, tracked);
                } else if elem_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                    self.collect_array_destructuring_assignments(elem_idx, assigned, tracked);
                }
            }
        }
    }

    /// Record a property assignment, handling both Ident and Computed forms.
    fn record_property_assignment(
        &self,
        key: PropertyKey,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        if tracked.contains(&key) {
            assigned.insert(key.clone());
        }

        match key {
            PropertyKey::Ident(name) => {
                let computed = PropertyKey::Computed(ComputedKey::String(name));
                if tracked.contains(&computed) {
                    assigned.insert(computed);
                }
            }
            PropertyKey::Computed(ComputedKey::String(name)) => {
                let ident = PropertyKey::Ident(name);
                if tracked.contains(&ident) {
                    assigned.insert(ident);
                }
            }
            _ => {}
        }
    }

    // =========================================================================
    // Property Key Extraction
    // =========================================================================

    /// Extract a PropertyKey from a property name node.
    pub(crate) fn property_key_from_name(&self, name_idx: NodeIndex) -> Option<PropertyKey> {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return None;
        };

        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            if name_node.kind == SyntaxKind::PrivateIdentifier as u16 {
                return Some(PropertyKey::Private(ident.escaped_text.clone()));
            }
            return Some(PropertyKey::Ident(ident.escaped_text.clone()));
        }

        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
        ) && let Some(lit) = self.ctx.arena.get_literal(name_node)
            && !lit.text.is_empty()
        {
            let key = if name_node.kind == SyntaxKind::NumericLiteral as u16 {
                PropertyKey::Computed(ComputedKey::Number(lit.text.clone()))
            } else {
                PropertyKey::Computed(ComputedKey::String(lit.text.clone()))
            };
            return Some(key);
        }

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.ctx.arena.get_computed_property(name_node)
        {
            return self
                .computed_key_from_expression(computed.expression)
                .map(PropertyKey::Computed);
        }

        None
    }

    /// Extract a PropertyKey from a property access expression on `this`.
    pub(crate) fn property_key_from_access(&self, access_idx: NodeIndex) -> Option<PropertyKey> {
        let Some(node) = self.ctx.arena.get(access_idx) else {
            return None;
        };
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return None;
        };
        let Some(expr_node) = self.ctx.arena.get(access.expression) else {
            return None;
        };
        if expr_node.kind != SyntaxKind::ThisKeyword as u16 {
            return None;
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
                return None;
            };
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                if name_node.kind == SyntaxKind::PrivateIdentifier as u16 {
                    return Some(PropertyKey::Private(ident.escaped_text.clone()));
                }
                return Some(PropertyKey::Ident(ident.escaped_text.clone()));
            }
            return None;
        }

        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return self
                .computed_key_from_expression(access.name_or_argument)
                .map(PropertyKey::Computed);
        }

        None
    }

    /// Extract a ComputedKey from an expression.
    fn computed_key_from_expression(&self, expr_idx: NodeIndex) -> Option<ComputedKey> {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return None;
        };

        if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
            return Some(ComputedKey::Ident(ident.escaped_text.clone()));
        }

        if let Some(lit) = self.ctx.arena.get_literal(expr_node) {
            match expr_node.kind {
                k if k == SyntaxKind::StringLiteral as u16
                    || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                {
                    return Some(ComputedKey::String(lit.text.clone()));
                }
                k if k == SyntaxKind::NumericLiteral as u16 => {
                    return Some(ComputedKey::Number(lit.text.clone()));
                }
                _ => {}
            }
        }

        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access_name) = self.qualified_name_from_property_access(expr_idx)
        {
            return Some(ComputedKey::Qualified(access_name));
        }

        // Handle call expressions like Symbol("key")
        if expr_node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.ctx.arena.get_call_expr(expr_node)
        {
            // Check if callee is "Symbol"
            if let Some(callee_node) = self.ctx.arena.get(call.expression)
                && let Some(callee_ident) = self.ctx.arena.get_identifier(callee_node)
                && callee_ident.escaped_text == "Symbol"
            {
                // Try to get the description argument if present
                let description = call
                    .arguments
                    .as_ref()
                    .and_then(|args| args.nodes.first())
                    .and_then(|&first_arg| self.ctx.arena.get(first_arg))
                    .and_then(|arg_node| {
                        if arg_node.kind == SyntaxKind::StringLiteral as u16 {
                            self.ctx
                                .arena
                                .get_literal(arg_node)
                                .map(|lit| lit.text.clone())
                        } else {
                            None
                        }
                    });
                return Some(ComputedKey::Symbol(description));
            }
        }

        None
    }

    /// Extract a qualified name from a property access expression.
    fn qualified_name_from_property_access(&self, access_idx: NodeIndex) -> Option<String> {
        let Some(node) = self.ctx.arena.get(access_idx) else {
            return None;
        };
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return None;
        };

        let base_name = if let Some(base_node) = self.ctx.arena.get(access.expression) {
            if let Some(ident) = self.ctx.arena.get_identifier(base_node) {
                Some(ident.escaped_text.clone())
            } else if base_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                self.qualified_name_from_property_access(access.expression)
            } else {
                None
            }
        } else {
            None
        }?;

        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return None;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return None;
        };

        Some(format!("{}.{}", base_name, ident.escaped_text))
    }

    // =========================================================================
    // Flow Set Operations
    // =========================================================================

    /// Combine two optional sets of assigned properties (intersection).
    pub(crate) fn combine_flow_sets(
        &self,
        left: Option<FxHashSet<PropertyKey>>,
        right: Option<FxHashSet<PropertyKey>>,
    ) -> Option<FxHashSet<PropertyKey>> {
        match (left, right) {
            (Some(a), Some(b)) => Some(self.intersect_sets(&a, &b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    /// Compute the intersection of two property sets.
    pub(crate) fn intersect_sets(
        &self,
        left: &FxHashSet<PropertyKey>,
        right: &FxHashSet<PropertyKey>,
    ) -> FxHashSet<PropertyKey> {
        if left.len() <= right.len() {
            left.iter()
                .filter(|key| right.contains(*key))
                .cloned()
                .collect()
        } else {
            right
                .iter()
                .filter(|key| left.contains(*key))
                .cloned()
                .collect()
        }
    }

    // =========================================================================
    // Expression Checking for Early Property Access
    // =========================================================================

    /// Check an expression for property accesses that occur before assignment.
    ///
    /// This is used to detect TS2565 errors: "Property 'x' is used before being assigned."
    pub(crate) fn check_expression_for_early_property_access(
        &mut self,
        expr_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        if expr_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                // Check if this is a this.X access
                if let Some(key) = self.property_key_from_access(expr_idx) {
                    // Check if this is a property read (not an assignment)
                    // We need to look at the parent to determine if this is the target of an assignment
                    // For now, we'll check if the property is being read before assignment
                    if tracked.contains(&key) && !assigned.contains(&key) {
                        // Emit TS2565 error
                        use crate::checker::types::diagnostics::format_message;
                        let property_name = self.get_property_name_from_key(&key);
                        self.error_at_node(
                            expr_idx,
                            &format_message(
                                crate::checker::types::diagnostics::diagnostic_messages::PROPERTY_USED_BEFORE_BEING_ASSIGNED,
                                &[&property_name],
                            ),
                            crate::checker::types::diagnostics::diagnostic_codes::PROPERTY_USED_BEFORE_BEING_ASSIGNED,
                        );
                    }
                }
                // Recursively check the expression part
                if let Some(access) = self.ctx.arena.get_access_expr(node) {
                    self.check_expression_for_early_property_access(
                        access.expression,
                        assigned,
                        tracked,
                    );
                    if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                        self.check_expression_for_early_property_access(
                            access.name_or_argument,
                            assigned,
                            tracked,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.ctx.arena.get_binary_expr(node) {
                    // If this is an assignment, handle it specially
                    if self.is_assignment_operator(bin.operator_token) {
                        // For simple assignment (=), the left side is being written to, not read
                        // We should NOT check it for "used before assigned" errors
                        // For compound assignments (+=, etc.), left side is both read AND written
                        let is_simple_assignment =
                            bin.operator_token == SyntaxKind::EqualsToken as u16;

                        // Check the right side first (it's being read)
                        self.check_expression_for_early_property_access(
                            bin.right, assigned, tracked,
                        );

                        // Track the assignment
                        self.track_assignment_in_expression(bin.left, assigned, tracked);

                        // For compound assignments, also check the left side (it's being read)
                        if !is_simple_assignment {
                            self.check_expression_for_early_property_access(
                                bin.left, assigned, tracked,
                            );
                        }
                    } else {
                        // Non-assignment binary expression: check both sides
                        self.check_expression_for_early_property_access(
                            bin.left, assigned, tracked,
                        );
                        self.check_expression_for_early_property_access(
                            bin.right, assigned, tracked,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
                    self.check_expression_for_early_property_access(
                        unary.operand,
                        assigned,
                        tracked,
                    );
                    // Track ++ and -- as both read and write
                    if unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16
                    {
                        self.track_assignment_in_expression(unary.operand, assigned, tracked);
                    }
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    self.check_expression_for_early_property_access(
                        call.expression,
                        assigned,
                        tracked,
                    );
                    if let Some(ref args) = call.arguments {
                        for &arg in &args.nodes {
                            self.check_expression_for_early_property_access(arg, assigned, tracked);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    self.check_expression_for_early_property_access(
                        cond.condition,
                        assigned,
                        tracked,
                    );
                    self.check_expression_for_early_property_access(
                        cond.when_true,
                        assigned,
                        tracked,
                    );
                    self.check_expression_for_early_property_access(
                        cond.when_false,
                        assigned,
                        tracked,
                    );
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.check_expression_for_early_property_access(
                        paren.expression,
                        assigned,
                        tracked,
                    );
                }
            }
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION =>
            {
                if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                    self.check_expression_for_early_property_access(
                        assertion.expression,
                        assigned,
                        tracked,
                    );
                }
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.check_expression_for_early_property_access(
                        unary.expression,
                        assigned,
                        tracked,
                    );
                }
            }
            _ => {}
        }
    }

    /// Track property assignments in an expression.
    pub(crate) fn track_assignment_in_expression(
        &self,
        target_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        if target_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(target_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(key) = self.property_key_from_access(target_idx)
                    && tracked.contains(&key)
                {
                    assigned.insert(key);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.track_assignment_in_expression(paren.expression, assigned, tracked);
                }
            }
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION =>
            {
                if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                    self.track_assignment_in_expression(assertion.expression, assigned, tracked);
                }
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.track_assignment_in_expression(unary.expression, assigned, tracked);
                }
            }
            _ => {}
        }
    }

    // =========================================================================
    // Definite Assignment Analysis
    // =========================================================================

    /// Apply control flow narrowing to a type at a specific identifier usage.
    ///
    /// This walks backwards through the control flow graph to determine what
    /// type guards (typeof, null checks, etc.) have been applied.
    ///
    /// ## Rule #42: CFA Invalidation in Closures
    ///
    /// When accessing a variable inside a closure (function expression or arrow function):
    /// - If the variable is `let` or `var` (mutable): Reset to declared type (ignore outer narrowing)
    /// - If the variable is `const` (immutable): Maintain narrowing (safe)
    ///
    /// This prevents unsound assumptions where a mutable variable's type is narrowed
    /// in the outer scope but the closure captures the variable and might execute
    /// after the variable has been reassigned to a different type.
    pub(crate) fn apply_flow_narrowing(&self, idx: NodeIndex, declared_type: TypeId) -> TypeId {
        // Get the flow node for this expression usage FIRST
        // If there's no flow info, no narrowing is possible regardless of node type
        let flow_node = match self.ctx.binder.get_node_flow(idx) {
            Some(flow) => flow,
            None => return declared_type, // No flow info - use declared type
        };

        // Fast path: types containing `any` cannot be meaningfully narrowed.
        // Skipping flow traversal here avoids pathological O(N^2) behavior on large
        // assignment-heavy files (e.g. largeControlFlowGraph.ts with `const data = []`).
        if declared_type == TypeId::ANY
            || declared_type == TypeId::ERROR
            || (!declared_type.is_intrinsic() && self.type_contains_any(declared_type))
        {
            return declared_type;
        }

        // Rule #42 only applies inside closures. Avoid symbol resolution work
        // on the common non-closure path.
        if self.is_inside_closure()
            && let Some(sym_id) = self.get_symbol_for_identifier(idx)
            && self.is_captured_variable(sym_id)
            && self.is_mutable_binding(sym_id)
        {
            // Rule #42: Reset narrowing for captured mutable bindings in closures
            // (const variables preserve narrowing, let/var reset to declared type)
            return declared_type;
        }

        // TEMPORARY FIX: Removed is_narrowable_type check to allow instanceof narrowing
        // This check was blocking class types from being narrowed
        // TODO: Re-enable with proper logic that allows instanceof-narrowable types
        // if !self.is_narrowable_type(declared_type) {
        //     return declared_type;
        // }

        // Create a flow analyzer and apply narrowing
        let analyzer = FlowAnalyzer::with_node_types(
            self.ctx.arena,
            self.ctx.binder,
            self.ctx.types,
            &self.ctx.node_types,
        )
        .with_flow_cache(&self.ctx.flow_analysis_cache)
        .with_type_environment(Rc::clone(&self.ctx.type_environment));

        analyzer.get_flow_type(idx, declared_type, flow_node)
    }

    /// Get the symbol for an identifier node.
    ///
    /// Returns None if the node is not an identifier or has no symbol.
    fn get_symbol_for_identifier(&self, idx: NodeIndex) -> Option<SymbolId> {
        use crate::scanner::SyntaxKind;

        let node = self.ctx.arena.get(idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        // First try get_node_symbol, then fall back to resolve_identifier
        self.ctx
            .binder
            .get_node_symbol(idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, idx))
    }

    /// Check if we're currently inside a closure (function expression or arrow function).
    ///
    /// This is used to apply Rule #42: CFA Invalidation in Closures.
    ///
    /// Returns true if inside a function expression, arrow function, or method expression.
    fn is_inside_closure(&self) -> bool {
        self.ctx.inside_closure_depth > 0
    }

    /// Check if a symbol is a mutable binding (let or var) vs immutable (const).
    ///
    /// This is used to implement TypeScript's Rule #42 for type narrowing in closures:
    /// - const variables preserve narrowing through closures (immutable)
    /// - let/var variables lose narrowing when accessed from closures (mutable)
    ///
    /// Implementation checks:
    /// 1. Get the symbol's value declaration
    /// 2. Check if it's a VariableDeclaration
    /// 3. Look at the parent VariableDeclarationList's NodeFlags
    /// 4. If CONST flag is set → const (immutable)
    /// 5. Otherwise → let/var (mutable)
    ///
    /// Returns true for let/var (mutable), false for const (immutable).
    fn is_mutable_binding(&self, sym_id: SymbolId) -> bool {
        use crate::parser::node_flags;
        use crate::parser::syntax_kind_ext;

        let symbol = match self.ctx.binder.get_symbol(sym_id) {
            Some(sym) => sym,
            None => return true, // Assume mutable if we can't determine
        };

        // Check the value declaration
        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return true; // Assume mutable if no declaration
        }

        let decl_node = match self.ctx.arena.get(decl_idx) {
            Some(node) => node,
            None => return true,
        };

        // For variable declarations, the CONST flag is on the VARIABLE_DECLARATION_LIST parent
        // The value_declaration points to VARIABLE_DECLARATION, we need to check its parent's flags
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            // Get the parent (VARIABLE_DECLARATION_LIST) via extended info
            if let Some(ext) = self.ctx.arena.get_extended(decl_idx) {
                if !ext.parent.is_none() {
                    if let Some(parent_node) = self.ctx.arena.get(ext.parent) {
                        let flags = parent_node.flags as u32;
                        let is_const = (flags & node_flags::CONST) != 0;
                        return !is_const; // Return true if NOT const (i.e., let or var)
                    }
                }
            }
        }

        // For other node types, check the node's own flags
        let flags = decl_node.flags as u32;
        let is_const = (flags & node_flags::CONST) != 0;
        !is_const // Return true if NOT const (i.e., let or var)
    }

    /// Check if a variable is captured from an outer scope (vs declared locally).
    ///
    /// Bug #1.2: Rule #42 should only apply to captured variables, not local variables.
    /// - Variables declared INSIDE the closure should narrow normally
    /// - Variables captured from OUTER scope reset narrowing (for let/var)
    ///
    /// This is determined by checking if the variable's declaration is in an ancestor scope.
    fn is_captured_variable(&self, sym_id: SymbolId) -> bool {
        use crate::binder::ScopeId;

        let symbol = match self.ctx.binder.get_symbol(sym_id) {
            Some(sym) => sym,
            None => return false, // If no symbol, assume not captured
        };

        // Get the declaration node
        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return false;
        }

        // Find the enclosing scope of the declaration
        let decl_scope_id = match self
            .ctx
            .binder
            .find_enclosing_scope(self.ctx.arena, decl_idx)
        {
            Some(scope_id) => scope_id,
            None => return false, // No scope info, assume not captured
        };

        // Get the current scope (where the variable is being accessed)
        // We need to get the current scope from the binder's state
        let current_scope_id = self.ctx.binder.current_scope_id;

        // If declared in current scope, not captured
        if decl_scope_id == current_scope_id {
            return false;
        }

        // Check if declaration scope is an ancestor of current scope
        // Walk up the scope chain from current scope to see if we find the declaration scope
        let mut scope_id = current_scope_id;
        let mut iterations = 0;
        while !scope_id.is_none() && iterations < MAX_TREE_WALK_ITERATIONS {
            if scope_id == decl_scope_id {
                // Found declaration scope in ancestor chain → captured variable
                return true;
            }

            // Move to parent scope
            scope_id = self
                .ctx
                .binder
                .scopes
                .get(scope_id.0 as usize)
                .map(|scope| scope.parent)
                .unwrap_or(ScopeId::NONE);

            iterations += 1;
        }

        false
    }

    /// Check flow-aware usage of a variable (definite assignment + type narrowing).
    ///
    /// This is the main entry point for flow analysis when variables are used.
    /// It combines two critical TypeScript features:
    /// 1. **Definite Assignment Analysis**: Catches use-before-assignment errors
    /// 2. **Type Narrowing**: Refines types based on control flow
    ///
    /// ## Definite Assignment Checking:
    /// - Block-scoped variables (let/const) without initializers are checked
    /// - Variables are tracked through all code paths
    /// - TS2454 error emitted if variable might not be assigned
    /// - Error: "Variable 'x' is used before being assigned"
    ///
    /// ## Type Narrowing:
    /// - If definitely assigned, applies flow-based type narrowing
    /// - typeof guards, discriminant checks, null checks refine types
    /// - Returns narrowed type for precise type checking
    ///
    /// ## Rule #42 Integration:
    /// - If inside a closure and variable is mutable (let/var): Returns declared type
    /// - If inside a closure and variable is const: Applies narrowing
    pub fn check_flow_usage(
        &mut self,
        idx: NodeIndex,
        declared_type: TypeId,
        sym_id: SymbolId,
    ) -> TypeId {
        // Check definite assignment for block-scoped variables without initializers
        if self.should_check_definite_assignment(sym_id, idx)
            && !self.skip_definite_assignment_for_type(declared_type)
            && !self.is_definitely_assigned_at(idx)
        {
            // Report TS2454 error: Variable used before assignment
            self.emit_definite_assignment_error(idx, sym_id);
            // Return declared type to avoid cascading errors
            return declared_type;
        }

        // Apply type narrowing based on control flow
        self.apply_flow_narrowing(idx, declared_type)
    }

    /// Emit TS2454 error for variable used before definite assignment.
    fn emit_definite_assignment_error(&mut self, idx: NodeIndex, sym_id: SymbolId) {
        // Get the location for error reporting and deduplication key
        let Some(node) = self.ctx.arena.get(idx) else {
            // If the node doesn't exist in the arena, we can't deduplicate by position
            // Skip error emission to avoid potential duplicates
            return;
        };

        let pos = node.pos;

        // Deduplicate: check if we've already emitted an error for this (node, symbol) pair
        let key = (pos, sym_id);
        if !self.ctx.emitted_ts2454_errors.insert(key) {
            // Already inserted - duplicate error, skip
            return;
        }

        // Get the variable name for the error message
        let name = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .map(|s| s.escaped_name.clone())
            .unwrap_or_else(|| "<unknown>".to_string());

        // Get the location for error reporting
        let length = node.end - node.pos;

        self.ctx.diagnostics.push(Diagnostic::error(
            self.ctx.file_name.clone(),
            pos,
            length,
            format!("Variable '{}' is used before being assigned", name),
            2454, // TS2454
        ));
    }

    /// Check if a node is within a parameter's default value initializer.
    /// This is used to detect `await` used in default parameter values (TS2524).
    pub(crate) fn is_in_default_parameter(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let ext = match self.ctx.arena.get_extended(current) {
                Some(ext) => ext,
                None => return false,
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }

            // Check if parent is a parameter and we're in its initializer
            if let Some(parent_node) = self.ctx.arena.get(parent_idx) {
                if parent_node.kind == syntax_kind_ext::PARAMETER
                    && let Some(param) = self.ctx.arena.get_parameter(parent_node)
                {
                    // Check if current node is within the initializer
                    if !param.initializer.is_none() {
                        let init_idx = param.initializer;
                        // Check if idx is within the initializer subtree
                        if self.is_node_within(idx, init_idx) {
                            return true;
                        }
                    }
                }
                // Stop at function/arrow boundaries - parameters are only at the top level
                if matches!(parent_node.kind,
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION ||
                         k == syntax_kind_ext::FUNCTION_EXPRESSION ||
                         k == syntax_kind_ext::ARROW_FUNCTION ||
                         k == syntax_kind_ext::METHOD_DECLARATION ||
                         k == syntax_kind_ext::CONSTRUCTOR ||
                         k == syntax_kind_ext::GET_ACCESSOR ||
                         k == syntax_kind_ext::SET_ACCESSOR
                ) {
                    return false;
                }
            }

            current = parent_idx;
        }
    }

    // =========================================================================
    // Definite Assignment Checking
    // =========================================================================

    /// Check if definite assignment checking should be skipped for a given type.
    /// TypeScript skips TS2454 when the declared type is `any`, `unknown`, or includes `undefined`.
    pub(crate) fn skip_definite_assignment_for_type(&self, declared_type: TypeId) -> bool {
        use crate::solver::TypeId;
        use crate::solver::type_contains_undefined;

        // Skip for any/unknown/error - these types allow uninitialized usage
        if declared_type == TypeId::ANY
            || declared_type == TypeId::UNKNOWN
            || declared_type == TypeId::ERROR
        {
            return true;
        }

        // Skip if the type includes undefined or void (uninitialized variables are undefined)
        type_contains_undefined(self.ctx.types, declared_type)
    }

    /// - Not in ambient contexts
    /// - Not in type-only positions
    pub(crate) fn should_check_definite_assignment(
        &mut self,
        sym_id: SymbolId,
        _idx: NodeIndex,
    ) -> bool {
        use crate::binder::symbol_flags;
        use crate::parser::node::NodeAccess;
        use crate::scanner::SyntaxKind;

        // Get the symbol
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Only check block-scoped variables (let/const)
        if (symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0 {
            return false;
        }

        // Get the value declaration
        let decl_id = symbol.value_declaration;
        if decl_id.is_none() {
            return false;
        }

        // Get the declaration node
        let Some(decl_node) = self.ctx.arena.get(decl_id) else {
            return false;
        };

        // Check if it's a variable declaration
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }

        // Get the variable declaration data
        let Some(var_data) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };

        // If there's an initializer, no need to check definite assignment
        if !var_data.initializer.is_none() {
            return false;
        }

        // If there's a definite assignment assertion (!), skip check
        if var_data.exclamation_token {
            return false;
        }

        // If the variable is declared in a for-in or for-of loop header,
        // it's always assigned by the loop iteration itself
        if let Some(decl_list_info) = self.ctx.arena.node_info(decl_id) {
            let decl_list_idx = decl_list_info.parent;
            if let Some(decl_list_node) = self.ctx.arena.get(decl_list_idx) {
                if decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                    if let Some(for_info) = self.ctx.arena.node_info(decl_list_idx) {
                        let for_idx = for_info.parent;
                        if let Some(for_node) = self.ctx.arena.get(for_idx) {
                            if for_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                                || for_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                            {
                                return false;
                            }
                        }
                    }
                }
            }
        }

        // Walk up the parent chain to check:
        // 1. Skip definite assignment checks in ambient declarations (declare const/let)
        // 2. Skip for module/global-level variables (TypeScript only checks function-local variables)
        let mut current = decl_id;
        let mut found_function_scope = false;
        for _ in 0..50 {
            let Some(info) = self.ctx.arena.node_info(current) else {
                break;
            };
            if let Some(node) = self.ctx.arena.get(current) {
                // Check for ambient declarations
                if node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                    || node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                {
                    if let Some(var_data) = self.ctx.arena.get_variable(node) {
                        if let Some(mods) = &var_data.modifiers {
                            for &mod_idx in &mods.nodes {
                                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                                    && mod_node.kind == SyntaxKind::DeclareKeyword as u16
                                {
                                    return false;
                                }
                            }
                        }
                    }
                }

                // Check if we're inside a function scope
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                    || node.kind == syntax_kind_ext::GET_ACCESSOR
                    || node.kind == syntax_kind_ext::SET_ACCESSOR
                {
                    found_function_scope = true;
                    break;
                }

                // If we reached the source file, this is a module-level variable
                if node.kind == syntax_kind_ext::SOURCE_FILE {
                    return false;
                }
            }

            current = info.parent;
            if current.is_none() {
                break;
            }
        }

        // Only check definite assignment for function-local variables
        if !found_function_scope {
            return false;
        }

        // Variable without initializer inside function scope - should be checked
        true
    }

    /// Check if a variable is definitely assigned at a given point.
    ///
    /// This performs flow-sensitive analysis to determine if a variable
    /// has been assigned on all code paths leading to the usage point.
    pub(crate) fn is_definitely_assigned_at(&self, idx: NodeIndex) -> bool {
        // Get the flow node for this identifier usage
        let flow_node = match self.ctx.binder.get_node_flow(idx) {
            Some(flow) => flow,
            None => return true, // No flow info - assume assigned to avoid false positives
        };

        // Create a flow analyzer and check definite assignment
        let analyzer = FlowAnalyzer::with_node_types(
            self.ctx.arena,
            self.ctx.binder,
            self.ctx.types,
            &self.ctx.node_types,
        )
        .with_flow_cache(&self.ctx.flow_analysis_cache)
        .with_type_environment(Rc::clone(&self.ctx.type_environment));

        analyzer.is_definitely_assigned(idx, flow_node)
    }

    // =========================================================================
    // Temporal Dead Zone (TDZ) Checking
    // =========================================================================

    /// Check if a variable is used before its declaration in a static block.
    ///
    /// This detects Temporal Dead Zone (TDZ) violations where a block-scoped variable
    /// is accessed inside a class static block before it has been declared in the source.
    ///
    /// # Example
    /// ```typescript
    /// class C {
    ///   static {
    ///     console.log(x); // Error: x used before declaration
    ///   }
    /// }
    /// let x = 1;
    /// ```
    pub(crate) fn is_variable_used_before_declaration_in_static_block(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        use crate::binder::symbol_flags;

        // 1. Get the symbol
        let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
            return false;
        };

        // 2. Check if it is a block-scoped variable (let, const, class, enum)
        // var and function are hoisted, so they don't have TDZ issues in this context.
        // Imports (ALIAS) are also hoisted or handled differently.
        let is_block_scoped = (symbol.flags
            & (symbol_flags::BLOCK_SCOPED_VARIABLE
                | symbol_flags::CLASS
                | symbol_flags::REGULAR_ENUM))
            != 0;

        if !is_block_scoped {
            return false;
        }

        // 3. Get the declaration node
        // Prefer value_declaration, fall back to first declaration
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };

        // 4. Check textual order: Usage must be textually before declaration
        // We ensure both nodes exist in the current arena
        let Some(usage_node) = self.ctx.arena.get(usage_idx) else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        // If usage is after declaration, it's valid
        if usage_node.pos >= decl_node.pos {
            return false;
        }

        // 5. Check if usage is inside a static block
        // Use find_enclosing_static_block which walks up the AST and stops at function boundaries.
        // This ensures we only catch immediate usage, not usage inside a closure/function
        // defined within the static block (which would execute later).
        if self.find_enclosing_static_block(usage_idx).is_some() {
            return true;
        }

        false
    }

    /// Check if a variable is used before its declaration in a computed property.
    ///
    /// Computed property names are evaluated before the property declaration,
    /// creating a TDZ for the class being declared.
    pub(crate) fn is_variable_used_before_declaration_in_computed_property(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        use crate::binder::symbol_flags;

        // 1. Get the symbol
        let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
            return false;
        };

        // 2. Check if it is a block-scoped variable (let, const, class, enum)
        let is_block_scoped = (symbol.flags
            & (symbol_flags::BLOCK_SCOPED_VARIABLE
                | symbol_flags::CLASS
                | symbol_flags::REGULAR_ENUM))
            != 0;

        if !is_block_scoped {
            return false;
        }

        // 3. Get the declaration node
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };

        // 4. Check textual order: Usage must be textually before declaration
        let Some(usage_node) = self.ctx.arena.get(usage_idx) else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        if usage_node.pos >= decl_node.pos {
            return false;
        }

        // 5. Check if usage is inside a computed property name
        if self.find_enclosing_computed_property(usage_idx).is_some() {
            return true;
        }

        false
    }

    /// Check if a variable is used before its declaration in a heritage clause.
    ///
    /// Heritage clauses (extends, implements) are evaluated before the class body,
    /// creating a TDZ for the class being declared.
    pub(crate) fn is_variable_used_before_declaration_in_heritage_clause(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        use crate::binder::symbol_flags;

        // 1. Get the symbol
        let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
            return false;
        };

        // 2. Check if it is a block-scoped variable (let, const, class, enum)
        let is_block_scoped = (symbol.flags
            & (symbol_flags::BLOCK_SCOPED_VARIABLE
                | symbol_flags::CLASS
                | symbol_flags::REGULAR_ENUM))
            != 0;

        if !is_block_scoped {
            return false;
        }

        // 3. Get the declaration node
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };

        // 4. Check textual order: Usage must be textually before declaration
        let Some(usage_node) = self.ctx.arena.get(usage_idx) else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        if usage_node.pos >= decl_node.pos {
            return false;
        }

        // 5. Check if usage is inside a heritage clause (extends/implements)
        if self.find_enclosing_heritage_clause(usage_idx).is_some() {
            return true;
        }

        false
    }

    // =========================================================================
    // Integration with Solver's Flow Analysis
    // =========================================================================

    /// Create a flow type evaluator that uses the solver's type operations.
    ///
    /// This provides a bridge between the checker's flow analysis and the
    /// solver's type narrowing capabilities.
    #[allow(dead_code)]
    pub(crate) fn create_flow_evaluator(&self) -> FlowTypeEvaluator<'_> {
        FlowTypeEvaluator::new(self.ctx.types)
    }
}
