use crate::transforms::async_es5_ir::{AsyncES5Transformer, opcodes};
use crate::transforms::ir::{IRCatchClause, IRGeneratorCase, IRNode};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> AsyncES5Transformer<'a> {
    pub(in crate::transforms) fn statement_is_using_variable_statement(
        &self,
        stmt_idx: NodeIndex,
    ) -> bool {
        self.using_variable_statement_flags(stmt_idx)
            .is_some_and(|flags| (flags & node_flags::USING) != 0)
    }

    pub(in crate::transforms) fn using_variable_statement_flags(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<u32> {
        let stmt_node = self.arena.get(stmt_idx)?;
        if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return None;
        }
        let var_stmt = self.arena.get_variable(stmt_node)?;
        var_stmt
            .declarations
            .nodes
            .iter()
            .find_map(|&decl_list_idx| {
                self.arena.get(decl_list_idx).and_then(|decl_list_node| {
                    ((decl_list_node.flags as u32 & node_flags::USING) != 0)
                        .then_some(decl_list_node.flags as u32)
                })
            })
    }

    pub(in crate::transforms) fn process_async_disposable_region(
        &mut self,
        statements: &[NodeIndex],
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        skipped_statements: &[NodeIndex],
    ) {
        let (env_name, error_name, result_name) = self.next_disposable_env_names();
        let using_async = self.statement_slice_has_await_using(statements, skipped_statements);
        let using_binding_names = self.collect_using_binding_names(statements, skipped_statements);
        let start_label = self.state.next_label();
        let try_push_placeholder = u32::MAX;

        current_statements.push(IRNode::VarDecl {
            name: env_name.clone().into(),
            initializer: Some(Box::new(self.disposable_env_initializer())),
        });
        for name in using_binding_names {
            current_statements.push(IRNode::VarDecl {
                name: name.into(),
                initializer: None,
            });
        }
        current_statements.push(IRNode::VarDecl {
            name: error_name.clone().into(),
            initializer: None,
        });
        // Only hoist `result_N` when the region awaits disposal. For pure-`using`
        // regions tsc emits `__disposeResources(env_N);` as a plain expression
        // statement and never assigns to `result_N`, so it never declares the
        // variable either.
        if using_async {
            current_statements.push(IRNode::VarDecl {
                name: result_name.clone().into(),
                initializer: None,
            });
        }
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
            catch_label: try_push_placeholder,
            finally_label: try_push_placeholder,
            end_label: try_push_placeholder,
        });

        for &stmt_idx in statements {
            if skipped_statements.contains(&stmt_idx) {
                continue;
            }
            self.push_preceding_line_comment(stmt_idx, current_statements);
            if self.statement_is_using_variable_statement(stmt_idx) {
                self.process_using_variable_statement_in_region(
                    stmt_idx,
                    &env_name,
                    current_statements,
                );
            } else {
                self.process_async_statement(stmt_idx, cases, current_statements, current_label);
            }
        }

        let catch_label = self.state.next_label();
        let finally_label = self.state.next_label();
        let dispose_resume_label = using_async.then(|| self.state.next_label());
        let dispose_done_label = if using_async {
            self.state.next_label()
        } else {
            finally_label
        };
        let end_label = self.state.next_label();
        Self::patch_generator_try_push(
            cases,
            current_statements,
            start_label,
            catch_label,
            finally_label,
            end_label,
        );

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
        // For pure-`using` regions tsc emits the dispose call as a bare
        // expression statement; only `await using` regions need the
        // `result_N = __disposeResources(env_N);` capture so the value can be
        // awaited before endfinally.
        let dispose_call = IRNode::CallExpr {
            callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
            arguments: vec![IRNode::id(env_name)],
        };
        if using_async {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(result_name.clone()),
                dispose_call,
            ))));
        } else {
            current_statements.push(IRNode::ExpressionStatement(Box::new(dispose_call)));
        }

        if using_async {
            current_statements.push(IRNode::IfBreak {
                condition: Box::new(IRNode::PrefixUnaryExpr {
                    operator: "!".into(),
                    operand: Box::new(IRNode::id(result_name.clone())),
                }),
                target_label: dispose_done_label,
            });
            let dispose_yield_value = if self.async_generator_mode {
                IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__await".into())),
                    arguments: vec![IRNode::id(result_name)],
                }
            } else {
                IRNode::id(result_name)
            };
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::YIELD,
                    value: Some(Box::new(dispose_yield_value)),
                    comment: Some("yield".into()),
                },
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label =
                dispose_resume_label.expect("async disposable regions reserve a resume label");
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::GeneratorLabel,
                IRNode::number(dispose_done_label.to_string()),
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
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
    }

    pub(in crate::transforms) fn disposable_env_initializer(&self) -> IRNode {
        IRNode::object(vec![
            crate::transforms::ir::IRProperty {
                key: crate::transforms::ir::IRPropertyKey::Identifier("stack".into()),
                value: IRNode::ArrayLiteral(Vec::new()),
                kind: crate::transforms::ir::IRPropertyKind::Init,
            },
            crate::transforms::ir::IRProperty {
                key: crate::transforms::ir::IRPropertyKey::Identifier("error".into()),
                value: IRNode::Undefined,
                kind: crate::transforms::ir::IRPropertyKind::Init,
            },
            crate::transforms::ir::IRProperty {
                key: crate::transforms::ir::IRPropertyKey::Identifier("hasError".into()),
                value: IRNode::BooleanLiteral(false),
                kind: crate::transforms::ir::IRPropertyKind::Init,
            },
        ])
    }

    pub(in crate::transforms) fn add_disposable_resource_call(
        &self,
        env_name: &str,
        value_name: &str,
        using_async: bool,
    ) -> IRNode {
        IRNode::CallExpr {
            callee: Box::new(IRNode::RuntimeHelper("__addDisposableResource".into())),
            arguments: vec![
                IRNode::id(env_name.to_string()),
                IRNode::id(value_name.to_string()),
                IRNode::BooleanLiteral(using_async),
            ],
        }
    }

    pub(in crate::transforms) fn generator_break_statement(target_label: u32) -> IRNode {
        IRNode::ReturnStatement(Some(Box::new(IRNode::GeneratorOp {
            opcode: opcodes::BREAK,
            value: Some(Box::new(IRNode::NumericLiteral(
                target_label.to_string().into(),
            ))),
            comment: Some("break".into()),
        })))
    }

    pub(in crate::transforms) fn patch_generator_try_push(
        cases: &mut [IRGeneratorCase],
        current_statements: &mut [IRNode],
        start_label: u32,
        catch_label: u32,
        finally_label: u32,
        end_label: u32,
    ) {
        for case in cases {
            Self::patch_generator_try_push_in_statements(
                &mut case.statements,
                start_label,
                catch_label,
                finally_label,
                end_label,
            );
        }
        Self::patch_generator_try_push_in_statements(
            current_statements,
            start_label,
            catch_label,
            finally_label,
            end_label,
        );
    }

    pub(in crate::transforms) fn patch_generator_try_push_in_statements(
        statements: &mut [IRNode],
        start_label: u32,
        catch_label: u32,
        finally_label: u32,
        end_label: u32,
    ) {
        for statement in statements {
            if let IRNode::GeneratorTryPush {
                start_label: candidate_start,
                catch_label: candidate_catch,
                finally_label: candidate_finally,
                end_label: candidate_end,
            } = statement
                && *candidate_start == start_label
                && *candidate_catch == u32::MAX
            {
                *candidate_catch = catch_label;
                *candidate_finally = finally_label;
                *candidate_end = end_label;
            }
        }
    }

    pub(in crate::transforms) fn statement_slice_has_await_using(
        &self,
        statements: &[NodeIndex],
        skipped_statements: &[NodeIndex],
    ) -> bool {
        statements.iter().copied().any(|stmt_idx| {
            !skipped_statements.contains(&stmt_idx)
                && self
                    .using_variable_statement_flags(stmt_idx)
                    .is_some_and(node_flags::is_await_using)
        })
    }

    pub(in crate::transforms) fn collect_using_binding_names(
        &self,
        statements: &[NodeIndex],
        skipped_statements: &[NodeIndex],
    ) -> Vec<String> {
        let mut names = Vec::new();
        for &stmt_idx in statements {
            if skipped_statements.contains(&stmt_idx)
                || !self.statement_is_using_variable_statement(stmt_idx)
            {
                continue;
            }
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                continue;
            };
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                    continue;
                };
                if (decl_list_node.flags as u32 & node_flags::USING) == 0 {
                    continue;
                }
                let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                    continue;
                };
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    let name = crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena, decl.name,
                    );
                    if !name.is_empty() && !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
        names
    }

    pub(in crate::transforms) fn process_using_variable_statement_in_region(
        &mut self,
        stmt_idx: NodeIndex,
        env_name: &str,
        current_statements: &mut Vec<IRNode>,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };
        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            if (decl_list_node.flags as u32 & node_flags::USING) == 0 {
                continue;
            }
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                self.process_using_variable_declaration_in_region(
                    decl_idx,
                    env_name,
                    node_flags::is_await_using(decl_list_node.flags as u32),
                    current_statements,
                );
            }
        }
    }

    pub(in crate::transforms) fn process_using_variable_declaration_in_region(
        &mut self,
        decl_idx: NodeIndex,
        env_name: &str,
        using_async: bool,
        current_statements: &mut Vec<IRNode>,
    ) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };
        let name = crate::transforms::emit_utils::identifier_text_or_empty(self.arena, decl.name);
        if name.is_empty() {
            return;
        }
        let value = if decl.initializer.is_none() {
            IRNode::Undefined
        } else if let Some((temp, lowered_init)) =
            self.lower_object_literal_es5_with_computed_properties(decl.initializer)
        {
            current_statements.push(IRNode::HoistedVarGroupBreak);
            current_statements.push(IRNode::VarDecl {
                name: temp.into(),
                initializer: None,
            });
            lowered_init
        } else {
            self.expression_to_ir(decl.initializer)
        };
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(name),
            IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__addDisposableResource".into())),
                arguments: vec![
                    IRNode::id(env_name.to_string()),
                    value,
                    IRNode::BooleanLiteral(using_async),
                ],
            },
        ))));
    }

    pub(in crate::transforms) fn process_for_of_using_statement_in_async(
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
        if for_in_of.await_modifier {
            return false;
        }
        let Some(using_info) =
            crate::transforms::emit_utils::for_of_using_info(self.arena, for_in_of.initializer)
        else {
            return false;
        };

        let env_id = self.disposable_env_counter.get();
        let (env_name, error_name, result_name) = self.next_disposable_env_names();
        let index_name = self.fresh_reserved_name("_i");
        let array_name = self.for_of_iterable_temp_name(for_in_of.expression, env_id);
        let value_temp_name =
            self.fresh_reserved_name(format!("{}_{}", using_info.binding_name, env_id));

        for name in [
            &index_name,
            &array_name,
            &value_temp_name,
            &env_name,
            &using_info.binding_name,
            &error_name,
            &result_name,
        ] {
            current_statements.push(IRNode::VarDecl {
                name: name.to_string().into(),
                initializer: None,
            });
        }

        let iterable = self.for_of_iterable_to_ir_with_es5_computed_temps(
            for_in_of.expression,
            current_statements,
        );
        let loop_label = self.state.next_label();
        let try_start_label = self.state.next_label();
        let catch_label = self.state.next_label();
        let finally_label = self.state.next_label();
        let dispose_resume_label = self.state.next_label();
        let dispose_done_label = self.state.next_label();
        let iteration_label = self.state.next_label();
        let end_label = self.state.next_label();

        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::binary(
            IRNode::assign(IRNode::id(index_name.clone()), IRNode::number("0")),
            ",",
            IRNode::assign(IRNode::id(array_name.clone()), iterable),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(loop_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = loop_label;
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".into(),
                operand: Box::new(IRNode::Parenthesized(Box::new(IRNode::binary(
                    IRNode::id(index_name.clone()),
                    "<",
                    IRNode::prop(IRNode::id(array_name.clone()), "length"),
                )))),
            }),
            target_label: end_label,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(value_temp_name.clone()),
            IRNode::elem(IRNode::id(array_name), IRNode::id(index_name.clone())),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(env_name.clone()),
            self.disposable_env_initializer(),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(try_start_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = try_start_label;
        current_statements.push(IRNode::GeneratorTryPush {
            start_label: try_start_label,
            catch_label,
            finally_label,
            end_label: iteration_label,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(using_info.binding_name),
            IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__addDisposableResource".into())),
                arguments: vec![
                    IRNode::id(env_name.clone()),
                    IRNode::id(value_temp_name),
                    IRNode::BooleanLiteral(using_info.using_async),
                ],
            },
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
        current_statements.push(Self::generator_break_statement(iteration_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = finally_label;
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
        let dispose_yield_value = if self.async_generator_mode {
            IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__await".into())),
                arguments: vec![IRNode::id(result_name)],
            }
        } else {
            IRNode::id(result_name)
        };
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::YIELD,
                value: Some(Box::new(dispose_yield_value)),
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

        *current_label = iteration_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(
            IRNode::PostfixUnaryExpr {
                operand: Box::new(IRNode::id(index_name)),
                operator: "++".into(),
            },
        )));
        current_statements.push(Self::generator_break_statement(loop_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = end_label;
        true
    }

    pub(in crate::transforms) fn process_for_await_using_statement_in_async(
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
            crate::transforms::emit_utils::for_of_using_info(self.arena, for_in_of.initializer)
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

    pub(in crate::transforms) fn process_for_initializer_using_statement_in_async(
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

    pub(in crate::transforms) fn for_initializer_using_declarations(
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

    pub(in crate::transforms) fn variable_declaration_name(
        &self,
        decl_idx: NodeIndex,
    ) -> Option<String> {
        let decl_node = self.arena.get(decl_idx)?;
        let decl = self.arena.get_variable_declaration(decl_node)?;
        let name = crate::transforms::emit_utils::identifier_text_or_empty(self.arena, decl.name);
        (!name.is_empty()).then_some(name)
    }

    pub(in crate::transforms) fn block_to_ir_in_async(&self, block_idx: NodeIndex) -> IRNode {
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

    pub(in crate::transforms) fn statement_to_ir_in_async_block(
        &self,
        stmt_idx: NodeIndex,
    ) -> IRNode {
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

    pub(in crate::transforms) fn loop_body_to_ir(&self, statement: NodeIndex) -> IRNode {
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

    pub(in crate::transforms) fn using_declaration_initializer_value(
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

    pub(in crate::transforms) fn comma_chain(mut expressions: Vec<IRNode>) -> Option<IRNode> {
        if expressions.is_empty() {
            return None;
        }
        let mut expression = expressions.remove(0);
        for next in expressions {
            expression = IRNode::binary(expression, ",", next);
        }
        Some(expression)
    }

    pub(in crate::transforms) fn for_of_iterable_temp_name(
        &self,
        expression: NodeIndex,
        env_id: u32,
    ) -> String {
        if let Some(expr_node) = self.arena.get(expression)
            && expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
        {
            let name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, expression);
            if !name.is_empty() {
                return self.fresh_reserved_name(format!("{name}_{env_id}"));
            }
        }
        self.generate_hoisted_temp()
    }

    pub(in crate::transforms) fn for_await_iterator_names(
        &self,
        expression: NodeIndex,
        env_id: u32,
    ) -> (String, String) {
        if let Some(expr_node) = self.arena.get(expression)
            && expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
        {
            let name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, expression);
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

    pub(in crate::transforms) fn for_of_iterable_to_ir_with_es5_computed_temps(
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
}
