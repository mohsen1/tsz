impl<'a> AstToIr<'a> {
    fn convert_arrow_function(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");

        // ArrowFunction uses FunctionData (has equals_greater_than_token set)
        if let Some(arrow) = self.arena.get_function(node) {
            if arrow.is_async {
                return self.convert_async_arrow_function(arrow);
            }
            let hoisted_before = self.hoisted_temps.borrow().len();
            let saved_temp_counter = self.temp_var_counter.get();
            self.temp_var_counter.set(0);

            // First check if there's a directive from LoweringPass
            let (captures_this, class_alias) = if let Some(ref transforms) = self.transforms {
                if let Some(crate::context::transform::TransformDirective::ES5ArrowFunction {
                    captures_this,
                    class_alias,
                    ..
                }) = transforms.get(idx)
                {
                    (
                        *captures_this,
                        class_alias.as_ref().map(std::string::ToString::to_string),
                    )
                } else {
                    // No directive, fall back to local analysis
                    (contains_this_reference(self.arena, idx), None)
                }
            } else {
                // No transforms available, fall back to local analysis
                (contains_this_reference(self.arena, idx), None)
            };

            // Save previous state and set captured flag if needed
            let prev_captured = self.this_captured.get();
            let prev_substitution = self.current_this_substitution.take();
            let lexical_this_capture_alias = self.lexical_this_capture_alias.take();
            self.lexical_this_capture_alias
                .set(lexical_this_capture_alias.clone());
            let class_alias = class_alias.map(ThisSubstitution::Identifier);
            let this_substitution = if captures_this {
                lexical_this_capture_alias
                    .or_else(|| prev_substitution.clone())
                    .or(class_alias)
            } else {
                None
            };

            if captures_this && this_substitution.is_none() {
                self.this_captured.set(true);
            }
            self.current_this_substitution.set(this_substitution);

            let params = self.convert_parameters(&arrow.parameters);
            let (body, is_expression_body, body_source_range) =
                if let Some(body_node) = self.arena.get(arrow.body) {
                    if self.arena.get_block(body_node).is_some() {
                        let stmts = self.convert_block_statements_with_using_region(arrow.body);
                        let range = Some((body_node.pos, body_node.end));
                        (stmts, false, range)
                    } else {
                        // Expression body
                        let expr = self.convert_expression(arrow.body);
                        (
                            vec![IRNode::ReturnStatement(Some(Box::new(expr)))],
                            true,
                            None,
                        )
                    }
                } else {
                    (vec![], false, None)
                };
            let mut body = body;
            self.temp_var_counter.set(saved_temp_counter);
            self.prepend_function_hoisted_temps(&mut body, hoisted_before);

            // Restore previous state
            self.this_captured.set(prev_captured);
            self.current_this_substitution.set(prev_substitution);

            // Arrow functions become regular functions in ES5

            // TypeScript's ES5 arrow transform:
            // - Convert arrow to plain function expression
            // - Containing function emits `var _this = this;` at body start
            // - Substitution of `this` -> `_this` is handled by IRNode::This { captured: true }
            //
            // Note: We no longer use IIFE wrappers like `(function (_this) { ... })(this)`
            // The `_this` capture should be hoisted to the containing function's body start.
            IRNode::FunctionExpr {
                name: None,
                parameters: params,
                body,
                is_expression_body,
                body_source_range,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_async_arrow_function(&self, arrow: &FunctionData) -> IRNode {
        let mut transformer = AsyncES5Transformer::new(self.arena);
        transformer.set_temp_var_counter(self.temp_var_counter.get());
        transformer.set_module_kind(self.module_kind);
        transformer.set_target_es5(self.target_es5);
        transformer
            .dynamic_import_promise_counter
            .set(self.dynamic_import_promise_counter.get());
        if let Some(source_text) = self.source_text {
            transformer.set_source_text(source_text);
        }
        let has_await = transformer.body_contains_await(arrow.body);
        let mut generator_body = transformer.transform_generator_body(arrow.body, has_await);
        self.temp_var_counter.set(transformer.temp_var_counter());
        self.dynamic_import_promise_counter
            .set(transformer.dynamic_import_promise_counter.get());
        let hoisted_var_groups =
            AsyncES5Transformer::extract_and_remove_var_decl_groups(&mut generator_body);
        let this_arg = self.async_arrow_awaiter_this_arg();
        // Capture `arguments` into the wrapper when the arrow body references it;
        // a captured-arguments wrapper becomes a block (see helper for details).
        let body = transformer.build_async_arrow_awaiter_body(
            this_arg,
            generator_body,
            hoisted_var_groups,
        );
        let is_expression_body = !transformer.state.captures_arguments;
        IRNode::FunctionExpr {
            name: None,
            parameters: self.convert_parameters(&arrow.parameters),
            body,
            is_expression_body,
            body_source_range: None,
        }
    }

    fn async_arrow_awaiter_this_arg(&self) -> IRNode {
        if let Some(substitution) = self.current_this_substitution.take() {
            self.current_this_substitution
                .set(Some(substitution.clone()));
            return match substitution {
                ThisSubstitution::Identifier(alias) => IRNode::id(alias),
                ThisSubstitution::Raw(expr) => IRNode::Raw(expr.into()),
            };
        }
        if let Some(substitution) = self.lexical_this_capture_alias.take() {
            self.lexical_this_capture_alias
                .set(Some(substitution.clone()));
            return match substitution {
                ThisSubstitution::Identifier(alias) => IRNode::id(alias),
                ThisSubstitution::Raw(expr) => IRNode::Raw(expr.into()),
            };
        }
        if self.this_captured.get() {
            IRNode::id("_this")
        } else {
            IRNode::void_0()
        }
    }

    fn prepend_function_hoisted_temps(&self, body: &mut Vec<IRNode>, hoisted_before: usize) {
        let hoisted_after = self.hoisted_temps.borrow().len();
        if hoisted_after <= hoisted_before {
            return;
        }

        let local_temps: Vec<String> = self
            .hoisted_temps
            .borrow_mut()
            .drain(hoisted_before..)
            .collect();
        let var_decls = local_temps
            .into_iter()
            .map(|name| IRNode::VarDecl {
                name: name.into(),
                initializer: None,
            })
            .collect();
        body.insert(0, IRNode::VarDeclList(var_decls));
    }

    fn convert_parameters(&self, params: &NodeList) -> Vec<IRParam> {
        params
            .nodes
            .iter()
            .filter_map(|&p| {
                let node = self.arena.get(p)?;
                let param = self.arena.get_parameter(node)?;
                let name = get_identifier_text(self.arena, param.name)?;
                let rest = param.dot_dot_dot_token;
                // Convert default value if present
                let default_value = (param.initializer.is_some())
                    .then(|| Box::new(self.convert_expression(param.initializer)));
                Some(IRParam {
                    name: name.into(),
                    rest,
                    default_value,
                    leading_comment: None,
                })
            })
            .collect()
    }

    fn convert_spread_element(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        // SpreadElement uses SpreadData
        if let Some(spread) = self.arena.get_spread(node) {
            IRNode::SpreadElement(Box::new(self.convert_expression(spread.expression)))
        } else {
            IRNode::ASTRef(idx)
        }
    }

    const fn convert_template_literal(&self, idx: NodeIndex) -> IRNode {
        // Template literals need string concatenation in ES5
        // For now, use ASTRef as a fallback
        IRNode::ASTRef(idx)
    }

    fn convert_await_expression(&self, idx: NodeIndex) -> IRNode {
        if self.emit_await_as_yield {
            let Some(node) = self.arena.get(idx) else {
                return IRNode::Raw("yield ".into());
            };
            let Some(await_expr) = self.arena.get_unary_expr_ex(node) else {
                return IRNode::Raw("yield ".into());
            };
            if await_expr.expression.is_none() {
                return IRNode::Raw("yield ".into());
            }
            return IRNode::Raw(
                format!(
                    "yield {}",
                    self.emit_ir_fragment_to_string(
                        &self.convert_expression(await_expr.expression)
                    )
                )
                .into(),
            );
        }
        // Await expressions are handled by the async transform.
        IRNode::ASTRef(idx)
    }

    fn convert_non_null(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        // NON_NULL_EXPRESSION uses UnaryExpressionData
        if let Some(unary) = self.arena.get_unary_expr_ex(node) {
            self.convert_expression(unary.expression)
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn is_destructuring_assignment_expr(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        let target_expr = if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            self.arena
                .get_parenthesized(expr_node)
                .map(|p| p.expression)
                .unwrap_or(expr_idx)
        } else {
            expr_idx
        };
        let Some(bin_node) = self.arena.get(target_expr) else {
            return false;
        };
        if bin_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(bin) = self.arena.get_binary_expr(bin_node) else {
            return false;
        };
        if bin.operator_token != SyntaxKind::EqualsToken as u16 {
            return false;
        }
        self.arena.get(bin.left).is_some_and(|left| {
            left.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || left.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        })
    }
}
