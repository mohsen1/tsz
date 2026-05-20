//! Lowering for `while`/`do-while` whose condition may suspend.
//!
//! When the top-level expression of an async loop's condition is `await x`
//! (or `yield x` in generator mode), the condition is lowered into a generator
//! yield whose `_a.sent()` becomes the boolean tested at the `IfBreak`. The
//! body still lowers any awaits it contains; unlabeled `break`/`continue`
//! route through the loop-control walker so they become generator branches
//! instead of leaving raw JS inside the `__generator` callback.
//!
//! Nested non-top-level suspensions (e.g. `while (x && await y())`) cannot
//! yet be hoisted out of the surrounding expression, so they fall back to
//! raw statement emission.

use crate::transforms::async_es5_ir::AsyncES5Transformer;
use crate::transforms::ir::{IRGeneratorCase, IRNode};
use tsz_parser::parser::NodeIndex;

use super::loop_control::AsyncLoopControlTargets;
use super::opcodes;

/// Three-way classification of a `while`/`do-while` condition for async
/// state-machine lowering. See [`AsyncES5Transformer::plan_loop_condition_await`].
pub(super) enum LoopConditionPlan {
    /// Condition is `await op` at the top level: lower it as a generator
    /// yield and use `_a.sent()` as the `IfBreak` predicate.
    Yield(NodeIndex),
    /// Condition has no suspensions: emit it inline as the `IfBreak` predicate.
    Plain,
    /// Condition contains a nested suspension that the loop lowering cannot
    /// hoist; the caller must fall back to raw statement emission.
    FallBackToRaw,
}

impl<'a> AsyncES5Transformer<'a> {
    /// If `idx` is exactly a suspension expression (possibly inside redundant
    /// parens), return its awaited/yielded operand expression. Otherwise
    /// return `None`. Delegates to `direct_suspension_expression` so the
    /// mode-aware suspension classification (async / generator /
    /// async-generator) is shared with the rest of the lowering.
    pub(super) fn top_level_suspension_operand(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let suspension_idx = self.direct_suspension_expression(idx)?;
        let suspension_node = self.arena.get(suspension_idx)?;
        let unary = self.arena.get_unary_expr_ex(suspension_node)?;
        Some(unary.expression)
    }

    /// Plan how to lower a `while`/`do-while` condition that may suspend.
    pub(super) fn plan_loop_condition_await(&self, condition_idx: NodeIndex) -> LoopConditionPlan {
        if let Some(operand) = self.top_level_suspension_operand(condition_idx) {
            return LoopConditionPlan::Yield(operand);
        }
        if self.contains_await_recursive(condition_idx) {
            // Nested non-top-level suspension; we cannot hoist it yet, so the
            // caller must fall back to raw emission rather than silently
            // leave invalid `await` syntax inside the generator body.
            return LoopConditionPlan::FallBackToRaw;
        }
        LoopConditionPlan::Plain
    }

    /// Process a while statement inside an async function body.
    ///
    /// `await` in the body must be lifted into generator cases before the loop
    /// body is emitted. A raw `while` statement around `await` would otherwise
    /// leave invalid `await` syntax inside the ES5 generator callback.
    pub(super) fn process_while_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(loop_data) = self.arena.get_loop(node) else {
            return;
        };

        let body_has_await = self.contains_await_recursive(loop_data.statement);
        let condition_await_operand = match self.plan_loop_condition_await(loop_data.condition) {
            LoopConditionPlan::FallBackToRaw => {
                current_statements.push(self.statement_to_ir(idx));
                return;
            }
            LoopConditionPlan::Yield(operand) => Some(operand),
            LoopConditionPlan::Plain => None,
        };

        if !body_has_await && condition_await_operand.is_none() {
            current_statements.push(self.statement_to_ir(idx));
            return;
        }

        // Correctness: the loop entry must be its own case, otherwise a
        // `break-to-loop` from the body re-enters the prefix statements
        // and re-executes them every iteration — an infinite-loop bug
        // when the prefix initializes the loop variable.
        Self::flush_preceding_case_for_new_label(
            cases,
            current_statements,
            current_label,
            &mut self.state,
        );

        let loop_label = *current_label;
        let exit_placeholder = self.next_loop_exit_placeholder();
        let condition = if let Some(operand_idx) = condition_await_operand {
            let operand = self.expression_to_ir(operand_idx);
            self.push_generator_yield(
                opcodes::YIELD,
                operand,
                "yield",
                cases,
                current_statements,
                current_label,
            );
            IRNode::GeneratorSent
        } else {
            self.expression_to_ir(loop_data.condition)
        };

