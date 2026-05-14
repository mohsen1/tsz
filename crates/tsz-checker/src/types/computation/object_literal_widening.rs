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
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
        else {
            return type_id;
        };

        let mut widened_shape = shape.as_ref().clone();
        let mut changed = false;
        for prop in &mut widened_shape.properties {
            let widened_read =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, prop.type_id);
            let widened_write = crate::query_boundaries::common::widen_literal_type(
                self.ctx.types,
                prop.write_type,
            );
            if widened_read != prop.type_id || widened_write != prop.write_type {
                changed = true;
            }
            prop.type_id = widened_read;
            prop.write_type = widened_write;
        }

        if changed {
            self.ctx.types.factory().object_with_index(widened_shape)
        } else {
            type_id
        }
    }
}
