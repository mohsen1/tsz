//! Element-access suspension lowering for async ES5 IR.
//!
//! When an element-access index suspends, the object expression must be
//! evaluated before the yield. This module plans that capture as structured IR
//! so the main async state-machine lowering does not grow another local
//! special case.

use crate::transforms::async_es5_ir::AsyncES5Transformer;
use crate::transforms::ir::{IRGeneratorCase, IRNode};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl AsyncES5Transformer<'_> {
    pub(super) fn lower_element_access_object_before_suspension(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => self
                .lower_suspended_element_access_object(
                    idx,
                    cases,
                    current_statements,
                    current_label,
                ),
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let binary = self.arena.get_binary_expr(node)?;
                if self.get_operator_text(binary.operator_token) != "=" {
                    return None;
                }
                let left = self.arena.get(binary.left)?;
                if left.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                    return None;
                }
                let right = self.lower_element_access_object_before_suspension(
                    binary.right,
                    cases,
                    current_statements,
                    current_label,
                )?;
                Some(IRNode::BinaryExpr {
                    left: Box::new(self.expression_to_ir(binary.left)),
                    operator: self.get_operator_text(binary.operator_token).into(),
                    right: Box::new(right),
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.arena.get_parenthesized(node)?;
                self.lower_element_access_object_before_suspension(
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
                self.lower_element_access_object_before_suspension(
                    assertion.expression,
                    cases,
                    current_statements,
                    current_label,
                )
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let unary = self.arena.get_unary_expr_ex(node)?;
                self.lower_element_access_object_before_suspension(
                    unary.expression,
                    cases,
                    current_statements,
                    current_label,
                )
            }
            _ => None,
        }
    }

    fn lower_suspended_element_access_object(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        let access = self.arena.get_access_expr(node)?;
        if !self.contains_await_recursive(access.name_or_argument)
            || self.contains_await_recursive(access.expression)
        {
            return None;
        }

        let temp = self.generate_hoisted_temp();
        current_statements.push(IRNode::VarDecl {
            name: temp.clone().into(),
            initializer: None,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(temp.clone()),
            self.expression_to_ir(access.expression),
        ))));

        self.emit_nested_suspension(
            access.name_or_argument,
            cases,
            current_statements,
            current_label,
        );

        Some(IRNode::elem(
            IRNode::id(temp),
            self.expression_to_ir(access.name_or_argument),
        ))
    }
}
