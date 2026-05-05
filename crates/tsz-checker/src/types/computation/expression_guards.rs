//! Expression-shape helpers shared by computation diagnostics.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn expression_is_intrinsically_non_promise_like(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                self.ctx.arena.get_parenthesized(node).is_some_and(|expr| {
                    self.expression_is_intrinsically_non_promise_like(expr.expression)
                })
            }
            k if k == syntax_kind_ext::AS_EXPRESSION => {
                self.ctx.arena.get_type_assertion(node).is_some_and(|expr| {
                    self.expression_is_intrinsically_non_promise_like(expr.expression)
                })
            }
            k if k == syntax_kind_ext::SATISFIES_EXPRESSION => {
                self.ctx.arena.get_type_assertion(node).is_some_and(|expr| {
                    self.expression_is_intrinsically_non_promise_like(expr.expression)
                })
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                self.ctx.arena.get_unary_expr_ex(node).is_some_and(|expr| {
                    self.expression_is_intrinsically_non_promise_like(expr.expression)
                })
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == tsz_scanner::SyntaxKind::StringLiteral as u16
                || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == tsz_scanner::SyntaxKind::NumericLiteral as u16
                || k == tsz_scanner::SyntaxKind::BigIntLiteral as u16
                || k == tsz_scanner::SyntaxKind::RegularExpressionLiteral as u16
                || k == tsz_scanner::SyntaxKind::TrueKeyword as u16
                || k == tsz_scanner::SyntaxKind::FalseKeyword as u16
                || k == tsz_scanner::SyntaxKind::NullKeyword as u16 =>
            {
                true
            }
            _ => false,
        }
    }

    pub(super) fn contextual_type_for_conditional_branch(
        &self,
        contextual: TypeId,
        branch_idx: NodeIndex,
    ) -> TypeId {
        if !self.expression_is_intrinsically_non_promise_like(branch_idx) {
            return contextual;
        }

        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, contextual)
        else {
            return contextual;
        };

        let mut non_promise_members = Vec::new();
        let mut saw_promise_member = false;
        for member in members {
            if self.type_ref_is_promise_like(member) {
                saw_promise_member = true;
            } else {
                non_promise_members.push(member);
            }
        }

        if saw_promise_member && !non_promise_members.is_empty() {
            self.ctx.types.factory().union(non_promise_members)
        } else {
            contextual
        }
    }

    pub(crate) fn is_identifier_reference_to_global_nan(&self, node_idx: NodeIndex) -> bool {
        let mut current_idx = node_idx;
        while let Some(node) = self.ctx.arena.get(current_idx) {
            if node.kind == tsz_parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(expr) = self.ctx.arena.get_parenthesized(node)
            {
                current_idx = expr.expression;
                continue;
            }
            break;
        }

        if let Some(node) = self.ctx.arena.get(current_idx)
            && node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(node)
            && ident.escaped_text == "NaN"
        {
            if let Some(sym_id) = self.resolve_identifier_symbol(current_idx) {
                // Only treat this as the global NaN if it actually comes from a
                // lib declaration file. User-declared locals (even at module
                // scope) have `parent.is_none()` too, so checking `parent` is
                // not a reliable discriminator; rely on the arena origin instead.
                return self.ctx.symbol_is_from_lib(sym_id);
            }
            return true; // Unresolved NaN treated as global
        }
        false
    }

    /// Check if a unary expression node is the direct left-hand side of a `**` binary.
    ///
    /// Used to suppress secondary diagnostics (TS2703 from `delete`, TS2872 from `!`) when
    /// the unary expression is in a grammar-error position. When `(delete X) ** Y` or
    /// `(!X) ** Y` is processed, binary.rs will emit TS17006 for this node as the LHS of `**`.
    /// Emitting TS2703/TS2872 on top would be a false positive, so we skip them here.
    pub(crate) fn is_lhs_of_exponentiation(&self, node_idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;
        if let Some(parent_idx) = self.ctx.arena.get_extended(node_idx).map(|e| e.parent)
            && let Some(parent_node) = self.ctx.arena.get(parent_idx)
            && parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(parent_binary) = self.ctx.arena.get_binary_expr(parent_node)
            && parent_binary.operator_token == SyntaxKind::AsteriskAsteriskToken as u16
            && parent_binary.left == node_idx
        {
            true
        } else {
            false
        }
    }

    /// Check if a node is a "literal expression of object" - one of:
    /// `ObjectLiteralExpression`, `ArrayLiteralExpression`, `RegularExpressionLiteral`,
    /// `FunctionExpression`, or `ClassExpression`. Used for TS2839 (object equality check).
    pub(crate) fn is_literal_expression_of_object(&self, node_idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;
        if let Some(node) = self.ctx.arena.get(node_idx) {
            matches!(
                node.kind,
                k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || k == SyntaxKind::RegularExpressionLiteral as u16
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::CLASS_EXPRESSION
            )
        } else {
            false
        }
    }
}
