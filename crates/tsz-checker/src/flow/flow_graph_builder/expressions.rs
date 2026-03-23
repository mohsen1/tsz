//! Expression-level flow graph construction for `FlowGraphBuilder`.
//!
//! Extracted from the main `flow_graph_builder` module to keep it focused on
//! statement-level control flow construction. This module provides:
//!
//! - **Variable tracking**: assignment and declaration flow nodes
//! - **Suspension points**: await/yield expression traversal
//! - **Assignment expressions**: short-circuit, binary, call, and access
//!   expression traversal for narrowing invalidation
//! - **Array mutation**: detection of known mutating array methods
//! - **Class declaration flow**: heritage clauses, static blocks/fields,
//!   computed property names

use tsz_binder::flow_flags;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

use super::FlowGraphBuilder;

impl<'a> FlowGraphBuilder<'a> {
    // =============================================================================
    // Variable Tracking
    // =============================================================================

    /// Track a variable declaration at the current flow point.
    ///
    /// Variable declarations are tracked to support:
    /// - Definite assignment analysis
    /// - Temporal dead zone (TDZ) checking
    /// - Variable scope tracking
    ///
    /// # Arguments
    /// * `var_decl` - The variable declaration node
    /// * `decl_node` - The AST node index of the declaration
    pub(super) fn track_variable_declaration(
        &mut self,
        var_decl: &tsz_parser::parser::node::VariableDeclarationData,
        decl_node: NodeIndex,
    ) {
        // Record the flow node at the declaration point
        self.record_node_flow(decl_node);

        // If the variable has an initializer, track it as an assignment
        if var_decl.initializer.is_some() {
            self.track_assignment(decl_node);
        }
    }

    /// Track an assignment at the current flow point.
    ///
    /// Assignments affect the definite assignment state of variables.
    ///
    /// # Arguments
    /// * `target` - The AST node being assigned to
    pub(super) fn track_assignment(&mut self, target: NodeIndex) {
        // Create an assignment flow node
        let flow = self.create_flow_node(flow_flags::ASSIGNMENT, self.current_flow, target);
        self.current_flow = flow;
    }

    // =============================================================================
    // Suspension Point Handling (await/yield)
    // =============================================================================

    /// Recursively traverse an expression to find and handle await/yield expressions.
    pub(super) fn handle_expression_for_suspension_points(&mut self, expr_idx: NodeIndex) {
        let Some(node) = self.arena.get(expr_idx) else {
            return;
        };

        // Robustness: if we're analyzing inside an async function context but the parser represented
        // `await` as an identifier (e.g., due to recovery or when the analysis context is injected),
        // still treat it as an await suspension point.
        if self.in_async_function()
            && node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.arena.get_identifier(node)
            && ident.escaped_text == "await"
        {
            self.handle_await_expression(expr_idx);
            return;
        }

        // Check if this is an await expression
        if node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
            self.handle_await_expression(expr_idx);
            // Also check the operand of the await expression
            if let Some(unary_data) = self.arena.get_unary_expr_ex(node) {
                self.handle_expression_for_suspension_points(unary_data.expression);
            }
            return;
        }

        // Check if this is a yield expression
        if node.kind == syntax_kind_ext::YIELD_EXPRESSION {
            self.handle_yield_expression(expr_idx);
            // Also check the operand of the yield expression (stored as UnaryExprDataEx)
            if let Some(unary_data) = self.arena.get_unary_expr_ex(node)
                && unary_data.expression.is_some()
            {
                self.handle_expression_for_suspension_points(unary_data.expression);
            }
            return;
        }

