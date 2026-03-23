//! Enum declaration helpers: self-reference and forward-reference checking.
//!
//! Extracted from `declarations.rs` to keep module size manageable.

use super::declarations::DeclarationChecker;
use tsz_parser::parser::NodeIndex;

impl<'a, 'ctx> DeclarationChecker<'a, 'ctx> {
    pub(super) fn check_enum_member_self_reference(
        &mut self,
        expr_idx: NodeIndex,
        member_name: &str,
        enum_name: Option<&str>,
    ) {
        if expr_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::node::NodeAccess;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(text) = self.ctx.arena.get_identifier_text(expr_idx)
                    && text == member_name
                {
                    self.ctx.error(
                        node.pos,
                        node.end - node.pos,
                        format!("Property '{text}' is used before being assigned."),
                        diagnostic_codes::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED,
                    );
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(prop) = self.ctx.arena.get_access_expr(node)
                    && let Some(left_node) = self.ctx.arena.get(prop.expression)
                {
                    let is_enum_ref = if left_node.kind == SyntaxKind::Identifier as u16 {
                        if let Some(text) = self.ctx.arena.get_identifier_text(prop.expression) {
                            Some(text) == enum_name
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if is_enum_ref {
                        if let Some(right_text) =
                            self.ctx.arena.get_identifier_text(prop.name_or_argument)
                            && right_text == member_name
                        {
                            self.ctx.error(
                                node.pos,
                                node.end - node.pos,
                                format!("Property '{right_text}' is used before being assigned."),
                                diagnostic_codes::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED,
                            );
                        }
                    } else {
                        self.check_enum_member_self_reference(
                            prop.expression,
                            member_name,
                            enum_name,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(elem) = self.ctx.arena.get_access_expr(node)
                    && let Some(left_node) = self.ctx.arena.get(elem.expression)
                {
                    let is_enum_ref = if left_node.kind == SyntaxKind::Identifier as u16 {
                        if let Some(text) = self.ctx.arena.get_identifier_text(elem.expression) {
                            Some(text) == enum_name
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if is_enum_ref {
                        if let Some(right_node) = self.ctx.arena.get(elem.name_or_argument) {
                            if right_node.kind == SyntaxKind::StringLiteral as u16
                                || right_node.kind
                                    == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                            {
                                if let Some(lit) = self.ctx.arena.get_literal(right_node)
                                    && lit.text == member_name
                                {
                                    self.ctx.error(
                                        node.pos,
                                        node.end - node.pos,
                                        format!(
                                            "Property '{}' is used before being assigned.",
                                            lit.text
                                        ),
                                        diagnostic_codes::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED,
                                    );
                                }
                            } else {
                                self.check_enum_member_self_reference(
                                    elem.name_or_argument,
                                    member_name,
                                    enum_name,
                                );
                            }
                        }
                    } else {
                        self.check_enum_member_self_reference(
                            elem.expression,
                            member_name,
                            enum_name,
                        );
                        self.check_enum_member_self_reference(
                            elem.name_or_argument,
                            member_name,
                            enum_name,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
                    self.check_enum_member_self_reference(unary.operand, member_name, enum_name);
                }
            }
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
                    self.check_enum_member_self_reference(unary.operand, member_name, enum_name);
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.ctx.arena.get_binary_expr(node) {
                    self.check_enum_member_self_reference(bin.left, member_name, enum_name);
                    self.check_enum_member_self_reference(bin.right, member_name, enum_name);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.check_enum_member_self_reference(paren.expression, member_name, enum_name);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    self.check_enum_member_self_reference(cond.condition, member_name, enum_name);
                    self.check_enum_member_self_reference(cond.when_true, member_name, enum_name);
                    self.check_enum_member_self_reference(cond.when_false, member_name, enum_name);
                }
            }
            _ => {}
        }
    }

    /// Check if an enum initializer contains a forward reference to a later member.
    pub(super) fn enum_has_forward_reference(
        &self,
        expr_idx: NodeIndex,
        later_members: &[&str],
        enum_name: Option<&str>,
    ) -> bool {
        if expr_idx.is_none() || later_members.is_empty() {
            return false;
        }
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        use tsz_parser::parser::node::NodeAccess;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(text) = self.ctx.arena.get_identifier_text(expr_idx) {
                    later_members.contains(&text)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(prop) = self.ctx.arena.get_access_expr(node) {
                    if let Some(left_node) = self.ctx.arena.get(prop.expression)
                        && left_node.kind == SyntaxKind::Identifier as u16
                        && let Some(left_name) = self.ctx.arena.get_identifier_text(prop.expression)
                        && Some(left_name) == enum_name
                        && let Some(right_text) =
                            self.ctx.arena.get_identifier_text(prop.name_or_argument)
                    {
                        return later_members.contains(&right_text);
                    }
                    self.enum_has_forward_reference(prop.expression, later_members, enum_name)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(elem) = self.ctx.arena.get_access_expr(node) {
                    if let Some(left_node) = self.ctx.arena.get(elem.expression)
                        && left_node.kind == SyntaxKind::Identifier as u16
                        && let Some(left_name) = self.ctx.arena.get_identifier_text(elem.expression)
                        && Some(left_name) == enum_name
                        && let Some(arg_node) = self.ctx.arena.get(elem.name_or_argument)
                        && (arg_node.kind == SyntaxKind::StringLiteral as u16
                            || arg_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
                        && let Some(lit) = self.ctx.arena.get_literal(arg_node)
                    {
                        return later_members.contains(&lit.text.as_str());
                    }
                    self.enum_has_forward_reference(elem.expression, later_members, enum_name)
                        || self.enum_has_forward_reference(
                            elem.name_or_argument,
                            later_members,
                            enum_name,
                        )
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
                    self.enum_has_forward_reference(unary.operand, later_members, enum_name)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.ctx.arena.get_binary_expr(node) {
                    self.enum_has_forward_reference(bin.left, later_members, enum_name)
                        || self.enum_has_forward_reference(bin.right, later_members, enum_name)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.enum_has_forward_reference(paren.expression, later_members, enum_name)
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}
