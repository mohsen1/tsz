//! Object-literal helpers for the async ES5 IR converter.

use crate::transforms::async_es5_ir::AsyncES5Transformer;
use crate::transforms::ir::{
    IRGeneratorCase, IRNode, IRParam, IRProperty, IRPropertyKey, IRPropertyKind,
};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::MethodDeclData;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl AsyncES5Transformer<'_> {
    /// Convert object literal properties to `IRProperty`
    pub(super) fn convert_object_properties(&self, nodes: &[NodeIndex]) -> Vec<IRProperty> {
        let mut props = Vec::new();
        for &prop_idx in nodes {
            let Some(prop_node) = self.arena.get(prop_idx) else {
                continue;
            };

            match prop_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    if let Some(pa) = self.arena.get_property_assignment(prop_node) {
                        let key = self.convert_property_key(pa.name);
                        let value = self.expression_to_ir(pa.initializer);
                        props.push(IRProperty {
                            key,
                            value,
                            kind: IRPropertyKind::Init,
                        });
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    if let Some(sp) = self.arena.get_shorthand_property(prop_node) {
                        let name = crate::transforms::emit_utils::identifier_text_or_empty(
                            self.arena, sp.name,
                        );
                        props.push(IRProperty {
                            key: IRPropertyKey::Identifier(name.clone().into()),
                            value: IRNode::Identifier(name.into()),
                            kind: IRPropertyKind::Init,
                        });
                    }
                }
                k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                    if let Some(spread) = self.arena.get_spread(prop_node) {
                        // For spread in objects, use SpreadElement
                        props.push(IRProperty {
                            key: IRPropertyKey::Identifier("...".to_string().into()),
                            value: IRNode::SpreadElement(Box::new(
                                self.expression_to_ir(spread.expression),
                            )),
                            kind: IRPropertyKind::Init,
                        });
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.arena.get_method_decl(prop_node) {
                        props.push(IRProperty {
                            key: self.convert_property_key(method.name),
                            value: self.method_function_ir(method),
                            kind: IRPropertyKind::Init,
                        });
                    }
                }
                // Skip other property types (getters/setters would need special handling)
                _ => {}
            }
        }
        props
    }

    pub(super) fn lower_object_literal_es5_with_computed_properties(
        &self,
        idx: NodeIndex,
    ) -> Option<(String, IRNode)> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let literal = self.arena.get_literal_expr(node)?;
        let first_computed_idx = literal
            .elements
            .nodes
            .iter()
            .position(|&elem_idx| self.object_element_needs_computed_lowering(elem_idx))?;

        let temp = self.generate_hoisted_temp();
        let mut parts = Vec::new();
        let initial_obj = IRNode::object(
            self.convert_object_properties(&literal.elements.nodes[..first_computed_idx]),
        );
        parts.push(IRNode::assign(IRNode::id(temp.clone()), initial_obj));

        for &elem_idx in literal.elements.nodes.iter().skip(first_computed_idx) {
            if let Some(assignment) = self.lower_object_property_es5(elem_idx, &temp) {
                parts.push(assignment);
            }
        }
        parts.push(IRNode::id(temp.clone()));

        Some((temp, IRNode::CommaExpr(parts)))
    }

    pub(super) fn lower_object_literal_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.lower_suspended_object_literal(idx, cases, current_statements, current_label)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let binary = self.arena.get_binary_expr(node)?;
                if self.get_operator_text(binary.operator_token) != "=" {
                    return None;
                }
                let right = self.lower_object_literal_before_suspension(
                    binary.right,
                    cases,
                    current_statements,
                    current_label,
                )?;
                Some(IRNode::assign(self.expression_to_ir(binary.left), right))
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.arena.get_parenthesized(node)?;
                self.lower_object_literal_before_suspension(
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
                self.lower_object_literal_before_suspension(
                    assertion.expression,
                    cases,
                    current_statements,
                    current_label,
                )
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let unary = self.arena.get_unary_expr_ex(node)?;
                self.lower_object_literal_before_suspension(
                    unary.expression,
                    cases,
                    current_statements,
                    current_label,
                )
            }
            _ => None,
        }
    }

    fn lower_suspended_object_literal(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        let literal = self.arena.get_literal_expr(node)?;
        let elements = &literal.elements.nodes;
        let first_suspension_index = elements
            .iter()
            .position(|&element| self.object_element_contains_suspension(element))?;
        let first_suspension = elements[first_suspension_index];
        let first_property = self.suspending_property_assignment(first_suspension)?;
        let first_property_name = first_property.name;
        let first_property_initializer = first_property.initializer;
        let first_key_suspends = self.computed_property_name_contains_await(first_property_name);
        let first_value_suspends = self.body_contains_await(first_property_initializer);
        let first_key_needs_temp = first_value_suspends
            && (first_key_suspends || self.object_property_name_is_computed(first_property_name));
        let first_key_temp = first_key_needs_temp.then(|| self.generate_hoisted_temp());
        let split_object_var_group = first_key_temp.is_some()
            || first_key_suspends
            || elements[..first_suspension_index]
                .iter()
                .any(|&element| self.object_element_needs_computed_lowering(element));

        if let Some(temp) = &first_key_temp {
            current_statements.push(IRNode::VarDecl {
                name: temp.clone().into(),
                initializer: None,
            });
        }
        if split_object_var_group {
            current_statements.push(IRNode::HoistedVarGroupBreak);
        }

        let object_temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: object_temp.clone().into(),
            initializer: None,
        });
        self.emit_object_literal_prefix_before_suspension(
            &elements[..first_suspension_index],
            &object_temp,
            Some((node.pos, node.end)),
            split_object_var_group,
            current_statements,
        );

        if let Some(key_temp) = &first_key_temp {
            if first_key_suspends {
                let key_expression = self.computed_property_expression(first_property_name)?;
                self.emit_nested_suspension(
                    key_expression,
                    cases,
                    current_statements,
                    current_label,
                );
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::id(key_temp.clone()),
                    IRNode::GeneratorSent,
                ))));
            } else {
                let key_expression = self.computed_property_expression(first_property_name)?;
                current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                    IRNode::id(key_temp.clone()),
                    self.expression_to_ir(key_expression),
                ))));
            }
        }

        let first_value_is_resume = if first_value_suspends {
            self.emit_nested_suspension(
                first_property_initializer,
                cases,
                current_statements,
                current_label,
            );
            true
        } else if first_key_suspends {
            let key_expression = self.computed_property_expression(first_property_name)?;
            self.emit_nested_suspension(key_expression, cases, current_statements, current_label);
            false
        } else {
            return None;
        };

        let suffix = &elements[first_suspension_index + 1..];
        let suffix_needs_computed_lowering = suffix
            .iter()
            .any(|&element| self.object_element_needs_computed_lowering(element));
        let result_temp = suffix_needs_computed_lowering.then(|| self.generate_hoisted_temp());
        if let Some(temp) = &result_temp {
            current_statements.push(IRNode::HoistedVarGroupBreak);
            current_statements.push(IRNode::VarDecl {
                name: temp.clone().into(),
                initializer: None,
            });
        }

        let key = if let Some(key_temp) = &first_key_temp {
            IRNode::elem(
                IRNode::id(object_temp.clone()),
                IRNode::id(key_temp.clone()),
            )
        } else if first_key_suspends {
            IRNode::elem(IRNode::id(object_temp.clone()), IRNode::GeneratorSent)
        } else {
            self.convert_property_key_to_element_access(first_property_name, &object_temp)?
        };
        let value = if first_value_is_resume {
            IRNode::GeneratorSent
        } else {
            self.expression_to_ir(first_property_initializer)
        };
        let first_assignment = IRNode::assign(key, value);

        if let Some(result_temp) = result_temp {
            let mut parts = vec![IRNode::assign(
                IRNode::id(result_temp.clone()),
                IRNode::CommaExprMultilineFlat(vec![first_assignment, IRNode::id(object_temp)]),
            )];
            for &element in suffix {
                if let Some(assignment) = self.lower_object_property_es5(element, &result_temp) {
                    parts.push(assignment);
                }
            }
            parts.push(IRNode::id(result_temp));
            return Some(IRNode::CommaExprMultiline(parts));
        }

        let mut parts = vec![first_assignment];
        for &element in suffix {
            if let Some(assignment) = self.lower_object_property_es5(element, &object_temp) {
                parts.push(assignment);
            }
        }
        parts.push(IRNode::id(object_temp));

        Some(IRNode::CommaExprMultiline(parts))
    }

    fn emit_object_literal_prefix_before_suspension(
        &self,
        elements: &[NodeIndex],
        temp: &str,
        source_range: Option<(u32, u32)>,
        extra_indent: bool,
        current_statements: &mut Vec<IRNode>,
    ) {
        let first_computed_idx = elements
            .iter()
            .position(|&element| self.object_element_needs_computed_lowering(element))
            .unwrap_or(elements.len());
        let initial_assignment = IRNode::assign(
            IRNode::id(temp.to_string()),
            IRNode::ObjectLiteral {
                properties: self.convert_object_properties(&elements[..first_computed_idx]),
                source_range,
                extra_indent: u8::from(extra_indent),
            },
        );

        if first_computed_idx < elements.len() {
            let mut parts = vec![initial_assignment];
            for &element in &elements[first_computed_idx..] {
                if let Some(assignment) = self.lower_object_property_es5(element, temp) {
                    parts.push(assignment);
                }
            }
            current_statements.push(IRNode::ExpressionStatement(Box::new(
                IRNode::CommaExprMultiline(parts),
            )));
            return;
        }

        current_statements.push(IRNode::ExpressionStatement(Box::new(initial_assignment)));
    }

    fn object_element_needs_computed_lowering(&self, elem_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(elem_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
            return self
                .arena
                .get_property_assignment(node)
                .is_some_and(|prop| self.object_property_name_is_computed(prop.name));
        }
        if node.kind == syntax_kind_ext::METHOD_DECLARATION {
            return self
                .arena
                .get_method_decl(node)
                .is_some_and(|method| self.object_property_name_is_computed(method.name));
        }
        false
    }

    fn object_element_contains_suspension(&self, elem_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(elem_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
            return self
                .arena
                .get_property_assignment(node)
                .is_some_and(|prop| {
                    self.computed_property_name_contains_await(prop.name)
                        || self.body_contains_await(prop.initializer)
                });
        }
        false
    }

    fn suspending_property_assignment(
        &self,
        elem_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::PropertyAssignmentData> {
        let node = self.arena.get(elem_idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
            return None;
        }
        self.arena.get_property_assignment(node)
    }

    fn object_property_name_is_computed(&self, name_idx: NodeIndex) -> bool {
        self.arena
            .get(name_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
    }

    fn computed_property_name_contains_await(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        self.arena
            .get_computed_property(name_node)
            .is_some_and(|computed| self.body_contains_await(computed.expression))
    }

    fn computed_property_expression(&self, name_idx: NodeIndex) -> Option<NodeIndex> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        self.arena
            .get_computed_property(name_node)
            .map(|computed| computed.expression)
    }

    fn lower_object_property_es5(&self, elem_idx: NodeIndex, temp: &str) -> Option<IRNode> {
        let node = self.arena.get(elem_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(node)?;
                let key = self.convert_property_key_to_element_access(prop.name, temp)?;
                let value = self.expression_to_ir(prop.initializer);
                Some(IRNode::assign(key, value))
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let shorthand = self.arena.get_shorthand_property(node)?;
                let name = crate::transforms::emit_utils::identifier_text_or_empty(
                    self.arena,
                    shorthand.name,
                );
                Some(IRNode::assign(
                    IRNode::prop(IRNode::id(temp.to_string()), name.clone()),
                    IRNode::id(name),
                ))
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                let key = self.convert_property_key_to_element_access(method.name, temp)?;
                Some(IRNode::assign(key, self.method_function_ir(method)))
            }
            _ => None,
        }
    }

    fn convert_property_key_to_element_access(
        &self,
        name_idx: NodeIndex,
        temp: &str,
    ) -> Option<IRNode> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(name_node)?;
            let expr = self.expression_to_ir(computed.expression);
            Some(IRNode::elem(IRNode::id(temp.to_string()), expr))
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            let ident =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, name_idx);
            Some(IRNode::prop(IRNode::id(temp.to_string()), ident))
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            let lit = self.arena.get_literal(name_node)?;
            Some(IRNode::elem(
                IRNode::id(temp.to_string()),
                IRNode::string(lit.text.clone()),
            ))
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16 {
            let lit = self.arena.get_literal(name_node)?;
            Some(IRNode::elem(
                IRNode::id(temp.to_string()),
                IRNode::number(lit.text.clone()),
            ))
        } else {
            None
        }
    }

    /// Convert a property name node to `IRPropertyKey`
    fn convert_property_key(&self, idx: NodeIndex) -> IRPropertyKey {
        let Some(node) = self.arena.get(idx) else {
            return IRPropertyKey::Identifier(String::new().into());
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => IRPropertyKey::Identifier(
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, idx).into(),
            ),
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    IRPropertyKey::StringLiteral(lit.text.clone().into())
                } else {
                    IRPropertyKey::StringLiteral(String::new().into())
                }
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    IRPropertyKey::NumericLiteral(lit.text.clone().into())
                } else {
                    IRPropertyKey::NumericLiteral("0".to_string().into())
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                // Computed property: [expr]
                if let Some(computed) = self.arena.get_computed_property(node) {
                    IRPropertyKey::Computed(Box::new(self.expression_to_ir(computed.expression)))
                } else {
                    IRPropertyKey::Identifier(String::new().into())
                }
            }
            _ => IRPropertyKey::Identifier(String::new().into()),
        }
    }

    fn method_function_ir(&self, method: &MethodDeclData) -> IRNode {
        let parameters = method
            .parameters
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_parameter(param_node)?;
                let name =
                    crate::transforms::emit_utils::identifier_text_or_empty(self.arena, param.name);
                Some(if param.dot_dot_dot_token {
                    IRParam::rest(name)
                } else {
                    IRParam::new(name)
                })
            })
            .collect::<Vec<_>>();

        let is_async = self
            .arena
            .has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword)
            && !method.asterisk_token;
        if is_async {
            let mut nested = AsyncES5Transformer::new(self.arena);
            if let Some(source_text) = self.source_text {
                nested.set_source_text(source_text);
            }
            nested.set_module_kind(self.module_kind);
            let has_await = nested.body_contains_await(method.body);
            let mut generator_body = nested.transform_generator_body(method.body, has_await);
            let hoisted_var_groups =
                AsyncES5Transformer::extract_and_remove_var_decl_groups(&mut generator_body);
            return IRNode::FunctionExpr {
                name: None,
                parameters,
                body: vec![IRNode::AwaiterCall {
                    this_arg: Box::new(IRNode::this()),
                    needs_lexical_this_capture: generator_body.contains_captured_this_reference(),
                    generator_body: Box::new(generator_body),
                    hoisted_var_groups,
                    promise_constructor: None,
                    multiline_callback: false,
                }],
                is_expression_body: false,
                body_source_range: None,
            };
        }

        let body = self
            .arena
            .get(method.body)
            .and_then(|body_node| self.arena.get_block(body_node))
            .map(|block| {
                block
                    .statements
                    .nodes
                    .iter()
                    .map(|&stmt_idx| self.statement_to_ir(stmt_idx))
                    .collect()
            })
            .unwrap_or_default();
        IRNode::FunctionExpr {
            name: None,
            parameters,
            body,
            is_expression_body: false,
            body_source_range: None,
        }
    }
}
