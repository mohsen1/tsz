impl<'a> AsyncES5Transformer<'a> {
    fn process_for_in_statement_in_async(
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

    fn process_simple_for_in_statement(
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

    fn for_in_assignment_target(
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

    fn for_in_direct_assignment_target(
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
            let name = super::emit_utils::identifier_text(self.arena, decl.name)?;
            return Some((IRNode::id(name.clone()), Some(name)));
        }
        if init_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let name = super::emit_utils::identifier_text(self.arena, initializer)?;
            return Some((IRNode::id(name), None));
        }
        if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || init_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return Some((self.expression_to_ir(initializer), None));
        }
        None
    }

    fn for_in_suspended_assignment_target(
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

    fn direct_suspension_expression(&self, expression: NodeIndex) -> Option<NodeIndex> {
        let expression = self.strip_parenthesized_expression(expression);
        self.is_suspension_expression(expression)
            .then_some(expression)
    }

    fn for_in_body_has_unsupported_control_flow(&self, idx: NodeIndex) -> bool {
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

    fn expression_statement(expression: IRNode) -> IRNode {
        IRNode::ExpressionStatement(Box::new(expression))
    }

    fn negated_condition(condition: IRNode) -> IRNode {
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

    fn generator_label_assignment(label: u32) -> IRNode {
        Self::expression_statement(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(label.to_string()),
        ))
    }

    fn loop_needs_async_capture(&self, idx: NodeIndex) -> bool {
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

    fn async_captured_for_loop_ordinal(&self, idx: NodeIndex) -> usize {
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

    fn captured_for_loop_state_name(&self, idx: NodeIndex) -> Option<String> {
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

    fn simple_for_loop_var_initializer(&self, initializer: NodeIndex) -> Option<(String, String)> {
        let init_node = self.arena.get(initializer)?;
        let var_list = self.arena.get_variable(init_node)?;
        let decl_idx = *var_list.declarations.nodes.first()?;
        let decl = self.arena.get_variable_declaration_at(decl_idx)?;
        Some((
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, decl.name),
            self.ir_text(self.expression_to_ir(decl.initializer)),
        ))
    }

    fn captured_for_loop_inner_generator(
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

    fn captured_for_loop_has_break(&self, body: NodeIndex) -> bool {
        self.block_contains_statement_kind(body, syntax_kind_ext::BREAK_STATEMENT)
    }

    fn captured_for_loop_has_continue(&self, body: NodeIndex) -> bool {
        self.block_contains_statement_kind(body, syntax_kind_ext::CONTINUE_STATEMENT)
    }

    fn captured_for_loop_has_value_return(&self, body: NodeIndex) -> bool {
        self.block_contains_statement_kind(body, syntax_kind_ext::RETURN_STATEMENT)
    }

    fn block_contains_statement_kind(&self, body: NodeIndex, kind: u16) -> bool {
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

    fn ir_text(&self, ir: IRNode) -> String {
        crate::transforms::ir_printer::IRPrinter::emit_to_string(&ir)
    }

    /// Process a try/catch/finally statement inside an async function body.
    ///
    /// When none of the blocks contain await, falls through to raw IR emission.
    /// When blocks contain await, generates proper state machine labels with
    /// try/catch/finally opcodes.
    fn process_try_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(try_data) = self.arena.get_try(node) else {
            return;
        };

        let try_has_await = self.contains_await_recursive(try_data.try_block);
        let catch_has_await = self.contains_await_recursive(try_data.catch_clause);
        let finally_has_await = self.contains_await_recursive(try_data.finally_block);

        if !try_has_await && !catch_has_await && !finally_has_await {
            // No await in any block -- emit as-is
            let ir = self.statement_to_ir(idx);
            current_statements.push(ir);
            return;
        }

        let has_catch =
            try_data.catch_clause.is_some() && self.arena.get(try_data.catch_clause).is_some();
        let has_finally =
            try_data.finally_block.is_some() && self.arena.get(try_data.finally_block).is_some();

        if !has_catch && !has_finally {
            self.process_block_or_statement_in_async(
                try_data.try_block,
                cases,
                current_statements,
                current_label,
            );
            return;
        }

        // Sentinels share `next_loop_exit_placeholder` so the patch sweep cannot
        // collide with loop-exit placeholders still living in a surrounding loop.
        let placeholders = TryRegionPlaceholders {
            catch_slot: self.next_loop_exit_placeholder(),
            finally_slot: self.next_loop_exit_placeholder(),
            end_slot: self.next_loop_exit_placeholder(),
            exit_break: self.next_loop_exit_placeholder(),
        };
        let start_label = *current_label;
        let cases_start = cases.len();

        current_statements.push(IRNode::generator_try_push(
            start_label,
            has_catch.then_some(placeholders.catch_slot),
            has_finally.then_some(placeholders.finally_slot),
            placeholders.end_slot,
        ));

        self.process_block_or_statement_in_async(
            try_data.try_block,
            cases,
            current_statements,
            current_label,
        );
        current_statements.push(Self::generator_break_statement(placeholders.exit_break));

        let catch_label = if has_catch {
            let cl = self.state.next_label();
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = cl;

            if let Some(catch_node) = self.arena.get(try_data.catch_clause)
                && let Some(catch_data) = self.arena.get_catch_clause(catch_node)
            {
                let catch_rename_depth = self.catch_binding_renames.len();
                if catch_data.variable_declaration.is_some() {
                    let catch_var_name =
                        self.get_catch_variable_name(catch_data.variable_declaration);
                    if !catch_var_name.is_empty() {
                        let catch_temp = self
                            .planned_catch_binding_temps
                            .borrow()
                            .get(&try_data.catch_clause.0)
                            .cloned()
                            .unwrap_or_else(|| {
                                self.fresh_catch_binding_temp(
                                    &catch_var_name,
                                    try_data.catch_clause,
                                )
                            });
                        self.blocked_temp_names
                            .borrow_mut()
                            .insert(catch_temp.clone());
                        current_statements.push(IRNode::VarDecl {
                            name: catch_temp.clone().into(),
                            initializer: None,
                        });
                        // tsc binds the exception via `_a.sent()`, not `_a[1]`.
                        current_statements.push(IRNode::ExpressionStatement(Box::new(
                            IRNode::assign(IRNode::id(catch_temp.clone()), IRNode::GeneratorSent),
                        )));
                        self.catch_binding_renames
                            .push((catch_var_name, catch_temp));
                    }
                }
                self.process_block_or_statement_in_async(
                    catch_data.block,
                    cases,
                    current_statements,
                    current_label,
                );
                if self.catch_binding_renames.len() > catch_rename_depth {
                    self.catch_binding_renames.pop();
                }
            }

            if !Self::async_statements_end_control_flow(current_statements) {
                current_statements.push(Self::generator_break_statement(placeholders.exit_break));
            }
            Some(cl)
        } else {
            None
        };

        let finally_label = if has_finally {
            let fl = self.state.next_label();
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = fl;

            self.process_block_or_statement_in_async(
                try_data.finally_block,
                cases,
                current_statements,
                current_label,
            );

            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::END_FINALLY,
                    value: None,
                    comment: Some("endfinally".to_string().into()),
                },
            ))));
            Some(fl)
        } else {
            None
        };

        // End label is allocated last so its number is past every interior resume.
        let end_label = self.state.next_label();

        let resolution = TryRegionResolution {
            placeholders,
            catch_label,
            finally_label,
            end_label,
            // Breaks from try/catch must target the region's end label even when
            // a finally exists; tsc's `__generator` driver detects the active try
            // entry on a `[3 /*break*/, end]` op, pushes the pending break onto
            // `_.ops`, then jumps to the finally label. After `[7 /*endfinally*/]`
            // pops `_.ops`, the driver resumes the original break against an
            // empty `_.trys` stack and lands at `end`. Breaking directly to the
            // finally label would jump there without pushing onto `_.ops`, so
            // `endfinally` would pop an empty stack and the state machine would
            // wedge.
            exit_target: end_label,
        };
        let cases_tail = cases[cases_start..]
            .iter_mut()
            .flat_map(|case| case.statements.iter_mut())
            .chain(current_statements.iter_mut());
        for stmt in cases_tail {
            patch_try_region_placeholders(stmt, &resolution);
        }

        if !current_statements.is_empty() {
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
        }
        *current_label = end_label;
    }
}
