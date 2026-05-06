//! JSX attribute value type helpers.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn compute_jsx_attr_value_type_without_context(
        &mut self,
        initializer: NodeIndex,
    ) -> TypeId {
        if initializer.is_none() {
            return TypeId::BOOLEAN_TRUE;
        }
        let init_node_idx = initializer;
        if let Some(init_node) = self.ctx.arena.get(init_node_idx) {
            let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                self.ctx
                    .arena
                    .get_jsx_expression(init_node)
                    .map(|expr| expr.expression)
                    .unwrap_or(init_node_idx)
            } else {
                init_node_idx
            };
            return self.compute_type_of_node(value_idx);
        }
        TypeId::ANY
    }
}
