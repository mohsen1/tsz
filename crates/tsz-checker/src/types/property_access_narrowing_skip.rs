//! Flow-narrowing skip predicates for property access results.
//!
//! Extracted from `property_access_type.rs` to keep that module under
//! the 2000 LOC architecture ceiling.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(crate) fn should_skip_property_result_flow_narrowing(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return false;
        };
        let parent = ext.parent;
        if parent.is_none() {
            return false;
        }

        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };

        // For optional-chain continuations like `obj?.a?.b`, applying flow
        // narrowing to the intermediate `obj?.a` result is redundant because
        // the continuation logic already handles nullish propagation.
        if let Some(access_node) = self.ctx.arena.get(idx)
            && let Some(access) = self.ctx.arena.get_access_expr(access_node)
            && access.question_dot_token
            && matches!(
                parent_node.kind,
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            )
            && let Some(parent_access) = self.ctx.arena.get_access_expr(parent_node)
            && parent_access.expression == idx
        {
            return true;
        }

        // For non-optional continuation accesses within an optional chain
        // (e.g., `.transport` in `options?.nested?.transport?.backoff?.base`),
        // flow narrowing is also redundant. The base expression `options?.nested`
        // already handles nullish propagation, and there's no new type narrowing
        // information from the chain continuation itself.
        if let Some(access_node) = self.ctx.arena.get(idx)
            && let Some(access) = self.ctx.arena.get_access_expr(access_node)
            && !access.question_dot_token
            && super::computation::access::is_optional_chain(self.ctx.arena, access.expression)
            && matches!(
                parent_node.kind,
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            )
        {
            return true;
        }

        if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(binary) = self.ctx.arena.get_binary_expr(parent_node) else {
            return false;
        };

        let is_equality = matches!(
            binary.operator_token,
            k if k == SyntaxKind::EqualsEqualsToken as u16
                || k == SyntaxKind::ExclamationEqualsToken as u16
                || k == SyntaxKind::EqualsEqualsEqualsToken as u16
                || k == SyntaxKind::ExclamationEqualsEqualsToken as u16
        );
        if !is_equality {
            return false;
        }

        let other = if binary.left == idx {
            binary.right
        } else if binary.right == idx {
            binary.left
        } else {
            return false;
        };
        let other = self.ctx.arena.skip_parenthesized(other);
        let Some(other_node) = self.ctx.arena.get(other) else {
            return false;
        };

        matches!(
            other_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        )
    }

    /// Additional skip conditions for applying flow narrowing to property
    /// access results.
    ///
    /// For `obj?.prop ?? fallback`, flow narrowing the left operand result is
    /// generally redundant and adds overhead in hot optional-chain paths.
    pub(crate) fn should_skip_property_result_flow_narrowing_for_result(
        &self,
        idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        if self.should_skip_property_result_flow_narrowing(idx) {
            return true;
        }

        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return false;
        };
        let parent = ext.parent;
        if parent.is_none() {
            return false;
        }

        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(binary) = self.ctx.arena.get_binary_expr(parent_node) else {
            return false;
        };

        if binary.operator_token != SyntaxKind::QuestionQuestionToken as u16 || binary.left != idx {
            return false;
        }

        let Some(access_node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(access) = self.ctx.arena.get_access_expr(access_node) else {
            return false;
        };
        access.question_dot_token
    }
}
