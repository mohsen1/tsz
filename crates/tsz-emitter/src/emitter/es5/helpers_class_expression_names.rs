//! ES5 class expression contextual constructor names.

use super::super::*;
use crate::transforms::emit_utils;

impl<'a> Printer<'a> {
    pub(super) fn get_class_expression_name(&mut self, class_node: NodeIndex) -> Option<String> {
        let mut current = class_node;
        let mut hops = 0;

        while hops < 8 {
            let parent = self.arena.get_extended(current)?.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent)?;

            match parent_node.kind {
                syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    current = parent;
                    hops += 1;
                    continue;
                }
                syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.arena.get_variable_declaration(parent_node)?;
                    if decl.initializer != current {
                        return None;
                    }
                    let name = emit_utils::identifier_text_or_empty(self.arena, decl.name);
                    if name.is_empty() || !is_valid_identifier_name(&name) {
                        return None;
                    }
                    return Some(name);
                }
                syntax_kind_ext::BINARY_EXPRESSION => {
                    let binary = self.arena.get_binary_expr(parent_node)?;
                    if binary.right != current
                        || binary.operator_token != SyntaxKind::EqualsToken as u16
                    {
                        return None;
                    }
                    return self.assignment_class_expression_name(binary.left);
                }
                syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.arena.get_property_assignment(parent_node)?;
                    if prop.initializer != current {
                        return None;
                    }
                    return self.contextual_class_expression_name(prop.name);
                }
                _ => return None,
            }
        }

        None
    }

    fn assignment_class_expression_name(&mut self, left: NodeIndex) -> Option<String> {
        let left_node = self.arena.get(left)?;
        if left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(left_node)?;
            return self.contextual_class_expression_name(access.name_or_argument);
        }

        let name = emit_utils::identifier_text_or_empty(self.arena, left);
        if name.is_empty() || !is_valid_identifier_name(&name) {
            return None;
        }
        Some(name)
    }

    fn contextual_class_expression_name(&mut self, name_idx: NodeIndex) -> Option<String> {
        let name = self.class_expression_property_name_text(name_idx)?;
        if name.is_empty() || !is_valid_identifier_name(&name) {
            return None;
        }
        let token = tsz_scanner::string_to_token(&name);
        if tsz_scanner::token_is_reserved_word(token)
            || tsz_scanner::token_is_strict_mode_reserved_word(token)
        {
            return Some(self.make_unique_name_from_base(&name));
        }
        Some(name)
    }

    fn class_expression_property_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(name_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone());
        }
        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NumericLiteral as u16
        {
            return self.arena.get_literal(node).map(|lit| lit.text.clone());
        }
        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(node)?;
            let expr = self.arena.get(computed.expression)?;
            if expr.kind == SyntaxKind::StringLiteral as u16
                || expr.kind == SyntaxKind::NumericLiteral as u16
            {
                return self.arena.get_literal(expr).map(|lit| lit.text.clone());
            }
            return None;
        }
        let token = SyntaxKind::try_from_u16(node.kind)?;
        tsz_scanner::keyword_to_text_static(token).map(str::to_string)
    }
}
