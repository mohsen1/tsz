//! Async ES5 call-expression state-machine lowering.

use super::AsyncES5Transformer;
use crate::transforms::ir::{IRGeneratorCase, IRNode};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> AsyncES5Transformer<'a> {
    pub(super) fn lower_array_literal_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.lower_suspended_array_literal(idx, cases, current_statements, current_label)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let binary = self.arena.get_binary_expr(node)?;
                if self.get_operator_text(binary.operator_token) != "=" {
                    return None;
                }
                let left = self.arena.get(binary.left)?;
                if left.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                    return None;
                }
                let right = self.lower_array_literal_before_suspension(
                    binary.right,
                    cases,
                    current_statements,
                    current_label,
                )?;
                Some(IRNode::assign(self.expression_to_ir(binary.left), right))
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.arena.get_parenthesized(node)?;
                self.lower_array_literal_before_suspension(
                    paren.expression,
                    cases,
                    current_statements,
                    current_label,
                )
                .map(|expr| IRNode::Parenthesized(Box::new(expr)))
            }
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                let assertion = self.arena.get_type_assertion(node)?;
                self.lower_array_literal_before_suspension(
                    assertion.expression,
                    cases,
                    current_statements,
                    current_label,
                )
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let unary = self.arena.get_unary_expr_ex(node)?;
                self.lower_array_literal_before_suspension(
                    unary.expression,
                    cases,
                    current_statements,
                    current_label,
                )
            }
            _ => None,
        }
    }

    fn lower_suspended_array_literal(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        let array = self.arena.get_literal_expr(node)?;
        let elements = &array.elements.nodes;
        let first_suspension_index = elements
            .iter()
            .position(|&element| self.contains_await_recursive(element))?;

        if self.args_contain_spread(elements) {
            return self.lower_suspended_spread_array_literal(
                elements,
                first_suspension_index,
                cases,
                current_statements,
                current_label,
            );
        }

        self.lower_suspended_plain_array_literal(
            elements,
            first_suspension_index,
            cases,
            current_statements,
            current_label,
        )
    }

    fn lower_suspended_plain_array_literal(
        &mut self,
        elements: &[NodeIndex],
        first_suspension_index: usize,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let suspension_indices = elements
            .iter()
            .enumerate()
            .filter_map(|(index, &element)| self.contains_await_recursive(element).then_some(index))
            .collect::<Vec<_>>();
        if suspension_indices.len() == 1 && first_suspension_index == 0 {
            self.emit_nested_suspension(
                elements[first_suspension_index],
                cases,
                current_statements,
                current_label,
            );
            return Some(IRNode::ArrayLiteral(
                elements
                    .iter()
                    .map(|&element| self.expression_to_ir(element))
                    .collect(),
            ));
        }

        let temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: temp.clone().into(),
            initializer: None,
        });
        let mut temp_initialized = false;
        if first_suspension_index > 0 {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(temp.clone()),
                IRNode::ArrayLiteral(
                    elements[..first_suspension_index]
                        .iter()
                        .map(|&element| self.expression_to_ir(element))
                        .collect(),
                ),
            ))));
            temp_initialized = true;
        }

        for (position, &suspension_index) in suspension_indices.iter().enumerate() {
            self.emit_nested_suspension(
                elements[suspension_index],
                cases,
                current_statements,
                current_label,
            );

            let next_suspension_index = suspension_indices.get(position + 1).copied();
            let segment_end = next_suspension_index.unwrap_or(elements.len());
            let segment = IRNode::ArrayLiteral(
                elements[suspension_index..segment_end]
                    .iter()
                    .map(|&element| self.expression_to_ir(element))
                    .collect(),
            );

            if next_suspension_index.is_some() {
                let value = if temp_initialized {
                    IRNode::call(
                        IRNode::prop(IRNode::id(temp.clone()), "concat"),
                        vec![segment],
                    )
                } else {
                    segment
                };
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::id(temp.clone()),
                    value,
                ))));
                temp_initialized = true;
            } else if temp_initialized {
                return Some(IRNode::call(
                    IRNode::prop(IRNode::id(temp), "concat"),
                    vec![segment],
                ));
            } else {
                return Some(segment);
            }
        }

        None
    }

    fn lower_suspended_spread_array_literal(
        &mut self,
        elements: &[NodeIndex],
        first_suspension_index: usize,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let suspension_indices = elements
            .iter()
            .enumerate()
            .filter_map(|(index, &element)| self.contains_await_recursive(element).then_some(index))
            .collect::<Vec<_>>();
        if suspension_indices.len() != 1 {
            return None;
        }

        self.helpers_needed.mark_spread_array();
        let suspension_is_spread = self.is_spread_arg(elements[first_suspension_index]);
        let needs_prefix_temp = first_suspension_index > 0 || suspension_is_spread;
        let prefix_temp = needs_prefix_temp.then(|| self.generate_hoisted_temp());

        if let Some(temp) = &prefix_temp {
            current_statements.push(IRNode::VarDecl {
                name: temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(temp.clone()),
                IRNode::ArrayLiteral(vec![
                    self.spread_array_base_array(&elements[..first_suspension_index]),
                ]),
            ))));
        }

        self.emit_nested_suspension(
            elements[first_suspension_index],
            cases,
            current_statements,
            current_label,
        );

        let current = if let Some(temp) = prefix_temp {
            let resumed = if suspension_is_spread {
                IRNode::Parenthesized(Box::new(IRNode::GeneratorSent))
            } else {
                IRNode::ArrayLiteral(vec![IRNode::GeneratorSent])
            };
            let pack = IRNode::BooleanLiteral(suspension_is_spread);
            self.spread_array_apply(IRNode::CallExpr {
                callee: Box::new(IRNode::prop(IRNode::id(temp), "concat")),
                arguments: vec![IRNode::ArrayLiteral(vec![resumed, pack])],
            })
        } else {
            IRNode::ArrayLiteral(vec![IRNode::GeneratorSent])
        };

        Some(self.append_spread_array_suffix(current, &elements[first_suspension_index + 1..]))
    }

    fn append_spread_array_suffix(&mut self, mut current: IRNode, suffix: &[NodeIndex]) -> IRNode {
        let mut segment = Vec::new();
        for &element in suffix {
            if self.is_spread_arg(element) {
                if !segment.is_empty() {
                    current = self.spread_array_apply(IRNode::ArrayLiteral(vec![
                        current,
                        IRNode::ArrayLiteral(std::mem::take(&mut segment)),
                        IRNode::BooleanLiteral(false),
                    ]));
                }
                current = self.spread_array_apply(IRNode::ArrayLiteral(vec![
                    current,
                    self.spread_arg_expression_to_ir(element),
                    IRNode::BooleanLiteral(true),
                ]));
            } else {
                segment.push(self.expression_to_ir(element));
            }
        }
        if !segment.is_empty() {
            current = self.spread_array_apply(IRNode::ArrayLiteral(vec![
                current,
                IRNode::ArrayLiteral(segment),
                IRNode::BooleanLiteral(false),
            ]));
        }
        current
    }

    fn spread_array_base_array(&mut self, elements: &[NodeIndex]) -> IRNode {
        if elements.is_empty() || !self.args_contain_spread(elements) {
            return IRNode::ArrayLiteral(
                elements
                    .iter()
                    .map(|&element| self.expression_to_ir(element))
                    .collect(),
            );
        }

        let mut current = IRNode::ArrayLiteral(Vec::new());
        let mut segment = Vec::new();
        for &element in elements {
            if self.is_spread_arg(element) {
                if !segment.is_empty() {
                    current = self.spread_array_apply(IRNode::ArrayLiteral(vec![
                        current,
                        IRNode::ArrayLiteral(std::mem::take(&mut segment)),
                        IRNode::BooleanLiteral(false),
                    ]));
                }
                current = self.spread_array_call(
                    current,
                    self.spread_arg_expression_to_ir(element),
                    true,
                );
            } else {
                segment.push(self.expression_to_ir(element));
            }
        }
        if !segment.is_empty() {
            current = self.spread_array_apply(IRNode::ArrayLiteral(vec![
                current,
                IRNode::ArrayLiteral(segment),
                IRNode::BooleanLiteral(false),
            ]));
        }
        current
    }

    pub(super) fn lower_call_callee_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(node)?;
        if self.contains_await_recursive(call.expression) {
            return None;
        }
        let args = call.arguments.as_ref()?;
        let suspension_arg_index = args
            .nodes
            .iter()
            .position(|&arg| self.contains_await_recursive(arg))?;

        let has_spread = self.args_contain_spread(&args.nodes);
        let (callee_temp, apply_receiver, this_arg) = if has_spread {
            self.capture_call_apply_before_suspension(call.expression, current_statements)?
        } else {
            let (callee_temp, this_arg) =
                self.capture_call_callee_before_suspension(call.expression, current_statements)?;
            (callee_temp, IRNode::Undefined, this_arg)
        };
        let arg_array = self.lower_suspended_call_arguments(
            &args.nodes,
            suspension_arg_index,
            current_statements,
        );

        self.emit_nested_suspension(idx, cases, current_statements, current_label);

        if has_spread {
            let apply_args_temp = this_arg;
            Some(IRNode::CallExpr {
                callee: Box::new(IRNode::prop(IRNode::id(callee_temp), "apply")),
                arguments: vec![
                    apply_receiver,
                    IRNode::CallExpr {
                        callee: Box::new(IRNode::prop(apply_args_temp, "concat")),
                        arguments: vec![IRNode::ArrayLiteral(vec![arg_array])],
                    },
                ],
            })
        } else {
            Some(IRNode::CallExpr {
                callee: Box::new(IRNode::prop(IRNode::id(callee_temp), "apply")),
                arguments: vec![this_arg, arg_array],
            })
        }
    }

    fn capture_call_callee_before_suspension(
        &self,
        callee: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) -> Option<(String, IRNode)> {
        let callee_node = self.arena.get(callee)?;

        if callee_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let callee_temp = self.generate_hoisted_temp();
            current_statements.push(IRNode::VarDecl {
                name: callee_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(callee_temp.clone()),
                self.expression_to_ir(callee),
            ))));
            return Some((callee_temp, IRNode::Undefined));
        }

        if callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(callee_node)?;
            let this_temp = self.generate_hoisted_temp();
            let callee_temp = self.generate_hoisted_temp();
            current_statements.push(IRNode::VarDecl {
                name: this_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::VarDecl {
                name: callee_temp.clone().into(),
                initializer: None,
            });
            let property = crate::transforms::emit_utils::identifier_text_or_empty(
                self.arena,
                access.name_or_argument,
            );
            let captured_receiver = IRNode::Parenthesized(Box::new(IRNode::assign(
                IRNode::id(this_temp.clone()),
                self.expression_to_ir(access.expression),
            )));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(callee_temp.clone()),
                IRNode::prop(captured_receiver, property),
            ))));

            return Some((callee_temp, IRNode::id(this_temp)));
        }

        if callee_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.arena.get_access_expr(callee_node)?;
        let this_temp = self.generate_hoisted_temp();
        let callee_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: this_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::VarDecl {
            name: callee_temp.clone().into(),
            initializer: None,
        });
        let captured_receiver = IRNode::Parenthesized(Box::new(IRNode::assign(
            IRNode::id(this_temp.clone()),
            self.expression_to_ir(access.expression),
        )));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(callee_temp.clone()),
            IRNode::elem(
                captured_receiver,
                self.expression_to_ir(access.name_or_argument),
            ),
        ))));

        Some((callee_temp, IRNode::id(this_temp)))
    }

    fn capture_call_apply_before_suspension(
        &self,
        callee: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) -> Option<(String, IRNode, IRNode)> {
        let callee_node = self.arena.get(callee)?;
        let receiver_temp = self.generate_hoisted_temp();
        let apply_temp = self.generate_hoisted_temp();
        let this_args_temp = self.generate_hoisted_temp();

        current_statements.push(IRNode::VarDecl {
            name: receiver_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::VarDecl {
            name: apply_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::VarDecl {
            name: this_args_temp.clone().into(),
            initializer: None,
        });

        let (receiver_value, this_arg) =
            if callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = self.arena.get_access_expr(callee_node)?;
                let property = crate::transforms::emit_utils::identifier_text_or_empty(
                    self.arena,
                    access.name_or_argument,
                );
                (
                    IRNode::prop(
                        IRNode::Parenthesized(Box::new(IRNode::assign(
                            IRNode::id(receiver_temp.clone()),
                            self.expression_to_ir(access.expression),
                        ))),
                        property,
                    ),
                    IRNode::id(receiver_temp.clone()),
                )
            } else if callee_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                let access = self.arena.get_access_expr(callee_node)?;
                (
                    IRNode::elem(
                        IRNode::Parenthesized(Box::new(IRNode::assign(
                            IRNode::id(receiver_temp.clone()),
                            self.expression_to_ir(access.expression),
                        ))),
                        self.expression_to_ir(access.name_or_argument),
                    ),
                    IRNode::id(receiver_temp.clone()),
                )
            } else {
                (
                    IRNode::Parenthesized(Box::new(IRNode::assign(
                        IRNode::id(receiver_temp.clone()),
                        self.expression_to_ir(callee),
                    ))),
                    IRNode::Undefined,
                )
            };

        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(apply_temp.clone()),
            IRNode::prop(receiver_value, "apply"),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(this_args_temp.clone()),
            IRNode::ArrayLiteral(vec![this_arg]),
        ))));

        Some((
            apply_temp,
            IRNode::id(receiver_temp),
            IRNode::id(this_args_temp),
        ))
    }

    fn lower_suspended_call_arguments(
        &mut self,
        args: &[NodeIndex],
        suspension_arg_index: usize,
        current_statements: &mut Vec<IRNode>,
    ) -> IRNode {
        if self.args_contain_spread(args) {
            return self.lower_suspended_spread_call_arguments(
                args,
                suspension_arg_index,
                current_statements,
            );
        }

        if suspension_arg_index == 0 {
            let lowered_args = args.iter().map(|&arg| self.expression_to_ir(arg)).collect();
            return IRNode::ArrayLiteral(lowered_args);
        }
        let prefix_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: prefix_temp.clone().into(),
            initializer: None,
        });
        let prefix_args = args[..suspension_arg_index]
            .iter()
            .map(|&arg| self.expression_to_ir(arg))
            .collect();
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(prefix_temp.clone()),
            IRNode::ArrayLiteral(prefix_args),
        ))));

        let suffix_args = args[suspension_arg_index..]
            .iter()
            .map(|&arg| self.expression_to_ir(arg))
            .collect();
        IRNode::CallExpr {
            callee: Box::new(IRNode::prop(IRNode::id(prefix_temp), "concat")),
            arguments: vec![IRNode::ArrayLiteral(suffix_args)],
        }
    }

    fn lower_suspended_spread_call_arguments(
        &mut self,
        args: &[NodeIndex],
        suspension_arg_index: usize,
        current_statements: &mut Vec<IRNode>,
    ) -> IRNode {
        self.helpers_needed.mark_spread_array();
        let prefix_base = self.spread_call_base_array(&args[..suspension_arg_index]);
        let suspension_is_spread = self.is_spread_arg(args[suspension_arg_index]);
        let has_prefix_spread = self.args_contain_spread(&args[..suspension_arg_index]);

        let mut current = if suspension_is_spread || has_prefix_spread {
            let prefix_temp = self.generate_hoisted_temp();
            current_statements.push(IRNode::VarDecl {
                name: prefix_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(prefix_temp.clone()),
                IRNode::ArrayLiteral(vec![prefix_base]),
            ))));
            let resumed = if suspension_is_spread {
                IRNode::Parenthesized(Box::new(IRNode::GeneratorSent))
            } else {
                IRNode::ArrayLiteral(vec![IRNode::GeneratorSent])
            };
            self.spread_array_apply(IRNode::CallExpr {
                callee: Box::new(IRNode::prop(IRNode::id(prefix_temp), "concat")),
                arguments: vec![IRNode::ArrayLiteral(vec![
                    resumed,
                    IRNode::BooleanLiteral(false),
                ])],
            })
        } else {
            IRNode::ArrayLiteral(vec![IRNode::GeneratorSent])
        };

        let mut segment = Vec::new();
        for &arg in &args[suspension_arg_index + 1..] {
            if self.is_spread_arg(arg) {
                if !segment.is_empty() {
                    current = self.spread_array_apply(IRNode::ArrayLiteral(vec![
                        current,
                        IRNode::ArrayLiteral(std::mem::take(&mut segment)),
                        IRNode::BooleanLiteral(false),
                    ]));
                }
                current = self.spread_array_apply(IRNode::ArrayLiteral(vec![
                    current,
                    self.spread_arg_expression_to_ir(arg),
                    IRNode::BooleanLiteral(false),
                ]));
            } else {
                segment.push(self.expression_to_ir(arg));
            }
        }
        if !segment.is_empty() {
            current = self.spread_array_apply(IRNode::ArrayLiteral(vec![
                current,
                IRNode::ArrayLiteral(segment),
                IRNode::BooleanLiteral(false),
            ]));
        }
        current
    }

    pub(super) fn lower_element_call_index_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(node)?;
        let callee_node = self.arena.get(call.expression)?;
        if callee_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(callee_node)?;
        if self.contains_await_recursive(access.expression)
            || !self.contains_await_recursive(access.name_or_argument)
        {
            return None;
        }

        let object_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: object_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(object_temp.clone()),
            self.expression_to_ir(access.expression),
        ))));

        self.emit_nested_suspension(
            access.name_or_argument,
            cases,
            current_statements,
            current_label,
        );

        let args = call
            .arguments
            .as_ref()
            .map(|args| {
                args.nodes
                    .iter()
                    .map(|&arg| self.expression_to_ir(arg))
                    .collect()
            })
            .unwrap_or_default();
        Some(IRNode::CallExpr {
            callee: Box::new(IRNode::elem(IRNode::id(object_temp), IRNode::GeneratorSent)),
            arguments: args,
        })
    }

    pub(super) fn lower_new_expression_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }
        let new_expr = self.arena.get_call_expr(node)?;
        if let Some(lowered) = self.lower_new_element_callee_index_before_suspension(
            new_expr.expression,
            new_expr
                .arguments
                .as_ref()
                .map(|args| args.nodes.as_slice()),
            cases,
            current_statements,
            current_label,
        ) {
            return Some(lowered);
        }
        if self.contains_await_recursive(new_expr.expression) {
            self.emit_nested_suspension(
                new_expr.expression,
                cases,
                current_statements,
                current_label,
            );
            let args = new_expr.arguments.as_ref();
            if let Some(args) = args
                && self.args_contain_spread(&args.nodes)
            {
                let receiver_temp = self.generate_hoisted_temp();
                current_statements.push(IRNode::VarDecl {
                    name: receiver_temp.clone().into(),
                    initializer: None,
                });
                let captured_receiver = IRNode::Parenthesized(Box::new(IRNode::assign(
                    IRNode::id(receiver_temp.clone()),
                    IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
                )));
                let arg_array = self.spread_new_base_array(&args.nodes);
                return Some(Self::new_from_bound_apply(
                    IRNode::prop(captured_receiver, "bind"),
                    IRNode::id(receiver_temp),
                    arg_array,
                ));
            }
            let args = args
                .map(|args| {
                    args.nodes
                        .iter()
                        .map(|&arg| self.expression_to_ir(arg))
                        .collect()
                })
                .unwrap_or_default();
            let callee = self
                .new_callee_after_suspension(new_expr.expression)
                .unwrap_or_else(|| IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)));
            return Some(IRNode::NewExpr {
                callee: Box::new(callee),
                arguments: args,
                explicit_arguments: new_expr.arguments.is_some(),
            });
        }

        let args = new_expr.arguments.as_ref()?;
        let suspension_arg_index = args
            .nodes
            .iter()
            .position(|&arg| self.contains_await_recursive(arg))?;
        let has_spread = self.args_contain_spread(&args.nodes);
        let spread_capture = has_spread.then(|| {
            self.capture_new_bind_apply_before_suspension(new_expr.expression, current_statements)
        });
        let bind_capture = (!has_spread).then(|| {
            self.capture_new_bind_before_suspension(new_expr.expression, current_statements)
        });
        let arg_array = self.lower_suspended_new_arguments(
            &args.nodes,
            suspension_arg_index,
            current_statements,
        );

        self.emit_nested_suspension(
            args.nodes[suspension_arg_index],
            cases,
            current_statements,
            current_label,
        );

        if let Some((bind_temp, apply_temp, receiver_args_temp)) = spread_capture {
            return Some(Self::new_from_apply_apply(
                IRNode::id(apply_temp),
                IRNode::id(bind_temp),
                IRNode::id(receiver_args_temp),
                arg_array,
            ));
        }

        let (bind_temp, receiver) = bind_capture?;
        Some(Self::new_from_bound_apply(
            IRNode::id(bind_temp),
            receiver,
            arg_array,
        ))
    }

    fn lower_new_element_callee_index_before_suspension(
        &mut self,
        callee: NodeIndex,
        args: Option<&[NodeIndex]>,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let callee_node = self.arena.get(callee)?;
        if callee_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(callee_node)?;
        if self.contains_await_recursive(access.expression)
            || !self.contains_await_recursive(access.name_or_argument)
        {
            return None;
        }

        let object_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: object_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(object_temp.clone()),
            self.expression_to_ir(access.expression),
        ))));

        self.emit_nested_suspension(
            access.name_or_argument,
            cases,
            current_statements,
            current_label,
        );

        let arguments = args
            .map(|args| args.iter().map(|&arg| self.expression_to_ir(arg)).collect())
            .unwrap_or_default();
        Some(IRNode::NewExpr {
            callee: Box::new(IRNode::ElementAccess {
                object: Box::new(IRNode::id(object_temp)),
                index: Box::new(IRNode::GeneratorSent),
            }),
            arguments,
            explicit_arguments: args.is_some(),
        })
    }

    fn new_callee_after_suspension(&self, callee: NodeIndex) -> Option<IRNode> {
        let callee_node = self.arena.get(callee)?;
        if callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(callee_node)?;
            if self.contains_await_recursive(access.expression)
                && !self.contains_await_recursive(access.name_or_argument)
            {
                let property = crate::transforms::emit_utils::identifier_text_or_empty(
                    self.arena,
                    access.name_or_argument,
                );
                return Some(IRNode::prop(
                    IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
                    property,
                ));
            }
        }
        if callee_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(callee_node)?;
            if self.contains_await_recursive(access.expression)
                && !self.contains_await_recursive(access.name_or_argument)
            {
                return Some(IRNode::elem(
                    IRNode::Parenthesized(Box::new(IRNode::GeneratorSent)),
                    self.expression_to_ir(access.name_or_argument),
                ));
            }
        }
        None
    }

    pub(super) fn lower_es5_new_spread(&mut self, idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }
        let new_expr = self.arena.get_call_expr(node)?;
        let args = new_expr.arguments.as_ref()?;
        if !self.args_contain_spread(&args.nodes) {
            return None;
        }
        let callee = self.expression_to_ir(new_expr.expression);
        let arg_array = self.spread_new_base_array(&args.nodes);
        Some(Self::new_from_bound_apply(
            IRNode::prop(callee.clone(), "bind"),
            callee,
            arg_array,
        ))
    }

    fn capture_new_bind_before_suspension(
        &mut self,
        callee: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) -> (String, IRNode) {
        let Some(callee_node) = self.arena.get(callee) else {
            return (String::new(), IRNode::Undefined);
        };
        if callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || callee_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let constructor_temp = self.generate_hoisted_temp();
            let bind_temp = self.generate_hoisted_temp();
            current_statements.push(IRNode::VarDecl {
                name: constructor_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::VarDecl {
                name: bind_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(bind_temp.clone()),
                IRNode::prop(
                    IRNode::Parenthesized(Box::new(IRNode::assign(
                        IRNode::id(constructor_temp.clone()),
                        self.expression_to_ir(callee),
                    ))),
                    "bind",
                ),
            ))));
            return (bind_temp, IRNode::id(constructor_temp));
        }

        let bind_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: bind_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(bind_temp.clone()),
            IRNode::prop(self.expression_to_ir(callee), "bind"),
        ))));
        (bind_temp, self.expression_to_ir(callee))
    }

    fn capture_new_bind_apply_before_suspension(
        &mut self,
        callee: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) -> (String, String, String) {
        let bind_temp = self.generate_hoisted_temp();
        let apply_temp = self.generate_hoisted_temp();
        let receiver_args_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: bind_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::VarDecl {
            name: apply_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::VarDecl {
            name: receiver_args_temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(apply_temp.clone()),
            IRNode::prop(
                IRNode::Parenthesized(Box::new(IRNode::assign(
                    IRNode::id(bind_temp.clone()),
                    IRNode::prop(self.expression_to_ir(callee), "bind"),
                ))),
                "apply",
            ),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(receiver_args_temp.clone()),
            IRNode::ArrayLiteral(vec![self.expression_to_ir(callee)]),
        ))));
        (bind_temp, apply_temp, receiver_args_temp)
    }

    fn lower_suspended_new_arguments(
        &mut self,
        args: &[NodeIndex],
        suspension_arg_index: usize,
        current_statements: &mut Vec<IRNode>,
    ) -> IRNode {
        if self.args_contain_spread(args) {
            return self.lower_suspended_spread_new_arguments(
                args,
                suspension_arg_index,
                current_statements,
            );
        }

        if suspension_arg_index == 0 {
            let mut lowered_args = Vec::with_capacity(args.len() + 1);
            lowered_args.push(IRNode::Undefined);
            lowered_args.extend(args.iter().map(|&arg| self.expression_to_ir(arg)));
            return IRNode::ArrayLiteral(lowered_args);
        }

        let prefix_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: prefix_temp.clone().into(),
            initializer: None,
        });
        let mut prefix_args = Vec::with_capacity(suspension_arg_index + 1);
        prefix_args.push(IRNode::Undefined);
        prefix_args.extend(
            args[..suspension_arg_index]
                .iter()
                .map(|&arg| self.expression_to_ir(arg)),
        );
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(prefix_temp.clone()),
            IRNode::ArrayLiteral(prefix_args),
        ))));

        let suffix_args = args[suspension_arg_index..]
            .iter()
            .map(|&arg| self.expression_to_ir(arg))
            .collect();
        IRNode::CallExpr {
            callee: Box::new(IRNode::prop(IRNode::id(prefix_temp), "concat")),
            arguments: vec![IRNode::ArrayLiteral(suffix_args)],
        }
    }

    fn lower_suspended_spread_new_arguments(
        &mut self,
        args: &[NodeIndex],
        suspension_arg_index: usize,
        current_statements: &mut Vec<IRNode>,
    ) -> IRNode {
        self.helpers_needed.mark_spread_array();
        let prefix_base = self.spread_new_base_array(&args[..suspension_arg_index]);
        let suspension_is_spread = self.is_spread_arg(args[suspension_arg_index]);
        let has_prefix_spread = self.args_contain_spread(&args[..suspension_arg_index]);
        let has_suffix_spread = self.args_contain_spread(&args[suspension_arg_index + 1..]);

        let mut current = if suspension_is_spread || has_prefix_spread {
            let prefix_temp = self.generate_hoisted_temp();
            current_statements.push(IRNode::VarDecl {
                name: prefix_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(prefix_temp.clone()),
                IRNode::ArrayLiteral(vec![prefix_base]),
            ))));
            let resumed = if suspension_is_spread {
                IRNode::Parenthesized(Box::new(IRNode::GeneratorSent))
            } else {
                IRNode::ArrayLiteral(vec![IRNode::GeneratorSent])
            };
            self.spread_array_apply(IRNode::CallExpr {
                callee: Box::new(IRNode::prop(IRNode::id(prefix_temp), "concat")),
                arguments: vec![IRNode::ArrayLiteral(vec![
                    resumed,
                    IRNode::BooleanLiteral(false),
                ])],
            })
        } else if has_suffix_spread {
            let prefix_temp = self.generate_hoisted_temp();
            current_statements.push(IRNode::VarDecl {
                name: prefix_temp.clone().into(),
                initializer: None,
            });
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(prefix_temp.clone()),
                prefix_base,
            ))));
            IRNode::CallExpr {
                callee: Box::new(IRNode::prop(IRNode::id(prefix_temp), "concat")),
                arguments: vec![IRNode::ArrayLiteral(vec![IRNode::GeneratorSent])],
            }
        } else {
            IRNode::ArrayLiteral(vec![IRNode::Undefined, IRNode::GeneratorSent])
        };

        let mut segment = Vec::new();
        for &arg in &args[suspension_arg_index + 1..] {
            if self.is_spread_arg(arg) {
                if !segment.is_empty() {
                    current = self.spread_array_apply(IRNode::ArrayLiteral(vec![
                        current,
                        IRNode::ArrayLiteral(std::mem::take(&mut segment)),
                        IRNode::BooleanLiteral(false),
                    ]));
                }
                current = self.spread_array_apply(IRNode::ArrayLiteral(vec![
                    current,
                    self.spread_arg_expression_to_ir(arg),
                    IRNode::BooleanLiteral(false),
                ]));
            } else {
                segment.push(self.expression_to_ir(arg));
            }
        }
        if !segment.is_empty() {
            current = self.spread_array_apply(IRNode::ArrayLiteral(vec![
                current,
                IRNode::ArrayLiteral(segment),
                IRNode::BooleanLiteral(false),
            ]));
        }
        current
    }

    fn spread_new_base_array(&mut self, args: &[NodeIndex]) -> IRNode {
        if !self.args_contain_spread(args) {
            let mut lowered = Vec::with_capacity(args.len() + 1);
            lowered.push(IRNode::Undefined);
            lowered.extend(args.iter().map(|&arg| self.expression_to_ir(arg)));
            return IRNode::ArrayLiteral(lowered);
        }

        let mut current = IRNode::ArrayLiteral(vec![IRNode::Undefined]);
        let mut segment = Vec::new();
        for &arg in args {
            if self.is_spread_arg(arg) {
                if !segment.is_empty() {
                    current = self.spread_array_call(
                        current,
                        IRNode::ArrayLiteral(std::mem::take(&mut segment)),
                        false,
                    );
                }
                current =
                    self.spread_array_call(current, self.spread_arg_expression_to_ir(arg), false);
            } else {
                segment.push(self.expression_to_ir(arg));
            }
        }
        if !segment.is_empty() {
            current = self.spread_array_call(current, IRNode::ArrayLiteral(segment), false);
        }
        current
    }

    fn new_from_bound_apply(bind: IRNode, receiver: IRNode, arg_array: IRNode) -> IRNode {
        IRNode::NewExpr {
            callee: Box::new(IRNode::Parenthesized(Box::new(IRNode::CallExpr {
                callee: Box::new(IRNode::prop(bind, "apply")),
                arguments: vec![receiver, arg_array],
            }))),
            arguments: Vec::new(),
            explicit_arguments: true,
        }
    }

    fn new_from_apply_apply(
        apply_method: IRNode,
        bind_function: IRNode,
        receiver_args: IRNode,
        arg_array: IRNode,
    ) -> IRNode {
        IRNode::NewExpr {
            callee: Box::new(IRNode::Parenthesized(Box::new(IRNode::CallExpr {
                callee: Box::new(IRNode::prop(apply_method, "apply")),
                arguments: vec![
                    bind_function,
                    IRNode::CallExpr {
                        callee: Box::new(IRNode::prop(receiver_args, "concat")),
                        arguments: vec![IRNode::ArrayLiteral(vec![arg_array])],
                    },
                ],
            }))),
            arguments: Vec::new(),
            explicit_arguments: true,
        }
    }

    fn args_contain_spread(&self, args: &[NodeIndex]) -> bool {
        args.iter().any(|&arg| self.is_spread_arg(arg))
    }

    pub(super) fn lower_es5_call_spread(&mut self, idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(node)?;
        let args = call.arguments.as_ref()?;
        if !self.args_contain_spread(&args.nodes) {
            return None;
        }

        self.helpers_needed.mark_spread_array();
        let callee_node = self.arena.get(call.expression)?;
        let (callee, this_arg) = if callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            let access = self.arena.get_access_expr(callee_node)?;
            let object = self.expression_to_ir(access.expression);
            let property = crate::transforms::emit_utils::identifier_text_or_empty(
                self.arena,
                access.name_or_argument,
            );
            (IRNode::prop(object.clone(), property), object)
        } else if callee_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(callee_node)?;
            let object = self.expression_to_ir(access.expression);
            (
                IRNode::elem(
                    object.clone(),
                    self.expression_to_ir(access.name_or_argument),
                ),
                object,
            )
        } else {
            (self.expression_to_ir(call.expression), IRNode::Undefined)
        };

        Some(IRNode::CallExpr {
            callee: Box::new(IRNode::prop(callee, "apply")),
            arguments: vec![this_arg, self.spread_call_base_array(&args.nodes)],
        })
    }

    fn is_spread_arg(&self, arg: NodeIndex) -> bool {
        self.arena
            .get(arg)
            .is_some_and(|node| node.kind == syntax_kind_ext::SPREAD_ELEMENT)
    }

    fn spread_arg_expression_to_ir(&self, arg: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(arg) else {
            return IRNode::Undefined;
        };
        if let Some(spread) = self.arena.get_spread(node) {
            return self.expression_to_ir(spread.expression);
        }
        if let Some(unary) = self.arena.get_unary_expr_ex(node) {
            return self.expression_to_ir(unary.expression);
        }
        self.expression_to_ir(arg)
    }

    fn spread_call_base_array(&mut self, args: &[NodeIndex]) -> IRNode {
        if !self.args_contain_spread(args) {
            return IRNode::ArrayLiteral(
                args.iter().map(|&arg| self.expression_to_ir(arg)).collect(),
            );
        }

        let mut current = IRNode::ArrayLiteral(Vec::new());
        let mut segment = Vec::new();
        for &arg in args {
            if self.is_spread_arg(arg) {
                if !segment.is_empty() {
                    current = self.spread_array_call(
                        current,
                        IRNode::ArrayLiteral(std::mem::take(&mut segment)),
                        false,
                    );
                }
                current =
                    self.spread_array_call(current, self.spread_arg_expression_to_ir(arg), false);
            } else {
                segment.push(self.expression_to_ir(arg));
            }
        }
        if !segment.is_empty() {
            current = self.spread_array_call(current, IRNode::ArrayLiteral(segment), false);
        }
        current
    }

    fn spread_array_call(&mut self, to: IRNode, from: IRNode, pack: bool) -> IRNode {
        self.helpers_needed.mark_spread_array();
        IRNode::CallExpr {
            callee: Box::new(IRNode::RuntimeHelper("__spreadArray".into())),
            arguments: vec![to, from, IRNode::BooleanLiteral(pack)],
        }
    }

    fn spread_array_apply(&mut self, arguments: IRNode) -> IRNode {
        self.helpers_needed.mark_spread_array();
        IRNode::CallExpr {
            callee: Box::new(IRNode::prop(
                IRNode::RuntimeHelper("__spreadArray".into()),
                "apply",
            )),
            arguments: vec![IRNode::Undefined, arguments],
        }
    }
}
