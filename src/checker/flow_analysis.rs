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

use crate::binder::{SymbolId, symbol_flags};
use crate::checker::FlowAnalyzer;
use crate::checker::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use crate::checker::types::diagnostics::Diagnostic;
use crate::interner::Atom;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::TypeId;
use rustc_hash::FxHashSet;

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
            // Return empty normal flow to indicate properties are not definitely assigned
            return FlowResult {
                normal: None,
                exits: Some(assigned.clone()),
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
    /// Handles: [this.a, this.b] = arr
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
            // Handle nested destructuring
            else if let Some(elem_node) = self.ctx.arena.get(elem_idx) {
                if elem_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
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
                    // Check both sides of the binary expression
                    self.check_expression_for_early_property_access(bin.left, assigned, tracked);
                    self.check_expression_for_early_property_access(bin.right, assigned, tracked);
                    // If this is an assignment, track the assignment
                    if self.is_assignment_operator(bin.operator_token) {
                        self.track_assignment_in_expression(bin.left, assigned, tracked);
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
    pub(crate) fn apply_flow_narrowing(&self, idx: NodeIndex, declared_type: TypeId) -> TypeId {
        // Get the flow node for this identifier usage
        let flow_node = match self.ctx.binder.get_node_flow(idx) {
            Some(flow) => flow,
            None => return declared_type, // No flow info - use declared type
        };

        // Skip narrowing for non-union types (nothing to narrow)
        // Also skip for primitives that can't be narrowed further
        if !self.is_narrowable_type(declared_type) {
            return declared_type;
        }

        // Create a flow analyzer and apply narrowing
        let analyzer = FlowAnalyzer::with_node_types(
            self.ctx.arena,
            self.ctx.binder,
            self.ctx.types,
            &self.ctx.node_types,
        );

        analyzer.get_flow_type(idx, declared_type, flow_node)
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
    pub fn check_flow_usage(
        &mut self,
        idx: NodeIndex,
        declared_type: TypeId,
        sym_id: SymbolId,
    ) -> TypeId {
        // Check definite assignment for block-scoped variables without initializers
        if self.should_check_definite_assignment(sym_id, idx)
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
        // Get the variable name for the error message
        let name = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .map(|s| s.escaped_name.clone())
            .unwrap_or_else(|| "<unknown>".to_string());

        // Get the location for error reporting
        let Some(node) = self.ctx.arena.get(idx) else {
            // If the node doesn't exist in the arena, emit error with position 0
            self.ctx.diagnostics.push(Diagnostic::error(
                self.ctx.file_name.clone(),
                0,
                0,
                format!("Variable '{}' is used before being assigned", name),
                2454, // TS2454
            ));
            return;
        };
        let start = node.pos;
        let length = node.end - node.pos;

        self.ctx.diagnostics.push(Diagnostic::error(
            self.ctx.file_name.clone(),
            start,
            length,
            format!("Variable '{}' is used before being assigned", name),
            2454, // TS2454
        ));
    }

    /// Check if definite assignment analysis should be performed for a symbol.
    ///
    /// Definite assignment analysis ensures that block-scoped variables (let/const)
    /// are assigned before use.
    pub(crate) fn should_check_definite_assignment(
        &mut self,
        sym_id: SymbolId,
        idx: NodeIndex,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        if (symbol.flags & symbol_flags::VARIABLE) == 0 {
            return false;
        }
        // Only check block-scoped (let/const) variables for definite assignment
        // Function-scoped (var) variables do not require definite assignment
        if (symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0 {
            return false;
        }

        if self.symbol_is_parameter(sym_id) {
            return false;
        }

        if self.symbol_has_definite_assignment_assertion(sym_id) {
            return false;
        }

        if self.is_for_in_of_assignment_target(idx) {
            return false;
        }

        // Skip if the variable declaration has an initializer
        if self.symbol_has_initializer(sym_id) {
            return false;
        }

        // Skip if the variable is in an ambient context (declare var x: T)
        if self.symbol_is_in_ambient_context(sym_id) {
            return false;
        }

        // Skip if the variable is captured in a closure (used in a different function).
        // TypeScript doesn't check definite assignment for variables captured in non-IIFE closures
        // because the closure might be called later when the variable is assigned.
        if self.is_variable_captured_in_closure(sym_id, idx) {
            return false;
        }

        // Skip definite assignment check for variables whose types allow uninitialized use:
        // - Literal types: `let key: "a"` - the type restricts to a single literal
        // - Union of literals: `let key: "a" | "b"` - all possible values are literals
        // - Types with undefined: `let obj: Foo | undefined` - undefined is the default
        if self.symbol_type_allows_uninitialized(sym_id) {
            return false;
        }

        true
    }

    /// Check if a variable symbol is in an ambient context (declared with `declare`).
    fn symbol_is_in_ambient_context(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Quick check: if lib_contexts is not empty and symbol is not in main binder's arena,
        // it's likely from lib.d.ts which is all ambient
        if !self.ctx.lib_contexts.is_empty() {
            // Check if symbol exists in main binder's symbol arena
            let is_from_lib = self.ctx.binder.get_symbols().get(sym_id).is_none();
            if is_from_lib {
                // Symbol is from lib.d.ts, which is all ambient (declare statements)
                return true;
            }
        }

        for &decl_idx in &symbol.declarations {
            // Check if the variable statement has a declare modifier
            if let Some(var_stmt_idx) = self.find_enclosing_variable_statement(decl_idx)
                && let Some(var_stmt_node) = self.ctx.arena.get(var_stmt_idx)
                && let Some(var_stmt) = self.ctx.arena.get_variable(var_stmt_node)
                && self.has_declare_modifier(&var_stmt.modifiers)
            {
                return true;
            }

            // Also check node flags for AMBIENT
            if let Some(node) = self.ctx.arena.get(decl_idx)
                && (node.flags as u32) & crate::parser::node_flags::AMBIENT != 0
            {
                return true;
            }
        }

        false
    }

    /// Check if a variable is captured in a closure (used in a different function than its declaration).
    /// TypeScript does not check definite assignment for variables captured in non-IIFE closures.
    fn is_variable_captured_in_closure(&self, sym_id: SymbolId, usage_idx: NodeIndex) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Get the enclosing function for the usage
        let usage_function = self.find_enclosing_function(usage_idx);

        // Get the enclosing function for the variable's declaration
        for &decl_idx in &symbol.declarations {
            let decl_function = self.find_enclosing_function(decl_idx);

            // If the usage is in a different function than the declaration,
            // the variable is captured in a closure
            if usage_function != decl_function {
                return true;
            }
        }

        false
    }

    /// Check if a variable symbol can be used without initialization.
    fn symbol_type_allows_uninitialized(&mut self, sym_id: SymbolId) -> bool {
        use crate::solver::{SymbolRef, TypeKey};

        let declared_type = self.get_type_of_symbol(sym_id);

        // TypeScript doesn't check definite assignment for `any` typed variables
        if declared_type == TypeId::ANY {
            return true;
        }

        // Check if it's undefined type
        if declared_type == TypeId::UNDEFINED {
            return true;
        }

        let Some(type_key) = self.ctx.types.lookup(declared_type) else {
            return false;
        };

        // Handle TypeQuery (typeof x) - resolve the underlying type
        if let TypeKey::TypeQuery(SymbolRef(ref_sym_id)) = type_key {
            let resolved = self.get_type_of_symbol(SymbolId(ref_sym_id));
            // Check if resolved type allows uninitialized use
            if resolved == TypeId::UNDEFINED || resolved == TypeId::ANY {
                return true;
            }
            // Also check if the resolved type is a union containing undefined
            if self.union_contains(resolved, TypeId::UNDEFINED) {
                return true;
            }
        }

        // Check if it's a single literal type
        if matches!(type_key, TypeKey::Literal(_)) {
            return true;
        }

        // Check if it's a union
        if let TypeKey::Union(members) = type_key {
            let member_ids = self.ctx.types.type_list(members);

            // Union of only literal types - allowed without initialization
            let all_literals = member_ids.iter().all(|&member_id| {
                matches!(self.ctx.types.lookup(member_id), Some(TypeKey::Literal(_)))
            });
            if all_literals {
                return true;
            }

            // If union includes undefined, allowed without initialization
            if member_ids.contains(&TypeId::UNDEFINED) {
                return true;
            }
        }

        false
    }

    /// Check if a variable symbol's declaration has an initializer.
    fn symbol_has_initializer(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        for &decl_idx in &symbol.declarations {
            let Some(var_decl_idx) = self.find_enclosing_variable_declaration(decl_idx) else {
                continue;
            };
            let Some(var_decl_node) = self.ctx.arena.get(var_decl_idx) else {
                continue;
            };
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(var_decl_node) else {
                continue;
            };
            // Variable has an initializer - it's definitely assigned at declaration
            if !var_decl.initializer.is_none() {
                return true;
            }
        }

        false
    }

    /// Check if a variable is definitely assigned at a usage location.
    pub(crate) fn is_definitely_assigned_at(&self, idx: NodeIndex) -> bool {
        let flow_node = match self.ctx.binder.get_node_flow(idx) {
            Some(flow) => flow,
            None => return false, // No flow info means variable is not definitely assigned
        };
        let analyzer = FlowAnalyzer::new(self.ctx.arena, self.ctx.binder, self.ctx.types);
        analyzer.is_definitely_assigned(idx, flow_node)
    }

    /// Check if a symbol is a function parameter.
    fn symbol_is_parameter(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        symbol
            .declarations
            .iter()
            .any(|&decl_idx| self.node_is_or_within_kind(decl_idx, syntax_kind_ext::PARAMETER))
    }

    /// Check if a symbol has a definite assignment assertion (! modifier).
    fn symbol_has_definite_assignment_assertion(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        for &decl_idx in &symbol.declarations {
            let Some(var_decl_idx) = self.find_enclosing_variable_declaration(decl_idx) else {
                continue;
            };
            let Some(var_decl_node) = self.ctx.arena.get(var_decl_idx) else {
                continue;
            };
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(var_decl_node) else {
                continue;
            };
            if var_decl.exclamation_token {
                return true;
            }
        }

        false
    }

    /// Check if a node is the same kind as or within a parent of that kind.
    fn node_is_or_within_kind(&self, idx: NodeIndex, kind: u16) -> bool {
        let mut current = idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let node = match self.ctx.arena.get(current) {
                Some(node) => node,
                None => return false,
            };
            if node.kind == kind {
                return true;
            }
            let ext = match self.ctx.arena.get_extended(current) {
                Some(ext) => ext,
                None => return false,
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
        }
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
    // TDZ (Temporal Dead Zone) Checks
    // =========================================================================

    /// Check if a variable is used in a static block before its declaration (TDZ check).
    ///
    /// In TypeScript, if a variable is declared at module level AFTER a class declaration,
    /// using that variable inside the class's static block should emit TS2454.
    pub(crate) fn is_variable_used_before_declaration_in_static_block(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        // Check if we're inside a static block
        let Some(static_block_idx) = self.find_enclosing_static_block(usage_idx) else {
            return false;
        };

        // Get the class containing the static block
        let Some(class_idx) = self.find_class_for_static_block(static_block_idx) else {
            return false;
        };

        // Get the class position
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let class_pos = class_node.pos;

        // Get the symbol's declaration
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Check if the symbol is a module-level variable (not a class member)
        // We're looking for variables declared outside the class
        if (symbol.flags & symbol_flags::VARIABLE) == 0 {
            return false;
        }

        // Get the position of the variable's declaration
        for &decl_idx in &symbol.declarations {
            // Check if this is a variable declaration
            let Some(var_stmt_idx) = self.find_enclosing_variable_statement(decl_idx) else {
                continue;
            };
            let Some(var_stmt_node) = self.ctx.arena.get(var_stmt_idx) else {
                continue;
            };

            // Variable is declared AFTER the class - this is TDZ error
            if var_stmt_node.pos > class_pos {
                return true;
            }
        }

        false
    }

    /// Check if a variable is used in a computed property name before its declaration (TDZ check).
    pub(crate) fn is_variable_used_before_declaration_in_computed_property(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        // Check if we're inside a computed property name
        let Some(computed_idx) = self.find_enclosing_computed_property(usage_idx) else {
            return false;
        };

        // Get the class containing the computed property
        let Some(class_idx) = self.find_class_for_computed_property(computed_idx) else {
            return false;
        };

        // Get the class position
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let class_pos = class_node.pos;

        // Get the symbol's declaration
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Check if the symbol is a module-level variable (not a class member)
        if (symbol.flags & symbol_flags::VARIABLE) == 0 {
            return false;
        }

        // Get the position of the variable's declaration
        for &decl_idx in &symbol.declarations {
            // Check if this is a variable declaration
            let Some(var_stmt_idx) = self.find_enclosing_variable_statement(decl_idx) else {
                continue;
            };
            let Some(var_stmt_node) = self.ctx.arena.get(var_stmt_idx) else {
                continue;
            };

            // Variable is declared AFTER the class - this is TDZ error
            if var_stmt_node.pos > class_pos {
                return true;
            }
        }

        false
    }

    /// Check if a variable is used in an extends clause before its declaration (TDZ check).
    pub(crate) fn is_variable_used_before_declaration_in_heritage_clause(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        // Check if we're inside a heritage clause
        let Some(heritage_idx) = self.find_enclosing_heritage_clause(usage_idx) else {
            return false;
        };

        // Get the class/interface containing the heritage clause
        let Some(class_idx) = self.find_class_for_heritage_clause(heritage_idx) else {
            return false;
        };

        // Get the class position
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let class_pos = class_node.pos;

        // Get the symbol's declaration
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Check if the symbol is a module-level variable
        if (symbol.flags & symbol_flags::VARIABLE) == 0 {
            return false;
        }

        // Get the position of the variable's declaration
        for &decl_idx in &symbol.declarations {
            let Some(var_stmt_idx) = self.find_enclosing_variable_statement(decl_idx) else {
                continue;
            };
            let Some(var_stmt_node) = self.ctx.arena.get(var_stmt_idx) else {
                continue;
            };

            // Variable is declared AFTER the class - this is TDZ error
            if var_stmt_node.pos > class_pos {
                return true;
            }
        }

        false
    }

    // =========================================================================
    // Type Narrowing
    // =========================================================================

    /// Narrow a union type by a typeof guard.
    ///
    /// This handles `typeof x === "string"` style checks, narrowing the type
    /// to only include members that match the typeof result.
    pub fn narrow_by_typeof(&self, source: TypeId, typeof_result: &str) -> TypeId {
        use crate::solver::NarrowingContext;
        let ctx = NarrowingContext::new(self.ctx.types);
        ctx.narrow_by_typeof(source, typeof_result)
    }

    /// Narrow a union type by a typeof guard (negative case).
    ///
    /// This handles the negated typeof check (`typeof x !== "string"`), narrowing
    /// the type to exclude the typeof result.
    pub fn narrow_by_typeof_negation(&self, source: TypeId, typeof_result: &str) -> TypeId {
        use crate::solver::NarrowingContext;
        let ctx = NarrowingContext::new(self.ctx.types);

        // Get the target type for this typeof result
        let target = match typeof_result {
            "string" => TypeId::STRING,
            "number" => TypeId::NUMBER,
            "boolean" => TypeId::BOOLEAN,
            "bigint" => TypeId::BIGINT,
            "symbol" => TypeId::SYMBOL,
            "undefined" => TypeId::UNDEFINED,
            "object" => TypeId::OBJECT,
            "function" => return ctx.narrow_excluding_function(source),
            _ => return source,
        };

        ctx.narrow_excluding_type(source, target)
    }

    /// Narrow a discriminated union by a discriminant property check.
    ///
    /// This implements TypeScript's discriminated union narrowing, where a common
    /// property with literal values is used to distinguish between union variants.
    pub fn narrow_by_discriminant(
        &self,
        union_type: TypeId,
        property_name: Atom,
        literal_value: TypeId,
    ) -> TypeId {
        use crate::solver::NarrowingContext;
        let ctx = NarrowingContext::new(self.ctx.types);
        ctx.narrow_by_discriminant(union_type, property_name, literal_value)
    }

    /// Narrow a discriminated union by excluding a discriminant value (negative case).
    pub fn narrow_by_excluding_discriminant(
        &self,
        union_type: TypeId,
        property_name: Atom,
        excluded_value: TypeId,
    ) -> TypeId {
        use crate::solver::NarrowingContext;
        let ctx = NarrowingContext::new(self.ctx.types);
        ctx.narrow_by_excluding_discriminant(union_type, property_name, excluded_value)
    }

    /// Find discriminant properties in a union type.
    pub fn find_discriminants(&self, union_type: TypeId) -> Vec<crate::solver::DiscriminantInfo> {
        use crate::solver::NarrowingContext;
        let ctx = NarrowingContext::new(self.ctx.types);
        ctx.find_discriminants(union_type)
    }

    /// Narrow a type to include only members assignable to target.
    pub fn narrow_to_type(&self, source: TypeId, target: TypeId) -> TypeId {
        use crate::solver::NarrowingContext;
        let ctx = NarrowingContext::new(self.ctx.types);
        ctx.narrow_to_type(source, target)
    }

    /// Narrow a type to exclude members assignable to target.
    pub fn narrow_excluding_type(&self, source: TypeId, excluded: TypeId) -> TypeId {
        use crate::solver::NarrowingContext;
        let ctx = NarrowingContext::new(self.ctx.types);
        ctx.narrow_excluding_type(source, excluded)
    }
}
