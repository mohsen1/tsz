//! Async ES5 call-expression state-machine lowering.

use super::AsyncES5Transformer;
use crate::transforms::ir::{IRGeneratorCase, IRNode};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> AsyncES5Transformer<'a> {
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
                    );
                }
                current = self.spread_array_call(current, self.spread_arg_expression_to_ir(arg));
            } else {
                segment.push(self.expression_to_ir(arg));
            }
        }
        if !segment.is_empty() {
            current = self.spread_array_call(current, IRNode::ArrayLiteral(segment));
        }
        current
    }

    fn spread_array_call(&mut self, to: IRNode, from: IRNode) -> IRNode {
        self.helpers_needed.mark_spread_array();
        IRNode::CallExpr {
            callee: Box::new(IRNode::RuntimeHelper("__spreadArray".into())),
            arguments: vec![to, from, IRNode::BooleanLiteral(false)],
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