        current_statements.push(IRNode::IfBreak {
            condition: Box::new(Self::negated_condition(condition)),
            target_label: exit_placeholder,
        });

        // For `while` the continue target is the loop-entry case (the yield
        // or plain condition check), so jumping there re-evaluates the
        // condition on the next iteration.
        let loop_control = AsyncLoopControlTargets {
            break_label: exit_placeholder,
            continue_label: loop_label,
        };
        self.process_loop_body_statement_in_async(
            loop_data.statement,
            cases,
            current_statements,
            current_label,
            loop_control,
        );

        current_statements.push(Self::generator_break_statement(loop_label));

        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        let exit_label = self.state.next_label();
        Self::patch_if_break_target(cases, exit_placeholder, exit_label);
        *current_label = exit_label;
    }

    /// Process a do-while statement inside an async function body.
    ///
    /// When the body suspends and the condition does not, the state machine must
    /// enter through the body case first. Emitting a raw `do` statement would
    /// leave `await` syntax inside the ES5 generator callback.
    pub(super) fn process_do_while_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(loop_data) = self.arena.get_loop(node) else {
            return;
        };

        let body_has_await = self.contains_await_recursive(loop_data.statement);
        let condition_await_operand = match self.plan_loop_condition_await(loop_data.condition) {
            LoopConditionPlan::FallBackToRaw => {
                current_statements.push(self.statement_to_ir(idx));
                return;
            }
            LoopConditionPlan::Yield(operand) => Some(operand),
            LoopConditionPlan::Plain => None,
        };

        if !body_has_await && condition_await_operand.is_none() {
            current_statements.push(self.statement_to_ir(idx));
            return;
        }

        let loop_label = *current_label;
        let exit_placeholder = self.next_loop_exit_placeholder();
        let has_loop_continue = self.contains_unlabeled_loop_local_continue(loop_data.statement);
        let continue_placeholder = if has_loop_continue {
            self.next_loop_exit_placeholder()
        } else {
            loop_label
        };
        let loop_control = AsyncLoopControlTargets {
            break_label: exit_placeholder,
            continue_label: continue_placeholder,
        };

        self.process_loop_body_statement_in_async(
            loop_data.statement,
            cases,
            current_statements,
            current_label,
            loop_control,
        );

        // When the condition itself yields we still want `continue` to skip
        // straight to the condition check (do-while semantics). With
        // `continue` present the yield must live in its own case so we can
        // jump there without re-running the body; otherwise the yield can
        // share the body case and the runtime's auto-increment carries us
        // into the post-resume case directly.
        let condition_label = if let Some(operand_idx) = condition_await_operand {
            if has_loop_continue {
                let yield_case_label = self.state.next_label();
                current_statements.push(Self::generator_label_assignment(yield_case_label));
                cases.push(IRGeneratorCase {
                    label: *current_label,
                    statements: std::mem::take(current_statements),
                });
                *current_label = yield_case_label;
            }
            let operand = self.expression_to_ir(operand_idx);
            let yield_case_label = *current_label;
            self.push_generator_yield(
                opcodes::YIELD,
                operand,
                "yield",
                cases,
                current_statements,
                current_label,
            );
            current_statements.push(IRNode::IfBreak {
                condition: Box::new(Self::negated_condition(IRNode::GeneratorSent)),
                target_label: exit_placeholder,
            });
            has_loop_continue.then_some(yield_case_label)
        } else {
            let condition_label = has_loop_continue.then(|| {
                let label = self.state.next_label();
                current_statements.push(Self::generator_label_assignment(label));
                label
            });
            let condition = self.expression_to_ir(loop_data.condition);
            current_statements.push(IRNode::IfBreak {
                condition: Box::new(Self::negated_condition(condition)),
                target_label: exit_placeholder,
            });
            condition_label
        };
        current_statements.push(Self::generator_break_statement(loop_label));

        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        let exit_label = self.state.next_label();
        Self::patch_if_break_target(cases, exit_placeholder, exit_label);
        if let Some(condition_label) = condition_label {
            Self::patch_if_break_target(cases, continue_placeholder, condition_label);
        }
        *current_label = exit_label;
    }
}
