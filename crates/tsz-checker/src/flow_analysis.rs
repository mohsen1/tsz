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

use crate::FlowAnalyzer;
use crate::query_boundaries::flow_analysis::{
    are_types_mutually_subtype_with_env, object_shape_for_type, tuple_elements_for_type,
    union_members_for_type,
};
use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use rustc_hash::FxHashSet;
use std::rc::Rc;
use tsz_binder::{SymbolId, flow_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

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

    /// Analyze a constructor body starting after the `super()` call.
    ///
    /// In derived classes, properties can only be assigned after `super()` is called.
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

    /// Find the index of the first statement after the `super()` call.
    pub(crate) fn find_super_statement_start(&self, statements: &[NodeIndex]) -> Option<usize> {
        for (idx, &stmt_idx) in statements.iter().enumerate() {
            if self.is_super_call_statement(stmt_idx) {
                return Some(idx + 1);
            }
        }
        None
    }

    /// Convert a `FlowResult` to a set of definitely assigned properties.
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
        try_data: &tsz_parser::parser::node::TryData,
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
        switch_data: &tsz_parser::parser::node::SwitchData,
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
                    if let Some(bin) = self.ctx.arena.get_binary_expr(elem_node)
                        && self.is_assignment_operator(bin.operator_token)
                    {
                        self.collect_assignment_target(bin.left, assigned, tracked);
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

    /// Extract a `PropertyKey` from a property name node.
    pub(crate) fn property_key_from_name(&self, name_idx: NodeIndex) -> Option<PropertyKey> {
        let name_node = self.ctx.arena.get(name_idx)?;

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

    /// Extract a `PropertyKey` from a property access expression on `this`.
    pub(crate) fn property_key_from_access(&self, access_idx: NodeIndex) -> Option<PropertyKey> {
        let node = self.ctx.arena.get(access_idx)?;
        let access = self.ctx.arena.get_access_expr(node)?;
        let expr_node = self.ctx.arena.get(access.expression)?;
        if expr_node.kind != SyntaxKind::ThisKeyword as u16 {
            return None;
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let name_node = self.ctx.arena.get(access.name_or_argument)?;
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

    /// Extract a `ComputedKey` from an expression.
    fn computed_key_from_expression(&self, expr_idx: NodeIndex) -> Option<ComputedKey> {
        let expr_node = self.ctx.arena.get(expr_idx)?;

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
            return Some(ComputedKey::Ident(access_name));
        }

        None
    }

    /// Extract a qualified name from a property access expression.
    fn qualified_name_from_property_access(&self, access_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(access_idx)?;
        let access = self.ctx.arena.get_access_expr(node)?;

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

        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        let ident = self.ctx.arena.get_identifier(name_node)?;

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
                        use crate::diagnostics::format_message;
                        let property_name = self.get_property_name_from_key(&key);
                        self.error_at_node(
                            expr_idx,
                            &format_message(
                                crate::diagnostics::diagnostic_messages::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED,
                                &[&property_name],
                            ),
                            crate::diagnostics::diagnostic_codes::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED,
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
        // Skip flow narrowing when getting assignment target types.
        // For assignments like `foo[x] = 1` after `if (foo[x] === undefined)`,
        // we need the declared type (e.g., `number | undefined`) not the narrowed type (`undefined`).
        if self.ctx.skip_flow_narrowing {
            return declared_type;
        }

        // Get the flow node for this expression usage FIRST
        // If there's no flow info, no narrowing is possible regardless of node type
        let flow_node = if let Some(flow) = self.ctx.binder.get_node_flow(idx) {
            flow
        } else {
            // Some nodes in type positions (e.g. `typeof x` inside a type alias)
            // don't carry direct flow links. Fall back to the nearest parent that
            // has flow information so narrowing can still apply at that site.
            let mut current = self.ctx.arena.get_extended(idx).map(|ext| ext.parent);
            let mut found = None;
            while let Some(parent) = current {
                if parent.is_none() {
                    break;
                }
                if let Some(flow) = self.ctx.binder.get_node_flow(parent) {
                    found = Some(flow);
                    break;
                }
                current = self.ctx.arena.get_extended(parent).map(|ext| ext.parent);
            }
            match found {
                Some(flow) => flow,
                None => return declared_type, // No flow info - use declared type
            }
        };

        // Fast path: `any` and `error` types cannot be meaningfully narrowed.
        // NOTE: We only skip for direct `any`/`error`, NOT for compound types that
        // contain `any` (e.g. unions of classes with `any`-returning methods).
        // TypeScript narrows such compound types normally via instanceof/typeof.
        if declared_type == TypeId::ANY || declared_type == TypeId::ERROR {
            return declared_type;
        }

        // Rule #42 only applies inside closures. Avoid symbol resolution work
        // on the common non-closure path.
        if self.is_inside_closure()
            && let Some(sym_id) = self.get_symbol_for_identifier(idx)
            && self.is_captured_variable(sym_id, idx)
            && self.is_mutable_binding(sym_id)
        {
            // Rule #42: Reset narrowing for captured mutable bindings in closures
            // (const variables preserve narrowing, let/var reset to declared type)
            return declared_type;
        }

        // Skip narrowing for `never` — it's the bottom type, nothing to narrow.
        // All other types (unions, objects, callables, type params, primitives, etc.)
        // can benefit from flow narrowing (instanceof, typeof, truthiness, etc.).
        if declared_type == TypeId::NEVER {
            return declared_type;
        }

        // Create a flow analyzer and apply narrowing
        let analyzer = FlowAnalyzer::with_node_types(
            self.ctx.arena,
            self.ctx.binder,
            self.ctx.types,
            &self.ctx.node_types,
        )
        .with_flow_cache(&self.ctx.flow_analysis_cache)
        .with_switch_reference_cache(&self.ctx.flow_switch_reference_cache)
        .with_reference_match_cache(&self.ctx.flow_reference_match_cache)
        .with_type_environment(Rc::clone(&self.ctx.type_environment))
        .with_narrowing_cache(&self.ctx.narrowing_cache)
        .with_flow_buffers(
            &self.ctx.flow_worklist,
            &self.ctx.flow_in_worklist,
            &self.ctx.flow_visited,
            &self.ctx.flow_results,
        );

        let narrowed = analyzer.get_flow_type(idx, declared_type, flow_node);

        // Correlated narrowing for destructured bindings.
        // When `const { data, isSuccess } = useQuery()` and we check `isSuccess`,
        // narrowing of `isSuccess` should also narrow `data`.
        if let Some(sym_id) = self.get_symbol_for_identifier(idx)
            && let Some(info) = self.ctx.destructured_bindings.get(&sym_id).cloned()
            && info.is_const
        {
            return self.apply_correlated_narrowing(&analyzer, sym_id, &info, narrowed, flow_node);
        }

        narrowed
    }

    /// Apply correlated narrowing for destructured bindings.
    ///
    /// When `const { data, isSuccess } = useQuery()` returns a union type,
    /// and `isSuccess` is narrowed (e.g. via truthiness check in `if (isSuccess)`),
    /// this function narrows the source union type and re-derives `data`'s type.
    fn apply_correlated_narrowing(
        &self,
        analyzer: &FlowAnalyzer<'_>,
        sym_id: SymbolId,
        info: &crate::context::DestructuredBindingInfo,
        declared_type: TypeId,
        flow_node: tsz_binder::FlowNodeId,
    ) -> TypeId {
        let Some(source_members) = union_members_for_type(self.ctx.types, info.source_type) else {
            return declared_type;
        };

        // Find all siblings in the same binding group
        let siblings: Vec<_> = self
            .ctx
            .destructured_bindings
            .iter()
            .filter(|(s, i)| **s != sym_id && i.group_id == info.group_id && i.is_const)
            .map(|(s, i)| (*s, i.clone()))
            .collect();

        if siblings.is_empty() {
            return declared_type;
        }

        // Start with the full source type members
        let source_member_count = source_members.len();
        let mut remaining_members = source_members;
        let member_binding_type =
            |member: TypeId, binding: &crate::context::DestructuredBindingInfo| -> Option<TypeId> {
                if !binding.property_name.is_empty() {
                    let mut current = member;
                    for segment in binding.property_name.split('.') {
                        let shape = object_shape_for_type(self.ctx.types, current)?;
                        let prop = shape.properties.iter().find(|p| {
                            self.ctx.types.resolve_atom_ref(p.name).as_ref() == segment
                        })?;
                        current = prop.type_id;
                    }
                    Some(current)
                } else if let Some(elems) = tuple_elements_for_type(self.ctx.types, member) {
                    elems.get(binding.element_index as usize).map(|e| e.type_id)
                } else {
                    None
                }
            };
        let symbol_identifier_ref = |sym: SymbolId| -> Option<NodeIndex> {
            let mut declaration_ident: Option<NodeIndex> = None;
            for (&node_id, &node_sym) in &self.ctx.binder.node_symbols {
                if node_sym != sym {
                    continue;
                }
                let idx = NodeIndex(node_id);
                let Some(node) = self.ctx.arena.get(idx) else {
                    continue;
                };
                if node.kind != SyntaxKind::Identifier as u16 {
                    continue;
                }

                // Prefer a usage site over declaration identifier nodes in binding/variable/parameter
                // declarations, because usage nodes carry richer flow facts (e.g. switch discriminants).
                let is_declaration_ident = self
                    .ctx
                    .arena
                    .get_extended(idx)
                    .and_then(|ext| self.ctx.arena.get(ext.parent))
                    .is_some_and(|parent| {
                        parent.kind == syntax_kind_ext::BINDING_ELEMENT
                            || parent.kind == syntax_kind_ext::VARIABLE_DECLARATION
                            || parent.kind == syntax_kind_ext::PARAMETER
                    });

                if !is_declaration_ident {
                    return Some(idx);
                }
                declaration_ident = Some(idx);
            }
            declaration_ident
        };
        let switch_flow_node = {
            let mut candidate = flow_node;
            let mut found = None;
            // Walk a short antecedent chain to recover switch-clause context for
            // nodes immediately after a clause (e.g. statements in default block).
            for _ in 0..4 {
                let Some(flow) = self.ctx.binder.flow_nodes.get(candidate) else {
                    break;
                };
                if flow.has_any_flags(flow_flags::SWITCH_CLAUSE) {
                    found = Some(candidate);
                    break;
                }
                let Some(&ant) = flow.antecedent.first() else {
                    break;
                };
                if ant.is_none() {
                    break;
                }
                candidate = ant;
            }
            found
        };
        let switch_clause_context = switch_flow_node
            .and_then(|switch_flow_id| self.ctx.binder.flow_nodes.get(switch_flow_id))
            .filter(|flow| flow.has_any_flags(flow_flags::SWITCH_CLAUSE))
            .and_then(|flow| {
                let clause_idx = flow.node;
                let is_implicit_default = self
                    .ctx
                    .arena
                    .get(clause_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::CASE_BLOCK);
                let switch_idx = if is_implicit_default {
                    self.ctx
                        .arena
                        .get_extended(clause_idx)
                        .and_then(|ext| (!ext.parent.is_none()).then_some(ext.parent))
                } else {
                    self.ctx.binder.get_switch_for_clause(clause_idx)
                }?;
                let switch_node = self.ctx.arena.get(switch_idx)?;
                let switch_data = self.ctx.arena.get_switch(switch_node)?;
                let switch_sym = self
                    .ctx
                    .binder
                    .resolve_identifier(self.ctx.arena, switch_data.expression)?;

                let collect_case_types = |case_block: NodeIndex| -> Vec<TypeId> {
                    let Some(case_block_node) = self.ctx.arena.get(case_block) else {
                        return Vec::new();
                    };
                    let Some(block) = self.ctx.arena.get_block(case_block_node) else {
                        return Vec::new();
                    };
                    block
                        .statements
                        .nodes
                        .iter()
                        .filter_map(|&case_clause_idx| {
                            let clause_node = self.ctx.arena.get(case_clause_idx)?;
                            let clause = self.ctx.arena.get_case_clause(clause_node)?;
                            if clause.expression.is_none() {
                                return None;
                            }
                            self.ctx.node_types.get(&clause.expression.0).copied()
                        })
                        .collect()
                };

                if is_implicit_default {
                    Some((switch_sym, None, collect_case_types(switch_data.case_block)))
                } else {
                    let clause_node = self.ctx.arena.get(clause_idx)?;
                    let clause = self.ctx.arena.get_case_clause(clause_node)?;
                    if clause.expression.is_none() {
                        Some((switch_sym, None, collect_case_types(switch_data.case_block)))
                    } else {
                        Some((
                            switch_sym,
                            self.ctx.node_types.get(&clause.expression.0).copied(),
                            Vec::new(),
                        ))
                    }
                }
            });

        // For each sibling, check if it's been narrowed
        for (sib_sym, sib_info) in &siblings {
            if let Some((switch_sym, case_type, default_case_types)) = &switch_clause_context
                && *switch_sym == *sib_sym
            {
                if let Some(case_ty) = *case_type {
                    remaining_members.retain(|&member| {
                        if let Some(prop_type) = member_binding_type(member, sib_info) {
                            prop_type == case_ty || {
                                let env = self.ctx.type_env.borrow();
                                are_types_mutually_subtype_with_env(
                                    self.ctx.types,
                                    &env,
                                    case_ty,
                                    prop_type,
                                    self.ctx.strict_null_checks(),
                                )
                            }
                        } else {
                            true
                        }
                    });
                } else if !default_case_types.is_empty() {
                    remaining_members.retain(|&member| {
                        let Some(prop_type) = member_binding_type(member, sib_info) else {
                            return true;
                        };
                        !default_case_types.iter().any(|&case_ty| {
                            prop_type == case_ty || {
                                let env = self.ctx.type_env.borrow();
                                are_types_mutually_subtype_with_env(
                                    self.ctx.types,
                                    &env,
                                    case_ty,
                                    prop_type,
                                    self.ctx.strict_null_checks(),
                                )
                            }
                        })
                    });
                }
                continue;
            }

            // Get the sibling's initial type (from the union source)
            let sib_initial = if let Some(&cached) = self.ctx.symbol_types.get(sib_sym) {
                cached
            } else {
                continue;
            };

            // Get the sibling's reference node (value_declaration)
            let Some(sib_sym_data) = self.ctx.binder.symbols.get(*sib_sym) else {
                continue;
            };
            let mut sib_ref = sib_sym_data.value_declaration;
            if sib_ref.is_none() {
                continue;
            }
            // Flow analysis expects an expression/identifier reference node. For destructured
            // symbols the declaration is often a BindingElement; use its identifier name node.
            if let Some(decl_node) = self.ctx.arena.get(sib_ref)
                && decl_node.kind == syntax_kind_ext::BINDING_ELEMENT
                && let Some(binding) = self.ctx.arena.get_binding_element(decl_node)
                && let Some(name_node) = self.ctx.arena.get(binding.name)
                && name_node.kind == SyntaxKind::Identifier as u16
            {
                sib_ref = binding.name;
            }

            // Get the sibling's narrowed type at this flow node
            let mut sib_narrowed = analyzer.get_flow_type(sib_ref, sib_initial, flow_node);
            if sib_narrowed == sib_initial
                && let Some(identifier_ref) = symbol_identifier_ref(*sib_sym)
                && identifier_ref != sib_ref
            {
                sib_narrowed = analyzer.get_flow_type(identifier_ref, sib_initial, flow_node);
            }

            // If the sibling wasn't narrowed, skip
            if sib_narrowed == sib_initial {
                continue;
            }

            remaining_members.retain(|&member| {
                let member_prop_type = member_binding_type(member, sib_info);

                if let Some(prop_type) = member_prop_type {
                    // Keep this member if the sibling's narrowed type overlaps
                    // with the member's property type
                    prop_type == sib_narrowed || {
                        let env = self.ctx.type_env.borrow();
                        are_types_mutually_subtype_with_env(
                            self.ctx.types,
                            &env,
                            sib_narrowed,
                            prop_type,
                            self.ctx.strict_null_checks(),
                        )
                    }
                } else {
                    true // Keep if we can't determine
                }
            });
        }

        // If no members were filtered, no correlated narrowing happened
        if remaining_members.len() == source_member_count {
            return declared_type;
        }

        // If all members were filtered, return never
        if remaining_members.is_empty() {
            return TypeId::NEVER;
        }

        // Re-derive this symbol's property type from the remaining source members
        let mut result_types = Vec::new();
        for member in &remaining_members {
            let member_prop_type = if !info.property_name.is_empty() {
                let mut current = *member;
                let mut resolved = Some(current);
                for segment in info.property_name.split('.') {
                    resolved = object_shape_for_type(self.ctx.types, current).and_then(|shape| {
                        shape
                            .properties
                            .iter()
                            .find(|p| self.ctx.types.resolve_atom_ref(p.name).as_ref() == segment)
                            .map(|p| p.type_id)
                    });
                    if let Some(next) = resolved {
                        current = next;
                    } else {
                        break;
                    }
                }
                resolved
            } else if let Some(elems) = tuple_elements_for_type(self.ctx.types, *member) {
                elems.get(info.element_index as usize).map(|e| e.type_id)
            } else {
                None
            };

            if let Some(ty) = member_prop_type {
                result_types.push(ty);
            }
        }

        if result_types.is_empty() {
            return declared_type;
        }
        tsz_solver::utils::union_or_single(self.ctx.types, result_types)
    }

    /// Get the symbol for an identifier node.
    ///
    /// Returns None if the node is not an identifier or has no symbol.
    fn get_symbol_for_identifier(&self, idx: NodeIndex) -> Option<SymbolId> {
        use tsz_scanner::SyntaxKind;

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
    const fn is_inside_closure(&self) -> bool {
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
    /// 2. Check if it's a `VariableDeclaration`
    /// 3. Look at the parent `VariableDeclarationList`'s `NodeFlags`
    /// 4. If CONST flag is set → const (immutable)
    /// 5. Otherwise → let/var (mutable)
    ///
    /// Returns true for let/var (mutable), false for const (immutable).
    fn is_mutable_binding(&self, sym_id: SymbolId) -> bool {
        use tsz_parser::parser::node_flags;
        use tsz_parser::parser::syntax_kind_ext;

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
            if let Some(ext) = self.ctx.arena.get_extended(decl_idx)
                && !ext.parent.is_none()
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            {
                let flags = parent_node.flags as u32;
                let is_const = (flags & node_flags::CONST) != 0;
                return !is_const; // Return true if NOT const (i.e., let or var)
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
    fn is_captured_variable(&self, sym_id: SymbolId, reference: NodeIndex) -> bool {
        let symbol = match self.ctx.binder.get_symbol(sym_id) {
            Some(sym) => sym,
            None => return false,
        };

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
            None => return false,
        };

        // Find the enclosing scope of the usage site (where the variable is accessed).
        let usage_scope_id = match self
            .ctx
            .binder
            .find_enclosing_scope(self.ctx.arena, reference)
        {
            Some(scope_id) => scope_id,
            None => return false,
        };

        // If declared and used in the same scope, not captured
        if decl_scope_id == usage_scope_id {
            return false;
        }

        // A variable is "captured" only if it crosses a function boundary.
        // Block scopes (if, while, for) within the same function don't count.
        // We walk up from the declaration scope and usage scope to find
        // their enclosing function/source-file scopes, then compare those.
        let decl_fn_scope = self.find_enclosing_function_scope(decl_scope_id);
        let usage_fn_scope = self.find_enclosing_function_scope(usage_scope_id);

        // If both are in the same function scope, the variable is NOT captured
        if decl_fn_scope == usage_fn_scope {
            return false;
        }

        // The declaration's function scope must be an ancestor of the usage's function scope
        // for the variable to be considered captured
        let mut scope_id = usage_fn_scope;
        let mut iterations = 0;
        while !scope_id.is_none() && iterations < MAX_TREE_WALK_ITERATIONS {
            if scope_id == decl_fn_scope {
                return true;
            }

            scope_id = self
                .ctx
                .binder
                .scopes
                .get(scope_id.0 as usize)
                .map_or(tsz_binder::ScopeId::NONE, |scope| scope.parent);

            iterations += 1;
        }

        false
    }

    /// Walk up the scope chain to find the nearest function/source-file/module scope.
    /// Block scopes are skipped.
    fn find_enclosing_function_scope(&self, scope_id: tsz_binder::ScopeId) -> tsz_binder::ScopeId {
        use tsz_binder::ContainerKind;

        let mut current = scope_id;
        let mut iterations = 0;
        while !current.is_none() && iterations < MAX_TREE_WALK_ITERATIONS {
            if let Some(scope) = self.ctx.binder.scopes.get(current.0 as usize) {
                match scope.kind {
                    ContainerKind::Function | ContainerKind::SourceFile | ContainerKind::Module => {
                        return current;
                    }
                    _ => {
                        current = scope.parent;
                    }
                }
            } else {
                break;
            }
            iterations += 1;
        }
        current
    }
}
