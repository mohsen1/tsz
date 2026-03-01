//! Variable declaration utilities and definite assignment traversal for `FlowAnalyzer`.
//!
//! Extracted from the main `control_flow` module to keep it focused on the core
//! flow-type narrowing algorithm. This module provides:
//!
//! - **Definite assignment**: worklist-based graph traversal (`check_definite_assignment`)
//! - **Variable declaration inspection**: type annotation presence, mutability, destructuring

use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::{FlowNodeId, flow_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use super::FlowAnalyzer;

impl<'a> FlowAnalyzer<'a> {
    /// Iterative flow graph traversal for definite assignment checks.
    ///
    /// This replaces the recursive implementation to prevent stack overflow
    /// on deeply nested control flow structures. Uses a worklist algorithm with
    /// fixed-point iteration to determine if a variable is definitely assigned.
    pub(crate) fn check_definite_assignment(
        &self,
        reference: NodeIndex,
        flow_id: FlowNodeId,
        _visited: &mut Vec<FlowNodeId>,
        cache: &mut FxHashMap<FlowNodeId, bool>,
    ) -> bool {
        // Helper: Add a node to the worklist if not already present
        let add_to_worklist =
            |node: FlowNodeId,
             worklist: &mut Vec<FlowNodeId>,
             in_worklist: &mut FxHashSet<FlowNodeId>| {
                if !in_worklist.contains(&node) {
                    worklist.push(node);
                    in_worklist.insert(node);
                }
            };

        // Result cache: flow_id -> is_assigned
        // We use a local cache that we'll merge into the provided cache
        let mut local_cache: FxHashMap<FlowNodeId, bool> = FxHashMap::default();

        // Worklist for processing nodes
        let mut worklist: Vec<FlowNodeId> = vec![flow_id];
        let mut in_worklist: FxHashSet<FlowNodeId> = FxHashSet::default();
        in_worklist.insert(flow_id);

        // Track nodes that are waiting for their antecedents to be computed
        // Map: node -> set of antecedents it's waiting for
        let mut waiting_for: FxHashMap<FlowNodeId, FxHashSet<FlowNodeId>> = FxHashMap::default();

        while let Some(current_flow) = worklist.pop() {
            in_worklist.remove(&current_flow);

            // Skip if we already have a result
            if local_cache.contains_key(&current_flow) {
                continue;
            }

            let Some(flow) = self.binder.flow_nodes.get(current_flow) else {
                // Flow node doesn't exist - mark as assigned
                local_cache.insert(current_flow, true);
                // Notify any nodes waiting for this one
                let ready: Vec<_> = waiting_for
                    .iter()
                    .filter(|(_, ants)| ants.contains(&current_flow))
                    .map(|(&node, _)| node)
                    .collect();
                for node in ready {
                    waiting_for.remove(&node);
                    add_to_worklist(node, &mut worklist, &mut in_worklist);
                }
                continue;
            };

            // Compute the result based on flow node type
            let result = if flow.has_any_flags(flow_flags::UNREACHABLE) {
                false
            } else if flow.has_any_flags(flow_flags::ASSIGNMENT) {
                if self.assignment_targets_reference(flow.node, reference)
                    && !self.is_compound_read_write_assignment(flow.node)
                {
                    // Simple assignment (x = value) counts as definite assignment.
                    // Compound assignments (x += 1, ++x, x--) do NOT — they read
                    // the variable first, so tsc considers the variable still
                    // "used before being assigned" even after the compound write.
                    true
                } else if let Some(&ant) = flow.antecedent.first() {
                    if let Some(&ant_result) = local_cache.get(&ant) {
                        ant_result
                    } else {
                        // Add antecedent to worklist and defer
                        add_to_worklist(ant, &mut worklist, &mut in_worklist);
                        waiting_for.entry(current_flow).or_default().insert(ant);
                        continue;
                    }
                } else {
                    false
                }
            } else if flow.has_any_flags(flow_flags::BRANCH_LABEL) {
                if flow.antecedent.is_empty() {
                    false
                } else {
                    // Check if all antecedents have results
                    let mut all_ready = true;
                    let mut results = Vec::new();

                    for &ant in &flow.antecedent {
                        if let Some(ant_node) = self.binder.flow_nodes.get(ant)
                            && ant_node.has_any_flags(flow_flags::UNREACHABLE)
                        {
                            // Unreachable branches satisfy the condition vacuously
                            results.push(true);
                            continue;
                        }

                        if let Some(&ant_result) = local_cache.get(&ant) {
                            results.push(ant_result);
                        } else {
                            all_ready = false;
                            add_to_worklist(ant, &mut worklist, &mut in_worklist);
                            waiting_for.entry(current_flow).or_default().insert(ant);
                        }
                    }

                    if !all_ready {
                        continue;
                    }

                    // All antecedents processed - compute result (all must be true)
                    results.iter().all(|&r| r)
                }
            } else if flow.has_any_flags(flow_flags::LOOP_LABEL | flow_flags::CONDITION) {
                // typeof/instanceof guards prove the variable has a value in the
                // "positive sense" branch. For `===`/`==` that's TRUE_CONDITION;
                // for `!==`/`!=` that's FALSE_CONDITION (double negative → positive).
                // instanceof is always positive in TRUE_CONDITION.
                if self.condition_proves_assignment(flow, reference) {
                    true
                } else if let Some(&ant) = flow.antecedent.first() {
                    if let Some(&ant_result) = local_cache.get(&ant) {
                        ant_result
                    } else {
                        add_to_worklist(ant, &mut worklist, &mut in_worklist);
                        waiting_for.entry(current_flow).or_default().insert(ant);
                        continue;
                    }
                } else {
                    false
                }
            } else if flow.has_any_flags(flow_flags::SWITCH_CLAUSE) {
                if flow.antecedent.is_empty() {
                    false
                } else {
                    // Similar to BRANCH_LABEL - check all antecedents
                    let mut all_ready = true;
                    let mut results = Vec::new();

                    for &ant in &flow.antecedent {
                        if let Some(ant_node) = self.binder.flow_nodes.get(ant)
                            && ant_node.has_any_flags(flow_flags::UNREACHABLE)
                        {
                            results.push(true);
                            continue;
                        }

                        if let Some(&ant_result) = local_cache.get(&ant) {
                            results.push(ant_result);
                        } else {
                            all_ready = false;
                            add_to_worklist(ant, &mut worklist, &mut in_worklist);
                            waiting_for.entry(current_flow).or_default().insert(ant);
                        }
                    }

                    if !all_ready {
                        continue;
                    }

                    results.iter().all(|&r| r)
                }
            } else if flow.has_any_flags(flow_flags::START) {
                false
            } else if let Some(&ant) = flow.antecedent.first() {
                if let Some(&ant_result) = local_cache.get(&ant) {
                    ant_result
                } else {
                    add_to_worklist(ant, &mut worklist, &mut in_worklist);
                    waiting_for.entry(current_flow).or_default().insert(ant);
                    continue;
                }
            } else {
                false
            };

            // Store the result
            local_cache.insert(current_flow, result);

            // Notify any nodes waiting for this one
            let ready: Vec<_> = waiting_for
                .iter()
                .filter(|(_, ants)| ants.contains(&current_flow))
                .map(|(&node, _)| node)
                .collect();
            for node in ready {
                waiting_for.remove(&node);
                add_to_worklist(node, &mut worklist, &mut in_worklist);
            }
        }

        // Get the final result
        let final_result = *local_cache.get(&flow_id).unwrap_or(&false);

        // Merge local cache into the provided cache
        cache.extend(local_cache);

        final_result
    }

    /// Check if a CONDITION flow node proves the reference variable is assigned.
    ///
    /// typeof/instanceof guards prove a variable has a value in the "positive
    /// sense" branch:
    /// - `typeof x === "string"` + `TRUE_CONDITION` → x is assigned (typeof returned "string")
    /// - `typeof x !== "string"` + `FALSE_CONDITION` → x is assigned (double negative)
    /// - `typeof x !== "string"` + `TRUE_CONDITION` → x might be uninitialized
    /// - `x instanceof C` + `TRUE_CONDITION` → x is assigned
    fn condition_proves_assignment(
        &self,
        flow: &tsz_binder::FlowNode,
        reference: NodeIndex,
    ) -> bool {
        let is_true_condition = flow.has_any_flags(flow_flags::TRUE_CONDITION)
            && !flow.has_any_flags(flow_flags::FALSE_CONDITION);
        let is_false_condition = flow.has_any_flags(flow_flags::FALSE_CONDITION)
            && !flow.has_any_flags(flow_flags::TRUE_CONDITION);

        if !is_true_condition && !is_false_condition {
            return false;
        }

        self.expr_proves_assignment(flow.node, is_true_condition, is_false_condition, reference)
    }

    /// Check if an expression node (possibly compound via `&&`/`||`) proves
    /// that `reference` has been assigned a value.
    fn expr_proves_assignment(
        &self,
        condition: NodeIndex,
        is_true_condition: bool,
        is_false_condition: bool,
        reference: NodeIndex,
    ) -> bool {
        let Some(node_data) = self.arena.get(condition) else {
            return false;
        };

        // Call expression in TRUE_CONDITION: if any argument is the reference,
        // the variable was evaluated (passed as an argument), proving assignment.
        // This handles user-defined type predicates like `isFoo(value)`.
        if node_data.kind == syntax_kind_ext::CALL_EXPRESSION && is_true_condition {
            if let Some(call) = self.arena.get_call_expr(node_data)
                && let Some(args) = &call.arguments
            {
                for &arg in &args.nodes {
                    if self.is_matching_reference(arg, reference) {
                        return true;
                    }
                }
            }
            return false;
        }

        if node_data.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }

        let Some(bin) = self.arena.get_binary_expr(node_data) else {
            return false;
        };

        // `&&`: TRUE_CONDITION means both operands are true, so either
        // proving assignment is sufficient.
        if bin.operator_token == SyntaxKind::AmpersandAmpersandToken as u16 && is_true_condition {
            return self.expr_proves_assignment(bin.left, true, false, reference)
                || self.expr_proves_assignment(bin.right, true, false, reference);
        }

        // `||`: FALSE_CONDITION means both operands are false, so either
        // proving assignment (in negative sense) is sufficient.
        if bin.operator_token == SyntaxKind::BarBarToken as u16 && is_false_condition {
            return self.expr_proves_assignment(bin.left, false, true, reference)
                || self.expr_proves_assignment(bin.right, false, true, reference);
        }

        // instanceof: `x instanceof C` → TRUE_CONDITION proves assignment
        if bin.operator_token == SyntaxKind::InstanceOfKeyword as u16 {
            return is_true_condition && self.is_matching_reference(bin.left, reference);
        }

        // typeof: check operator polarity
        let is_positive_equality = bin.operator_token == SyntaxKind::EqualsEqualsEqualsToken as u16
            || bin.operator_token == SyntaxKind::EqualsEqualsToken as u16;
        let is_negative_equality = bin.operator_token
            == SyntaxKind::ExclamationEqualsEqualsToken as u16
            || bin.operator_token == SyntaxKind::ExclamationEqualsToken as u16;

        if !is_positive_equality && !is_negative_equality {
            return false;
        }

        // Determine if this is the "positive sense" branch:
        // - `=== "type"` + TRUE_CONDITION → positive
        // - `!== "type"` + FALSE_CONDITION → positive (double negative)
        let is_positive_sense = (is_positive_equality && is_true_condition)
            || (is_negative_equality && is_false_condition);

        if !is_positive_sense {
            return false;
        }

        // Check if either side is a typeof expression targeting the reference
        if let Some(typeof_operand) = self.get_typeof_operand(bin.left) {
            return self.is_matching_reference(typeof_operand, reference);
        }
        if let Some(typeof_operand) = self.get_typeof_operand(bin.right) {
            return self.is_matching_reference(typeof_operand, reference);
        }

        false
    }

    /// Check if an assignment node is a compound read-write operation.
    ///
    /// Compound read-write operations (`++x`, `x--`, `x += 1`, `x **= 2`, etc.)
    /// read the variable before writing it. For definite assignment analysis,
    /// these do NOT count as "definitely assigning" the variable — tsc still
    /// reports TS2454 for uses after a compound assignment if the variable was
    /// never properly initialized with `=`.
    pub(crate) fn is_compound_read_write_assignment(&self, node: NodeIndex) -> bool {
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };

        // Prefix/postfix ++/-- (e.g., `++x`, `x--`)
        if (node_data.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node_data.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
            && let Some(unary) = self.arena.get_unary_expr(node_data)
        {
            return unary.operator == tsz_scanner::SyntaxKind::PlusPlusToken as u16
                || unary.operator == tsz_scanner::SyntaxKind::MinusMinusToken as u16;
        }

        // Compound assignment operators (+=, -=, *=, /=, %=, **=, <<=, >>=, >>>=,
        // &=, |=, ^=, &&=, ||=, ??=)
        if node_data.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(node_data)
        {
            use tsz_scanner::SyntaxKind;
            let op = bin.operator_token;
            return op == SyntaxKind::PlusEqualsToken as u16
                || op == SyntaxKind::MinusEqualsToken as u16
                || op == SyntaxKind::AsteriskEqualsToken as u16
                || op == SyntaxKind::SlashEqualsToken as u16
                || op == SyntaxKind::PercentEqualsToken as u16
                || op == SyntaxKind::AsteriskAsteriskEqualsToken as u16
                || op == SyntaxKind::LessThanLessThanEqualsToken as u16
                || op == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || op == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
                || op == SyntaxKind::AmpersandEqualsToken as u16
                || op == SyntaxKind::BarEqualsToken as u16
                || op == SyntaxKind::CaretEqualsToken as u16
                || op == SyntaxKind::BarBarEqualsToken as u16
                || op == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || op == SyntaxKind::QuestionQuestionEqualsToken as u16;
        }

        false
    }

    /// Check if an assignment node is a mutable variable declaration (let/var) without a type annotation.
    /// Used to determine when literal types should be widened to their base types.
    pub(crate) fn is_mutable_var_decl_without_annotation(&self, node: NodeIndex) -> bool {
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };

        // Handle VARIABLE_DECLARATION directly
        if node_data.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let Some(decl) = self.arena.get_variable_declaration(node_data) else {
                return false;
            };
            // If there's a type annotation, don't widen - the user specified the type
            if decl.type_annotation.is_some() {
                return false;
            }
            // Check if the parent declaration list is let/var (not const)
            return !self.arena.is_const_variable_declaration(node);
        }

        // Handle VARIABLE_DECLARATION_LIST or VARIABLE_STATEMENT: check flags on the list
        if node_data.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            || node_data.kind == syntax_kind_ext::VARIABLE_STATEMENT
        {
            use tsz_parser::parser::node_flags;
            let flags = node_data.flags as u32;
            if (flags & node_flags::CONST) != 0 {
                return false;
            }
            // Check individual declarations for type annotations
            if let Some(list) = self.arena.get_variable(node_data) {
                for &decl_idx in &list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        && decl.type_annotation.is_none()
                    {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Check if an assignment flow node is a variable declaration with a type annotation.
    ///
    /// When a variable has an explicit type annotation, the flow analysis should
    /// use the declared type (not the initializer's structural type) for non-literal
    /// assignments. This prevents the initializer's type from overriding the declared
    /// type in the flow graph.
    pub(crate) fn is_var_decl_with_type_annotation(&self, node: NodeIndex) -> bool {
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };

        if node_data.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(decl) = self.arena.get_variable_declaration(node_data)
        {
            return decl.type_annotation.is_some();
        }

        // Handle VARIABLE_DECLARATION_LIST or VARIABLE_STATEMENT
        if (node_data.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            || node_data.kind == syntax_kind_ext::VARIABLE_STATEMENT)
            && let Some(list) = self.arena.get_variable(node_data)
        {
            for &decl_idx in &list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    && decl.type_annotation.is_some()
                {
                    return true;
                }
            }
        }

        false
    }

    /// Get the declared annotation type for a variable declaration node, if available.
    ///
    /// Returns `Some(type_id)` when `assignment_node` is a `VARIABLE_DECLARATION` with a
    /// type annotation whose type has already been computed and cached in `node_types`.
    /// Returns `None` otherwise (no annotation, wrong node kind, or not cached yet).
    ///
    /// The type annotation node index is used as the cache key (not the declaration node),
    /// matching how `get_type_from_type_node` caches in `node_types`.
    pub(crate) fn annotation_type_from_var_decl_node(
        &self,
        assignment_node: NodeIndex,
    ) -> Option<TypeId> {
        let decl_data = self.arena.get(assignment_node)?;
        if decl_data.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.arena.get_variable_declaration(decl_data)?;
        if var_decl.type_annotation.is_none() {
            return None;
        }
        let node_types = self.node_types?;
        node_types.get(&var_decl.type_annotation.0).copied()
    }

    /// Check if an assignment node represents a destructuring assignment.
    /// Destructuring assignments widen literals to primitives, unlike direct assignments.
    pub(crate) fn is_destructuring_assignment(&self, node: NodeIndex) -> bool {
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };

        match node_data.kind {
            syntax_kind_ext::BINARY_EXPRESSION => {
                let Some(bin) = self.arena.get_binary_expr(node_data) else {
                    return false;
                };
                // Check if left side is a binding pattern OR array/object literal (for destructuring)
                let left_is_binding = self.is_binding_pattern(bin.left);
                let left_is_literal = self.contains_destructuring_pattern(bin.left);
                left_is_binding || left_is_literal
            }
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let Some(decl) = self.arena.get_variable_declaration(node_data) else {
                    return false;
                };
                // Check if name is a binding pattern (destructuring in variable declaration)
                self.is_binding_pattern(decl.name)
            }
            _ => false,
        }
    }

    /// Check if an assignment node is a logical assignment (&&=, ||=, ??=).
    pub(crate) fn is_logical_assignment(&self, node: NodeIndex) -> bool {
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };
        if node_data.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(bin) = self.arena.get_binary_expr(node_data) else {
            return false;
        };
        bin.operator_token == SyntaxKind::AmpersandAmpersandEqualsToken as u16
            || bin.operator_token == SyntaxKind::BarBarEqualsToken as u16
            || bin.operator_token == SyntaxKind::QuestionQuestionEqualsToken as u16
    }

    /// Check if a node is a binding pattern (array or object destructuring pattern)
    fn is_binding_pattern(&self, node: NodeIndex) -> bool {
        self.arena.get(node).is_some_and(|n| n.is_binding_pattern())
    }

    /// Check if a node contains a destructuring pattern (array/object literal with binding elements).
    /// This handles cases like `[x] = [1]` where the left side is an array literal containing binding patterns.
    ///
    /// Note: In TypeScript, if an array or object literal appears on the left side of an assignment,
    /// it's ALWAYS a destructuring pattern, regardless of what elements it contains.
    fn contains_destructuring_pattern(&self, node: NodeIndex) -> bool {
        if node.is_none() {
            return false;
        }
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };

        // If this is an array or object literal, it's a destructuring pattern when on the left side of an assignment
        matches!(
            node_data.kind,
            syntax_kind_ext::ARRAY_LITERAL_EXPRESSION | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        )
    }
}
