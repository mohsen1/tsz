//! Lowering for `switch` statements that suspend, into the ES5 `__generator`
//! state machine.
//!
//! This mirrors tsc's `transformAndEmitSwitchStatement` (plus `cacheExpression`)
//! from `src/compiler/transformers/generators.ts`. The structural rule:
//!
//! > When a `switch` statement's case block contains a suspension (`await` in an
//! > async function, `yield` in a generator) — whether in a case-clause
//! > expression or a clause body — tsc cannot keep the `switch` intact, because
//! > a suspension splits the surrounding code into `__generator` cases. Instead
//! > it caches the discriminant into a temp, emits one or more *dispatch*
//! > `switch` statements that compare the cached discriminant against each
//! > case-clause expression and `return [3 /*break*/, L]` to that clause's body
//! > label, then lays out each clause body at its own label with `break`
//! > rewritten to a jump to the switch-end label. This change makes tsz do the
//! > same — independent of identifier spelling, target, which clause holds the
//! > suspension, and the presence/position of a `default` clause.
//!
//! When only the discriminant suspends (the case block does not), the `switch`
//! body stays intact: the discriminant is yielded and the whole `switch` is
//! emitted verbatim using the sent value as its expression — matching tsc.

use crate::transforms::async_es5_ir::AsyncES5Transformer;
use crate::transforms::ir::{IRGeneratorCase, IRNode, IRSwitchCase};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::CaseClauseData;

use super::loop_control::AsyncLoopControlTargets;
use super::opcodes;

impl<'a> AsyncES5Transformer<'a> {
    /// Process a `switch` statement inside an async/generator body.
    pub(super) fn process_switch_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(switch) = self.arena.get_switch(node) else {
            return;
        };
        let discriminant_idx = switch.expression;
        let case_block_idx = switch.case_block;
        let Some(case_block_node) = self.arena.get(case_block_idx) else {
            return;
        };
        let Some(case_block) = self.arena.get_block(case_block_node) else {
            return;
        };
        let clause_indices: Vec<NodeIndex> = case_block.statements.nodes.clone();

        if !self.contains_await_recursive(case_block_idx) {
            // The case block has no suspension. If the discriminant suspends at
            // the top level, yield it and keep the `switch` intact using the
            // sent value as the discriminant. Otherwise emit it verbatim.
            if let Some(operand_idx) = self.top_level_suspension_operand(discriminant_idx) {
                let operand = self.expression_to_ir(operand_idx);
                self.push_generator_yield(
                    opcodes::YIELD,
                    operand,
                    "yield",
                    cases,
                    current_statements,
                    current_label,
                );
                let verbatim = self.build_verbatim_switch(&clause_indices, IRNode::GeneratorSent);
                current_statements.push(verbatim);
                return;
            }
            // No top-level suspension to hoist (no suspension at all, or only a
            // nested one in the discriminant that we cannot lift): emit verbatim.
            current_statements.push(self.statement_to_ir(idx));
            return;
        }

