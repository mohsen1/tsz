//! Flow-producing statement binding helpers.

use crate::state::BinderState;
use crate::{ContainerKind, flow_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl BinderState {
    pub(crate) fn bind_if_statement(&mut self, arena: &NodeArena, idx: NodeIndex) {
        self.record_flow(idx);
        let Some(node) = arena.get(idx) else {
            return;
        };
        let Some(if_stmt) = arena.get_if_statement(node) else {
            return;
        };

        use tracing::trace;

        self.bind_expression(arena, if_stmt.expression);

        let pre_condition_flow = self.current_flow;
        trace!(
            pre_condition_flow = pre_condition_flow.0,
            "if statement: pre_condition_flow",
        );

        let true_flow = self.create_flow_condition(
            flow_flags::TRUE_CONDITION,
            pre_condition_flow,
            if_stmt.expression,
        );
        trace!(
            true_flow = true_flow.0,
            "if statement: created TRUE_CONDITION flow",
        );

        self.current_flow = true_flow;
        trace!("if statement: binding then branch with TRUE_CONDITION flow");
        self.bind_node(arena, if_stmt.then_statement);
        let after_then_flow = self.current_flow;
        trace!(
            after_then_flow = after_then_flow.0,
            "if statement: after_then_flow",
        );

        let after_else_flow = if if_stmt.else_statement.is_none() {
            self.create_flow_condition(
                flow_flags::FALSE_CONDITION,
                pre_condition_flow,
                if_stmt.expression,
            )
        } else {
            let false_flow = self.create_flow_condition(
                flow_flags::FALSE_CONDITION,
                pre_condition_flow,
                if_stmt.expression,
            );
            trace!(
                false_flow = false_flow.0,
                "if statement: created FALSE_CONDITION flow",
            );

            self.current_flow = false_flow;
            trace!("if statement: binding else branch with FALSE_CONDITION flow");
            self.bind_node(arena, if_stmt.else_statement);
            let result = self.current_flow;
            trace!(result = result.0, "if statement: after_else_flow",);
            result
        };

        let merge_label = self.create_branch_label();
        trace!(
            merge_label = merge_label.0,
            "if statement: created merge label",
        );
        self.add_antecedent(merge_label, after_then_flow);
        self.add_antecedent(merge_label, after_else_flow);
        self.current_flow = merge_label;
    }

    pub(crate) fn bind_while_or_do_statement(&mut self, arena: &NodeArena, node: &Node) {
        let Some(loop_data) = arena.get_loop(node) else {
            return;
        };

        let loop_label = self.create_loop_label();
        if self.current_flow.is_some() {
            self.add_antecedent(loop_label, self.current_flow);
        }
        self.current_flow = loop_label;

        let post_loop = self.create_branch_label();
        self.break_targets.push(post_loop);
        self.continue_targets.push(loop_label);

        if node.kind == syntax_kind_ext::DO_STATEMENT {
            self.bind_node(arena, loop_data.statement);
            self.bind_expression(arena, loop_data.condition);

            let pre_condition_flow = self.current_flow;
            let true_flow = self.create_flow_condition(
                flow_flags::TRUE_CONDITION,
                pre_condition_flow,
                loop_data.condition,
            );
            self.add_antecedent(loop_label, true_flow);

            if !Self::is_syntactically_true_condition(arena, loop_data.condition) {
                let false_flow = self.create_flow_condition(
                    flow_flags::FALSE_CONDITION,
                    pre_condition_flow,
                    loop_data.condition,
                );
                self.add_antecedent(post_loop, pre_condition_flow);
                self.add_antecedent(post_loop, false_flow);
            }
        } else {
            self.bind_expression(arena, loop_data.condition);

            let pre_condition_flow = self.current_flow;
            let true_flow = self.create_flow_condition(
                flow_flags::TRUE_CONDITION,
                pre_condition_flow,
                loop_data.condition,
            );
            self.current_flow = true_flow;
            self.bind_node(arena, loop_data.statement);
            self.add_antecedent(loop_label, self.current_flow);

            if !Self::is_syntactically_true_condition(arena, loop_data.condition) {
                let false_flow = self.create_flow_condition(
                    flow_flags::FALSE_CONDITION,
                    pre_condition_flow,
                    loop_data.condition,
                );
                self.add_antecedent(post_loop, false_flow);
            }
        }

        self.break_targets.pop();
        self.continue_targets.pop();
        self.current_flow = post_loop;
    }

    pub(crate) fn bind_for_statement(&mut self, arena: &NodeArena, node: &Node, idx: NodeIndex) {
        self.record_flow(idx);
        let Some(loop_data) = arena.get_loop(node) else {
            return;
        };

        self.enter_scope(ContainerKind::Block, idx);
        self.bind_node(arena, loop_data.initializer);

        let loop_label = self.create_loop_label();
        if self.current_flow.is_some() {
            self.add_antecedent(loop_label, self.current_flow);
        }
        self.current_flow = loop_label;

        let post_loop = self.create_branch_label();
        self.break_targets.push(post_loop);
        let continue_target = if loop_data.incrementor.is_some() {
            self.create_branch_label()
        } else {
            loop_label
        };
        self.continue_targets.push(continue_target);

        if loop_data.condition.is_none() {
            self.bind_node(arena, loop_data.statement);
            self.add_antecedent(continue_target, self.current_flow);
            if loop_data.incrementor.is_some() {
                self.current_flow = continue_target;
            }
            self.bind_expression(arena, loop_data.incrementor);
            self.add_antecedent(loop_label, self.current_flow);
            self.add_antecedent(post_loop, loop_label);
            self.add_antecedent(post_loop, self.current_flow);
        } else {
            self.bind_expression(arena, loop_data.condition);
            let pre_condition_flow = self.current_flow;
            let true_flow = self.create_flow_condition(
                flow_flags::TRUE_CONDITION,
                pre_condition_flow,
                loop_data.condition,
            );
            self.current_flow = true_flow;
            self.bind_node(arena, loop_data.statement);
            self.add_antecedent(continue_target, self.current_flow);
            if loop_data.incrementor.is_some() {
                self.current_flow = continue_target;
            }
            self.bind_expression(arena, loop_data.incrementor);
            self.add_antecedent(loop_label, self.current_flow);

            let false_flow = self.create_flow_condition(
                flow_flags::FALSE_CONDITION,
                pre_condition_flow,
                loop_data.condition,
            );
            self.add_antecedent(post_loop, false_flow);
        }

        self.break_targets.pop();
        self.continue_targets.pop();
        self.current_flow = post_loop;
        self.exit_scope(arena);
    }

    pub(crate) fn bind_for_in_or_for_of_statement(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        idx: NodeIndex,
    ) {
        self.record_flow(idx);
        let Some(for_data) = arena.get_for_in_of(node) else {
            return;
        };

        self.enter_scope(ContainerKind::Block, idx);
        self.bind_node(arena, for_data.initializer);
        let loop_label = self.create_loop_label();
        if self.current_flow.is_some() {
            self.add_antecedent(loop_label, self.current_flow);
        }
        self.current_flow = loop_label;

        let post_loop = self.create_branch_label();
        self.break_targets.push(post_loop);
        self.continue_targets.push(loop_label);

        self.add_antecedent(post_loop, loop_label);

        self.bind_expression(arena, for_data.expression);
        if node.kind == syntax_kind_ext::FOR_IN_STATEMENT && for_data.expression.is_some() {
            let true_flow = self.create_flow_condition(
                flow_flags::TRUE_CONDITION,
                self.current_flow,
                for_data.expression,
            );
            self.current_flow = true_flow;
        }
        if for_data.initializer.is_some() {
            let flow = self.create_flow_assignment(for_data.initializer);
            self.current_flow = flow;
        }
        self.bind_node(arena, for_data.statement);
        self.add_antecedent(loop_label, self.current_flow);

        self.break_targets.pop();
        self.continue_targets.pop();
        self.current_flow = post_loop;
        self.exit_scope(arena);
    }

    pub(crate) fn bind_return_or_throw_statement(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        idx: NodeIndex,
    ) {
        self.record_flow(idx);
        if let Some(ret) = arena.get_return_statement(node)
            && ret.expression.is_some()
        {
            tracing::debug!(
                return_idx = idx.0,
                expr_idx = ret.expression.0,
                "Binding return expression"
            );
            self.bind_node(arena, ret.expression);
        }

        if node.kind == syntax_kind_ext::RETURN_STATEMENT
            && let Some(&return_target) = self.return_targets.last()
        {
            self.add_antecedent(return_target, self.current_flow);
        }
        self.current_flow = self.unreachable_flow;
    }

    pub(crate) fn bind_break_statement(&mut self) {
        if let Some(&break_target) = self.break_targets.last() {
            self.add_antecedent(break_target, self.current_flow);
        }
        self.current_flow = self.unreachable_flow;
    }

    pub(crate) fn bind_continue_statement(&mut self) {
        if let Some(&continue_target) = self.continue_targets.last() {
            self.add_antecedent(continue_target, self.current_flow);
        }
        self.current_flow = self.unreachable_flow;
    }

    fn is_syntactically_true_condition(arena: &NodeArena, condition: NodeIndex) -> bool {
        let Some(node) = arena.get(condition) else {
            return false;
        };

        if node.kind == SyntaxKind::TrueKeyword as u16 {
            return true;
        }

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(parenthesized) = arena.get_parenthesized(node)
        {
            return Self::is_syntactically_true_condition(arena, parenthesized.expression);
        }

        false
    }
}
