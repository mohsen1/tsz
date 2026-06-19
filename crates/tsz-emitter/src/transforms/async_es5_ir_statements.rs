use crate::transforms::async_es5_ir::{AsyncES5Transformer, ES5ClassFactoryParts, opcodes};
use crate::transforms::class_es5_ir::ES5ClassTransformer;
use crate::transforms::ir::{IRGeneratorCase, IRNode, IRParam};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> AsyncES5Transformer<'a> {
    pub(in crate::transforms) fn process_async_statement_list(
        &mut self,
        statements: &[NodeIndex],
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        skipped_statements: &[NodeIndex],
    ) {
        let mut index = 0;
        while index < statements.len() {
            let stmt_idx = statements[index];
            if skipped_statements.contains(&stmt_idx) {
                index += 1;
                continue;
            }
            if self.statement_is_using_variable_statement(stmt_idx) {
                self.process_async_disposable_region(
                    &statements[index..],
                    cases,
                    current_statements,
                    current_label,
                    skipped_statements,
                );
                break;
            }
            self.push_preceding_line_comment(stmt_idx, current_statements);
            self.process_async_statement(stmt_idx, cases, current_statements, current_label);
            index += 1;
        }
    }

    pub(in crate::transforms) fn push_preceding_line_comment(
        &self,
        stmt_idx: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let actual_start = crate::transforms::emit_utils::skip_trivia_forward(
            self.source_text,
            stmt_node.pos,
            stmt_node.end,
        );
        if let Some(comment) = self.extract_preceding_line_comment(actual_start) {
            current_statements.push(IRNode::Raw(comment.into()));
        }
    }

    pub(in crate::transforms) fn process_async_statement(
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

    pub(in crate::transforms) fn process_with_statement_in_async(
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

    pub(in crate::transforms) fn wrap_statements_in_with(
        temp: &str,
        mut statements: Vec<IRNode>,
    ) -> Vec<IRNode> {
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

    pub(in crate::transforms) fn is_generator_label_assignment(node: &IRNode) -> bool {
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

    pub(in crate::transforms) fn statement_to_hoistable_ir(&mut self, idx: NodeIndex) -> IRNode {
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

    pub(in crate::transforms) fn process_expression_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if self.lower_destructuring_assignment_expression(idx, current_statements) {
            return;
        }

        // Check for await expression
        if node.kind == self.suspension_kind() {
            self.process_await_expression(idx, cases, current_statements, current_label);
            // Add _a.sent() to consume the result
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
            return;
        }

        // Check for nested await inside the expression
        if self.contains_await_recursive(idx) {
            // Try specialized lowering in priority order before falling back to the
            // generic emit_nested_suspension path.  Each helper handles a specific
            // structural pattern and returns false/None if the pattern doesn't match.

            // `target = base[await index]` — element access with await in index
            if let Some(lowered) = self.lower_element_access_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                current_statements.push(IRNode::ExpressionStatement(Box::new(lowered)));
                return;
            }

            // `target = cond ? await T : F` or `target = cond ? T : await F`
            if self.lower_assignment_with_conditional_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                return;
            }

            // `(await lhs) op= await rhs` — compound assignment with await in BOTH sides
            if self.lower_compound_assignment_double_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                return;
            }

            // `lhs op= await rhs` — compound assignment with await in RHS
            if self.lower_compound_assignment_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                return;
            }

            // `L OP await R` (non-assignment, non-short-circuit)
            if let Some(lowered) = self.lower_exponentiation_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                current_statements.push(IRNode::ExpressionStatement(Box::new(lowered)));
                return;
            }

            if let Some(lowered) = self.lower_binary_non_short_circuit_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                current_statements.push(IRNode::ExpressionStatement(Box::new(lowered)));
                return;
            }

            // `L && await R`, `L || await R`, `L ?? await R`
            if let Some(lowered) = self.lower_logical_short_circuit_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                current_statements.push(IRNode::ExpressionStatement(Box::new(lowered)));
                return;
            }

            // Existing handler: property/element assignment target saving
            if self.lower_assignment_target_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                return;
            }

            // `obj[await idx] = rhs` or `obj[await idx] op= rhs` — await in LHS index
            if self.lower_lhs_element_access_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                return;
            }

            // `(obj[await idx]).prop = rhs` — property access with await in element index
            if self.lower_lhs_chained_element_access_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                return;
            }

            if let Some(lowered_array) = self.lower_array_literal_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                current_statements.push(IRNode::ExpressionStatement(Box::new(lowered_array)));
                return;
            }
            if let Some(lowered_object) = self.lower_object_literal_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                current_statements.push(IRNode::ExpressionStatement(Box::new(lowered_object)));
                return;
            }
            if let Some(lowered_access) = self.lower_element_access_object_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                current_statements.push(IRNode::ExpressionStatement(Box::new(lowered_access)));
                return;
            }
            if let Some(lowered_call) = self.lower_call_callee_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                current_statements.push(IRNode::ExpressionStatement(Box::new(lowered_call)));
                return;
            }
            if let Some(lowered_call) = self.lower_element_call_index_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                current_statements.push(IRNode::ExpressionStatement(Box::new(lowered_call)));
                return;
            }
            if let Some(lowered_new) = self.lower_new_expression_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                current_statements.push(IRNode::ExpressionStatement(Box::new(lowered_new)));
                return;
            }

            if self.async_generator_mode
                && (node.kind == syntax_kind_ext::YIELD_EXPRESSION
                    || self.node_text_contains_yield(idx))
            {
                self.emit_nested_suspension(idx, cases, current_statements, current_label);
                self.push_generator_yield(
                    opcodes::YIELD,
                    IRNode::GeneratorSent,
                    "yield",
                    cases,
                    current_statements,
                    current_label,
                );
                current_statements
                    .push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
                return;
            }
            self.emit_nested_suspension(idx, cases, current_statements, current_label);
            let ir = self
                .lower_es5_call_spread(idx)
                .unwrap_or_else(|| self.expression_to_ir(idx));
            current_statements.push(IRNode::ExpressionStatement(Box::new(ir)));
            return;
        }

        // For other expressions, convert to IR and add as expression statement
        let ir = self.expression_to_ir(idx);
        current_statements.push(IRNode::ExpressionStatement(Box::new(ir)));
    }

    pub(in crate::transforms) fn lower_destructuring_assignment_expression(
        &self,
        idx: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) -> bool {
        let target_idx = self.unwrap_parenthesized_expression(idx);
        let Some(node) = self.arena.get(target_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(bin) = self.arena.get_binary_expr(node) else {
            return false;
        };
        if self.get_operator_text(bin.operator_token) != "=" {
            return false;
        }
        let Some(left_node) = self.arena.get(bin.left) else {
            return false;
        };
        if left_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(pattern) = self.arena.get_literal_expr(left_node) else {
            return false;
        };
        if pattern.elements.nodes.is_empty() {
            return false;
        }

        let source = self.expression_to_ir(bin.right);
        for &elem_idx in &pattern.elements.nodes {
            let Some(assignment) = self.destructuring_object_assignment(elem_idx, source.clone())
            else {
                return false;
            };
            current_statements.push(IRNode::ExpressionStatement(Box::new(
                IRNode::Parenthesized(Box::new(assignment)),
            )));
        }
        true
    }

    pub(in crate::transforms) fn unwrap_parenthesized_expression(
        &self,
        mut idx: NodeIndex,
    ) -> NodeIndex {
        while let Some(node) = self.arena.get(idx)
            && node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            idx = paren.expression;
        }
        idx
    }

    pub(in crate::transforms) fn destructuring_object_assignment(
        &self,
        elem_idx: NodeIndex,
        source: IRNode,
    ) -> Option<IRNode> {
        let elem_node = self.arena.get(elem_idx)?;
        match elem_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(elem_node)?;
                let target = self.expression_to_ir(prop.initializer);
                let value = self.destructuring_object_property_value(source, prop.name)?;
                Some(IRNode::assign(target, value))
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_shorthand_property(elem_node)?;
                let name =
                    crate::transforms::emit_utils::identifier_text_or_empty(self.arena, prop.name);
                let target = IRNode::id(name.clone());
                let value = IRNode::prop(source, name);
                Some(IRNode::assign(target, value))
            }
            _ => None,
        }
    }

    pub(in crate::transforms) fn destructuring_object_property_value(
        &self,
        source: IRNode,
        name_idx: NodeIndex,
    ) -> Option<IRNode> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(name_node)?;
            return Some(IRNode::elem(
                source,
                self.expression_to_ir(computed.expression),
            ));
        }
        if name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, name_idx);
            return Some(IRNode::prop(source, name));
        }
        if name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16 {
            let lit = self.arena.get_literal(name_node)?;
            return Some(IRNode::elem(source, IRNode::string(lit.text.clone())));
        }
        if name_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16 {
            let lit = self.arena.get_literal(name_node)?;
            return Some(IRNode::elem(source, IRNode::number(lit.text.clone())));
        }
        None
    }

    pub(in crate::transforms) fn emit_nested_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        if let Some(await_idx) = self.find_suspension_expression(idx) {
            self.process_await_expression(await_idx, cases, current_statements, current_label);
        }
    }

    pub(in crate::transforms) fn process_await_expression(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        self.process_await_expression_with_trailing_comment(
            idx,
            cases,
            current_statements,
            current_label,
            None,
        );
    }

    pub(in crate::transforms) fn process_await_expression_with_trailing_comment(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        trailing_comment: Option<&str>,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        // await/yield uses UnaryExprDataEx
        if let Some(await_expr) = self.arena.get_unary_expr_ex(node) {
            if self.async_generator_mode && node.kind == syntax_kind_ext::YIELD_EXPRESSION {
                self.process_async_generator_yield_expression(
                    await_expr,
                    cases,
                    current_statements,
                    current_label,
                );
                return;
            }

            // A delegating `yield* expr` in a (non-async) generator lowers to the
            // `__values`-wrapped delegate with op 5 (`yield**`), matching tsc; the
            // `__values` helper is requested by the lowering pass. A plain `yield`
            // (and `await`) uses op 4. Async generators take the dedicated
            // `process_async_generator_yield_expression` path above.
            let is_yield_star = self.generator_mode
                && node.kind == syntax_kind_ext::YIELD_EXPRESSION
                && await_expr.asterisk_token;

            // Get the awaited expression. A bare generator `yield;` lowers to
            // `[4 /*yield*/]`, while `await;` keeps the historical empty
            // operand shape for invalid/recovered async input.
            let operand = if await_expr.expression.is_none()
                && self.generator_mode
                && node.kind == syntax_kind_ext::YIELD_EXPRESSION
            {
                None
            } else if await_expr.expression.is_none() {
                Some(IRNode::Raw("".to_string().into()))
            } else if is_yield_star {
                // `yield* x` delegates iteration: wrap the operand in `__values(x)`
                // so the runtime drives the delegate's iterator protocol.
                Some(IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__values".into())),
                    arguments: vec![self.generator_yield_operand_to_ir(await_expr.expression)],
                })
            } else if self.generator_mode && node.kind == syntax_kind_ext::YIELD_EXPRESSION {
                Some(self.generator_yield_operand_to_ir(await_expr.expression))
            } else {
                let operand = self
                    .lower_es5_call_spread(await_expr.expression)
                    .or_else(|| self.lower_es5_new_spread(await_expr.expression))
                    .unwrap_or_else(|| self.expression_to_ir(await_expr.expression));
                if self.async_generator_mode && node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
                    Some(IRNode::CallExpr {
                        callee: Box::new(IRNode::RuntimeHelper("__await".into())),
                        arguments: vec![operand],
                    })
                } else {
                    Some(operand)
                }
            };

            // Emit: return [4 /*yield*/, operand] (or [5 /*yield**/, __values(...)]).
            let (opcode, comment) = if is_yield_star {
                (opcodes::YIELD_STAR, "yield*")
            } else {
                (opcodes::YIELD, "yield")
            };
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode,
                    value: operand.map(Box::new),
                    comment: Some(comment.to_string().into()),
                },
            ))));
            if let Some(comment) = trailing_comment {
                current_statements.push(IRNode::TrailingComment(comment.to_string().into()));
            }

            // Create new case for code after await
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label = self.state.next_label();
        }
    }

    pub(in crate::transforms) fn process_async_generator_yield_expression(
        &mut self,
        yield_expr: &tsz_parser::parser::node::UnaryExprDataEx,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        if yield_expr.asterisk_token {
            let delegated = IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__values".into())),
                arguments: vec![IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__asyncDelegator".into())),
                    arguments: vec![IRNode::CallExpr {
                        callee: Box::new(IRNode::RuntimeHelper("__asyncValues".into())),
                        arguments: vec![self.expression_to_ir(yield_expr.expression)],
                    }],
                }],
            };
            self.push_generator_yield(
                opcodes::YIELD_STAR,
                delegated,
                "yield*",
                cases,
                current_statements,
                current_label,
            );

            let awaited_delegated_value = IRNode::CallExpr {
                callee: Box::new(IRNode::PropertyAccess {
                    object: Box::new(IRNode::RuntimeHelper("__await".into())),
                    property: "apply".into(),
                }),
                arguments: vec![
                    IRNode::Undefined,
                    IRNode::ArrayLiteral(vec![IRNode::GeneratorSent]),
                ],
            };
            self.push_generator_yield(
                opcodes::YIELD,
                awaited_delegated_value,
                "yield",
                cases,
                current_statements,
                current_label,
            );
            return;
        }

        let operand = if self
            .arena
            .get(yield_expr.expression)
            .is_some_and(|n| n.kind == syntax_kind_ext::AWAIT_EXPRESSION)
        {
            let awaited = self
                .arena
                .get(yield_expr.expression)
                .and_then(|n| self.arena.get_unary_expr_ex(n))
                .map_or(IRNode::Undefined, |await_expr| {
                    self.wrap_async_generator_await(await_expr.expression)
                });
            self.push_generator_yield(
                opcodes::YIELD,
                awaited,
                "yield",
                cases,
                current_statements,
                current_label,
            );
            IRNode::GeneratorSent
        } else {
            self.wrap_async_generator_await(yield_expr.expression)
        };

        self.push_generator_yield(
            opcodes::YIELD,
            operand,
            "yield",
            cases,
            current_statements,
            current_label,
        );
        self.push_generator_yield(
            opcodes::YIELD,
            IRNode::GeneratorSent,
            "yield",
            cases,
            current_statements,
            current_label,
        );
    }

    pub(in crate::transforms) fn push_generator_yield(
        &mut self,
        opcode: u32,
        value: IRNode,
        comment: &str,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode,
                value: Some(Box::new(value)),
                comment: Some(comment.to_string().into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });
        *current_label = self.state.next_label();
    }

    pub(in crate::transforms) fn wrap_async_generator_await(
        &self,
        expression: NodeIndex,
    ) -> IRNode {
        IRNode::CallExpr {
            callee: Box::new(IRNode::RuntimeHelper("__await".into())),
            arguments: vec![self.expression_to_ir(expression)],
        }
    }

    pub(in crate::transforms) fn process_variable_declaration(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        trailing_comment: &mut Option<String>,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if let Some(decl) = self.arena.get_variable_declaration(node) {
            let name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, decl.name);

            // Check if initializer contains await
            if decl.initializer.is_some() && self.is_suspension_expression(decl.initializer) {
                let trailing_comment = trailing_comment.take();
                // var x = await foo(); -> first declare var x, then yield foo(), then x = _a.sent()
                // We need to declare the variable first to avoid ReferenceError in strict mode
                current_statements.push(IRNode::VarDecl {
                    name: name.clone().into(),
                    initializer: None,
                });

                self.process_await_expression_with_trailing_comment(
                    decl.initializer,
                    cases,
                    current_statements,
                    current_label,
                    trailing_comment.as_deref(),
                );

                // Assign the sent value to the variable
                current_statements.push(IRNode::ExpressionStatement(Box::new(
                    IRNode::BinaryExpr {
                        left: Box::new(IRNode::Identifier(name.into())),
                        operator: "=".to_string().into(),
                        right: Box::new(IRNode::GeneratorSent),
                    },
                )));
                if let Some(comment) = trailing_comment {
                    current_statements.push(IRNode::TrailingComment(comment.into()));
                }
            } else if decl.initializer.is_some() && self.contains_await_recursive(decl.initializer)
            {
                // Initializer contains await but is not a direct await expression
                // (e.g., var x = (await foo()) + 1;)
                // Declare variable first, then process
                current_statements.push(IRNode::VarDecl {
                    name: name.clone().into(),
                    initializer: None,
                });

                if let Some(lowered_init) = self.lower_object_literal_before_suspension(
                    decl.initializer,
                    cases,
                    current_statements,
                    current_label,
                ) {
                    current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                        IRNode::Identifier(name.into()),
                        lowered_init,
                    ))));
                    return;
                }

                // Emit the yield for the nested await
                if let Some(lowered_init) = self.lower_call_callee_before_suspension(
                    decl.initializer,
                    cases,
                    current_statements,
                    current_label,
                ) {
                    current_statements.push(IRNode::ExpressionStatement(Box::new(
                        IRNode::BinaryExpr {
                            left: Box::new(IRNode::Identifier(name.into())),
                            operator: "=".to_string().into(),
                            right: Box::new(lowered_init),
                        },
                    )));
                    return;
                }

                if let Some(lowered_init) = self.lower_array_literal_before_suspension(
                    decl.initializer,
                    cases,
                    current_statements,
                    current_label,
                ) {
                    current_statements.push(IRNode::ExpressionStatement(Box::new(
                        IRNode::BinaryExpr {
                            left: Box::new(IRNode::Identifier(name.into())),
                            operator: "=".to_string().into(),
                            right: Box::new(lowered_init),
                        },
                    )));
                    return;
                }

                if let Some(lowered_init) = self.lower_element_access_object_before_suspension(
                    decl.initializer,
                    cases,
                    current_statements,
                    current_label,
                ) {
                    current_statements.push(IRNode::ExpressionStatement(Box::new(
                        IRNode::BinaryExpr {
                            left: Box::new(IRNode::Identifier(name.into())),
                            operator: "=".to_string().into(),
                            right: Box::new(lowered_init),
                        },
                    )));
                    return;
                }

                self.emit_nested_suspension(
                    decl.initializer,
                    cases,
                    current_statements,
                    current_label,
                );
                let init = self.expression_to_ir(decl.initializer);
                current_statements.push(IRNode::ExpressionStatement(Box::new(
                    IRNode::BinaryExpr {
                        left: Box::new(IRNode::Identifier(name.into())),
                        operator: "=".to_string().into(),
                        right: Box::new(init),
                    },
                )));
            } else {
                // No await in initializer - emit as normal
                if let Some((temp, lowered_init)) =
                    self.lower_object_literal_es5_with_computed_properties(decl.initializer)
                {
                    current_statements.push(IRNode::VarDecl {
                        name: name.clone().into(),
                        initializer: None,
                    });
                    current_statements.push(IRNode::VarDecl {
                        name: temp.into(),
                        initializer: None,
                    });
                    current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                        IRNode::Identifier(name.into()),
                        lowered_init,
                    ))));
                    return;
                }

                let init = if decl.initializer.is_none() {
                    None
                } else {
                    Some(Box::new(self.expression_to_ir(decl.initializer)))
                };

                current_statements.push(IRNode::VarDecl {
                    name: name.into(),
                    initializer: init,
                });
            }
        }
    }

    pub(in crate::transforms) fn lower_class_declaration_to_assignment(
        &mut self,
        idx: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) -> bool {
        let mut class_transformer = ES5ClassTransformer::new(self.arena);
        class_transformer.set_module_kind(self.module_kind);
        class_transformer.set_target_es5(self.target_es5);
        if let Some(source_text) = self.source_text {
            class_transformer.set_source_text(source_text);
        }
        let Some(class_ir) = class_transformer.transform_class_to_ir(idx) else {
            return false;
        };

        let IRNode::ES5ClassIIFE {
            name,
            binding_name: _,
            base_class,
            super_param,
            body,
            weakmap_decls,
            computed_prop_temp_decls,
            computed_prop_temp_inits,
            weakmap_inits,
            leading_comment,
            deferred_static_blocks,
            deferred_block_class_alias,
            ..
        } = class_ir
        else {
            return false;
        };

        for decl_name in weakmap_decls
            .into_iter()
            .chain(computed_prop_temp_decls)
            .chain(deferred_block_class_alias.iter().cloned())
            .chain(std::iter::once(name.to_string()))
        {
            current_statements.push(IRNode::VarDecl {
                name: decl_name.into(),
                initializer: None,
            });
        }
        current_statements.push(IRNode::ES5ClassAssignment {
            name,
            base_class,
            super_param,
            body,
            computed_prop_temp_inits,
            weakmap_inits,
            leading_comment,
            deferred_static_blocks,
            deferred_static_result_temp: None,
            deferred_block_class_alias,
        });

        true
    }

    pub(in crate::transforms) fn class_extends_suspension(
        &self,
        class_idx: NodeIndex,
    ) -> Option<(String, NodeIndex, NodeIndex)> {
        let node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(node)?;
        let class_name =
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, class_data.name);
        if class_name.is_empty() {
            return None;
        }
        let extends_expr = crate::transforms::emit_utils::get_extends_expression_index(
            self.arena,
            &class_data.heritage_clauses,
        )?;
        let suspension_idx = self.find_suspension_expression(extends_expr)?;
        Some((class_name, extends_expr, suspension_idx))
    }

    pub(in crate::transforms::async_es5_ir) fn es5_class_factory(
        &self,
        class_idx: NodeIndex,
        class_name: &str,
    ) -> Option<ES5ClassFactoryParts> {
        let mut class_transformer = ES5ClassTransformer::new(self.arena);
        class_transformer.set_module_kind(self.module_kind);
        class_transformer.set_target_es5(self.target_es5);
        let class_ir =
            class_transformer.transform_class_to_ir_with_name(class_idx, Some(class_name))?;
        let IRNode::ES5ClassIIFE {
            binding_name: _,
            body,
            super_param,
            weakmap_decls,
            computed_prop_temp_decls: _,
            computed_prop_temp_inits: _,
            weakmap_inits,
            deferred_static_blocks,
            ..
        } = class_ir
        else {
            return None;
        };
        Some(ES5ClassFactoryParts {
            factory: IRNode::FunctionExpr {
                name: None,
                parameters: vec![IRParam::new(
                    super_param.as_deref().unwrap_or("_super").to_string(),
                )],
                body,
                is_expression_body: false,
                body_source_range: None,
            },
            weakmap_decls,
            weakmap_inits,
            deferred_static_blocks,
        })
    }

    pub(in crate::transforms) fn extends_value_after_suspension(
        &self,
        extends_expr: NodeIndex,
    ) -> IRNode {
        let stripped = self.strip_parenthesized_expression(extends_expr);
        if self.is_suspension_expression(stripped) {
            IRNode::GeneratorSent
        } else {
            self.expression_to_ir(extends_expr)
        }
    }

    pub(in crate::transforms) fn strip_parenthesized_expression(
        &self,
        mut idx: NodeIndex,
    ) -> NodeIndex {
        loop {
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return idx;
            }
            let Some(paren) = self.arena.get_parenthesized(node) else {
                return idx;
            };
            idx = paren.expression;
        }
    }
}
