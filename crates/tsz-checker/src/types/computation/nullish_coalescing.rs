//! Nullish-coalescing (`??`) diagnostic and result helpers.

use super::binary_support::SyntacticNullishness;
use crate::query_boundaries::type_computation::expression_results as result_query;
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

pub(super) struct NullishCoalescingLeftDiagnostics {
    pub(super) never_nullish_diag: Option<NodeIndex>,
    pub(super) always_nullish_diag: Option<NodeIndex>,
}

impl<'a> CheckerState<'a> {
    pub(super) fn nullish_coalescing_left_diagnostics(
        &self,
        left_idx: NodeIndex,
    ) -> NullishCoalescingLeftDiagnostics {
        // tsc's `checkNullishCoalesceOperandLeft` is a purely syntactic check:
        // it anchors at `skipOuterExpressions(left, All)` (through parentheses,
        // type assertions, `satisfies`, and non-null assertions) and classifies
        // that target with `getSyntacticNullishnessSemantics` — the static type
        // of the operand never participates. We mirror that exactly, reusing the
        // shared `get_syntactic_nullishness` classifier.
        let target = self.ctx.arena.skip_parenthesized_and_assertions(left_idx);
        match self.get_syntactic_nullishness(target) {
            SyntacticNullishness::Always => NullishCoalescingLeftDiagnostics {
                never_nullish_diag: None,
                always_nullish_diag: Some(target),
            },
            SyntacticNullishness::Never => NullishCoalescingLeftDiagnostics {
                never_nullish_diag: Some(target),
                always_nullish_diag: None,
            },
            SyntacticNullishness::Sometimes => NullishCoalescingLeftDiagnostics {
                never_nullish_diag: None,
                always_nullish_diag: None,
            },
        }
    }

    pub(super) fn nullish_coalescing_result_type(
        &mut self,
        evaluated_left: TypeId,
        non_nullish: Option<TypeId>,
        right_type: TypeId,
        right_idx: NodeIndex,
    ) -> TypeId {
        let Some(non_nullish) = non_nullish else {
            return right_type;
        };

        // tsc's non-nullish operand for `??` is `getNonNullableType(left)`. For
        // `unknown` (= `{} | null | undefined`) that is the empty object `{}`,
        // not `unknown`: `unknown ?? X` is `{} | X` (e.g.
        // `Object.entries(data ?? {})` with `data: unknown`). The nullish split
        // deliberately keeps `unknown` whole for flow `!= null` narrowing, so the
        // `??` result type substitutes the empty-object non-nullable form here.
        let non_nullish = if evaluated_left == TypeId::UNKNOWN {
            result_query::empty_object_type(self.ctx.types)
        } else {
            non_nullish
        };

        // Match tsc's `NonNullable<D>` approximation: when D is an
        // unconstrained type parameter, `(D | undefined) ?? X` yields
        // `(D & {}) | X` rather than `D | X`.
        let evaluator = crate::query_boundaries::common::new_binary_op_evaluator(self.ctx.types);
        let non_nullish = evaluator.apply_non_nullable_approximation(evaluated_left, non_nullish);

        let right_is_fresh_object =
            crate::query_boundaries::common::is_fresh_object_type(self.ctx.types, right_type);
        let right_is_empty_object_literal = self.is_empty_object_literal_expression(right_idx);
        let right_subtype = self
            .diagnostic_subtype_outcome(right_type, non_nullish)
            .related;

        if non_nullish == right_type
            || ((!right_is_fresh_object || right_is_empty_object_literal) && right_subtype)
        {
            return non_nullish;
        }
        if self
            .diagnostic_subtype_outcome(non_nullish, right_type)
            .related
        {
            return right_type;
        }

        result_query::nullish_coalescing_union(self.ctx.types, non_nullish, right_type)
    }

    fn is_empty_object_literal_expression(&self, idx: NodeIndex) -> bool {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        self.ctx
            .arena
            .get(idx)
            .filter(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
            .and_then(|node| self.ctx.arena.get_literal_expr(node))
            .is_some_and(|lit| lit.elements.nodes.is_empty())
    }
}
