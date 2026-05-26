use crate::transforms::async_es5_ir::{AsyncES5Transformer, AsyncTransformState};
use crate::transforms::ir::{IRGeneratorCase, IRNode};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

use super::opcodes;

#[derive(Clone, Copy)]
pub(super) struct AsyncLoopControlTargets {
    pub(super) break_label: u32,
    pub(super) continue_label: u32,
}

impl<'a> AsyncES5Transformer<'a> {
    /// Finalize prefix statements before opening a loop-entry case. Later
    /// backedges must land on the loop case without re-running the prefix.
    pub(super) fn flush_preceding_case_for_new_label(
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        state: &mut AsyncTransformState,
    ) {
        if current_statements.is_empty() {
            return;
        }
        let new_label = state.next_label();
        current_statements.push(Self::generator_label_assignment(new_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });
        *current_label = new_label;
    }

    pub(super) fn process_loop_body_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        loop_control: AsyncLoopControlTargets,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::BREAK_STATEMENT
                && self.jump_statement_is_unlabeled_loop_local(idx) =>
            {
                current_statements.push(Self::generator_break_statement(loop_control.break_label));
            }
            k if k == syntax_kind_ext::CONTINUE_STATEMENT
                && self.jump_statement_is_unlabeled_loop_local(idx) =>
            {
                current_statements
                    .push(Self::generator_break_statement(loop_control.continue_label));
            }
            k if k == syntax_kind_ext::CONTINUE_STATEMENT => {
                if let Some(target) = self.labeled_continue_target(idx) {
                    current_statements.push(Self::generator_break_statement(target));
                } else {
                    self.process_async_statement(idx, cases, current_statements, current_label);
                }
            }
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.process_loop_body_statement_in_async(
                            stmt,
                            cases,
                            current_statements,
                            current_label,
                            loop_control,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.process_if_statement_in_async_with_loop_control(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                    loop_control,
                );
            }
            k if self.statement_starts_inner_loop_or_function(k) => {
                if let Some(rewritten) = self.statement_to_ir_with_labeled_continue(idx) {
                    current_statements.push(rewritten);
                } else {
                    self.process_async_statement(idx, cases, current_statements, current_label);
                }
            }
            _ => {
                self.process_async_statement(idx, cases, current_statements, current_label);
            }
        }
    }

    fn process_if_statement_in_async_with_loop_control(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        loop_control: AsyncLoopControlTargets,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(if_stmt) = self.arena.get_if_statement(node) else {
            return;
        };

        let then_has_await = self.contains_await_recursive(if_stmt.then_statement);
        let else_has_await = if_stmt.else_statement.is_some()
            && self.contains_await_recursive(if_stmt.else_statement);
        let then_has_loop_control =
            self.contains_unlabeled_loop_local_control(if_stmt.then_statement);
        let else_has_loop_control = if_stmt.else_statement.is_some()
            && self.contains_unlabeled_loop_local_control(if_stmt.else_statement);

        if !then_has_await && !else_has_await && !then_has_loop_control && !else_has_loop_control {
            current_statements.push(self.statement_to_ir(idx));
            return;
        }

        let has_else = if_stmt.else_statement.is_some()
            && self
                .arena
                .get(if_stmt.else_statement)
                .is_some_and(|n| n.kind != syntax_kind_ext::EMPTY_STATEMENT);

        let delayed_else_label = has_else && then_has_await;
        let else_placeholder = delayed_else_label.then(|| self.next_loop_exit_placeholder());
        let (mut else_label, mut end_label) = if delayed_else_label {
            (None, None)
        } else {
            let else_label = self.state.next_label();
            let end_label = if has_else {
                self.state.next_label()
            } else {
                else_label
            };
            (Some(else_label), Some(end_label))
        };

        let target_label = else_placeholder.unwrap_or_else(|| {
            if has_else {
                else_label.expect("else label must be allocated without delayed scheduling")
            } else {
                end_label.expect("end label must be allocated without delayed scheduling")
            }
        });
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(Self::negated_condition(
                self.expression_to_ir(if_stmt.expression),
            )),
            target_label,
        });

        self.process_loop_body_statement_in_async(
            if_stmt.then_statement,
            cases,
            current_statements,
            current_label,
            loop_control,
        );

        if has_else {
            if let Some(placeholder) = else_placeholder {
                let patched_else_label = self.state.next_label();
                let patched_end_label = self.state.next_label();
                Self::patch_if_break_target(cases, placeholder, patched_else_label);
                Self::patch_if_break_target_in_statements(
                    current_statements,
                    placeholder,
                    patched_else_label,
                );
                else_label = Some(patched_else_label);
                end_label = Some(patched_end_label);
            }
            let else_label = else_label.expect("else label must be available before else branch");
            let end_label = end_label.expect("end label must be available before then break");

            current_statements.push(Self::generator_break_statement(end_label));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = else_label;

            self.process_loop_body_statement_in_async(
                if_stmt.else_statement,
                cases,
                current_statements,
                current_label,
                loop_control,
            );
        }

        if !current_statements.is_empty() {
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
        }
        *current_label = end_label.expect("end label must be available after if lowering");
    }

    fn contains_unlabeled_loop_local_control(&self, idx: NodeIndex) -> bool {
        self.contains_unlabeled_loop_local_control_kind(idx, None)
    }

    pub(super) fn contains_unlabeled_loop_local_continue(&self, idx: NodeIndex) -> bool {
        self.contains_unlabeled_loop_local_control_kind(
            idx,
            Some(syntax_kind_ext::CONTINUE_STATEMENT),
        )
    }

    fn contains_unlabeled_loop_local_control_kind(
        &self,
        idx: NodeIndex,
        expected_kind: Option<u16>,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.is_function_expression_or_arrow()
            || self.statement_starts_inner_loop_or_function(node.kind)
        {
            return false;
        }
        match node.kind {
            k if (k == syntax_kind_ext::BREAK_STATEMENT
                || k == syntax_kind_ext::CONTINUE_STATEMENT)
                && expected_kind.is_none_or(|expected_kind| k == expected_kind)
                && self.jump_statement_is_unlabeled_loop_local(idx) =>
            {
                true
            }
            k if k == syntax_kind_ext::BLOCK || k == syntax_kind_ext::CASE_BLOCK => {
                self.arena.get_block(node).is_some_and(|block| {
                    block.statements.nodes.iter().any(|&stmt| {
                        self.contains_unlabeled_loop_local_control_kind(stmt, expected_kind)
                    })
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.arena.get_if_statement(node).is_some_and(|if_stmt| {
                    self.contains_unlabeled_loop_local_control_kind(
                        if_stmt.then_statement,
                        expected_kind,
                    ) || self.contains_unlabeled_loop_local_control_kind(
                        if_stmt.else_statement,
                        expected_kind,
                    )
                })
            }
            _ => false,
        }
    }

    fn jump_statement_is_unlabeled_loop_local(&self, idx: NodeIndex) -> bool {
        self.arena
            .get(idx)
            .and_then(|node| self.arena.get_jump_data(node))
            .is_some_and(|jump| jump.label.is_none())
    }

    fn labeled_continue_target(&self, idx: NodeIndex) -> Option<u32> {
        let jump = self
            .arena
            .get(idx)
            .and_then(|node| self.arena.get_jump_data(node))?;
        let label = jump.label.into_option()?;
        let label = crate::transforms::emit_utils::identifier_text_or_empty(self.arena, label);
        self.labeled_continue_targets
            .iter()
            .rev()
            .find_map(|(candidate, target)| (candidate == &label).then_some(*target))
    }

    fn statement_to_ir_with_labeled_continue(&self, idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.is_function_expression_or_arrow()
            || node.kind == syntax_kind_ext::CLASS_DECLARATION
        {
            return None;
        }

        match node.kind {
            k if k == syntax_kind_ext::CONTINUE_STATEMENT => self
                .labeled_continue_target(idx)
                .map(Self::generator_break_statement),
            k if k == syntax_kind_ext::BLOCK || k == syntax_kind_ext::CASE_BLOCK => {
                let block = self.arena.get_block(node)?;
                let mut changed = false;
                let statements = block
                    .statements
                    .nodes
                    .iter()
                    .map(|&stmt| {
                        if let Some(rewritten) = self.statement_to_ir_with_labeled_continue(stmt) {
                            changed = true;
                            rewritten
                        } else {
                            self.statement_to_ir(stmt)
                        }
                    })
                    .collect();
                changed.then_some(IRNode::Block(statements))
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT => {
                let loop_data = self.arena.get_loop(node)?;
                let body = self.statement_to_ir_with_labeled_continue(loop_data.statement)?;
                Some(IRNode::WhileStatement {
                    condition: Box::new(self.expression_to_ir(loop_data.condition)),
                    body: Box::new(body),
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                let if_stmt = self.arena.get_if_statement(node)?;
                let then_branch =
                    self.statement_to_ir_with_labeled_continue(if_stmt.then_statement);
                let else_statement = if_stmt.else_statement.into_option();
                let else_branch = else_statement
                    .and_then(|stmt| self.statement_to_ir_with_labeled_continue(stmt));
                if then_branch.is_none() && else_branch.is_none() {
                    return None;
                }
                Some(IRNode::IfStatement {
                    condition: Box::new(self.expression_to_ir(if_stmt.expression)),
                    then_branch: Box::new(
                        then_branch.unwrap_or_else(|| self.statement_to_ir(if_stmt.then_statement)),
                    ),
                    else_branch: else_statement.map(|stmt| {
                        Box::new(else_branch.unwrap_or_else(|| self.statement_to_ir(stmt)))
                    }),
                })
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                let labeled = self.arena.get_labeled_statement(node)?;
                let label = crate::transforms::emit_utils::identifier_text_or_empty(
                    self.arena,
                    labeled.label,
                );
                if self
                    .labeled_continue_targets
                    .iter()
                    .any(|(candidate, _)| candidate == &label)
                {
                    return None;
                }
                let statement = self.statement_to_ir_with_labeled_continue(labeled.statement)?;
                Some(IRNode::LabeledStatement {
                    label: label.into(),
                    statement: Box::new(statement),
                })
            }
            _ => None,
        }
    }

    const fn statement_starts_inner_loop_or_function(&self, kind: u16) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::WHILE_STATEMENT
            || kind == syntax_kind_ext::DO_STATEMENT
            || kind == syntax_kind_ext::FOR_STATEMENT
            || kind == syntax_kind_ext::FOR_IN_STATEMENT
            || kind == syntax_kind_ext::FOR_OF_STATEMENT
            || kind == syntax_kind_ext::SWITCH_STATEMENT
            || kind == syntax_kind_ext::LABELED_STATEMENT
    }

    pub(super) fn patch_if_break_target(
        cases: &mut [IRGeneratorCase],
        placeholder_label: u32,
        target_label: u32,
    ) {
        for case in cases {
            for statement in &mut case.statements {
                Self::patch_if_break_target_in_node(statement, placeholder_label, target_label);
            }
        }
    }

    pub(super) fn patch_if_break_target_in_statements(
        statements: &mut [IRNode],
        placeholder_label: u32,
        target_label: u32,
    ) {
        for statement in statements {
            Self::patch_if_break_target_in_node(statement, placeholder_label, target_label);
        }
    }

    fn patch_if_break_target_in_node(node: &mut IRNode, placeholder_label: u32, target_label: u32) {
        if let IRNode::IfBreak {
            target_label: candidate,
            ..
        } = node
            && *candidate == placeholder_label
        {
            *candidate = target_label;
            return;
        }
        if let IRNode::ReturnStatement(Some(expr)) = node
            && let IRNode::GeneratorOp {
                opcode,
                value: Some(value),
                ..
            } = expr.as_mut()
            && *opcode == opcodes::BREAK
            && let IRNode::NumericLiteral(candidate) = value.as_mut()
            && candidate.as_ref() == placeholder_label.to_string()
        {
            *candidate = target_label.to_string().into();
            return;
        }
        // Recurse into dispatch `switch` statements emitted by the suspending
        // switch lowering, whose case bodies hold `return [3 /*break*/, L]`
        // jumps to placeholder clause-body labels.
        if let IRNode::SwitchStatement { cases, .. } = node {
            for case in cases {
                for statement in &mut case.statements {
                    Self::patch_if_break_target_in_node(statement, placeholder_label, target_label);
                }
            }
        }
    }
}
