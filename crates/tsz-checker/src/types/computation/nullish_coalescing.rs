//! Nullish-coalescing (`??`) diagnostic and result helpers.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeArena, NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

pub(super) struct NullishCoalescingLeftDiagnostics {
    pub(super) never_nullish_diag: Option<NodeIndex>,
    pub(super) always_nullish_diag: Option<NodeIndex>,
}

impl<'a> CheckerState<'a> {
    pub(super) fn nullish_coalescing_left_diagnostics(
        &self,
        left_idx: NodeIndex,
        non_nullish: Option<TypeId>,
        nullish_cause: Option<TypeId>,
        left_is_top_type: bool,
    ) -> NullishCoalescingLeftDiagnostics {
        let left_is_nullish_chain_or_literal =
            is_nullish_coalescing_or_literal(self.ctx.arena, left_idx);
        let always_nullish_literal_diag =
            self.nullish_coalescing_always_nullish_literal_diag(left_idx);

        let never_nullish_diag =
            if nullish_cause.is_none() && !left_is_top_type && left_is_nullish_chain_or_literal {
                Some(self.ctx.arena.skip_parenthesized(left_idx))
            } else {
                None
            };

        let always_nullish_diag = always_nullish_literal_diag.or_else(|| {
            (non_nullish.is_none()
                && nullish_cause.is_some()
                && !left_is_top_type
                && left_is_nullish_chain_or_literal)
                .then(|| self.ctx.arena.skip_parenthesized(left_idx))
        });

        NullishCoalescingLeftDiagnostics {
            never_nullish_diag,
            always_nullish_diag,
        }
    }

    fn nullish_coalescing_always_nullish_literal_diag(
        &self,
        left_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let unwrapped_left_idx = self.ctx.arena.skip_parenthesized_and_assertions(left_idx);
        self.is_literal_null_or_undefined_node(unwrapped_left_idx)
            .then_some(unwrapped_left_idx)
    }

    pub(super) fn nullish_coalescing_result_type(
        &mut self,
        evaluated_left: TypeId,
        non_nullish: Option<TypeId>,
        right_type: TypeId,
    ) -> TypeId {
        let Some(non_nullish) = non_nullish else {
            return right_type;
        };

        // Match tsc's `NonNullable<D>` approximation: when D is an
        // unconstrained type parameter, `(D | undefined) ?? X` yields
        // `(D & {}) | X` rather than `D | X`.
        let evaluator = crate::query_boundaries::common::new_binary_op_evaluator(self.ctx.types);
        let non_nullish = evaluator.apply_non_nullable_approximation(evaluated_left, non_nullish);

        if non_nullish == right_type || self.is_subtype_of(right_type, non_nullish) {
            return non_nullish;
        }
        if self.is_subtype_of(non_nullish, right_type) {
            return right_type;
        }

        self.ctx.types.factory().union2(non_nullish, right_type)
    }
}

/// Check if an AST node is a nullish-coalescing expression (`??`) or a
/// literal value (string, number, boolean, bigint, template), unwrapping
/// parentheses. TSC only emits TS2869 for these syntactic forms; general
/// non-nullable expressions (identifiers, property access, `&&` chains)
/// do not trigger TS2869 even when their type is never nullish.
fn is_nullish_coalescing_or_literal(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(node_idx) else {
        return false;
    };

    // Unwrap parentheses: (expr) -> expr
    if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
        if let Some(paren) = arena.get_parenthesized(node) {
            return is_nullish_coalescing_or_literal(arena, paren.expression);
        }
        return false;
    }

    if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
        if let Some(binary) = arena.get_binary_expr(node) {
            return binary.operator_token == SyntaxKind::QuestionQuestionToken as u16;
        }
        return false;
    }

    let kind = node.kind;
    kind == SyntaxKind::StringLiteral as u16
        || kind == SyntaxKind::NumericLiteral as u16
        || kind == SyntaxKind::BigIntLiteral as u16
        || kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        || kind == SyntaxKind::TrueKeyword as u16
        || kind == SyntaxKind::FalseKeyword as u16
        || kind == syntax_kind_ext::TEMPLATE_EXPRESSION
}
