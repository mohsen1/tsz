//! ES5 rest-parameter prologue lowering for downleveled generators.
//!
//! A trailing identifier rest parameter on a `function*` downleveled to the
//! `__generator` state machine is lowered to the `arguments`-copy prologue,
//! identical to a non-generator function.

use super::AsyncES5Transformer;
use crate::transforms::ir::IRNode;

impl AsyncES5Transformer<'_> {
    /// Return `(name, index)` of a trailing identifier rest parameter, if any.
    /// Binding-pattern rest parameters are left to the existing path.
    pub(super) fn identifier_rest_param_info(
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
    pub(super) fn push_rest_param_prologue(
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