        self.lower_suspending_switch(
            discriminant_idx,
            &clause_indices,
            cases,
            current_statements,
            current_label,
        );
    }

    /// Lower a `switch` whose case block suspends into dispatch switches plus
    /// per-clause-body generator cases.
    fn lower_suspending_switch(
        &mut self,
        discriminant_idx: NodeIndex,
        clause_indices: &[NodeIndex],
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let clause_count = clause_indices.len();

        // Cache the discriminant into a hoisted temp so each dispatch switch can
        // compare against it across yield boundaries.
        let discriminant_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: discriminant_temp.clone().into(),
            initializer: None,
        });
        let discriminant_value =
            self.lower_suspending_value(discriminant_idx, cases, current_statements, current_label);
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(discriminant_temp.clone()),
            discriminant_value,
        ))));

        // Reserve a placeholder label for each clause body and the switch end.
        // Real labels are assigned in clause order once the dispatch is built,
        // matching tsc's label-numbering (which is determined by mark order).
        let clause_labels: Vec<u32> = (0..clause_count)
            .map(|_| self.next_loop_exit_placeholder())
            .collect();
        let end_label = self.next_loop_exit_placeholder();
        let default_index = clause_indices.iter().position(|&clause_idx| {
            self.switch_clause(clause_idx)
                .is_some_and(|clause| clause.expression.is_none())
        });

        self.build_switch_dispatch(
            &discriminant_temp,
            clause_indices,
            &clause_labels,
            cases,
            current_statements,
            current_label,
        );

        // After the dispatch, fall through to the default clause (or the end of
        // the switch when there is no default).
        let fallthrough_target = default_index.map_or(end_label, |index| clause_labels[index]);
        current_statements.push(Self::generator_break_statement(fallthrough_target));

        self.emit_switch_clause_bodies(
            clause_indices,
            &clause_labels,
            end_label,
            cases,
            current_statements,
            current_label,
        );
    }

    /// Build the dispatch `switch` statements: a faithful port of the
    /// clause-grouping loop in tsc's `transformAndEmitSwitchStatement`.
    ///
    /// Consecutive case clauses are grouped into a single dispatch `switch`;
    /// a case clause whose expression itself suspends starts a new group (its
    /// suspension is yielded before the group's dispatch is emitted). `default`
    /// clauses are not part of any dispatch (they are the fallthrough target).
    fn build_switch_dispatch(
        &mut self,
        discriminant_temp: &str,
        clause_indices: &[NodeIndex],
        clause_labels: &[u32],
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let clause_count = clause_indices.len();
        let mut clauses_written = 0usize;
        while clauses_written < clause_count {
            let mut pending: Vec<IRSwitchCase> = Vec::new();
            let mut default_skipped = 0usize;
            let mut index = clauses_written;
            while index < clause_count {
                let Some(clause) = self.switch_clause(clause_indices[index]) else {
                    index += 1;
                    continue;
                };
                // A `default` clause has no expression; it is not part of any
                // dispatch and is reached via the post-dispatch fallthrough.
                let clause_expr = clause.expression;
                if clause_expr.is_none() {
                    default_skipped += 1;
                    index += 1;
                    continue;
                }
                // A suspending case expression starts a fresh dispatch group so
                // its yield does not land in the middle of an already-started
                // dispatch switch.
                if self.contains_await_recursive(clause_expr) && !pending.is_empty() {
                    break;
                }
                let test = self.lower_suspending_value(
                    clause_expr,
                    cases,
                    current_statements,
                    current_label,
                );
                pending.push(IRSwitchCase {
                    test: Some(test),
                    statements: vec![Self::generator_break_statement(clause_labels[index])],
                    inline: true,
                });
                index += 1;
            }

            if !pending.is_empty() {
                let written = pending.len();
                current_statements.push(IRNode::SwitchStatement {
                    expression: Box::new(IRNode::id(discriminant_temp.to_string())),
                    cases: std::mem::take(&mut pending),
                });
                clauses_written += written;
            }
            clauses_written += default_skipped;
        }
    }

    /// Lower an expression that may suspend into an IR value, yielding first
    /// when it is a top-level suspension (`await x` / `yield x`). Used for both
    /// the cached discriminant and each dispatch-clause test.
    fn lower_suspending_value(
        &mut self,
        expr_idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> IRNode {
        if let Some(operand_idx) = self.top_level_suspension_operand(expr_idx) {
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
        } else if self.contains_await_recursive(expr_idx) {
            self.emit_nested_suspension(expr_idx, cases, current_statements, current_label);
            self.expression_to_ir(expr_idx)
        } else {
            self.expression_to_ir(expr_idx)
        }
    }

    /// Emit each clause body at its own generator-case label, with unlabeled
    /// `break` rewritten to a jump to the switch-end label and fallthrough
    /// between non-terminating clauses preserved via `_.label = next`.
    fn emit_switch_clause_bodies(
        &mut self,
        clause_indices: &[NodeIndex],
        clause_labels: &[u32],
        end_label: u32,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        // Inside a `switch`, an unlabeled `break` exits the switch (the end
        // label). An unlabeled `continue` is only meaningful inside an enclosing
        // loop; switch clauses do not establish a continue target, so it is left
        // to the surrounding loop's lowering. We pass the end label as the
        // continue target as a conservative bound for the (rare) switch-in-loop
        // case, which is not exercised by the suspending-switch corpus.
        let control = AsyncLoopControlTargets {
            break_label: end_label,
            continue_label: end_label,
        };

        for (index, &clause_idx) in clause_indices.iter().enumerate() {
            let label = self.state.next_label();
            self.open_switch_case(label, cases, current_statements, current_label);
            Self::patch_if_break_target(cases, clause_labels[index], label);

            let Some(clause) = self.switch_clause(clause_idx) else {
                continue;
            };
            let statements = clause.statements.nodes.clone();
            for stmt in statements {
                self.process_loop_body_statement_in_async(
                    stmt,
                    cases,
                    current_statements,
                    current_label,
                    control,
                );
            }
        }

        let label = self.state.next_label();
        self.open_switch_case(label, cases, current_statements, current_label);
        Self::patch_if_break_target(cases, end_label, label);
    }

    /// Flush the current generator case and open a new one at `label`. When the
    /// flushed case does not end in a terminating generator op, append
    /// `_.label = label` so execution falls through to the new case — matching
    /// tsc's `flushLabel` behavior.
    fn open_switch_case(
        &mut self,
        label: u32,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        if !current_statements.is_empty() && !Self::statements_terminate(current_statements) {
            current_statements.push(Self::generator_label_assignment(label));
        }
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });
        *current_label = label;
    }

    /// Whether a case's statements end in a terminating generator op (a
    /// `return [...]`), in which case the runtime will not fall through.
    const fn statements_terminate(statements: &[IRNode]) -> bool {
        matches!(statements.last(), Some(IRNode::ReturnStatement(_)))
    }

    /// Resolve a `case`/`default` clause node to its data (works for both, with
    /// a `default` clause carrying a none expression).
    fn switch_clause(&self, clause_idx: NodeIndex) -> Option<&CaseClauseData> {
        let node = self.arena.get(clause_idx)?;
        self.arena.get_case_clause(node)
    }

    /// Build a verbatim `switch` IR node from clauses (no suspension in the case
    /// block), substituting `discriminant` for the switch expression.
    fn build_verbatim_switch(&self, clause_indices: &[NodeIndex], discriminant: IRNode) -> IRNode {
        let mut switch_cases = Vec::new();
        for &clause_idx in clause_indices {
            let Some(clause) = self.switch_clause(clause_idx) else {
                continue;
            };
            let test = if clause.expression.is_none() {
                None
            } else {
                Some(self.expression_to_ir(clause.expression))
            };
            let statements = clause
                .statements
                .nodes
                .iter()
                .map(|&stmt| self.statement_to_ir(stmt))
                .collect();
            switch_cases.push(IRSwitchCase {
                test,
                statements,
                inline: false,
            });
        }
        IRNode::SwitchStatement {
            expression: Box::new(discriminant),
            cases: switch_cases,
        }
    }
}
