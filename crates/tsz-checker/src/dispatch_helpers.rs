//! Helpers for the expression type computation dispatcher.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use super::dispatch::ExpressionDispatcher;

impl<'a, 'b> ExpressionDispatcher<'a, 'b> {
    /// TS1355: Check that an expression is a valid target for `as const`.
    pub(crate) fn check_const_assertion_expression(&mut self, expr_idx: NodeIndex) {
        if self.is_valid_const_assertion_arg(expr_idx) {
            return;
        }
        self.checker.error_at_node(
            expr_idx,
            diagnostic_messages::A_CONST_ASSERTION_CAN_ONLY_BE_APPLIED_TO_REFERENCES_TO_ENUM_MEMBERS_OR_STRING_NU,
            diagnostic_codes::A_CONST_ASSERTION_CAN_ONLY_BE_APPLIED_TO_REFERENCES_TO_ENUM_MEMBERS_OR_STRING_NU,
        );
    }

    fn is_valid_const_assertion_arg(&self, expr_idx: NodeIndex) -> bool {
        let Some(node) = self.checker.ctx.arena.get(expr_idx) else {
            return false;
        };
        match node.kind {
            // Literal types
            k if k == SyntaxKind::StringLiteral as u16 => true,
            k if k == SyntaxKind::NumericLiteral as u16 => true,
            k if k == SyntaxKind::BigIntLiteral as u16 => true,
            k if k == SyntaxKind::TrueKeyword as u16 => true,
            k if k == SyntaxKind::FalseKeyword as u16 => true,
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => true,
            // Compound literal types
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => true,
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => true,
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => true,
            // Prefix unary: `-` or `+` on numeric/bigint literal
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.checker.ctx.arena.get_unary_expr(node)
                    && (unary.operator == SyntaxKind::MinusToken as u16
                        || unary.operator == SyntaxKind::PlusToken as u16)
                    && let Some(operand) = self.checker.ctx.arena.get(unary.operand)
                {
                    return operand.kind == SyntaxKind::NumericLiteral as u16
                        || operand.kind == SyntaxKind::BigIntLiteral as u16;
                }
                false
            }
            // Parenthesized: recurse
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.checker.ctx.arena.get_parenthesized(node) {
                    return self.is_valid_const_assertion_arg(paren.expression);
                }
                false
            }
            // Property access: valid only if it's an enum member reference
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.checker.ctx.arena.get_access_expr(node) {
                    return self.checker.is_enum_member_property(access.expression, "");
                }
                false
            }
            _ => false,
        }
    }
}

/// Maps a syntax kind to its keyword type name and `TypeId` for TS2693 checking.
pub(crate) const fn keyword_type_mapping(kind: u16) -> Option<(&'static str, TypeId)> {
    match kind {
        k if k == SyntaxKind::NumberKeyword as u16 => Some(("number", TypeId::NUMBER)),
        k if k == SyntaxKind::StringKeyword as u16 => Some(("string", TypeId::STRING)),
        k if k == SyntaxKind::BooleanKeyword as u16 => Some(("boolean", TypeId::BOOLEAN)),
        k if k == SyntaxKind::VoidKeyword as u16 => Some(("void", TypeId::VOID)),
        k if k == SyntaxKind::AnyKeyword as u16 => Some(("any", TypeId::ANY)),
        k if k == SyntaxKind::NeverKeyword as u16 => Some(("never", TypeId::NEVER)),
        k if k == SyntaxKind::UnknownKeyword as u16 => Some(("unknown", TypeId::UNKNOWN)),
        k if k == SyntaxKind::UndefinedKeyword as u16 => Some(("undefined", TypeId::UNDEFINED)),
        k if k == SyntaxKind::ObjectKeyword as u16 => Some(("object", TypeId::OBJECT)),
        k if k == SyntaxKind::BigIntKeyword as u16 => Some(("bigint", TypeId::BIGINT)),
        k if k == SyntaxKind::SymbolKeyword as u16 => Some(("symbol", TypeId::SYMBOL)),
        _ => None,
    }
}
