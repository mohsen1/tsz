//! Helpers for checked-JS constructor provisional `this` property writes.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(crate) fn is_checked_js_constructor_provisional_this_property_assignment(
        &self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
    ) -> bool {
        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return false;
        }
        if !self.is_js_provisional_initializer_syntax(right_idx) {
            return false;
        }
        let Some(member_name) = self.direct_this_property_name(left_idx) else {
            return false;
        };
        let Some(stmt_idx) = self.enclosing_statement_node(left_idx) else {
            return false;
        };
        let Some(block_idx) = self.ctx.arena.parent_of(stmt_idx) else {
            return false;
        };
        let Some(block_node) = self.ctx.arena.get(block_idx) else {
            return false;
        };
        let Some(block) = self.ctx.arena.get_block(block_node) else {
            return false;
        };
        let Some(func_idx) = self.ctx.arena.parent_of(block_idx) else {
            return false;
        };
        let Some(func_node) = self.ctx.arena.get(func_idx) else {
            return false;
        };
        if !func_node.is_function_like() {
            return false;
        }
        if self
            .js_prototype_owner_expression_for_node(func_idx)
            .is_some()
        {
            return false;
        }

        let Some(current_pos) = block
            .statements
            .nodes
            .iter()
            .position(|&candidate| candidate == stmt_idx)
        else {
            return false;
        };

        block
            .statements
            .nodes
            .iter()
            .skip(current_pos + 1)
            .any(|&stmt| {
                self.direct_this_property_assignment(stmt)
                    .is_some_and(|(name, rhs)| {
                        name == member_name && !self.is_js_provisional_initializer_syntax(rhs)
                    })
            })
    }

    fn direct_this_property_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(node)?;
        let receiver = self.ctx.arena.get(access.expression)?;
        if receiver.kind != SyntaxKind::ThisKeyword as u16 {
            return None;
        }
        self.ctx
            .arena
            .get(access.name_or_argument)
            .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.clone())
    }

    fn direct_this_property_assignment(&self, stmt_idx: NodeIndex) -> Option<(String, NodeIndex)> {
        let stmt_node = self.ctx.arena.get(stmt_idx)?;
        if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return None;
        }
        let expr_stmt = self.ctx.arena.get_expression_statement(stmt_node)?;
        let expr_node = self.ctx.arena.get(expr_stmt.expression)?;
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.ctx.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }
        Some((self.direct_this_property_name(binary.left)?, binary.right))
    }

    fn is_js_provisional_initializer_syntax(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind == SyntaxKind::NullKeyword as u16 {
            return true;
        }
        if node.kind == SyntaxKind::Identifier as u16
            && self
                .ctx
                .arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text == "undefined")
        {
            return true;
        }
        node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            && self
                .ctx
                .arena
                .get_literal_expr(node)
                .is_some_and(|lit| lit.elements.nodes.is_empty())
    }
}
