//! Flow-based helpers for identifier type computation.
//!
//! Contains evolving-array diagnostics, implicit-any capture analysis,
//! assignment tracking, and flow-graph traversal utilities used during
//! identifier type resolution.

use crate::FlowAnalyzer;
use crate::context::PendingImplicitAnyKind;
use crate::query_boundaries::common as common_query;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::{FlowNodeId, SymbolId, flow_flags, symbol_flags};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn should_emit_pending_implicit_any_capture_diagnostic(
        &self,
        idx: NodeIndex,
        sym_id: SymbolId,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return true;
        };

        if (symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0 {
            return true;
        }

        if !self.has_non_initializer_assignment_for_reference(idx, sym_id) {
            return false;
        }

        if self.reference_is_before_last_assignment(idx, sym_id) {
            return true;
        }

        let Some(outer_flow) = self.captured_reference_outer_flow(idx) else {
            return true;
        };

        let analyzer = self.flow_analyzer();
        analyzer
            .reference_symbol_cache
            .borrow_mut()
            .insert(idx.0, Some(sym_id));
        self.ctx
            .flow_reference_match_cache
            .borrow_mut()
            .retain(|&(a, b), _| a != idx.0 && b != idx.0);

        !analyzer.is_definitely_assigned(idx, outer_flow)
    }

    fn reference_is_before_last_assignment(&self, idx: NodeIndex, sym_id: SymbolId) -> bool {
        let ref_pos = self.ctx.arena.get(idx).map(|node| node.pos).unwrap_or(0);
        if ref_pos == 0 {
            return false;
        }

        let last_assign_pos =
            if let Some(&pos) = self.ctx.symbol_last_assignment_pos.borrow().get(&sym_id) {
                pos
            } else {
                let analyzer = self.flow_analyzer();
                analyzer
                    .reference_symbol_cache
                    .borrow_mut()
                    .insert(idx.0, Some(sym_id));
                self.ctx
                    .flow_reference_match_cache
                    .borrow_mut()
                    .retain(|&(a, b), _| a != idx.0 && b != idx.0);

                let mut last_pos = 0;
                for i in 0..self.ctx.binder.flow_nodes.len() {
                    let flow_id = FlowNodeId(i as u32);
                    let Some(flow) = self.ctx.binder.flow_nodes.get(flow_id) else {
                        continue;
                    };
                    if !flow.has_any_flags(flow_flags::ASSIGNMENT) {
                        continue;
                    }

                    let Some(node) = self.ctx.arena.get(flow.node) else {
                        continue;
                    };
                    if matches!(
                        node.kind,
                        syntax_kind_ext::VARIABLE_DECLARATION
                            | syntax_kind_ext::VARIABLE_DECLARATION_LIST
                            | syntax_kind_ext::PARAMETER
                    ) {
                        continue;
                    }

                    if analyzer.assignment_targets_reference(flow.node, idx) {
                        last_pos = last_pos.max(node.pos);
                    }
                }

                self.ctx
                    .symbol_last_assignment_pos
                    .borrow_mut()
                    .insert(sym_id, last_pos);
                last_pos
            };

        last_assign_pos > ref_pos
    }

    pub(super) fn maybe_emit_pending_evolving_array_diagnostic(
        &mut self,
        idx: NodeIndex,
        sym_id: SymbolId,
        flow_type: TypeId,
    ) {
        // TS7005/TS7034 for evolving arrays only applies when noImplicitAny is enabled
        if !self.ctx.no_implicit_any() {
            return;
        }
        let pending = self.ctx.pending_implicit_any_vars.get(&sym_id).copied();
        let reported = self.ctx.reported_implicit_any_vars.get(&sym_id).copied();
        if pending.is_none() && reported.is_none() {
            return;
        }

        let array_kind = common_query::array_element_type(self.ctx.types, flow_type)
            .filter(|&elem| elem == TypeId::ANY)
            .and_then(|_| {
                let pending_kind = pending.map(|info| info.kind);
                if pending_kind == Some(PendingImplicitAnyKind::EvolvingArray)
                    || reported == Some(PendingImplicitAnyKind::EvolvingArray)
                    || self.symbol_has_direct_empty_array_initializer(sym_id)
                    || self.reference_has_reachable_empty_array_assignment(idx, sym_id)
                {
                    Some(PendingImplicitAnyKind::EvolvingArray)
                } else {
                    None
                }
            });
        if array_kind != Some(PendingImplicitAnyKind::EvolvingArray)
            || !self.should_emit_evolving_array_implicit_any_usage(idx)
        {
            return;
        }
        if self.is_same_function_scope_as_declaration(idx, sym_id)
            && self.reference_has_reachable_array_mutation(idx)
        {
            return;
        }
        // Skip TS7005/TS7034 for truthiness checks (if/while/do-while conditions)
        // of evolving arrays in same scope. tsc doesn't flag `if (arr)` as
        // implicit any usage — only actual value reads trigger the diagnostic.
        if self.is_same_function_scope_as_declaration(idx, sym_id)
            && self.is_condition_expression_only(idx)
        {
            return;
        }

        let Some(sym_name) = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .map(|sym| sym.escaped_name.clone())
        else {
            return;
        };

        use crate::diagnostics::diagnostic_codes;
        if let Some(pending) = pending {
            self.ctx.pending_implicit_any_vars.remove(&sym_id);
            self.ctx
                .reported_implicit_any_vars
                .insert(sym_id, PendingImplicitAnyKind::EvolvingArray);
            self.error_at_node_msg(
                pending.name_node,
                diagnostic_codes::VARIABLE_IMPLICITLY_HAS_TYPE_IN_SOME_LOCATIONS_WHERE_ITS_TYPE_CANNOT_BE_DETERMIN,
                &[&sym_name, "any[]"],
            );
        }
        self.error_at_node_msg(
            idx,
            diagnostic_codes::VARIABLE_IMPLICITLY_HAS_AN_TYPE,
            &[&sym_name, "any[]"],
        );
    }

    fn should_emit_evolving_array_implicit_any_usage(&self, idx: NodeIndex) -> bool {
        !self.is_in_type_query_position(idx)
            && !self.is_non_null_assertion_operand(idx)
            && !self.is_for_in_of_initializer(idx)
            && !self.is_destructuring_assignment_target(idx)
            && !self.is_simple_assignment_target(idx)
            && !self.is_identifier_array_mutation_receiver(idx)
            && !self.is_identifier_array_length_receiver(idx)
    }

    /// Check if the identifier is used only as a condition expression
    /// (in if/while/do-while/ternary), not as a value read.
    fn is_condition_expression_only(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::*;
        let Some(info) = self.ctx.arena.node_info(idx) else {
            return false;
        };
        let parent = info.parent;
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };
        matches!(
            parent_node.kind,
            IF_STATEMENT | WHILE_STATEMENT | DO_STATEMENT
        ) || (parent_node.kind == CONDITIONAL_EXPRESSION
            && self
                .ctx
                .arena
                .get_conditional_expr(parent_node)
                .is_some_and(|cond| cond.condition == idx))
    }

    fn is_simple_assignment_target(&self, idx: NodeIndex) -> bool {
        let Some(info) = self.ctx.arena.node_info(idx) else {
            return false;
        };
        let parent = info.parent;
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };

        if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.ctx.arena.get_binary_expr(parent_node)
        {
            return bin.left == idx && bin.operator_token == SyntaxKind::EqualsToken as u16;
        }

        if (parent_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || parent_node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
            && let Some(unary) = self.ctx.arena.get_unary_expr(parent_node)
        {
            return unary.operand == idx
                && (unary.operator == SyntaxKind::PlusPlusToken as u16
                    || unary.operator == SyntaxKind::MinusMinusToken as u16);
        }

        false
    }

    fn is_identifier_array_mutation_receiver(&self, idx: NodeIndex) -> bool {
        let Some(parent_info) = self.ctx.arena.node_info(idx) else {
            return false;
        };
        let parent = parent_info.parent;
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && parent_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(parent_node) else {
            return false;
        };
        if access.expression != idx || access.question_dot_token {
            return false;
        }
        let Some(grand_info) = self.ctx.arena.node_info(parent) else {
            return false;
        };
        let grand = grand_info.parent;
        let Some(grand_node) = self.ctx.arena.get(grand) else {
            return false;
        };
        // Method calls that mutate: x.push(), x.splice(), etc.
        if grand_node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.ctx.arena.get_call_expr(grand_node)
            && call.expression == parent
        {
            return matches!(
                self.identifier_member_name(access.name_or_argument),
                Some(
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
            );
        }
        // Element access assignments: x[0] = value
        if grand_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            && let Some(bin) = self.ctx.arena.get_binary_expr(grand_node)
            && bin.operator_token == SyntaxKind::EqualsToken as u16
            && bin.left == parent
        {
            return true;
        }
        false
    }

    fn is_identifier_array_length_receiver(&self, idx: NodeIndex) -> bool {
        let Some(parent_info) = self.ctx.arena.node_info(idx) else {
            return false;
        };
        let parent = parent_info.parent;
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && parent_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(parent_node) else {
            return false;
        };
        access.expression == idx
            && self.identifier_member_name(access.name_or_argument) == Some("length")
    }

    fn identifier_member_name(&self, idx: NodeIndex) -> Option<&str> {
        let node = self.ctx.arena.get(idx)?;
        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            return Some(ident.escaped_text.as_str());
        }
        let literal = self.ctx.arena.get_literal(node)?;
        (node.kind == SyntaxKind::StringLiteral as u16).then_some(literal.text.as_str())
    }

    fn symbol_has_direct_empty_array_initializer(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let value_decl = symbol.value_declaration;
        let Some(mut decl_node) = self.ctx.arena.get(value_decl) else {
            return false;
        };
        if decl_node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.ctx.arena.get_extended(value_decl)
            && ext.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
        {
            decl_node = parent_node;
        }
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }
        let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        decl.initializer.is_some() && self.node_is_empty_array_literal(decl.initializer)
    }

    pub(crate) fn is_same_function_scope_as_declaration(
        &self,
        idx: NodeIndex,
        sym_id: SymbolId,
    ) -> bool {
        let decl_node = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .and_then(|sym| sym.declarations.first().copied());
        self.find_enclosing_function(idx)
            == decl_node.and_then(|decl| self.find_enclosing_function(decl))
    }

    fn reference_has_reachable_empty_array_assignment(
        &self,
        idx: NodeIndex,
        sym_id: SymbolId,
    ) -> bool {
        let Some(flow_node) = self.flow_node_for_identifier_usage(idx) else {
            return false;
        };

        let analyzer = self.flow_analyzer();
        analyzer
            .reference_symbol_cache
            .borrow_mut()
            .insert(idx.0, Some(sym_id));
        self.ctx
            .flow_reference_match_cache
            .borrow_mut()
            .retain(|&(a, b), _| a != idx.0 && b != idx.0);

        let mut worklist = vec![flow_node];
        let mut visited = FxHashSet::default();
        while let Some(current) = worklist.pop() {
            if !visited.insert(current) {
                continue;
            }
            let Some(flow) = self.ctx.binder.flow_nodes.get(current) else {
                continue;
            };
            if flow.has_any_flags(flow_flags::ASSIGNMENT)
                && let Some(rhs) = analyzer.assignment_rhs_for_reference(flow.node, idx)
                && self.node_is_empty_array_literal(rhs)
            {
                return true;
            }
            for &antecedent in flow.antecedent.iter().rev() {
                if antecedent.is_some() {
                    worklist.push(antecedent);
                }
            }
        }

        false
    }

    fn reference_has_reachable_array_mutation(&self, idx: NodeIndex) -> bool {
        let Some(flow_node) = self.flow_node_for_identifier_usage(idx) else {
            return false;
        };

        let analyzer = self.flow_analyzer();
        let mut worklist = vec![flow_node];
        let mut visited = FxHashSet::default();
        while let Some(current) = worklist.pop() {
            if !visited.insert(current) {
                continue;
            }
            let Some(flow) = self.ctx.binder.flow_nodes.get(current) else {
                continue;
            };
            if flow.has_any_flags(flow_flags::ARRAY_MUTATION)
                && let Some(node) = self.ctx.arena.get(flow.node)
                && let Some(call) = self.ctx.arena.get_call_expr(node)
                && analyzer.array_mutation_affects_reference(call, idx)
            {
                return true;
            }
            for &antecedent in flow.antecedent.iter().rev() {
                if antecedent.is_some() {
                    worklist.push(antecedent);
                }
            }
        }

        false
    }

    fn node_is_empty_array_literal(&self, idx: NodeIndex) -> bool {
        self.ctx.arena.get(idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                && self
                    .ctx
                    .arena
                    .get_literal_expr(node)
                    .is_some_and(|lit| lit.elements.nodes.is_empty())
        })
    }

    fn has_non_initializer_assignment_for_reference(
        &self,
        idx: NodeIndex,
        sym_id: SymbolId,
    ) -> bool {
        let analyzer = self.flow_analyzer();
        analyzer
            .reference_symbol_cache
            .borrow_mut()
            .insert(idx.0, Some(sym_id));
        self.ctx
            .flow_reference_match_cache
            .borrow_mut()
            .retain(|&(a, b), _| a != idx.0 && b != idx.0);

        for i in 0..self.ctx.binder.flow_nodes.len() {
            let flow_id = FlowNodeId(i as u32);
            let Some(flow) = self.ctx.binder.flow_nodes.get(flow_id) else {
                continue;
            };
            if !flow.has_any_flags(flow_flags::ASSIGNMENT) {
                continue;
            }

            let Some(node) = self.ctx.arena.get(flow.node) else {
                continue;
            };
            if matches!(
                node.kind,
                syntax_kind_ext::VARIABLE_DECLARATION
                    | syntax_kind_ext::VARIABLE_DECLARATION_LIST
                    | syntax_kind_ext::PARAMETER
            ) {
                continue;
            }

            if analyzer.assignment_targets_reference(flow.node, idx) {
                return true;
            }
        }

        false
    }

    fn captured_reference_outer_flow(&self, idx: NodeIndex) -> Option<FlowNodeId> {
        let flow_node = self.flow_node_for_identifier_usage(idx)?;
        let mut worklist = vec![flow_node];
        let mut visited = FxHashSet::default();

        while let Some(current) = worklist.pop() {
            if !visited.insert(current) {
                continue;
            }

            let Some(flow) = self.ctx.binder.flow_nodes.get(current) else {
                continue;
            };
            if flow.has_any_flags(flow_flags::START) {
                return flow
                    .antecedent
                    .first()
                    .copied()
                    .filter(|flow| flow.is_some());
            }

            for &antecedent in flow.antecedent.iter().rev() {
                if antecedent.is_some() {
                    worklist.push(antecedent);
                }
            }
        }

        None
    }

    fn flow_node_for_identifier_usage(&self, idx: NodeIndex) -> Option<FlowNodeId> {
        if let Some(flow) = self.ctx.binder.get_node_flow(idx) {
            return Some(flow);
        }

        let mut current = self.ctx.arena.get_extended(idx).map(|ext| ext.parent);
        while let Some(parent) = current {
            if parent.is_none() {
                break;
            }
            if let Some(flow) = self.ctx.binder.get_node_flow(parent) {
                return Some(flow);
            }
            current = self.ctx.arena.get_extended(parent).map(|ext| ext.parent);
        }

        None
    }

    pub(super) fn flow_analyzer(&self) -> FlowAnalyzer<'_> {
        let analyzer = FlowAnalyzer::with_node_types(
            self.ctx.arena,
            self.ctx.binder,
            self.ctx.types,
            &self.ctx.node_types,
        )
        .with_flow_cache(&self.ctx.flow_analysis_cache)
        .with_switch_reference_cache(&self.ctx.flow_switch_reference_cache)
        .with_numeric_atom_cache(&self.ctx.flow_numeric_atom_cache)
        .with_reference_match_cache(&self.ctx.flow_reference_match_cache)
        .with_type_environment(&self.ctx.type_environment)
        .with_narrowing_cache(&self.ctx.narrowing_cache)
        .with_call_type_predicates(&self.ctx.call_type_predicates)
        .with_flow_buffers(
            &self.ctx.flow_worklist,
            &self.ctx.flow_in_worklist,
            &self.ctx.flow_visited,
            &self.ctx.flow_results,
        )
        .with_symbol_last_assignment_pos(&self.ctx.symbol_last_assignment_pos)
        .with_destructured_bindings(&self.ctx.destructured_bindings);

        if let Some(class_info) = &self.ctx.enclosing_class
            && let Some(instance_this_type) = class_info.cached_instance_this_type
        {
            return analyzer.with_concrete_this_type(instance_this_type);
        }

        analyzer
    }
}
