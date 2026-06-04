impl<'a> AsyncES5Transformer<'a> {
    fn process_expression_in_async(
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

    fn lower_destructuring_assignment_expression(
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

    fn unwrap_parenthesized_expression(&self, mut idx: NodeIndex) -> NodeIndex {
        while let Some(node) = self.arena.get(idx)
            && node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            idx = paren.expression;
        }
        idx
    }

    fn destructuring_object_assignment(
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

    fn destructuring_object_property_value(
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

    pub(super) fn emit_nested_suspension(
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

    fn process_await_expression(
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

    fn process_await_expression_with_trailing_comment(
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

            // Emit: return [4 /*yield*/, operand];
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::YIELD,
                    value: operand.map(Box::new),
                    comment: Some("yield".to_string().into()),
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

    fn process_async_generator_yield_expression(
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

    fn push_generator_yield(
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

    fn wrap_async_generator_await(&self, expression: NodeIndex) -> IRNode {
        IRNode::CallExpr {
            callee: Box::new(IRNode::RuntimeHelper("__await".into())),
            arguments: vec![self.expression_to_ir(expression)],
        }
    }

    fn process_variable_declaration(
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

    fn lower_class_declaration_to_assignment(
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

    fn class_extends_suspension(
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

    fn es5_class_factory(
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

    fn extends_value_after_suspension(&self, extends_expr: NodeIndex) -> IRNode {
        let stripped = self.strip_parenthesized_expression(extends_expr);
        if self.is_suspension_expression(stripped) {
            IRNode::GeneratorSent
        } else {
            self.expression_to_ir(extends_expr)
        }
    }

    fn strip_parenthesized_expression(&self, mut idx: NodeIndex) -> NodeIndex {
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

    /// Process an if statement inside an async function body.
    ///
    /// When neither branch contains await, falls through to raw IR emission.
    /// When branches contain await, generates proper state machine labels.
    fn process_if_statement_in_async(
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

        let mut else_label: Option<u32> = if delayed_else_label {
            None
        } else {
            Some(self.state.next_label())
        };
        let mut end_label: Option<u32> = if delayed_end_label {
            None
        } else {
            // No branch suspends: both else_label and end_label are safe to allocate now.
            if has_else {
                Some(self.state.next_label())
            } else {
                // No else: end_label == else_label (the next case after the then block)
                else_label
            }
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
            // No else branch.
            // Flush current case and start end label
            if !current_statements.is_empty() {
                cases.push(IRGeneratorCase {
                    label: *current_label,
                    statements: std::mem::take(current_statements),
                });
            }
            *current_label = end_label.expect("end label must be available after if lowering");
        }
    }

    fn process_captured_for_statement_in_async(
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
}
