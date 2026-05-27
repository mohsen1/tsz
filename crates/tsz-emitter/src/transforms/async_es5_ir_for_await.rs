//! Async ES5 `for await..of` state-machine lowering.

use super::{AsyncES5Transformer, loop_control, opcodes};
use crate::transforms::ir::{IRGeneratorCase, IRNode};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> AsyncES5Transformer<'a> {
    pub(super) fn process_for_await_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        labeled_continue: Option<&str>,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
            return false;
        }
        let Some(for_in_of) = self.arena.get_for_in_of(node) else {
            return false;
        };
        if !for_in_of.await_modifier
            || crate::transforms::emit_utils::for_of_using_info(self.arena, for_in_of.initializer)
                .is_some()
        {
            return false;
        }
        let Some((target_name, declared_name)) =
            self.simple_for_await_iteration_target(for_in_of.initializer)
        else {
            return false;
        };
        let captured_loop_fn =
            self.captured_for_await_loop_function_name(idx, for_in_of, declared_name.as_deref());

        self.helpers_needed.mark_async_values();

        let loop_guard_name = self.generate_hoisted_temp();
        let (iterator_name, result_name) = self.for_await_iterator_names(for_in_of.expression, 1);
        let catch_value_name = self.fresh_reserved_name("e_1_1");

        if let Some(loop_fn) = &captured_loop_fn {
            current_statements.push(IRNode::var_decl(loop_fn.clone(), None));
        }
        for name in [
            loop_guard_name.as_str(),
            iterator_name.as_str(),
            result_name.as_str(),
        ] {
            current_statements.push(IRNode::var_decl(name.to_string(), None));
        }
        if declared_name.is_some() && captured_loop_fn.is_none() {
            current_statements.push(IRNode::var_decl(target_name.clone(), None));
        }
        current_statements.push(IRNode::var_decl(catch_value_name.clone(), None));

        current_statements.push(IRNode::HoistedVarGroupBreak);
        let done_name = self.generate_hoisted_temp();
        let error_name = self.fresh_reserved_name("e_1");
        let return_name = self.generate_hoisted_temp();
        let value_name = self.generate_hoisted_temp();
        for name in [&done_name, &error_name, &return_name, &value_name] {
            current_statements.push(IRNode::var_decl(name.clone(), None));
        }
        let captured_loop_assignment = if let Some(loop_fn) = &captured_loop_fn {
            let Some(loop_assignment) = self.captured_for_await_loop_function_assignment(
                loop_fn,
                &target_name,
                &value_name,
                &result_name,
                &loop_guard_name,
                for_in_of.statement,
            ) else {
                return false;
            };
            Some(loop_assignment)
        } else {
            None
        };

        let loop_yield_label = self.state.next_label();
        let after_next_label = self.state.next_label();
        let iteration_label = self.state.next_label();
        let loop_exit_label = self.state.next_label();
        let catch_label = self.state.next_label();
        let finally_label = self.state.next_label();
        let return_resume_label = self.state.next_label();
        let return_done_label = self.state.next_label();
        let rethrow_label = self.state.next_label();
        let outer_endfinally_label = self.state.next_label();
        let end_label = self.state.next_label();

        let iterable = self.for_of_iterable_to_ir_with_es5_computed_temps(
            for_in_of.expression,
            current_statements,
        );

        current_statements.push(IRNode::GeneratorTryPush {
            start_label: *current_label,
            catch_label,
            finally_label,
            end_label,
        });
        if let Some(loop_assignment) = captured_loop_assignment {
            current_statements.push(loop_assignment);
        }
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::binary(
            IRNode::assign(
                IRNode::id(loop_guard_name.clone()),
                IRNode::BooleanLiteral(true),
            ),
            ",",
            IRNode::assign(
                IRNode::id(iterator_name.clone()),
                IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__asyncValues".into())),
                    arguments: vec![iterable],
                },
            ),
        ))));
        current_statements.push(Self::generator_label_assignment(loop_yield_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = loop_yield_label;
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::YIELD,
                value: Some(Box::new(self.for_await_yield_value(IRNode::CallExpr {
                    callee: Box::new(IRNode::prop(IRNode::id(iterator_name.clone()), "next")),
                    arguments: vec![],
                }))),
                comment: Some("yield".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = after_next_label;
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".into(),
                operand: Box::new(IRNode::CommaExpr(vec![
                    IRNode::assign(IRNode::id(result_name.clone()), IRNode::GeneratorSent),
                    IRNode::assign(
                        IRNode::id(done_name.clone()),
                        IRNode::prop(IRNode::id(result_name.clone()), "done"),
                    ),
                    IRNode::PrefixUnaryExpr {
                        operator: "!".into(),
                        operand: Box::new(IRNode::id(done_name.clone())),
                    },
                ])),
            }),
            target_label: loop_exit_label,
        });
        let using_captured_loop = captured_loop_fn.is_some();
        if let Some(loop_fn) = captured_loop_fn {
            current_statements.push(Self::expression_statement(IRNode::call(
                IRNode::id(loop_fn),
                vec![],
            )));
        } else {
            current_statements.push(Self::expression_statement(IRNode::assign(
                IRNode::id(value_name.clone()),
                IRNode::prop(IRNode::id(result_name), "value"),
            )));
            current_statements.push(Self::expression_statement(IRNode::assign(
                IRNode::id(loop_guard_name.clone()),
                IRNode::BooleanLiteral(false),
            )));
            current_statements.push(Self::expression_statement(IRNode::assign(
                IRNode::id(target_name),
                IRNode::id(value_name),
            )));
        }

        let label_stack_len = self.labeled_continue_targets.len();
        if let Some(label) = labeled_continue {
            self.labeled_continue_targets
                .push((label.to_string(), iteration_label));
        }
        if !using_captured_loop {
            self.process_loop_body_statement_in_async(
                for_in_of.statement,
                cases,
                current_statements,
                current_label,
                loop_control::AsyncLoopControlTargets {
                    break_label: loop_exit_label,
                    continue_label: iteration_label,
                },
            );
        }
        self.labeled_continue_targets.truncate(label_stack_len);

        current_statements.push(Self::generator_label_assignment(iteration_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = iteration_label;
        current_statements.push(Self::expression_statement(IRNode::assign(
            IRNode::id(loop_guard_name.clone()),
            IRNode::BooleanLiteral(true),
        )));
        current_statements.push(Self::generator_break_statement(loop_yield_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = loop_exit_label;
        current_statements.push(Self::generator_break_statement(end_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = catch_label;
        current_statements.push(Self::expression_statement(IRNode::assign(
            IRNode::id(catch_value_name.clone()),
            IRNode::GeneratorSent,
        )));
        current_statements.push(Self::expression_statement(IRNode::assign(
            IRNode::id(error_name.clone()),
            IRNode::object(vec![crate::transforms::ir::IRProperty {
                key: crate::transforms::ir::IRPropertyKey::Identifier("error".into()),
                value: IRNode::id(catch_value_name),
                kind: crate::transforms::ir::IRPropertyKind::Init,
            }]),
        )));
        current_statements.push(Self::generator_break_statement(end_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = finally_label;
        current_statements.push(IRNode::GeneratorTryPushFinally {
            start_label: finally_label,
            finally_label: rethrow_label,
            end_label: outer_endfinally_label,
        });
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".into(),
                operand: Box::new(IRNode::Parenthesized(Box::new(IRNode::logical_and(
                    IRNode::logical_and(
                        IRNode::PrefixUnaryExpr {
                            operator: "!".into(),
                            operand: Box::new(IRNode::id(loop_guard_name)),
                        },
                        IRNode::PrefixUnaryExpr {
                            operator: "!".into(),
                            operand: Box::new(IRNode::id(done_name)),
                        },
                    ),
                    IRNode::Parenthesized(Box::new(IRNode::assign(
                        IRNode::id(return_name.clone()),
                        IRNode::prop(IRNode::id(iterator_name.clone()), "return"),
                    ))),
                )))),
            }),
            target_label: return_done_label,
        });
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::YIELD,
                value: Some(Box::new(self.for_await_yield_value(IRNode::CallExpr {
                    callee: Box::new(IRNode::prop(IRNode::id(return_name), "call")),
                    arguments: vec![IRNode::id(iterator_name)],
                }))),
                comment: Some("yield".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = return_resume_label;
        current_statements.push(Self::expression_statement(IRNode::GeneratorSent));
        current_statements.push(Self::generator_label_assignment(return_done_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = return_done_label;
        current_statements.push(Self::generator_break_statement(outer_endfinally_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = rethrow_label;
        current_statements.push(IRNode::IfStatement {
            condition: Box::new(IRNode::id(error_name.clone())),
            then_branch: Box::new(IRNode::ThrowStatement(Box::new(IRNode::prop(
                IRNode::id(error_name),
                "error",
            )))),
            else_branch: None,
        });
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::END_FINALLY,
                value: None,
                comment: Some("endfinally".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = outer_endfinally_label;
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::END_FINALLY,
                value: None,
                comment: Some("endfinally".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = end_label;
        true
    }

    fn simple_for_await_iteration_target(
        &self,
        initializer: NodeIndex,
    ) -> Option<(String, Option<String>)> {
        let node = self.arena.get(initializer)?;
        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            let decl_list = self.arena.get_variable(node)?;
            if decl_list.declarations.nodes.len() != 1 {
                return None;
            }
            let decl_idx = *decl_list.declarations.nodes.first()?;
            let decl_node = self.arena.get(decl_idx)?;
            let decl = self.arena.get_variable_declaration(decl_node)?;
            if decl.initializer.is_some() {
                return None;
            }
            let name_node = self.arena.get(decl.name)?;
            if name_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                return None;
            }
            let name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, decl.name);
            return (!name.is_empty()).then(|| (name.clone(), Some(name)));
        }

        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, initializer);
            return (!name.is_empty()).then_some((name, None));
        }

        None
    }

    fn captured_for_await_loop_function_name(
        &self,
        idx: NodeIndex,
        for_await: &tsz_parser::parser::node::ForInOfData,
        declared_name: Option<&str>,
    ) -> Option<String> {
        declared_name?;
        if self.contains_await_recursive(for_await.initializer)
            || self.contains_await_recursive(for_await.expression)
            || self.contains_await_recursive(for_await.statement)
            || self.captured_for_loop_has_break(for_await.statement)
            || self.captured_for_loop_has_continue(for_await.statement)
            || self.captured_for_loop_has_value_return(for_await.statement)
            || !self.for_await_needs_iteration_capture(for_await)
        {
            return None;
        }

        let loop_suffix = self.async_captured_for_await_loop_ordinal(idx);
        Some(self.fresh_reserved_name(format!("_loop_{loop_suffix}")))
    }

    fn for_await_needs_iteration_capture(
        &self,
        for_await: &tsz_parser::parser::node::ForInOfData,
    ) -> bool {
        let loop_vars = crate::transforms::block_scoping_es5::collect_loop_vars(
            self.arena,
            for_await.initializer,
        );
        if loop_vars.is_empty() {
            return false;
        }
        crate::transforms::block_scoping_es5::analyze_loop_capture(
            self.arena,
            for_await.statement,
            &loop_vars,
        )
        .needs_capture
    }

    fn async_captured_for_await_loop_ordinal(&self, idx: NodeIndex) -> usize {
        let Some(current) = self.arena.get(idx) else {
            return 1;
        };
        self.arena
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, node)| {
                node.pos <= current.pos
                    && matches!(
                        node.kind,
                        k if k == syntax_kind_ext::FOR_STATEMENT
                            || k == syntax_kind_ext::FOR_OF_STATEMENT
                    )
                    && self.async_captured_for_await_ordinal_candidate(NodeIndex(*i as u32))
            })
            .count()
            .max(1)
    }

    fn async_captured_for_await_ordinal_candidate(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::FOR_STATEMENT {
            return self.loop_needs_async_capture(idx);
        }
        if node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
            return false;
        }
        self.arena.get_for_in_of(node).is_some_and(|for_of| {
            if for_of.await_modifier {
                self.for_await_needs_iteration_capture(for_of)
            } else {
                self.contains_await_recursive(for_of.statement)
                    && self.for_await_needs_iteration_capture(for_of)
            }
        })
    }

    fn captured_for_await_loop_function_assignment(
        &self,
        loop_fn: &str,
        target_name: &str,
        value_name: &str,
        result_name: &str,
        loop_guard_name: &str,
        body: NodeIndex,
    ) -> Option<IRNode> {
        let mut function_body = vec![
            Self::expression_statement(IRNode::assign(
                IRNode::id(value_name.to_string()),
                IRNode::prop(IRNode::id(result_name.to_string()), "value"),
            )),
            Self::expression_statement(IRNode::assign(
                IRNode::id(loop_guard_name.to_string()),
                IRNode::BooleanLiteral(false),
            )),
            IRNode::var_decl(
                target_name.to_string(),
                Some(IRNode::id(value_name.to_string())),
            ),
        ];

        let body_node = self.arena.get(body)?;
        if body_node.kind == syntax_kind_ext::BLOCK {
            let block = self.arena.get_block(body_node)?;
            for &stmt in &block.statements.nodes {
                function_body.push(self.captured_for_await_body_statement_ir(stmt)?);
            }
        } else {
            function_body.push(self.captured_for_await_body_statement_ir(body)?);
        }

        Some(Self::expression_statement(IRNode::assign(
            IRNode::id(loop_fn.to_string()),
            IRNode::FunctionExpr {
                name: None,
                parameters: Vec::new(),
                body: function_body,
                is_expression_body: false,
                body_source_range: None,
            },
        )))
    }

    fn captured_for_await_body_statement_ir(&self, statement: NodeIndex) -> Option<IRNode> {
        let options = crate::emitter::PrinterOptions {
            target: crate::emitter::ScriptTarget::ES5,
            remove_comments: true,
            ..Default::default()
        };
        let ctx = crate::context::emit::EmitContext::with_options(options.clone());
        let transforms = crate::lowering::LoweringPass::new(self.arena, &ctx).run(statement);
        let mut printer =
            crate::emitter::Printer::with_transforms_and_options(self.arena, transforms, options);
        if let Some(text) = self.source_text {
            printer.set_source_text(text);
        }
        printer.emit(statement);
        let output = printer
            .get_output()
            .trim()
            .replace('\n', "\n                    ");
        (!output.is_empty()).then(|| IRNode::Raw(output.into()))
    }

    fn for_await_yield_value(&self, value: IRNode) -> IRNode {
        if self.async_generator_mode {
            IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__await".into())),
                arguments: vec![value],
            }
        } else {
            value
        }
    }
}
