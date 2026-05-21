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
use tsz_parser::parser::syntax_kind_ext;

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

    /// Process a `for (init; cond; incr) body` statement inside an async
    /// function body.
    ///
    /// Mirrors [`Self::process_while_statement_in_async`]: any suspension in the
    /// initializer, condition, incrementor, or body must be lifted into
    /// generator cases. A raw `for` left around `await` would otherwise emit
    /// invalid `await` syntax inside the ES5 `__generator` callback. The
    /// continue target is the incrementor case (so `continue` runs the
    /// incrementor then re-checks the condition, matching for-loop semantics),
    /// and the backedge returns to the condition case.
    ///
    /// Returns `true` when the statement was handled (either lowered to a state
    /// machine, or emitted with hoisted initializer `var`s). Returns `false`
    /// for shapes the caller should emit verbatim.
    pub(super) fn process_for_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        let Some(loop_data) = self.arena.get_loop(node) else {
            return false;
        };
        let init_idx = loop_data.initializer;
        let cond_idx = loop_data.condition;
        let incr_idx = loop_data.incrementor;
        let body_idx = loop_data.statement;

        let suspends = (init_idx.is_some() && self.contains_await_recursive(init_idx))
            || (cond_idx.is_some() && self.contains_await_recursive(cond_idx))
            || (incr_idx.is_some() && self.contains_await_recursive(incr_idx))
            || self.contains_await_recursive(body_idx);

        if !suspends {
            // No suspension anywhere: this is not a state-machine loop. tsc
            // still hoists any `var` declared in the initializer into the
            // `__awaiter` closure, rewriting `for (var x = e; …)` to a hoisted
            // `var x;` plus `for (x = e; …)`. Emit that shape; fall back to the
            // verbatim path when there is nothing to hoist.
            return self.emit_for_with_hoisted_var_initializer(
                init_idx,
                cond_idx,
                incr_idx,
                body_idx,
                current_statements,
            );
        }

        // The condition can only be lowered when its suspension (if any) is a
        // top-level `await cond`. A nested suspension cannot be hoisted out of
        // the surrounding expression yet, so fall back to verbatim emission
        // (no worse than today) rather than emit invalid syntax.
        let condition_plan = if cond_idx.is_some() {
            self.plan_loop_condition_await(cond_idx)
        } else {
            LoopConditionPlan::Plain
        };
        if matches!(condition_plan, LoopConditionPlan::FallBackToRaw) {
            return false;
        }

        // --- initializer (runs once, in the entry case; no-op when absent) ---
        self.process_for_initializer_in_async(init_idx, cases, current_statements, current_label);

        // The loop entry must be its own case so a backedge does not re-run the
        // initializer every iteration.
        Self::flush_preceding_case_for_new_label(
            cases,
            current_statements,
            current_label,
            &mut self.state,
        );
        let condition_label = *current_label;
        let exit_placeholder = self.next_loop_exit_placeholder();
        let continue_placeholder = self.next_loop_exit_placeholder();

        // --- condition (absent for `for (;;)`, where the loop only `break`s) ---
        let condition = match condition_plan {
            LoopConditionPlan::Yield(operand_idx) => {
                let operand = self.expression_to_ir(operand_idx);
                self.push_generator_yield(
                    opcodes::YIELD,
                    operand,
                    "yield",
                    cases,
                    current_statements,
                    current_label,
                );
                Some(IRNode::GeneratorSent)
            }
            LoopConditionPlan::Plain if cond_idx.is_some() => Some(self.expression_to_ir(cond_idx)),
            _ => None,
        };
        if let Some(condition) = condition {
            current_statements.push(IRNode::IfBreak {
                condition: Box::new(Self::negated_condition(condition)),
                target_label: exit_placeholder,
            });
        }

        // --- body ---
        let loop_control = AsyncLoopControlTargets {
            break_label: exit_placeholder,
            continue_label: continue_placeholder,
        };
        self.process_loop_body_statement_in_async(
            body_idx,
            cases,
            current_statements,
            current_label,
            loop_control,
        );

        // --- incrementor (the continue target) ---
        Self::flush_preceding_case_for_new_label(
            cases,
            current_statements,
            current_label,
            &mut self.state,
        );
        let incrementor_label = *current_label;
        if incr_idx.is_some() {
            self.process_expression_in_async(incr_idx, cases, current_statements, current_label);
        }

        // --- backedge to the condition case ---
        current_statements.push(Self::generator_break_statement(condition_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        let exit_label = self.state.next_label();
        Self::patch_if_break_target(cases, exit_placeholder, exit_label);
        Self::patch_if_break_target(cases, continue_placeholder, incrementor_label);
        *current_label = exit_label;
        true
    }

    /// Process a `for` initializer for the async state machine. A
    /// `VariableDeclarationList` initializer is lowered declaration-by-
    /// declaration (so its `var`s are hoisted and any await in an initializer
    /// is lifted); an expression initializer is processed as an expression
    /// statement.
    fn process_for_initializer_in_async(
        &mut self,
        init_idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(init_node) = self.arena.get(init_idx) else {
            return;
        };
        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            if let Some(var_list) = self.arena.get_variable(init_node) {
                let decls: Vec<NodeIndex> = var_list.declarations.nodes.clone();
                for decl_idx in decls {
                    self.process_variable_declaration(
                        decl_idx,
                        cases,
                        current_statements,
                        current_label,
                    );
                }
            }
        } else {
            self.process_expression_in_async(init_idx, cases, current_statements, current_label);
        }
    }

    /// Emit a non-suspending `for` whose initializer declares `var`s, hoisting
    /// the bindings into the `__awaiter` closure (matching tsc). Returns `true`
    /// when handled; `false` (caller emits verbatim) when the initializer is
    /// not a hoistable `var` declaration list.
    fn emit_for_with_hoisted_var_initializer(
        &mut self,
        init_idx: NodeIndex,
        cond_idx: NodeIndex,
        incr_idx: NodeIndex,
        body_idx: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) -> bool {
        if init_idx.is_none() {
            return false;
        }
        let Some(init_node) = self.arena.get(init_idx) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return false;
        }
        let Some(var_list) = self.arena.get_variable(init_node) else {
            return false;
        };
        let decls: Vec<NodeIndex> = var_list.declarations.nodes.clone();
        if decls.is_empty() {
            return false;
        }

        // Collect the (name, initializer) pairs. Each name is hoisted; an
        // initializer becomes an in-place assignment in the rewritten for-init.
        let mut hoisted: Vec<IRNode> = Vec::with_capacity(decls.len());
        let mut assignments: Vec<IRNode> = Vec::new();
        for decl_idx in decls {
            let Some(decl) = self.arena.get_variable_declaration_at(decl_idx) else {
                return false;
            };
            let name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, decl.name);
            hoisted.push(IRNode::VarDecl {
                name: name.clone().into(),
                initializer: None,
            });
            if decl.initializer.is_some() {
                let value = self.expression_to_ir(decl.initializer);
                assignments.push(IRNode::assign(IRNode::id(name), value));
            }
        }

        // Only the single-assignment shape prints exactly like tsc's for-init
        // (a comma sequence would print with parentheses); for the rare
        // multi-initializer case, fall back to verbatim emission.
        let initializer = match assignments.len() {
            0 => None,
            1 => Some(Box::new(
                assignments.into_iter().next().expect("len checked"),
            )),
            _ => return false,
        };

        current_statements.extend(hoisted);
        current_statements.push(IRNode::ForStatement {
            initializer,
            condition: cond_idx
                .is_some()
                .then(|| Box::new(self.expression_to_ir(cond_idx))),
            incrementor: incr_idx
                .is_some()
                .then(|| Box::new(self.expression_to_ir(incr_idx))),
            body: Box::new(self.statement_to_ir(body_idx)),
        });
        true
    }
}