        // Recursively check child expressions based on node kind
        match node.kind {
            syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(binary) = self.arena.get_binary_expr(node) {
                    self.handle_expression_for_suspension_points(binary.left);
                    self.handle_expression_for_suspension_points(binary.right);
                }
            }
            syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.arena.get_conditional_expr(node) {
                    self.handle_expression_for_suspension_points(cond.condition);
                    self.handle_expression_for_suspension_points(cond.when_true);
                    self.handle_expression_for_suspension_points(cond.when_false);
                }
            }
            syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    self.handle_expression_for_suspension_points(call.expression);
                    if let Some(args) = &call.arguments {
                        for &arg in &args.nodes {
                            if arg.is_some() {
                                self.handle_expression_for_suspension_points(arg);
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.handle_expression_for_suspension_points(access.expression);
                    if access.name_or_argument.is_some() {
                        self.handle_expression_for_suspension_points(access.name_or_argument);
                    }
                }
            }
            _ => {
                // For other expression types, don't descend further for now
                // This could be extended to handle more cases as needed
            }
        }
    }

    /// Handle an await expression by creating an `AWAIT_POINT` flow node.
    pub(super) fn handle_await_expression(&mut self, await_node: NodeIndex) {
        if self.in_async_function() {
            // Create an AWAIT_POINT flow node to track this suspension point
            let await_point =
                self.create_flow_node(flow_flags::AWAIT_POINT, self.current_flow, await_node);
            self.current_flow = await_point;
        }
        // If not in async function, this is a semantic error but we still continue flow analysis
    }

    /// Handle a yield expression by creating a `YIELD_POINT` flow node.
    pub(super) fn handle_yield_expression(&mut self, yield_node: NodeIndex) {
        if self.in_generator_function() {
            // Create a YIELD_POINT flow node to track this suspension point
            let yield_point =
                self.create_flow_node(flow_flags::YIELD_POINT, self.current_flow, yield_node);
            self.current_flow = yield_point;
        }
        // If not in generator function, this is a semantic error but we still continue flow analysis
    }

    // =============================================================================
    // Assignment Expression Handling
    // =============================================================================

    /// Recursively traverse an expression to create ASSIGNMENT flow nodes.
    ///
    /// This is used for definite assignment and narrowing invalidation logic.
    pub(super) fn handle_expression_for_assignments(&mut self, expr_idx: NodeIndex) {
        let Some(node) = self.arena.get(expr_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::BINARY_EXPRESSION => {
                let Some(binary) = self.arena.get_binary_expr(node) else {
                    return;
                };

                // Check for short-circuit operators (&&, ||, ??)
                let is_short_circuit = binary.operator_token
                    == tsz_scanner::SyntaxKind::AmpersandAmpersandToken as u16
                    || binary.operator_token == tsz_scanner::SyntaxKind::BarBarToken as u16
                    || binary.operator_token
                        == tsz_scanner::SyntaxKind::QuestionQuestionToken as u16;

                if is_short_circuit {
                    // Handle short-circuit expressions with proper flow branching
                    // Bind left operand and save the flow after it
                    self.handle_expression_for_assignments(binary.left);
                    let after_left_flow = self.current_flow;

                    if binary.operator_token
                        == tsz_scanner::SyntaxKind::AmpersandAmpersandToken as u16
                    {
                        // For &&: right side is only evaluated when left is truthy
                        let true_condition = self.create_flow_node(
                            flow_flags::TRUE_CONDITION,
                            after_left_flow,
                            binary.left,
                        );
                        self.current_flow = true_condition;
                        self.handle_expression_for_assignments(binary.right);
                        let after_right_flow = self.current_flow;

                        // Short-circuit path: left is falsy, right is not evaluated
                        let false_condition = self.create_flow_node(
                            flow_flags::FALSE_CONDITION,
                            after_left_flow,
                            binary.left,
                        );

                        // Merge both paths
                        let merge = self.graph.nodes.alloc(flow_flags::BRANCH_LABEL);
                        self.add_antecedent(merge, after_right_flow);
                        self.add_antecedent(merge, false_condition);
                        self.current_flow = merge;
                    } else {
                        // For || and ??: right side is only evaluated when left is falsy/nullish
                        let false_condition = self.create_flow_node(
                            flow_flags::FALSE_CONDITION,
                            after_left_flow,
                            binary.left,
                        );
                        self.current_flow = false_condition;
                        self.handle_expression_for_assignments(binary.right);
                        let after_right_flow = self.current_flow;

                        // Short-circuit path: left is truthy, right is not evaluated
                        let true_condition = self.create_flow_node(
                            flow_flags::TRUE_CONDITION,
                            after_left_flow,
                            binary.left,
                        );

                        // Merge both paths
                        let merge = self.graph.nodes.alloc(flow_flags::BRANCH_LABEL);
                        self.add_antecedent(merge, after_right_flow);
                        self.add_antecedent(merge, true_condition);
                        self.current_flow = merge;
                    }
                } else {
                    // Regular binary expression
                    if Self::is_assignment_operator_token(binary.operator_token) {
                        let flow = self.create_flow_node(
                            flow_flags::ASSIGNMENT,
                            self.current_flow,
                            expr_idx,
                        );
                        self.current_flow = flow;
                    }

                    self.handle_expression_for_assignments(binary.left);
                    self.handle_expression_for_assignments(binary.right);
                }
            }
            syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.arena.get_conditional_expr(node) {
                    self.handle_expression_for_assignments(cond.condition);
                    self.handle_expression_for_assignments(cond.when_true);
                    self.handle_expression_for_assignments(cond.when_false);
                }
            }
            syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    self.handle_expression_for_assignments(call.expression);
                    if let Some(args) = &call.arguments {
                        for &arg in &args.nodes {
                            if arg.is_some() {
                                self.handle_expression_for_assignments(arg);
                            }
                        }
                    }

                    // Check for array mutation and create appropriate flow node
                    if self.is_array_mutation_call(expr_idx) {
                        let flow = self.create_flow_array_mutation(expr_idx);
                        self.current_flow = flow;
                    }
                }
            }
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.handle_expression_for_assignments(access.expression);
                    if access.name_or_argument.is_some() {
                        self.handle_expression_for_assignments(access.name_or_argument);
                    }
                }
            }
            syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                // Unwrap parenthesized expressions so that assignments inside
                // parentheses are visible to flow analysis.
                // e.g., `d ?? (d = x ?? "x")` — the inner `d = x ?? "x"` must
                // create an ASSIGNMENT flow node for `d`.
                if let Some(inner) = self.arena.get_parenthesized(node) {
                    self.handle_expression_for_assignments(inner.expression);
                }
            }
            _ => {}
        }
    }

    pub(super) const fn is_assignment_operator_token(operator_token: u16) -> bool {
        matches!(
            operator_token,
            x if x == SyntaxKind::EqualsToken as u16
                || x == SyntaxKind::PlusEqualsToken as u16
                || x == SyntaxKind::MinusEqualsToken as u16
                || x == SyntaxKind::AsteriskEqualsToken as u16
                || x == SyntaxKind::SlashEqualsToken as u16
                || x == SyntaxKind::PercentEqualsToken as u16
                || x == SyntaxKind::AsteriskAsteriskEqualsToken as u16
                || x == SyntaxKind::LessThanLessThanEqualsToken as u16
                || x == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || x == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
                || x == SyntaxKind::AmpersandEqualsToken as u16
                || x == SyntaxKind::CaretEqualsToken as u16
                || x == SyntaxKind::BarEqualsToken as u16
                || x == SyntaxKind::BarBarEqualsToken as u16
                || x == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || x == SyntaxKind::QuestionQuestionEqualsToken as u16
        )
    }

    // =============================================================================
    // Array Mutation Detection
    // =============================================================================

    /// Check if a call expression is a known array mutation method.
    pub(super) fn is_array_mutation_call(&self, call_idx: NodeIndex) -> bool {
        let Some(call_node) = self.arena.get(call_idx) else {
            return false;
        };
        let Some(call) = self.arena.get_call_expr(call_node) else {
            return false;
        };
        let Some(callee_node) = self.arena.get(call.expression) else {
            return false;
        };
        let Some(access) = self.arena.get_access_expr(callee_node) else {
            return false;
        };

        // Optional chains (?.) do not definitely mutate
        if access.question_dot_token {
            return false;
        }

        let Some(name_node) = self.arena.get(access.name_or_argument) else {
            return false;
        };

        let name = if let Some(ident) = self.arena.get_identifier(name_node) {
            ident.escaped_text.as_str()
        } else if let Some(literal) = self.arena.get_literal(name_node) {
            if name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16 {
                literal.text.as_str()
            } else {
                return false;
            }
        } else {
            return false;
        };

        matches!(
            name,
            "copyWithin"
                | "fill"
                | "pop"
                | "push"
                | "reverse"
                | "shift"
                | "sort"
                | "splice"
                | "unshift"
        )
    }

    /// Create a flow node for array mutation.
    pub(super) fn create_flow_array_mutation(
        &mut self,
        call_idx: NodeIndex,
    ) -> tsz_binder::FlowNodeId {
        let id = self.graph.nodes.alloc(flow_flags::ARRAY_MUTATION);
        if let Some(node) = self.graph.nodes.get_mut(id) {
            node.node = call_idx;
            if self.current_flow.is_some() && self.current_flow != self.graph.unreachable_flow {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    // =============================================================================
    // Class Declaration Flow (CFA for classes)
    // =============================================================================

    /// Build flow graph for a class declaration.
    ///
    /// Class declarations have control flow for:
    /// - Heritage clause expressions (extends expression executes first)
    /// - Static blocks (execute during class definition)
    /// - Static field initializers (execute during class definition)
    /// - Computed property names (execute during evaluation)
    pub(super) fn build_class_declaration(&mut self, idx: NodeIndex) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(node) else {
            return;
        };

        // 1. Heritage clauses (extends expression executes first)
        if let Some(heritage_clauses) = &class.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                if clause_idx.is_none() {
                    continue;
                }
                if let Some(clause_node) = self.arena.get(clause_idx)
                    && let Some(heritage) = self.arena.get_heritage_clause(clause_node)
                {
                    // For 'extends', the expression is evaluated
                    if heritage.token == SyntaxKind::ExtendsKeyword as u16 {
                        for &type_idx in &heritage.types.nodes {
                            if type_idx.is_none() {
                                continue;
                            }
                            if let Some(expr_with_type) = self.arena.get(type_idx)
                                && let Some(data) = self.arena.get_expr_type_args(expr_with_type)
                            {
                                // The extends expression is evaluated at class definition time
                                self.handle_expression_for_suspension_points(data.expression);
                            }
                        }
                    }
                }
            }
        }

        // Clone members to avoid borrow issues
        let members = class.members.nodes.clone();

        // 2. Class members (static fields and blocks execute during class definition)
        // Note: Instance fields execute during construction, but for top-level flow
        // we only care about static side effects. Instance fields are handled
        // when checking the constructor.
        for &member_idx in &members {
            if member_idx.is_none() {
                continue;
            }
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.arena.get_property_decl(member_node) {
                        let prop_name = prop.name;
                        let prop_initializer = prop.initializer;
                        let prop_modifiers = prop.modifiers.clone();

                        // Computed property name executes
                        if let Some(name_node) = self.arena.get(prop_name)
                            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                            && let Some(computed) = self.arena.get_computed_property(name_node)
                        {
                            self.handle_expression_for_suspension_points(computed.expression);
                        }

                        // Static initializer executes
                        if self
                            .arena
                            .has_modifier(&prop_modifiers, SyntaxKind::StaticKeyword)
                            && prop_initializer.is_some()
                        {
                            self.handle_expression_for_suspension_points(prop_initializer);
                            // Track assignment for static fields
                            let flow = self.create_flow_node(
                                flow_flags::ASSIGNMENT,
                                self.current_flow,
                                member_idx,
                            );
                            self.current_flow = flow;
                        }
                    }
                }
                k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                    // Static block executes immediately during class definition
                    if let Some(block) = self.arena.get_block(member_node) {
                        self.build_block(block);
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    // Computed property name executes
                    if let Some(method) = self.arena.get_method_decl(member_node) {
                        let method_name = method.name;
                        if let Some(name_node) = self.arena.get(method_name)
                            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                            && let Some(computed) = self.arena.get_computed_property(name_node)
                        {
                            self.handle_expression_for_suspension_points(computed.expression);
                        }
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    // Computed property name executes
                    if let Some(accessor) = self.arena.get_accessor(member_node) {
                        let accessor_name = accessor.name;
                        if let Some(name_node) = self.arena.get(accessor_name)
                            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                            && let Some(computed) = self.arena.get_computed_property(name_node)
                        {
                            self.handle_expression_for_suspension_points(computed.expression);
                        }
                    }
                }
                _ => {}
            }
        }

        self.record_node_flow(idx);
    }
}
