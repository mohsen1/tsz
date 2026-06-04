impl<'a> AsyncES5Transformer<'a> {
    fn process_for_await_using_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
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
        if !for_in_of.await_modifier {
            return false;
        }
        let Some(using_info) =
            super::emit_utils::for_of_using_info(self.arena, for_in_of.initializer)
        else {
            return false;
        };

        self.helpers_needed.mark_async_values();
        self.helpers_needed.add_disposable_resource = true;
        self.helpers_needed.dispose_resources = true;

        let loop_guard_name = self.generate_hoisted_temp();
        let env_id = self.disposable_env_counter.get();
        let (iterator_name, result_name) =
            self.for_await_iterator_names(for_in_of.expression, env_id);
        let binding_name = if using_info.recovered_missing_binding {
            self.generate_hoisted_temp()
        } else {
            using_info.binding_name
        };
        let value_binding_name = format!("{binding_name}_1");

        let (env_name, resource_error_name, dispose_result_name, resource_error_id) =
            if using_info.using_async {
                let (env_name, error_name, result_name, error_id) =
                    self.next_disposable_env_names_allowing_error_gap();
                (env_name, error_name, Some(result_name), error_id)
            } else {
                let env_id = self.disposable_env_counter.get();
                self.disposable_env_counter.set(env_id + 1);
                let env_name = format!("env_{env_id}");
                self.blocked_disposable_env_names.insert(env_name.clone());
                self.generated_disposable_env_names.push(env_name.clone());
                (env_name, format!("e_{}", env_id + 1), None, env_id + 1)
            };

        let outer_error_id = if using_info.using_async {
            resource_error_id + 1
        } else {
            self.env_id_from_name(&env_name).unwrap_or(1)
        };
        let outer_error_name = format!("e_{outer_error_id}");
        let outer_catch_error_name = format!("{outer_error_name}_1");

        for name in [
            loop_guard_name.as_str(),
            iterator_name.as_str(),
            result_name.as_str(),
            value_binding_name.as_str(),
            env_name.as_str(),
            binding_name.as_str(),
        ] {
            current_statements.push(IRNode::var_decl(name.to_string(), None));
        }
        if using_info.using_async {
            current_statements.push(IRNode::var_decl(resource_error_name.clone(), None));
            if let Some(dispose_result_name) = &dispose_result_name {
                current_statements.push(IRNode::var_decl(dispose_result_name.clone(), None));
            }
        }
        current_statements.push(IRNode::var_decl(outer_catch_error_name.clone(), None));

        let iterable = self.for_of_iterable_to_ir_with_es5_computed_temps(
            for_in_of.expression,
            current_statements,
        );

        current_statements.push(IRNode::HoistedVarGroupBreak);
        let done_name = self.generate_hoisted_temp();
        let return_name = self.generate_hoisted_temp();
        let value_name = self.generate_hoisted_temp();
        for name in [&done_name, &outer_error_name, &return_name, &value_name] {
            current_statements.push(IRNode::var_decl(name.clone(), None));
        }

        let loop_yield_label = self.state.next_label();
        let after_next_label = self.state.next_label();
        let (
            resource_start_label,
            resource_catch_label,
            resource_finally_label,
            dispose_resume_label,
            dispose_done_label,
            iteration_label,
            loop_exit_label,
        ) = if using_info.using_async {
            (
                self.state.next_label(),
                self.state.next_label(),
                self.state.next_label(),
                Some(self.state.next_label()),
                Some(self.state.next_label()),
                self.state.next_label(),
                self.state.next_label(),
            )
        } else {
            (
                u32::MAX,
                u32::MAX,
                u32::MAX,
                None,
                None,
                self.state.next_label(),
                self.state.next_label(),
            )
        };
        let outer_catch_label = self.state.next_label();
        let outer_finally_label = self.state.next_label();
        let return_resume_label = self.state.next_label();
        let return_done_label = self.state.next_label();
        let rethrow_label = self.state.next_label();
        let outer_endfinally_label = self.state.next_label();
        let end_label = self.state.next_label();

        current_statements.push(IRNode::GeneratorTryPush {
            start_label: *current_label,
            catch_label: outer_catch_label,
            finally_label: outer_finally_label,
            end_label,
        });
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
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(loop_yield_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = loop_yield_label;
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::YIELD,
                value: Some(Box::new(IRNode::CallExpr {
                    callee: Box::new(IRNode::prop(IRNode::id(iterator_name.clone()), "next")),
                    arguments: vec![],
                })),
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
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(value_name.clone()),
            IRNode::prop(IRNode::id(result_name), "value"),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(loop_guard_name.clone()),
            IRNode::BooleanLiteral(false),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(value_binding_name.clone()),
            IRNode::id(value_name),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(env_name.clone()),
            self.disposable_env_initializer(),
        ))));

        if using_info.using_async {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::GeneratorLabel,
                IRNode::number(resource_start_label.to_string()),
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label = resource_start_label;
            current_statements.push(IRNode::GeneratorTryPush {
                start_label: resource_start_label,
                catch_label: resource_catch_label,
                finally_label: resource_finally_label,
                end_label: iteration_label,
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(binding_name),
                self.add_disposable_resource_call(
                    &env_name,
                    &value_binding_name,
                    using_info.using_async,
                ),
            ))));
            self.process_block_or_statement_in_async(
                for_in_of.statement,
                cases,
                current_statements,
                current_label,
            );
            current_statements.push(Self::generator_break_statement(iteration_label));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label = resource_catch_label;
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(resource_error_name.clone()),
                IRNode::GeneratorSent,
            ))));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::prop(IRNode::id(env_name.clone()), "error"),
                IRNode::id(resource_error_name),
            ))));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::prop(IRNode::id(env_name.clone()), "hasError"),
                IRNode::BooleanLiteral(true),
            ))));
            current_statements.push(Self::generator_break_statement(iteration_label));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            let dispose_result_name =
                dispose_result_name.expect("await using reserves a dispose result");
            *current_label = resource_finally_label;
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(dispose_result_name.clone()),
                IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
                    arguments: vec![IRNode::id(env_name)],
                },
            ))));
            current_statements.push(IRNode::IfBreak {
                condition: Box::new(IRNode::PrefixUnaryExpr {
                    operator: "!".into(),
                    operand: Box::new(IRNode::id(dispose_result_name.clone())),
                }),
                target_label: dispose_done_label.expect("await using reserves done label"),
            });
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::YIELD,
                    value: Some(Box::new(IRNode::id(dispose_result_name))),
                    comment: Some("yield".into()),
                },
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label = dispose_resume_label.expect("await using reserves resume label");
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::GeneratorLabel,
                IRNode::number(
                    dispose_done_label
                        .expect("await using reserves done label")
                        .to_string(),
                ),
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label = dispose_done_label.expect("await using reserves done label");
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
        } else {
            current_statements.push(IRNode::TryStatement {
                try_block: Box::new(IRNode::Block(vec![IRNode::ExpressionStatement(Box::new(
                    IRNode::assign(
                        IRNode::id(binding_name),
                        self.add_disposable_resource_call(
                            &env_name,
                            &value_binding_name,
                            using_info.using_async,
                        ),
                    ),
                ))])),
                catch_clause: Some(IRCatchClause {
                    param: Some(resource_error_name.into()),
                    body: vec![
                        IRNode::ExpressionStatement(Box::new(IRNode::assign(
                            IRNode::prop(IRNode::id(env_name.clone()), "error"),
                            IRNode::id(format!(
                                "e_{}",
                                self.env_id_from_name(&env_name).unwrap_or(1) + 1
                            )),
                        ))),
                        IRNode::ExpressionStatement(Box::new(IRNode::assign(
                            IRNode::prop(IRNode::id(env_name.clone()), "hasError"),
                            IRNode::BooleanLiteral(true),
                        ))),
                    ],
                    single_line: false,
                }),
                finally_block: Some(Box::new(IRNode::Block(vec![IRNode::ExpressionStatement(
                    Box::new(IRNode::CallExpr {
                        callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
                        arguments: vec![IRNode::id(env_name)],
                    }),
                )]))),
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::GeneratorLabel,
                IRNode::number(iteration_label.to_string()),
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
        }

        *current_label = iteration_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(loop_guard_name.clone()),
            IRNode::BooleanLiteral(true),
        ))));
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

        *current_label = outer_catch_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(outer_catch_error_name.clone()),
            IRNode::GeneratorSent,
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(outer_error_name.clone()),
            IRNode::object(vec![crate::transforms::ir::IRProperty {
                key: crate::transforms::ir::IRPropertyKey::Identifier("error".into()),
                value: IRNode::id(outer_catch_error_name),
                kind: crate::transforms::ir::IRPropertyKind::Init,
            }]),
        ))));
        current_statements.push(Self::generator_break_statement(end_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = outer_finally_label;
        current_statements.push(IRNode::GeneratorTryPushFinally {
            start_label: outer_finally_label,
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
                value: Some(Box::new(IRNode::CallExpr {
                    callee: Box::new(IRNode::prop(IRNode::id(return_name), "call")),
                    arguments: vec![IRNode::id(iterator_name)],
                })),
                comment: Some("yield".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = return_resume_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(return_done_label.to_string()),
        ))));
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
            condition: Box::new(IRNode::id(outer_error_name.clone())),
            then_branch: Box::new(IRNode::ThrowStatement(Box::new(IRNode::prop(
                IRNode::id(outer_error_name),
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

    fn process_for_initializer_using_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FOR_STATEMENT {
            return false;
        }
        let Some(loop_data) = self.arena.get_loop(node) else {
            return false;
        };
        let Some((using_async, declarations)) =
            self.for_initializer_using_declarations(loop_data.initializer)
        else {
            return false;
        };

        self.helpers_needed.add_disposable_resource = true;
        self.helpers_needed.dispose_resources = true;

        let (env_name, error_name, result_name) = self.next_disposable_env_names();
        current_statements.push(IRNode::var_decl(env_name.clone(), None));
        for &decl_idx in &declarations {
            if let Some(name) = self.variable_declaration_name(decl_idx) {
                current_statements.push(IRNode::var_decl(name, None));
            }
        }
        current_statements.push(IRNode::var_decl(error_name.clone(), None));
        if using_async {
            current_statements.push(IRNode::var_decl(result_name.clone(), None));
        }

        let mut registration_exprs = Vec::new();
        let mut started_computed_temp_group = false;
        for &decl_idx in &declarations {
            let Some(name) = self.variable_declaration_name(decl_idx) else {
                continue;
            };
            let value = self.using_declaration_initializer_value(
                decl_idx,
                current_statements,
                &mut started_computed_temp_group,
            );
            registration_exprs.push(IRNode::assign(
                IRNode::id(name),
                IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__addDisposableResource".into())),
                    arguments: vec![
                        IRNode::id(env_name.clone()),
                        value,
                        IRNode::BooleanLiteral(using_async),
                    ],
                },
            ));
        }

        let start_label = self.state.next_label();
        let catch_label = self.state.next_label();
        let finally_label = self.state.next_label();
        let dispose_resume_label = self.state.next_label();
        let dispose_done_label = self.state.next_label();
        let end_label = self.state.next_label();

        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(env_name.clone()),
            self.disposable_env_initializer(),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(start_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = start_label;
        current_statements.push(IRNode::GeneratorTryPush {
            start_label,
            catch_label,
            finally_label,
            end_label,
        });
        if let Some(registration_expr) = Self::comma_chain(registration_exprs) {
            current_statements.push(IRNode::ExpressionStatement(Box::new(registration_expr)));
        }
        current_statements.push(IRNode::ForStatement {
            initializer: None,
            condition: loop_data
                .condition
                .is_some()
                .then(|| Box::new(self.expression_to_ir(loop_data.condition))),
            incrementor: loop_data
                .incrementor
                .is_some()
                .then(|| Box::new(self.expression_to_ir(loop_data.incrementor))),
            body: Box::new(self.loop_body_to_ir(loop_data.statement)),
        });
        current_statements.push(Self::generator_break_statement(end_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = catch_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(error_name.clone()),
            IRNode::GeneratorSent,
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::prop(IRNode::id(env_name.clone()), "error"),
            IRNode::id(error_name),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::prop(IRNode::id(env_name.clone()), "hasError"),
            IRNode::BooleanLiteral(true),
        ))));
        current_statements.push(Self::generator_break_statement(end_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = finally_label;
        if using_async {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(result_name.clone()),
                IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
                    arguments: vec![IRNode::id(env_name)],
                },
            ))));
            current_statements.push(IRNode::IfBreak {
                condition: Box::new(IRNode::PrefixUnaryExpr {
                    operator: "!".into(),
                    operand: Box::new(IRNode::id(result_name.clone())),
                }),
                target_label: dispose_done_label,
            });
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::YIELD,
                    value: Some(Box::new(IRNode::id(result_name))),
                    comment: Some("yield".into()),
                },
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label = dispose_resume_label;
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::GeneratorLabel,
                IRNode::number(dispose_done_label.to_string()),
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
        } else {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
                arguments: vec![IRNode::id(env_name)],
            })));
        }

        *current_label = dispose_done_label;
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

    fn for_initializer_using_declarations(
        &self,
        initializer: NodeIndex,
    ) -> Option<(bool, Vec<NodeIndex>)> {
        let init_node = self.arena.get(initializer)?;
        let flags = init_node.flags as u32;
        if (flags & node_flags::USING) == 0 && !node_flags::is_await_using(flags) {
            return None;
        }
        let decl_list = self.arena.get_variable(init_node)?;
        Some((
            node_flags::is_await_using(flags),
            decl_list.declarations.nodes.clone(),
        ))
    }

    fn variable_declaration_name(&self, decl_idx: NodeIndex) -> Option<String> {
        let decl_node = self.arena.get(decl_idx)?;
        let decl = self.arena.get_variable_declaration(decl_node)?;
        let name = super::emit_utils::identifier_text_or_empty(self.arena, decl.name);
        (!name.is_empty()).then_some(name)
    }

    fn block_to_ir_in_async(&self, block_idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(block_idx) else {
            return IRNode::Block(Vec::new());
        };
        let Some(block) = self.arena.get_block(node) else {
            return IRNode::Block(Vec::new());
        };
        IRNode::Block(
            block
                .statements
                .nodes
                .iter()
                .map(|&stmt| self.statement_to_ir_in_async_block(stmt))
                .collect(),
        )
    }

    fn statement_to_ir_in_async_block(&self, stmt_idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(stmt_idx) else {
            return IRNode::EmptyStatement;
        };
        match node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let value = self.arena.get_return_statement(node).and_then(|ret| {
                    ret.expression
                        .into_option()
                        .map(|expr| Box::new(self.expression_to_ir(expr)))
                });
                IRNode::ReturnStatement(Some(Box::new(IRNode::GeneratorOp {
                    opcode: opcodes::RETURN,
                    value,
                    comment: Some("return".to_string().into()),
                })))
            }
            k if k == syntax_kind_ext::BLOCK => self.block_to_ir_in_async(stmt_idx),
            _ => self.statement_to_ir(stmt_idx),
        }
    }

    fn loop_body_to_ir(&self, statement: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(statement) else {
            return IRNode::EmptyStatement;
        };
        if node.kind != syntax_kind_ext::BLOCK {
            return self.statement_to_ir(statement);
        }
        let Some(block) = self.arena.get_block(node) else {
            return IRNode::Block(Vec::new());
        };
        IRNode::Block(
            block
                .statements
                .nodes
                .iter()
                .map(|&stmt| self.statement_to_ir(stmt))
                .collect(),
        )
    }

    fn using_declaration_initializer_value(
        &self,
        decl_idx: NodeIndex,
        current_statements: &mut Vec<IRNode>,
        started_computed_temp_group: &mut bool,
    ) -> IRNode {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return IRNode::Undefined;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return IRNode::Undefined;
        };
        if decl.initializer.is_none() {
            return IRNode::Undefined;
        }
        if let Some((temp, lowered)) =
            self.lower_object_literal_es5_with_computed_properties(decl.initializer)
        {
            if !*started_computed_temp_group {
                current_statements.push(IRNode::HoistedVarGroupBreak);
                *started_computed_temp_group = true;
            }
            current_statements.push(IRNode::VarDecl {
                name: temp.into(),
                initializer: None,
            });
            lowered
        } else {
            self.expression_to_ir(decl.initializer)
        }
    }

    fn comma_chain(mut expressions: Vec<IRNode>) -> Option<IRNode> {
        if expressions.is_empty() {
            return None;
        }
        let mut expression = expressions.remove(0);
        for next in expressions {
            expression = IRNode::binary(expression, ",", next);
        }
        Some(expression)
    }

    fn for_of_iterable_temp_name(&self, expression: NodeIndex, env_id: u32) -> String {
        if let Some(expr_node) = self.arena.get(expression)
            && expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
        {
            let name = super::emit_utils::identifier_text_or_empty(self.arena, expression);
            if !name.is_empty() {
                return self.fresh_reserved_name(format!("{name}_{env_id}"));
            }
        }
        self.generate_hoisted_temp()
    }

    fn for_await_iterator_names(&self, expression: NodeIndex, env_id: u32) -> (String, String) {
        if let Some(expr_node) = self.arena.get(expression)
            && expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
        {
            let name = super::emit_utils::identifier_text_or_empty(self.arena, expression);
            if !name.is_empty() {
                let mut ordinal = env_id;
                loop {
                    let iterator_name = format!("{name}_{ordinal}");
                    let result_name = format!("{iterator_name}_1");
                    let mut blocked = self.blocked_temp_names.borrow_mut();
                    if !blocked.contains(&iterator_name) && !blocked.contains(&result_name) {
                        blocked.insert(iterator_name.clone());
                        blocked.insert(result_name.clone());
                        return (iterator_name, result_name);
                    }
                    ordinal += 1;
                }
            }
        }
        (self.generate_hoisted_temp(), self.generate_hoisted_temp())
    }

    fn for_of_iterable_to_ir_with_es5_computed_temps(
        &self,
        expression: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) -> IRNode {
        let Some(expr_node) = self.arena.get(expression) else {
            return IRNode::Undefined;
        };
        if expr_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return self.expression_to_ir(expression);
        }
        let Some(array) = self.arena.get_literal_expr(expr_node) else {
            return IRNode::ArrayLiteral(Vec::new());
        };

        let mut started_computed_temp_group = false;
        let elements = array
            .elements
            .nodes
            .iter()
            .map(|&element| {
                if let Some((temp, lowered)) =
                    self.lower_object_literal_es5_with_computed_properties(element)
                {
                    if !started_computed_temp_group {
                        current_statements.push(IRNode::HoistedVarGroupBreak);
                        started_computed_temp_group = true;
                    }
                    current_statements.push(IRNode::VarDecl {
                        name: temp.into(),
                        initializer: None,
                    });
                    lowered
                } else {
                    self.expression_to_ir(element)
                }
            })
            .collect();

        IRNode::ArrayLiteral(elements)
    }

    fn process_async_statement(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::EMPTY_STATEMENT => {
                current_statements.push(IRNode::EmptyStatement);
            }

            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
                    if self.is_suspension_expression(expr_stmt.expression) {
                        let trailing_comment = self.extract_trailing_line_comment_in_node(idx);
                        self.process_await_expression(
                            expr_stmt.expression,
                            cases,
                            current_statements,
                            current_label,
                        );
                        current_statements
                            .push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
                        if let Some(comment) = trailing_comment {
                            current_statements.push(IRNode::TrailingComment(comment.into()));
                        }
                        return;
                    }
                    self.process_expression_in_async(
                        expr_stmt.expression,
                        cases,
                        current_statements,
                        current_label,
                    );
                }
            }

            k if k == syntax_kind_ext::BLOCK => {
                if self.contains_await_recursive(idx) {
                    if let Some(block) = self.arena.get_block(node) {
                        self.process_async_statement_list(
                            &block.statements.nodes,
                            cases,
                            current_statements,
                            current_label,
                            &[],
                        );
                    }
                } else {
                    current_statements.push(self.block_to_ir_in_async(idx));
                }
            }

            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(node) {
                    if ret.expression.is_none() {
                        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                            IRNode::GeneratorOp {
                                opcode: opcodes::RETURN,
                                value: None,
                                comment: Some("return".to_string().into()),
                            },
                        ))));
                    } else if self.is_suspension_expression(ret.expression) {
                        // return await/yield expr; -> yield, then return _a.sent()
                        self.process_await_expression(
                            ret.expression,
                            cases,
                            current_statements,
                            current_label,
                        );

                        // After the yield resumes, return the sent value
                        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                            IRNode::GeneratorOp {
                                opcode: opcodes::RETURN,
                                value: Some(Box::new(IRNode::GeneratorSent)),
                                comment: Some("return".to_string().into()),
                            },
                        ))));
                    } else if self.contains_await_recursive(ret.expression) {
                        let value = if let Some(lowered_comma) = self
                            .lower_return_comma_before_suspension(
                                ret.expression,
                                cases,
                                current_statements,
                                current_label,
                            ) {
                            lowered_comma
                        } else if let Some(lowered_object) = self
                            .lower_object_literal_before_suspension(
                                ret.expression,
                                cases,
                                current_statements,
                                current_label,
                            )
                        {
                            lowered_object
                        } else if let Some(lowered_call) = self.lower_call_callee_before_suspension(
                            ret.expression,
                            cases,
                            current_statements,
                            current_label,
                        ) {
                            lowered_call
                        } else if let Some(lowered_array) = self
                            .lower_array_literal_before_suspension(
                                ret.expression,
                                cases,
                                current_statements,
                                current_label,
                            )
                        {
                            lowered_array
                        } else if let Some(lowered_access) = self
                            .lower_element_access_object_before_suspension(
                                ret.expression,
                                cases,
                                current_statements,
                                current_label,
                            )
                        {
                            lowered_access
                        } else {
                            self.emit_nested_suspension(
                                ret.expression,
                                cases,
                                current_statements,
                                current_label,
                            );
                            self.expression_to_ir(ret.expression)
                        };
                        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                            IRNode::GeneratorOp {
                                opcode: opcodes::RETURN,
                                value: Some(Box::new(value)),
                                comment: Some("return".to_string().into()),
                            },
                        ))));
                    } else {
                        let value = self.expression_to_ir(ret.expression);
                        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                            IRNode::GeneratorOp {
                                opcode: opcodes::RETURN,
                                value: Some(Box::new(value)),
                                comment: Some("return".to_string().into()),
                            },
                        ))));
                    }
                }
            }

            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                // Structure: VARIABLE_STATEMENT -> VARIABLE_DECLARATION_LIST -> VARIABLE_DECLARATION
                if let Some(var_stmt) = self.arena.get_variable(node) {
                    let mut trailing_comment = self.extract_trailing_line_comment_in_node(idx);
                    for &decl_list_idx in &var_stmt.declarations.nodes {
                        if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                            && let Some(decl_list) = self.arena.get_variable(decl_list_node)
                        {
                            for &decl_idx in &decl_list.declarations.nodes {
                                self.process_variable_declaration(
                                    decl_idx,
                                    cases,
                                    current_statements,
                                    current_label,
                                    &mut trailing_comment,
                                );
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node) {
                    if func.is_async {
                        // Nested async function declarations inside async bodies must be
                        // lowered as standalone functions in the generator case block.
                        current_statements.push(self.transform_async_function(idx));
                    } else {
                        current_statements.push(IRNode::ASTRef(idx));
                    }
                } else {
                    current_statements.push(IRNode::ASTRef(idx));
                }
            }

            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if self.lower_class_extends_before_suspension(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) {
                    return;
                }
                if self.lower_class_declaration_to_assignment(idx, current_statements) {
                    return;
                }
                current_statements.push(self.statement_to_ir(idx));
            }

            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.process_if_statement_in_async(idx, cases, current_statements, current_label);
            }

            k if k == syntax_kind_ext::WHILE_STATEMENT => {
                self.process_while_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                );
            }

            k if k == syntax_kind_ext::DO_STATEMENT => {
                self.process_do_while_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                );
            }

            k if k == syntax_kind_ext::FOR_STATEMENT => {
                if !self.process_for_initializer_using_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) && !self.process_captured_for_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) && !self.process_for_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) {
                    current_statements.push(self.statement_to_ir(idx));
                }
            }

            k if k == syntax_kind_ext::FOR_IN_STATEMENT => {
                if !self.process_for_in_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) {
                    current_statements.push(self.statement_to_ir(idx));
                }
            }

            k if k == syntax_kind_ext::FOR_OF_STATEMENT => {
                if !self.process_for_await_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                    None,
                ) && !self.process_for_await_using_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) && !self.process_for_of_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) && !self.process_for_of_using_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ) {
                    current_statements.push(self.statement_to_ir(idx));
                }
            }

            k if k == syntax_kind_ext::THROW_STATEMENT => {
                if let Some(throw_data) = self.arena.get_return_statement(node) {
                    if self.contains_await_recursive(throw_data.expression) {
                        // throw await expr; -> yield expr, then throw _a.sent()
                        if self.is_suspension_expression(throw_data.expression) {
                            self.process_await_expression(
                                throw_data.expression,
                                cases,
                                current_statements,
                                current_label,
                            );
                            current_statements
                                .push(IRNode::ThrowStatement(Box::new(IRNode::GeneratorSent)));
                        } else {
                            self.emit_nested_suspension(
                                throw_data.expression,
                                cases,
                                current_statements,
                                current_label,
                            );
                            let expr = self.expression_to_ir(throw_data.expression);
                            current_statements.push(IRNode::ThrowStatement(Box::new(expr)));
                        }
                    } else {
                        let expr = self.expression_to_ir(throw_data.expression);
                        current_statements.push(IRNode::ThrowStatement(Box::new(expr)));
                    }
                }
            }

            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.process_try_statement_in_async(idx, cases, current_statements, current_label);
            }

            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                self.process_labeled_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                );
            }

            k if k == syntax_kind_ext::BLOCK => {
                self.process_block_or_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                );
            }

            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                self.process_switch_statement_in_async(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                );
            }

            k if k == syntax_kind_ext::WITH_STATEMENT => {
                self.process_with_statement_in_async(idx, cases, current_statements, current_label);
            }

            _ => {
                // Pass through other statements as-is
                let ir = self.statement_to_ir(idx);
                current_statements.push(ir);
            }
        }
    }

    fn process_with_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(with_data) = self.arena.get_with_statement(node) else {
            return;
        };

        let expression_has_await = self.contains_await_recursive(with_data.expression);
        let body_has_await = self.contains_await_recursive(with_data.then_statement);
        if !expression_has_await && !body_has_await {
            current_statements.push(self.statement_to_ir(idx));
            return;
        }

        let temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: temp.clone().into(),
            initializer: None,
        });

        if expression_has_await {
            self.emit_nested_suspension(
                with_data.expression,
                cases,
                current_statements,
                current_label,
            );
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(temp.clone()),
                IRNode::GeneratorSent,
            ))));
        } else {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(temp.clone()),
                self.expression_to_ir(with_data.expression),
            ))));
        }

        let body_label = self.state.next_label();
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(body_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });
        *current_label = body_label;

        let mut nested_cases = Vec::new();
        let mut nested_current = Vec::new();
        let mut nested_label = *current_label;
        self.process_block_or_statement_in_async(
            with_data.then_statement,
            &mut nested_cases,
            &mut nested_current,
            &mut nested_label,
        );

        for mut case in nested_cases {
            cases.push(IRGeneratorCase {
                label: case.label,
                statements: Self::wrap_statements_in_with(
                    &temp,
                    std::mem::take(&mut case.statements),
                ),
            });
        }

        *current_label = nested_label;
        if nested_current.is_empty() {
            return;
        }
        current_statements.extend(Self::wrap_statements_in_with(&temp, nested_current));
        let end_label = self.state.next_label();
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(end_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });
        *current_label = end_label;
    }

    fn wrap_statements_in_with(temp: &str, mut statements: Vec<IRNode>) -> Vec<IRNode> {
        let leading_var_count = statements
            .iter()
            .take_while(|statement| matches!(statement, IRNode::VarDecl { .. }))
            .count();
        let mut leading_vars = statements.drain(..leading_var_count).collect::<Vec<_>>();
        let trailing_label_assignment = statements
            .last()
            .is_some_and(Self::is_generator_label_assignment)
            .then(|| statements.pop().expect("checked last statement"));

        let mut wrapped = Vec::new();
        wrapped.append(&mut leading_vars);
        if !statements.is_empty() {
            wrapped.push(IRNode::WithStatement {
                expression: Box::new(IRNode::id(temp.to_string())),
                body: Box::new(IRNode::Block(statements)),
            });
        }
        if let Some(label_assignment) = trailing_label_assignment {
            wrapped.push(label_assignment);
        }
        wrapped
    }

    fn is_generator_label_assignment(node: &IRNode) -> bool {
        matches!(
            node,
            IRNode::ExpressionStatement(expr)
                if matches!(
                    expr.as_ref(),
                    IRNode::BinaryExpr {
                        left,
                        operator,
                        ..
                    } if matches!(left.as_ref(), IRNode::GeneratorLabel)
                        && operator.as_ref() == "="
                )
        )
    }

    fn statement_to_hoistable_ir(&mut self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::EmptyStatement;
        };

        let mut cases = Vec::new();
        let mut statements = Vec::new();
        let mut label = 0;
        self.process_block_or_statement_in_async(idx, &mut cases, &mut statements, &mut label);
        if !cases.is_empty() {
            return self.statement_to_ir(idx);
        }
        if node.kind == syntax_kind_ext::BLOCK {
            return IRNode::Block(statements);
        }
        if statements.len() == 1 {
            statements.pop().expect("len checked")
        } else {
            IRNode::Block(statements)
        }
    }
}
