//! Object-literal property widening helpers.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn is_bare_object_literal_expression(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        self.ctx
            .arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
    }

    pub(crate) fn expression_is_const_assertion(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        self.ctx
            .arena
            .get(expr_idx)
            .and_then(|node| self.ctx.arena.get_type_assertion(node))
            .is_some_and(|assertion| self.is_const_assertion_type_node(assertion.type_node))
    }

    /// Returns `true` when `expr_idx` (after skipping parentheses) is a plain
    /// `expr as T` / `<T>expr` type assertion — an `as`/angle-bracket assertion
    /// that is **not** `as const` and **not** a `satisfies` expression.
    ///
    /// The value of such an assertion is the asserted type `T` exactly as
    /// written, which `tsc` treats as a *regular* (non-fresh) type: the literal
    /// element/property types of `T` are preserved verbatim in assignability
    /// diagnostics rather than widened the way a fresh object/array literal
    /// expression is. Diagnostic source-display paths consult this to suppress
    /// the fresh-literal widening for assertion operands, so
    /// `[1, 2, 3] as [1, 2, 3]` renders as `[1, 2, 3]` (matching `tsc`) instead
    /// of `[number, number, number]`. `satisfies` is excluded (it is a distinct
    /// node kind whose value is the operand's own type), and `as const` is
    /// excluded because it already preserves a `readonly` literal surface
    /// through its own path.
    pub(crate) fn expression_is_plain_type_assertion(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        // `AS_EXPRESSION` / `TYPE_ASSERTION` only — `SATISFIES_EXPRESSION` is a
        // distinct kind and is excluded inherently. Read the node once and
        // exclude `as const` via the shared `ConstKeyword` type-node primitive
        // so there is a single const-assertion check.
        if node.kind != syntax_kind_ext::AS_EXPRESSION
            && node.kind != syntax_kind_ext::TYPE_ASSERTION
        {
            return false;
        }
        self.ctx
            .arena
            .get_type_assertion(node)
            .is_none_or(|assertion| !self.is_const_assertion_type_node(assertion.type_node))
    }

    pub(crate) fn widen_mutable_object_literal_property_types(&self, type_id: TypeId) -> TypeId {
        crate::query_boundaries::type_computation::core::widen_mutable_object_literal_property_types(
            self.ctx.types,
            type_id,
        )
    }
}
