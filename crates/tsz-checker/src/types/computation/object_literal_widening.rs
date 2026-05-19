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

    pub(crate) fn widen_mutable_object_literal_property_types(&self, type_id: TypeId) -> TypeId {
        crate::query_boundaries::type_computation::core::widen_mutable_object_literal_property_types(
            self.ctx.types,
            type_id,
        )
    }
}
