//! AST-to-IR conversion utilities for the async ES5 transformer.
//!
//! Contains generic expression/statement/property conversion from AST nodes
//! to `IRNode`, `IRProperty`, and `IRPropertyKey`. These are pure read-only
//! traversals of the `NodeArena` with no async-transform-specific state.

use crate::transforms::ir::{IRNode, IRParam, IRProperty, IRPropertyKey, IRPropertyKind};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::async_es5_ir::AsyncES5Transformer;

impl<'a> AsyncES5Transformer<'a> {
    /// Collect parameter names from a parameter list
    pub fn collect_parameters(&self, params: &tsz_parser::parser::NodeList) -> Vec<String> {
        let mut result = Vec::new();
        for &param_idx in &params.nodes {
            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                result.push(crate::transforms::emit_utils::identifier_text_or_empty(
                    self.arena, param.name,
                ));
            }
        }
        result
    }

    /// Convert an AST expression to IR
    pub fn expression_to_ir(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::Undefined;
        };

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    IRNode::NumericLiteral(lit.text.clone().into())
                } else {
                    IRNode::NumericLiteral("0".to_string().into())
                }
            }

            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    IRNode::StringLiteral(lit.text.clone().into())
                } else {
                    IRNode::StringLiteral("".to_string().into())
                }
            }

            k if k == SyntaxKind::TrueKeyword as u16 => IRNode::BooleanLiteral(true),
            k if k == SyntaxKind::FalseKeyword as u16 => IRNode::BooleanLiteral(false),
            k if k == SyntaxKind::NullKeyword as u16 => IRNode::NullLiteral,
            k if k == SyntaxKind::ThisKeyword as u16 => IRNode::This {
                captured: self.captures_this_references(),
            },

            k if k == SyntaxKind::Identifier as u16 => {
                let text = crate::transforms::emit_utils::identifier_text_or_empty(self.arena, idx);
                // When capturing arguments, replace `arguments` with `arguments_1`
                if self.state.captures_arguments && text == "arguments" {
                    IRNode::Identifier("arguments_1".to_string().into())
                } else {
                    IRNode::Identifier(text.into())
                }
            }

            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    let callee = self.expression_to_ir(call.expression);
                    let mut args = Vec::new();
                    if let Some(arg_list) = &call.arguments {
                        for &arg_idx in &arg_list.nodes {
                            args.push(self.expression_to_ir(arg_idx));
                        }
                    }
                    IRNode::CallExpr {
                        callee: Box::new(callee),
                        arguments: args,
                    }
                } else {
                    IRNode::Undefined
                }
            }

            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    let obj = self.expression_to_ir(access.expression);
                    let prop = crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena,
                        access.name_or_argument,
                    );
                    IRNode::PropertyAccess {
                        object: Box::new(obj),
                        property: prop.into(),
                    }
                } else {
                    IRNode::Undefined
                }
            }

            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    let left = self.expression_to_ir(bin.left);
                    let right = self.expression_to_ir(bin.right);
                    let op = self.get_operator_text(bin.operator_token);
                    IRNode::BinaryExpr {
                        left: Box::new(left),
                        operator: op.into(),
                        right: Box::new(right),
                    }
                } else {
                    IRNode::Undefined
                }
            }

            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(arr) = self.arena.get_literal_expr(node) {
                    let elements: Vec<IRNode> = arr
                        .elements
                        .nodes
                        .iter()
                        .map(|&idx| self.expression_to_ir(idx))
                        .collect();
                    IRNode::ArrayLiteral(elements)
                } else {
                    IRNode::ArrayLiteral(vec![])
                }
            }

            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    IRNode::Parenthesized(Box::new(self.expression_to_ir(paren.expression)))
                } else {
                    IRNode::Undefined
                }
            }

            k if k == syntax_kind_ext::AWAIT_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION =>
            {
                IRNode::GeneratorSent
            }

            // NEW_EXPRESSION: `new Foo(args)`
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    let callee = self.expression_to_ir(call.expression);
                    let mut args = Vec::new();
                    if let Some(arg_list) = &call.arguments {
                        for &arg_idx in &arg_list.nodes {
                            args.push(self.expression_to_ir(arg_idx));
                        }
                    }
                    IRNode::NewExpr {
                        callee: Box::new(callee),
                        arguments: args,
                        explicit_arguments: call.arguments.is_some(),
                    }
                } else {
                    IRNode::Undefined
                }
            }

            // SPREAD_ELEMENT: `...expr`
            k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                if let Some(spread) = self.arena.get_spread(node) {
                    IRNode::SpreadElement(Box::new(self.expression_to_ir(spread.expression)))
                } else if let Some(unary_ex) = self.arena.get_unary_expr_ex(node) {
                    // Fallback: Some spread elements use UnaryExprDataEx
                    IRNode::SpreadElement(Box::new(self.expression_to_ir(unary_ex.expression)))
                } else {
                    IRNode::Undefined
                }
            }

            // CONDITIONAL_EXPRESSION: `a ? b : c`
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.arena.get_conditional_expr(node) {
                    IRNode::ConditionalExpr {
                        condition: Box::new(self.expression_to_ir(cond.condition)),
                        when_true: Box::new(self.expression_to_ir(cond.when_true)),
                        when_false: Box::new(self.expression_to_ir(cond.when_false)),
                    }
                } else {
                    IRNode::Undefined
                }
            }

            // PREFIX_UNARY_EXPRESSION: `!x`, `-x`, `++x`, `--x`
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    let op = self.get_unary_operator_text(unary.operator);
                    IRNode::PrefixUnaryExpr {
                        operator: op.into(),
                        operand: Box::new(self.expression_to_ir(unary.operand)),
                    }
                } else {
                    IRNode::Undefined
                }
            }

            // POSTFIX_UNARY_EXPRESSION: `x++`, `x--`
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    let op = self.get_unary_operator_text(unary.operator);
                    IRNode::PostfixUnaryExpr {
                        operand: Box::new(self.expression_to_ir(unary.operand)),
                        operator: op.into(),
                    }
                } else {
                    IRNode::Undefined
                }
            }

            // ELEMENT_ACCESS_EXPRESSION: `object[index]`
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    let obj = self.expression_to_ir(access.expression);
                    let index = self.expression_to_ir(access.name_or_argument);
                    IRNode::ElementAccess {
                        object: Box::new(obj),
                        index: Box::new(index),
                    }
                } else {
                    IRNode::Undefined
                }
            }

            // OBJECT_LITERAL_EXPRESSION: `{ key: value, ... }`
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(obj) = self.arena.get_literal_expr(node) {
                    let props = self.convert_object_properties(&obj.elements.nodes);
                    IRNode::object(props)
                } else {
                    IRNode::empty_object()
                }
            }

            // TEMPLATE_EXPRESSION: `hello ${name}!`
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => self.convert_template_expression(idx),

            // NoSubstitutionTemplateLiteral: `hello world`
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    // Return the text as a string literal with quotes
                    IRNode::StringLiteral(lit.text.clone().into())
                } else {
                    IRNode::StringLiteral("".to_string().into())
                }
            }

            // SuperKeyword: `super`
            k if k == SyntaxKind::SuperKeyword as u16 => IRNode::Super,

            // FUNCTION_EXPRESSION: `function foo() { ... }` or `async function() { ... }`
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => self.convert_function_expression(idx),

            // ARROW_FUNCTION: `() => { ... }` or `async () => expr`
            k if k == syntax_kind_ext::ARROW_FUNCTION => self.convert_arrow_function(idx),

            _ => IRNode::ASTRef(idx),
        }
    }

    /// Convert object literal properties to `IRProperty`
    fn convert_object_properties(&self, nodes: &[NodeIndex]) -> Vec<IRProperty> {
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

    pub(super) fn lower_object_literal_es5_after_computed_suspension(
        &self,
        idx: NodeIndex,
    ) -> Option<(String, IRNode, IRNode)> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let literal = self.arena.get_literal_expr(node)?;
        let first_suspending_computed_idx =
            literal.elements.nodes.iter().position(|&elem_idx| {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    return false;
                };
                if elem_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                    return false;
                }
                self.arena
                    .get_property_assignment(elem_node)
                    .is_some_and(|prop| self.computed_property_name_contains_await(prop.name))
            })?;

        let temp = self.generate_hoisted_temp();
        let initial_obj =
            IRNode::object(self.convert_object_properties(
                &literal.elements.nodes[..first_suspending_computed_idx],
            ));

        let mut parts = Vec::new();
        for &elem_idx in literal
            .elements
            .nodes
            .iter()
            .skip(first_suspending_computed_idx)
        {
            if let Some(assignment) = self.lower_object_property_es5(elem_idx, &temp) {
                parts.push(assignment);
            }
        }
        parts.push(IRNode::id(temp.clone()));

        Some((temp, initial_obj, IRNode::CommaExpr(parts)))
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
        false
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

    /// Convert a template expression to IR (concatenation of strings)
    fn convert_template_expression(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::StringLiteral("".to_string().into());
        };

        let Some(template) = self.arena.get_template_expr(node) else {
            return IRNode::StringLiteral("".to_string().into());
        };

        // Get head (the initial string before first ${...})
        let mut parts: Vec<IRNode> = Vec::new();
        if let Some(head_node) = self.arena.get(template.head)
            && let Some(lit) = self.arena.get_literal(head_node)
            && !lit.text.is_empty()
        {
            parts.push(IRNode::StringLiteral(lit.text.clone().into()));
        }

        // Process template spans (expression + literal pairs)
        for &span_idx in &template.template_spans.nodes {
            let Some(span_node) = self.arena.get(span_idx) else {
                continue;
            };
            if let Some(span) = self.arena.get_template_span(span_node) {
                // Add the expression
                parts.push(self.expression_to_ir(span.expression));

                // Add the literal part after the expression
                if let Some(lit_node) = self.arena.get(span.literal)
                    && let Some(lit) = self.arena.get_literal(lit_node)
                    && !lit.text.is_empty()
                {
                    parts.push(IRNode::StringLiteral(lit.text.clone().into()));
                }
            }
        }

        // If there's only one part, return it directly
        if parts.len() == 1 {
            return parts.remove(0);
        }

        // Otherwise, build a concatenation chain: part1 + part2 + part3 + ...
        if parts.is_empty() {
            return IRNode::StringLiteral("".to_string().into());
        }

        let mut result = parts.remove(0);
        for part in parts {
            result = IRNode::BinaryExpr {
                left: Box::new(result),
                operator: "+".to_string().into(),
                right: Box::new(part),
            };
        }
        result
    }

    /// Convert a function expression to IR
    fn convert_function_expression(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::Undefined;
        };

        let Some(func) = self.arena.get_function(node) else {
            return IRNode::Undefined;
        };

        // Get the function name if any
        let name = if func.name.is_none() {
            None
        } else {
            Some(crate::transforms::emit_utils::identifier_text_or_empty(
                self.arena, func.name,
            ))
        };

        // Convert parameters
        let params = self.convert_parameters(&func.parameters.nodes);

        // Convert body to IR statements
        let body = self.convert_function_body(func.body);

        IRNode::FunctionExpr {
            name: name.map(Into::into),
            parameters: params,
            body,
            is_expression_body: false,
            body_source_range: None,
        }
    }

    /// Convert an arrow function to IR
    fn convert_arrow_function(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::Undefined;
        };

        // Arrow functions also use FunctionData
        let Some(func) = self.arena.get_function(node) else {
            return IRNode::Undefined;
        };

        if func.is_async
            && crate::transforms::emit_utils::block_is_empty(self.arena, func.body)
            && let Some(param_name) = crate::transforms::emit_utils::first_await_default_param_name(
                self.arena,
                &func.parameters.nodes,
            )
            && func.parameters.nodes.iter().copied().any(|param_idx| {
                crate::transforms::emit_utils::param_initializer_has_top_level_await(
                    self.arena, param_idx,
                )
            })
        {
            let generated = format!(
                "function () {{\n\
            var args_1 = [];\n\
            for (var _i = 0; _i < arguments.length; _i++) {{\n\
                args_1[_i] = arguments[_i];\n\
            }}\n\
            return __awaiter(void 0, __spreadArray([], args_1, true), void 0, function ({param_name}) {{\n\
                if ({param_name} === void 0) {{ {param_name} = _a.sent(); }}\n\
                return __generator(this, function (_a) {{\n\
                    switch (_a.label) {{\n\
                        case 0: return [4 /*yield*/, ];\n\
                        case 1: return [2 /*return*/];\n\
                    }}\n\
                }});\n\
            }});\n\
        }}"
            );
            return IRNode::Raw(generated.into());
        }

        // Convert parameters
        let params = self.convert_parameters(&func.parameters.nodes);

        // Check if body is expression or block
        let Some(body_node) = self.arena.get(func.body) else {
            return IRNode::FunctionExpr {
                name: None,
                parameters: params,
                body: vec![],
                is_expression_body: false,
                body_source_range: None,
            };
        };

        let previous_this_capture = self.captures_this_references();
        if self.captures_lexical_this() {
            self.set_capture_this_references(true);
        }

        let result = if body_node.kind == syntax_kind_ext::BLOCK {
            // Block body
            let body = self.convert_function_body(func.body);
            IRNode::FunctionExpr {
                name: None,
                parameters: params,
                body,
                is_expression_body: false,
                body_source_range: None,
            }
        } else {
            // Expression body - wrap in return
            let expr = self.expression_to_ir(func.body);
            IRNode::FunctionExpr {
                name: None,
                parameters: params,
                body: vec![IRNode::ReturnStatement(Some(Box::new(expr)))],
                is_expression_body: true,
                body_source_range: None,
            }
        };

        self.set_capture_this_references(previous_this_capture);
        result
    }

    /// Convert function parameters to `IRParam` vec
    fn convert_parameters(&self, param_nodes: &[NodeIndex]) -> Vec<IRParam> {
        let mut params = Vec::new();
        for &param_idx in param_nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };

            if param_node.kind == syntax_kind_ext::PARAMETER
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                let name =
                    crate::transforms::emit_utils::identifier_text_or_empty(self.arena, param.name);
                if param.dot_dot_dot_token {
                    params.push(IRParam::rest(name));
                } else {
                    params.push(IRParam::new(name));
                }
            }
        }
        params
    }

    /// Convert a function body (block) to IR statements
    fn convert_function_body(&self, body_idx: NodeIndex) -> Vec<IRNode> {
        let Some(body_node) = self.arena.get(body_idx) else {
            return vec![];
        };

        if body_node.kind != syntax_kind_ext::BLOCK {
            return vec![];
        }

        let Some(block) = self.arena.get_block(body_node) else {
            return vec![];
        };

        block
            .statements
            .nodes
            .iter()
            .map(|&stmt_idx| self.statement_to_ir(stmt_idx))
            .collect()
    }

    /// Get unary operator text from a token kind
    pub fn get_unary_operator_text(&self, op: u16) -> String {
        crate::transforms::emit_utils::operator_to_str(op).to_string()
    }

    /// Convert an AST statement to IR
    pub fn statement_to_ir(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::EmptyStatement;
        };

        match node.kind {
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
                    let expr = self.expression_to_ir(expr_stmt.expression);
                    IRNode::ExpressionStatement(Box::new(expr))
                } else {
                    IRNode::EmptyStatement
                }
            }

            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(node) {
                    if ret.expression.is_none() {
                        IRNode::ReturnStatement(None)
                    } else {
                        IRNode::ReturnStatement(Some(Box::new(
                            self.expression_to_ir(ret.expression),
                        )))
                    }
                } else {
                    IRNode::ReturnStatement(None)
                }
            }

            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_data) = self.arena.get_variable(node) {
                    let mut decls = Vec::new();
                    for &decl_idx in &var_data.declarations.nodes {
                        if let Some(decl_node) = self.arena.get(decl_idx)
                            && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        {
                            let name = crate::transforms::emit_utils::identifier_text_or_empty(
                                self.arena, decl.name,
                            );
                            let init = if decl.initializer.is_none() {
                                None
                            } else {
                                Some(Box::new(self.expression_to_ir(decl.initializer)))
                            };
                            decls.push(IRNode::VarDecl {
                                name: name.into(),
                                initializer: init,
                            });
                        }
                    }
                    if decls.len() == 1 {
                        decls.remove(0)
                    } else {
                        IRNode::VarDeclList(decls)
                    }
                } else {
                    IRNode::EmptyStatement
                }
            }

            _ => IRNode::ASTRef(idx),
        }
    }

    /// Get operator text from a token kind
    pub fn get_operator_text(&self, op: u16) -> String {
        crate::transforms::emit_utils::operator_to_str(op).to_string()
    }
}
