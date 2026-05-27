//! ES5 transform for `function*` generator functions: the full-function
//! lowering plus rest-parameter prologue handling. Kept out of the oversized
//! `async_es5_ir.rs` so this owns the generator-function shape on its own.

use super::AsyncES5Transformer;
use crate::transforms::ir::{IRNode, IRParam};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl AsyncES5Transformer<'_> {
    pub fn transform_generator_function(&mut self, func_idx: NodeIndex) -> IRNode {
        self.state.reset();
        self.reset_loop_exit_placeholders();
        self.generator_mode = true;
        self.helpers_needed.generator = true;
        let Some(node) = self.arena.get(func_idx) else {
            self.generator_mode = false;
            return IRNode::Undefined;
        };
        let (name, mut params, param_binding_names, body_idx, rest_param) = if node.kind
            == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
        {
            if let Some(func) = self.arena.get_function(node) {
                let name = if func.name.is_none() {
                    None
                } else {
                    Some(crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena, func.name,
                    ))
                };
                let params = self.collect_parameters(&func.parameters);
                let mut param_binding_names = Vec::new();
                self.collect_parameter_binding_names(&func.parameters, &mut param_binding_names);
                let rest_param = self.identifier_rest_param_info(&func.parameters);
                (name, params, param_binding_names, func.body, rest_param)
            } else {
                self.generator_mode = false;
                return IRNode::Undefined;
            }
        } else {
            self.generator_mode = false;
            return IRNode::Undefined;
        };
        // A trailing identifier rest parameter is downleveled to an
        // `arguments`-copy prologue at ES5, identical to a non-generator
        // function. The index variable is hoisted (tsc shares loop-index temps
        // across the generator body), so it is prepended to the hoisted vars
        // and the loop reads `_i = N` rather than `for (var _i = N`.
        let rest_index_name = rest_param.as_ref().map(|_| self.fresh_reserved_name("_i"));
        if rest_param.is_some() {
            params.pop();
        }
        let has_yield = self.body_contains_await(body_idx);
        self.state.has_await = has_yield;
        self.state.captures_arguments =
            tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body_idx);
        if self.state.captures_arguments {
            self.state.arguments_capture_name =
                self.fresh_arguments_capture_name(body_idx, &param_binding_names);
        }
        let mut generator_body = self.build_generator_body(body_idx, has_yield, &[]);
        let mut hoisted_var_groups = self.extract_hoisted_var_groups(&mut generator_body);
        let ir_params: Vec<IRParam> = params.iter().map(|p| IRParam::new(p.clone())).collect();
        // The rest-loop index joins the first hoisted var group (or seeds one).
        if let Some(index_name) = &rest_index_name {
            if let Some(first) = hoisted_var_groups.first_mut() {
                first.insert(0, index_name.clone());
            } else {
                hoisted_var_groups.push(vec![index_name.clone()]);
            }
        }
        let mut body = Vec::new();
        for group in hoisted_var_groups {
            let declarations = group
                .into_iter()
                .map(|name| IRNode::VarDecl {
                    name: name.into(),
                    initializer: None,
                })
                .collect();
            body.push(IRNode::VarDeclList(declarations));
        }
        if self.state.captures_arguments {
            body.push(IRNode::VarDecl {
                name: self.state.arguments_capture_name.clone().into(),
                initializer: Some(Box::new(IRNode::Raw("arguments".to_string().into()))),
            });
        }
        if let (Some((rest_name, rest_index)), Some(index_name)) = (&rest_param, &rest_index_name) {
            self.push_rest_param_prologue(&mut body, rest_name, *rest_index, index_name);
        }
        body.push(generator_body);
        self.generator_mode = false;
        if let Some(func_name) = name {
            IRNode::FunctionDecl {
                name: func_name.into(),
                parameters: ir_params,
                body,
                body_source_range: None,
                leading_comment: None,
            }
        } else {
            IRNode::FunctionExpr {
                name: None,
                parameters: ir_params,
                body,
                is_expression_body: false,
                body_source_range: None,
            }
        }
    }

    /// Return `(name, index)` of a trailing identifier rest parameter, if any.
    /// Binding-pattern rest parameters are left to the existing path.
    fn identifier_rest_param_info(
        &self,
        params: &tsz_parser::parser::NodeList,
    ) -> Option<(String, usize)> {
        for (index, &param_idx) in params.nodes.iter().enumerate() {
            let param_node = self.arena.get(param_idx)?;
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if !param.dot_dot_dot_token {
                continue;
            }
            let name_node = self.arena.get(param.name)?;
            if name_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                return None;
            }
            let name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, param.name);
            if name.is_empty() {
                return None;
            }
            return Some((name, index));
        }
        None
    }

    /// Emit `var <rest> = []; for (<idx> = N; <idx> < arguments.length; <idx>++)
    /// { <rest>[<idx> - N] = arguments[<idx>]; }` for an ES5 rest parameter.
    fn push_rest_param_prologue(
        &self,
        body: &mut Vec<IRNode>,
        rest_name: &str,
        rest_index: usize,
        index_name: &str,
    ) {
        body.push(IRNode::VarDecl {
            name: rest_name.to_string().into(),
            initializer: Some(Box::new(IRNode::ArrayLiteral(Vec::new()))),
        });
        let lhs_index = if rest_index > 0 {
            IRNode::binary(
                IRNode::id(index_name.to_string()),
                "-",
                IRNode::number(rest_index.to_string()),
            )
        } else {
            IRNode::id(index_name.to_string())
        };
        let copy = Self::expression_statement(IRNode::assign(
            IRNode::elem(IRNode::id(rest_name.to_string()), lhs_index),
            IRNode::elem(
                IRNode::id("arguments".to_string()),
                IRNode::id(index_name.to_string()),
            ),
        ));
        body.push(IRNode::ForStatement {
            initializer: Some(Box::new(IRNode::assign(
                IRNode::id(index_name.to_string()),
                IRNode::number(rest_index.to_string()),
            ))),
            condition: Some(Box::new(IRNode::binary(
                IRNode::id(index_name.to_string()),
                "<",
                IRNode::prop(IRNode::id("arguments".to_string()), "length"),
            ))),
            incrementor: Some(Box::new(IRNode::PostfixUnaryExpr {
                operand: Box::new(IRNode::id(index_name.to_string())),
                operator: "++".into(),
            })),
            body: Box::new(IRNode::Block(vec![copy])),
        });
    }
}
