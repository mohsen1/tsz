use crate::transforms::async_es5_ir::state::{
    ForInAssignmentTarget, ForInSuspendedElementIndex, ForInSuspendedObject,
};
use crate::transforms::async_es5_ir::{AsyncES5Transformer, opcodes};
use crate::transforms::ir::{IRGeneratorCase, IRNode};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> AsyncES5Transformer<'a> {
    /// Process an if statement inside an async function body.
    ///
    /// When neither branch contains await, falls through to raw IR emission.
    /// When branches contain await, generates proper state machine labels.
    pub(in crate::transforms) fn process_if_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(if_stmt) = self.arena.get_if_statement(node) else {
            return;
        };

        let cond_has_await = self.contains_await_recursive(if_stmt.expression);
        let then_has_await = self.contains_await_recursive(if_stmt.then_statement);
        let else_has_await = if_stmt.else_statement.is_some()
            && self.contains_await_recursive(if_stmt.else_statement);

        if !cond_has_await && !then_has_await && !else_has_await {
            let has_else = if_stmt.else_statement.is_some()
                && self
                    .arena
                    .get(if_stmt.else_statement)
                    .is_some_and(|n| n.kind != syntax_kind_ext::EMPTY_STATEMENT);
            current_statements.push(IRNode::IfStatement {
                condition: Box::new(self.expression_to_ir(if_stmt.expression)),
                then_branch: Box::new(self.statement_to_hoistable_ir(if_stmt.then_statement)),
                else_branch: has_else
                    .then(|| Box::new(self.statement_to_hoistable_ir(if_stmt.else_statement))),
            });
            return;
        }

        // When the condition itself is or contains an await expression, yield the
        // condition first and use _a.sent() as the condition for the branch.
        // When no branch contains await but the condition does, we still need to
        // split cases around the yield.
        let cond_ir = if self.is_suspension_expression(if_stmt.expression) {
            // Condition IS directly an await expression: yield it, then check sent()
            self.process_await_expression(
                if_stmt.expression,
                cases,
                current_statements,
                current_label,
            );
            IRNode::GeneratorSent
        } else if cond_has_await {
            // Condition contains nested await: emit the suspension first
            self.emit_nested_suspension(
                if_stmt.expression,
                cases,
                current_statements,
                current_label,
            );
            self.expression_to_ir(if_stmt.expression)
        } else {
            self.expression_to_ir(if_stmt.expression)
        };

        if !then_has_await && !else_has_await {
            // Only the condition had await; the branches are await-free so emit a
            // simple if statement using the (now-resolved) condition IR value.
            let has_else = if_stmt.else_statement.is_some()
                && self
                    .arena
                    .get(if_stmt.else_statement)
                    .is_some_and(|n| n.kind != syntax_kind_ext::EMPTY_STATEMENT);
            let then_ir = self.statement_to_ir(if_stmt.then_statement);
            let else_ir = if has_else {
                Some(Box::new(self.statement_to_ir(if_stmt.else_statement)))
            } else {
                None
            };
            current_statements.push(IRNode::IfStatement {
                condition: Box::new(cond_ir),
                then_branch: Box::new(then_ir),
                else_branch: else_ir,
            });
            return;
        }

        let has_else = if_stmt.else_statement.is_some()
            && self
                .arena
                .get(if_stmt.else_statement)
                .is_some_and(|n| n.kind != syntax_kind_ext::EMPTY_STATEMENT);

        // Label allocation strategy:
        //
        // We need three logical labels:
        //   else_label  – where the else branch begins (or end_label when no else)
        //   end_label   – the merge point after both branches
        //
        // The problem: branches that contain `await` consume extra labels when they
        // are processed. Pre-allocating a label too early causes collisions with
        // the labels the branch allocates internally.
        //
        // Solution: use placeholders (MAX - counter) for labels that must be
        // allocated AFTER a suspending branch is processed, then patch them.
        //
        // Rules:
        //  - When then_has_await: else_label must be delayed (then branch allocates
        //    its yield-resume label first).
        //  - When either branch has await: end_label must be delayed (the awaiting
        //    branch allocates its yield-resume label, which must precede end_label).
        //
        // Non-awaiting branches that fall through to end_label need an explicit
        // `_a.label = end_label` assignment so the state machine advances correctly
        // on re-entry.

        let delayed_else_label = has_else && then_has_await;
        let delayed_end_label = then_has_await || else_has_await;

        let else_placeholder = delayed_else_label.then(|| self.next_loop_exit_placeholder());
        let end_placeholder = delayed_end_label.then(|| self.next_loop_exit_placeholder());

        let mut else_label: Option<u32> = if has_else && !delayed_else_label {
            Some(self.state.next_label())
        } else {
            // No else branch (no separate else label is needed), or the else label
            // must be delayed until the suspending then branch allocates its
            // resume labels.
            None
        };
        let mut end_label: Option<u32> = if delayed_end_label {
            None
        } else {
            // No branch suspends: end_label is safe to allocate now. With an else
            // branch the else label was just allocated above; without one this is
            // simply the next case after the then block.
            Some(self.state.next_label())
        };

        // Emit: if (!(condition)) return [3 /*break*/, else_or_end_placeholder];
        // - When there's an else branch: skip to else_label (or its placeholder).
        // - When no else branch: skip to end_label (or its placeholder).
        let branch_skip_target = if has_else {
            else_placeholder.unwrap_or_else(|| {
                else_label.expect("else label must be allocated when not delayed")
            })
        } else {
            end_placeholder.unwrap_or_else(|| {
                end_label.expect("end label must be allocated when not delayed and no else")
            })
        };
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".to_string().into(),
                operand: Box::new(cond_ir),
            }),
            target_label: branch_skip_target,
        });

        // Process then branch
        self.process_block_or_statement_in_async(
            if_stmt.then_statement,
            cases,
            current_statements,
            current_label,
        );

        if has_else {
            // Allocate else_label (and possibly end_label) now that then has been processed.
            if let Some(placeholder) = else_placeholder {
                let patched_else_label = self.state.next_label();
                Self::patch_if_break_target(cases, placeholder, patched_else_label);
                Self::patch_if_break_target_in_statements(
                    current_statements,
                    placeholder,
                    patched_else_label,
                );
                else_label = Some(patched_else_label);
            }
            // If end_label is also delayed and then_has_await, allocate it now (after
            // then-branch labels are consumed) but before the else branch runs.
            // When else_has_await, end_label must wait until after the else branch.
            if let Some(end_ph) = end_placeholder
                && !else_has_await
            {
                let patched_end_label = self.state.next_label();
                Self::patch_if_break_target(cases, end_ph, patched_end_label);
                Self::patch_if_break_target_in_statements(
                    current_statements,
                    end_ph,
                    patched_end_label,
                );
                end_label = Some(patched_end_label);
            }

            let else_l = else_label.expect("else label must be available before else branch");
            let end_l_or_ph = end_label.unwrap_or_else(|| {
                end_placeholder.expect("end placeholder must exist when end_label not yet resolved")
            });

            // Emit: return [3 /*break*/, end_label]; at end of then branch
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::BREAK,
                    value: Some(Box::new(IRNode::NumericLiteral(
                        end_l_or_ph.to_string().into(),
                    ))),
                    comment: Some("break".to_string().into()),
                },
            ))));

            // Flush current case and start else branch
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = else_l;

            // Process else branch
            self.process_block_or_statement_in_async(
                if_stmt.else_statement,
                cases,
                current_statements,
                current_label,
            );

            // Allocate end_label after the else branch if it was delayed.
            if let Some(end_ph) = end_placeholder
                && else_has_await
            {
                let patched_end_label = self.state.next_label();
                Self::patch_if_break_target(cases, end_ph, patched_end_label);
                Self::patch_if_break_target_in_statements(
                    current_statements,
                    end_ph,
                    patched_end_label,
                );
                end_label = Some(patched_end_label);
            }
            let end_l = end_label.expect("end label must be resolved after else branch");

            // Emit `_a.label = end_label` so the state machine falls through
            // correctly to the merge point on re-entry.  This is needed whenever
            // the last case of the else branch does not already return/break:
            //  - Else branch with no await: statements end without a return.
            //  - Else branch with await: after the yield-resume, `_a.sent()` is
            //    in current_statements and the generator needs the label hint.
            if !current_statements.is_empty()
                && !matches!(
                    current_statements.last(),
                    Some(
                        IRNode::ReturnStatement(_)
                            | IRNode::ThrowStatement(_)
                            | IRNode::BreakStatement(_)
                    )
                )
            {
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::GeneratorLabel,
                    IRNode::number(end_l.to_string()),
                ))));
            }

            // Flush current case and start end label
            if !current_statements.is_empty() {
                cases.push(IRGeneratorCase {
                    label: *current_label,
                    statements: std::mem::take(current_statements),
                });
            }
            *current_label = end_l;
        } else {
            // No else branch. When the then branch suspended, `end_label` was
            // delayed (see `delayed_end_label`) so the then branch's yield-resume
            // labels are allocated first. Resolve it now and patch the placeholder
            // that the initial `if (!cond) break` skip target referenced.
            if let Some(end_ph) = end_placeholder {
                let patched_end_label = self.state.next_label();
                Self::patch_if_break_target(cases, end_ph, patched_end_label);
                Self::patch_if_break_target_in_statements(
                    current_statements,
                    end_ph,
                    patched_end_label,
                );
                end_label = Some(patched_end_label);
            }
            let end_l = end_label.expect("end label must be available after if lowering");

            // Emit `_a.label = end_label` so the state machine falls through to the
            // merge point on re-entry, mirroring the else-branch path above. The
            // then branch suspended, so its yield-resume statements (`_a.sent()`)
            // are the last case; without the hint a re-entry would not advance.
            if !current_statements.is_empty()
                && !matches!(
                    current_statements.last(),
                    Some(
                        IRNode::ReturnStatement(_)
                            | IRNode::ThrowStatement(_)
                            | IRNode::BreakStatement(_)
                    )
                )
            {
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::GeneratorLabel,
                    IRNode::number(end_l.to_string()),
                ))));
            }

            // Flush current case and start end label
            if !current_statements.is_empty() {
                cases.push(IRGeneratorCase {
                    label: *current_label,
                    statements: std::mem::take(current_statements),
                });
            }
            *current_label = end_l;
        }
    }

    pub(in crate::transforms) fn process_captured_for_statement_in_async(
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
        if !self.loop_needs_async_capture(idx) {
            return false;
        }

        let Some((loop_var, init_text)) =
            self.simple_for_loop_var_initializer(loop_data.initializer)
        else {
            return false;
        };

        let loop_suffix = self.async_captured_for_loop_ordinal(idx);
        let loop_fn = format!("_loop_{loop_suffix}");
        let state_name = self.captured_for_loop_state_name(idx);
        let condition = self.ir_text(self.expression_to_ir(loop_data.condition));
        let incrementor = self.ir_text(self.expression_to_ir(loop_data.incrementor));
        let inner_body = self.captured_for_loop_inner_generator(loop_data.statement, &state_name);

        current_statements.push(IRNode::VarDecl {
            name: loop_fn.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::Raw(
            format!(
                "{loop_fn} = function ({loop_var}) {{\n                        return __generator(this, function (_b) {{\n                            switch (_b.label) {{\n{inner_body}                            }}\n                        }});\n                    }};"
            )
            .into(),
        ));
        current_statements.push(IRNode::VarDecl {
            name: loop_var.clone().into(),
            initializer: None,
        });
        if let Some(state_name) = &state_name {
            current_statements.push(IRNode::VarDecl {
                name: state_name.clone().into(),
                initializer: None,
            });
        }
        current_statements.push(IRNode::Raw(format!("{loop_var} = {init_text};").into()));
        current_statements.push(IRNode::Raw(
            format!("_a.label = {};", self.state.label_counter).into(),
        ));

        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        let condition_label = self.state.next_label();
        *current_label = condition_label;
        let after_yield_label = self.state.next_label();
        let increment_label = self.state.next_label();
        let exit_label = self.state.next_label();

        current_statements.push(IRNode::Raw(
            format!("if (!({condition})) return [3 /*break*/, {exit_label}];").into(),
        ));
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::YIELD_STAR,
                value: Some(Box::new(IRNode::CallExpr {
                    callee: Box::new(IRNode::Identifier(loop_fn.into())),
                    arguments: vec![IRNode::Identifier(loop_var.into())],
                })),
                comment: Some("yield*".to_string().into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: condition_label,
            statements: std::mem::take(current_statements),
        });

        if let Some(state_name) = &state_name {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::BinaryExpr {
                left: Box::new(IRNode::Identifier(state_name.clone().into())),
                operator: "=".to_string().into(),
                right: Box::new(IRNode::GeneratorSent),
            })));
            if self.captured_for_loop_has_break(loop_data.statement) {
                current_statements.push(IRNode::Raw(format!(
                    "if ({state_name} === \"break\")\n                        return [3 /*break*/, {exit_label}];"
                ).into()));
            }
            if self.captured_for_loop_has_value_return(loop_data.statement) {
                current_statements.push(IRNode::Raw(format!(
                    "if (typeof {state_name} === \"object\")\n                        return [2 /*return*/, {state_name}.value];"
                ).into()));
            }
        } else {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
        }
        current_statements.push(IRNode::Raw(format!("_a.label = {increment_label};").into()));
        cases.push(IRGeneratorCase {
            label: after_yield_label,
            statements: std::mem::take(current_statements),
        });

        current_statements.push(IRNode::Raw(format!("{incrementor};").into()));
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::BREAK,
                value: Some(Box::new(IRNode::NumericLiteral(
                    condition_label.to_string().into(),
                ))),
                comment: Some("break".to_string().into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: increment_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = exit_label;
        true
    }

    pub(in crate::transforms) fn process_for_in_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FOR_IN_STATEMENT {
            return false;
        }
        let Some(for_in) = self.arena.get_for_in_of(node) else {
            return false;
        };

        let initializer_has_suspension = self.contains_await_recursive(for_in.initializer);
        let expression_has_suspension = self.contains_await_recursive(for_in.expression);
        let body_has_suspension = self.contains_await_recursive(for_in.statement);
        if !initializer_has_suspension && !expression_has_suspension && !body_has_suspension {
            return self.process_simple_for_in_statement(for_in, current_statements);
        }
        if self.for_in_body_has_unsupported_control_flow(for_in.statement) {
            return false;
        }

        let object_suspension = self.direct_suspension_expression(for_in.expression);
        if expression_has_suspension && object_suspension.is_none() {
            return false;
        }

        let Some((assignment_target, declared_iteration_name)) =
            self.for_in_assignment_target(for_in.initializer)
        else {
            return false;
        };

        let object_temp = self.generate_hoisted_temp();
        let keys_temp = self.generate_hoisted_temp();
        let key_temp = self.generate_hoisted_temp();
        let index_temp = self.fresh_reserved_name("_i");
        let target_object_temp = if matches!(
            assignment_target,
            ForInAssignmentTarget::SuspendedElement {
                index: ForInSuspendedElementIndex::Suspended(_),
                ..
            }
        ) {
            Some(self.generate_hoisted_temp())
        } else {
            None
        };

        for name in [&object_temp, &keys_temp, &key_temp, &index_temp] {
            current_statements.push(IRNode::VarDecl {
                name: name.clone().into(),
                initializer: None,
            });
        }
        if let Some(temp) = &target_object_temp {
            current_statements.push(IRNode::VarDecl {
                name: temp.clone().into(),
                initializer: None,
            });
        }
        if let Some(iteration_name) = declared_iteration_name {
            current_statements.push(IRNode::VarDecl {
                name: iteration_name.into(),
                initializer: None,
            });
        }

        let object_value = if let Some(suspension) = object_suspension {
            self.process_await_expression(suspension, cases, current_statements, current_label);
            IRNode::GeneratorSent
        } else {
            self.expression_to_ir(for_in.expression)
        };

        current_statements.push(Self::expression_statement(IRNode::assign(
            IRNode::id(object_temp.clone()),
            object_value,
        )));
        current_statements.push(Self::expression_statement(IRNode::assign(
            IRNode::id(keys_temp.clone()),
            IRNode::ArrayLiteral(Vec::new()),
        )));
        current_statements.push(IRNode::ForInOfStatement {
            kind: "in".into(),
            initializer: Box::new(IRNode::id(key_temp.clone())),
            expression: Box::new(IRNode::id(object_temp.clone())),
            body: Box::new(Self::expression_statement(IRNode::CallExpr {
                callee: Box::new(IRNode::prop(IRNode::id(keys_temp.clone()), "push")),
                arguments: vec![IRNode::id(key_temp.clone())],
            })),
            multiline_body: true,
        });
        current_statements.push(Self::expression_statement(IRNode::assign(
            IRNode::id(index_temp.clone()),
            IRNode::number("0"),
        )));

        let loop_label = self.state.next_label();
        let increment_placeholder = self.next_loop_exit_placeholder();
        let end_placeholder = self.next_loop_exit_placeholder();
        current_statements.push(Self::generator_label_assignment(loop_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });
        *current_label = loop_label;

        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".into(),
                operand: Box::new(IRNode::Parenthesized(Box::new(IRNode::binary(
                    IRNode::id(index_temp.clone()),
                    "<",
                    IRNode::prop(IRNode::id(keys_temp.clone()), "length"),
                )))),
            }),
            target_label: end_placeholder,
        });
        current_statements.push(Self::expression_statement(IRNode::assign(
            IRNode::id(key_temp.clone()),
            IRNode::elem(IRNode::id(keys_temp), IRNode::id(index_temp.clone())),
        )));
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".into(),
                operand: Box::new(IRNode::Parenthesized(Box::new(IRNode::binary(
                    IRNode::id(key_temp.clone()),
                    "in",
                    IRNode::id(object_temp),
                )))),
            }),
            target_label: increment_placeholder,
        });
        match assignment_target {
            ForInAssignmentTarget::Direct(target) => {
                current_statements.push(Self::expression_statement(IRNode::assign(
                    *target,
                    IRNode::id(key_temp),
                )));
            }
            ForInAssignmentTarget::SuspendedProperty {
                object_suspension,
                property,
            } => {
                self.process_await_expression(
                    object_suspension,
                    cases,
                    current_statements,
                    current_label,
                );
                current_statements.push(Self::expression_statement(IRNode::assign(
                    IRNode::prop(
                        IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
                        property,
                    ),
                    IRNode::id(key_temp),
                )));
            }
            ForInAssignmentTarget::SuspendedElement { object, index } => match index {
                ForInSuspendedElementIndex::Direct(index) => {
                    let ForInSuspendedObject::Suspended(object_suspension) = object else {
                        return false;
                    };
                    self.process_await_expression(
                        object_suspension,
                        cases,
                        current_statements,
                        current_label,
                    );
                    current_statements.push(Self::expression_statement(IRNode::assign(
                        IRNode::elem(
                            IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
                            *index,
                        ),
                        IRNode::id(key_temp),
                    )));
                }
                ForInSuspendedElementIndex::Suspended(index_suspension) => {
                    let Some(temp) = target_object_temp else {
                        return false;
                    };
                    match object {
                        ForInSuspendedObject::Direct(object) => {
                            current_statements.push(Self::expression_statement(IRNode::assign(
                                IRNode::id(temp.clone()),
                                *object,
                            )));
                        }
                        ForInSuspendedObject::Suspended(object_suspension) => {
                            self.process_await_expression(
                                object_suspension,
                                cases,
                                current_statements,
                                current_label,
                            );
                            current_statements.push(Self::expression_statement(IRNode::assign(
                                IRNode::id(temp.clone()),
                                IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
                            )));
                        }
                    }
                    self.process_await_expression(
                        index_suspension,
                        cases,
                        current_statements,
                        current_label,
                    );
                    current_statements.push(Self::expression_statement(IRNode::assign(
                        IRNode::elem(IRNode::id(temp), IRNode::GeneratorSent),
                        IRNode::id(key_temp),
                    )));
                }
            },
        }

        self.process_block_or_statement_in_async(
            for_in.statement,
            cases,
            current_statements,
            current_label,
        );

        let increment_label = self.state.next_label();
        let end_label = self.state.next_label();
        current_statements.push(Self::generator_label_assignment(increment_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        current_statements.push(Self::expression_statement(IRNode::PostfixUnaryExpr {
            operand: Box::new(IRNode::id(index_temp)),
            operator: "++".into(),
        }));
        current_statements.push(Self::generator_break_statement(loop_label));
        cases.push(IRGeneratorCase {
            label: increment_label,
            statements: std::mem::take(current_statements),
        });

        Self::patch_if_break_target(cases, increment_placeholder, increment_label);
        Self::patch_if_break_target(cases, end_placeholder, end_label);
        *current_label = end_label;
        true
    }

    pub(in crate::transforms) fn process_simple_for_in_statement(
        &self,
        for_in: &tsz_parser::parser::node::ForInOfData,
        current_statements: &mut Vec<IRNode>,
    ) -> bool {
        let Some((target, declared_iteration_name)) =
            self.for_in_direct_assignment_target(for_in.initializer)
        else {
            return false;
        };
        if let Some(iteration_name) = declared_iteration_name {
            current_statements.push(IRNode::VarDecl {
                name: iteration_name.into(),
                initializer: None,
            });
        }
        current_statements.push(IRNode::ForInOfStatement {
            kind: "in".into(),
            initializer: Box::new(target),
            expression: Box::new(self.expression_to_ir(for_in.expression)),
            body: Box::new(self.statement_to_ir(for_in.statement)),
            multiline_body: false,
        });
        true
    }

    pub(in crate::transforms::async_es5_ir) fn for_in_assignment_target(
        &self,
        initializer: NodeIndex,
    ) -> Option<(ForInAssignmentTarget, Option<String>)> {
        if self.contains_await_recursive(initializer) {
            return self
                .for_in_suspended_assignment_target(initializer)
                .map(|target| (target, None));
        }
        self.for_in_direct_assignment_target(initializer)
            .map(|(target, declared_name)| {
                (
                    ForInAssignmentTarget::Direct(Box::new(target)),
                    declared_name,
                )
            })
    }

    pub(in crate::transforms) fn for_in_direct_assignment_target(
        &self,
        initializer: NodeIndex,
    ) -> Option<(IRNode, Option<String>)> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            let decl_list = self.arena.get_variable(init_node)?;
            if decl_list.declarations.nodes.len() != 1 {
                return None;
            }
            let decl_idx = *decl_list.declarations.nodes.first()?;
            let decl_node = self.arena.get(decl_idx)?;
            let decl = self.arena.get_variable_declaration(decl_node)?;
            if decl.initializer.is_some() {
                return None;
            }
            let name = crate::transforms::emit_utils::identifier_text(self.arena, decl.name)?;
            return Some((IRNode::id(name.clone()), Some(name)));
        }
        if init_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let name = crate::transforms::emit_utils::identifier_text(self.arena, initializer)?;
            return Some((IRNode::id(name), None));
        }
        if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || init_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return Some((self.expression_to_ir(initializer), None));
        }
        None
    }

    pub(in crate::transforms::async_es5_ir) fn for_in_suspended_assignment_target(
        &self,
        initializer: NodeIndex,
    ) -> Option<ForInAssignmentTarget> {
        let initializer = self.strip_parenthesized_expression(initializer);
        let init_node = self.arena.get(initializer)?;
        if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(init_node)?;
            let object_suspension = self.direct_suspension_expression(access.expression)?;
            let property = crate::transforms::emit_utils::identifier_text_or_empty(
                self.arena,
                access.name_or_argument,
            );
            return Some(ForInAssignmentTarget::SuspendedProperty {
                object_suspension,
                property,
            });
        }
        if init_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(init_node)?;
            let object = if let Some(object_suspension) =
                self.direct_suspension_expression(access.expression)
            {
                ForInSuspendedObject::Suspended(object_suspension)
            } else if self.contains_await_recursive(access.expression) {
                return None;
            } else {
                ForInSuspendedObject::Direct(Box::new(self.expression_to_ir(access.expression)))
            };
            let index = if let Some(index_suspension) =
                self.direct_suspension_expression(access.name_or_argument)
            {
                ForInSuspendedElementIndex::Suspended(index_suspension)
            } else if self.contains_await_recursive(access.name_or_argument) {
                return None;
            } else {
                ForInSuspendedElementIndex::Direct(Box::new(
                    self.expression_to_ir(access.name_or_argument),
                ))
            };
            if matches!(object, ForInSuspendedObject::Direct(_))
                && matches!(index, ForInSuspendedElementIndex::Direct(_))
            {
                return None;
            }
            return Some(ForInAssignmentTarget::SuspendedElement { object, index });
        }
        None
    }

    pub(in crate::transforms) fn direct_suspension_expression(
        &self,
        expression: NodeIndex,
    ) -> Option<NodeIndex> {
        let expression = self.strip_parenthesized_expression(expression);
        self.is_suspension_expression(expression)
            .then_some(expression)
    }

    pub(in crate::transforms) fn for_in_body_has_unsupported_control_flow(
        &self,
        idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.is_function_expression_or_arrow()
        {
            return false;
        }
        match node.kind {
            k if k == syntax_kind_ext::BREAK_STATEMENT
                || k == syntax_kind_ext::CONTINUE_STATEMENT
                || k == syntax_kind_ext::RETURN_STATEMENT =>
            {
                true
            }
            k if k == syntax_kind_ext::BLOCK || k == syntax_kind_ext::CASE_BLOCK => {
                self.arena.get_block(node).is_some_and(|block| {
                    block
                        .statements
                        .nodes
                        .iter()
                        .any(|&stmt| self.for_in_body_has_unsupported_control_flow(stmt))
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.arena.get_if_statement(node).is_some_and(|if_stmt| {
                    self.for_in_body_has_unsupported_control_flow(if_stmt.then_statement)
                        || self.for_in_body_has_unsupported_control_flow(if_stmt.else_statement)
                })
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT
                || k == syntax_kind_ext::SWITCH_STATEMENT
                || k == syntax_kind_ext::TRY_STATEMENT
                || k == syntax_kind_ext::LABELED_STATEMENT =>
            {
                true
            }
            _ => false,
        }
    }

    pub(in crate::transforms) fn expression_statement(expression: IRNode) -> IRNode {
        IRNode::ExpressionStatement(Box::new(expression))
    }

    pub(in crate::transforms) fn negated_condition(condition: IRNode) -> IRNode {
        let operand = match condition {
            IRNode::BinaryExpr { .. }
            | IRNode::LogicalOr { .. }
            | IRNode::LogicalAnd { .. }
            | IRNode::ConditionalExpr { .. }
            | IRNode::CommaExpr(_)
            | IRNode::CommaExprMultiline(_)
            | IRNode::CommaExprMultilineFlat(_) => IRNode::Parenthesized(Box::new(condition)),
            _ => condition,
        };
        IRNode::PrefixUnaryExpr {
            operator: "!".into(),
            operand: Box::new(operand),
        }
    }

    pub(in crate::transforms) fn generator_label_assignment(label: u32) -> IRNode {
        Self::expression_statement(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(label.to_string()),
        ))
    }

    pub(in crate::transforms) fn loop_needs_async_capture(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        let Some(loop_data) = self.arena.get_loop(node) else {
            return false;
        };
        if !self.contains_await_recursive(loop_data.statement) {
            return false;
        }
        let loop_vars = crate::transforms::block_scoping_es5::collect_loop_vars(
            self.arena,
            loop_data.initializer,
        );
        if loop_vars.is_empty() {
            return false;
        }
        crate::transforms::block_scoping_es5::analyze_loop_capture(
            self.arena,
            loop_data.statement,
            &loop_vars,
        )
        .needs_capture
    }

    pub(in crate::transforms) fn async_captured_for_loop_ordinal(&self, idx: NodeIndex) -> usize {
        let Some(current) = self.arena.get(idx) else {
            return 1;
        };
        self.arena
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, node)| {
                node.pos <= current.pos
                    && node.kind == syntax_kind_ext::FOR_STATEMENT
                    && self.loop_needs_async_capture(NodeIndex(*i as u32))
            })
            .count()
    }

    pub(in crate::transforms) fn captured_for_loop_state_name(
        &self,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = self.arena.get(idx)?;
        let loop_data = self.arena.get_loop(node)?;
        if !self.captured_for_loop_has_break(loop_data.statement)
            && !self.captured_for_loop_has_value_return(loop_data.statement)
        {
            return None;
        }
        let current = self.arena.get(idx)?;
        let ordinal = self
            .arena
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, node)| {
                node.pos <= current.pos
                    && node.kind == syntax_kind_ext::FOR_STATEMENT
                    && self.loop_needs_async_capture(NodeIndex(*i as u32))
                    && self.arena.get_loop(node).is_some_and(|loop_data| {
                        self.captured_for_loop_has_break(loop_data.statement)
                            || self.captured_for_loop_has_value_return(loop_data.statement)
                    })
            })
            .count();
        Some(format!("state_{ordinal}"))
    }

    pub(in crate::transforms) fn simple_for_loop_var_initializer(
        &self,
        initializer: NodeIndex,
    ) -> Option<(String, String)> {
        let init_node = self.arena.get(initializer)?;
        let var_list = self.arena.get_variable(init_node)?;
        let decl_idx = *var_list.declarations.nodes.first()?;
        let decl = self.arena.get_variable_declaration_at(decl_idx)?;
        Some((
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, decl.name),
            self.ir_text(self.expression_to_ir(decl.initializer)),
        ))
    }

    pub(in crate::transforms) fn captured_for_loop_inner_generator(
        &mut self,
        body: NodeIndex,
        state_name: &Option<String>,
    ) -> String {
        let mut lines = Vec::new();
        lines.push("                                case 0: return [4 /*yield*/, 1];".to_string());
        lines.push("                                case 1:".to_string());
        lines.push("                                    _b.sent();".to_string());

        if let Some(block_node) = self.arena.get(body)
            && let Some(block) = self.arena.get_block(block_node)
        {
            for &stmt_idx in &block.statements.nodes {
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
                    if let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node)
                        && self.is_suspension_expression(expr_stmt.expression)
                    {
                        continue;
                    }
                    lines.push(format!(
                        "                                    {}",
                        self.ir_text(self.statement_to_ir(stmt_idx))
                    ));
                } else if stmt_node.kind == syntax_kind_ext::BREAK_STATEMENT {
                    lines.push(
                        "                                    return [2 /*return*/, \"break\"];"
                            .to_string(),
                    );
                } else if stmt_node.kind == syntax_kind_ext::CONTINUE_STATEMENT {
                    lines.push(
                        "                                    return [2 /*return*/, \"continue\"];"
                            .to_string(),
                    );
                } else if stmt_node.kind == syntax_kind_ext::RETURN_STATEMENT {
                    if let Some(ret) = self.arena.get_return_statement(stmt_node) {
                        let value = self.ir_text(self.expression_to_ir(ret.expression));
                        lines.push(format!(
                            "                                    return [2 /*return*/, {{ value: {value} }}];"
                        ));
                    }
                }
            }
        }

        if state_name.is_none() && !self.captured_for_loop_has_continue(body) {
            lines.push("                                    return [2 /*return*/];".to_string());
        }
        lines.join("\n") + "\n"
    }

    pub(in crate::transforms) fn captured_for_loop_has_break(&self, body: NodeIndex) -> bool {
        self.block_contains_statement_kind(body, syntax_kind_ext::BREAK_STATEMENT)
    }

    pub(in crate::transforms) fn captured_for_loop_has_continue(&self, body: NodeIndex) -> bool {
        self.block_contains_statement_kind(body, syntax_kind_ext::CONTINUE_STATEMENT)
    }

    pub(in crate::transforms) fn captured_for_loop_has_value_return(
        &self,
        body: NodeIndex,
    ) -> bool {
        self.block_contains_statement_kind(body, syntax_kind_ext::RETURN_STATEMENT)
    }

    pub(in crate::transforms) fn block_contains_statement_kind(
        &self,
        body: NodeIndex,
        kind: u16,
    ) -> bool {
        let Some(block_node) = self.arena.get(body) else {
            return false;
        };
        let Some(block) = self.arena.get_block(block_node) else {
            return false;
        };
        block.statements.nodes.iter().any(|&stmt_idx| {
            self.arena
                .get(stmt_idx)
                .is_some_and(|stmt_node| stmt_node.kind == kind)
        })
    }

    pub(in crate::transforms) fn ir_text(&self, ir: IRNode) -> String {
        crate::transforms::ir_printer::IRPrinter::emit_to_string(&ir)
    }
}
