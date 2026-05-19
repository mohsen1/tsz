//! Literal-surface preservation helpers for diagnostic source displays.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn target_preserves_literal_surface(&mut self, target: TypeId) -> bool {
        let target = self.evaluate_type_for_assignability(target);

        let has_literal_member = |shape: &tsz_solver::ObjectShape| {
            shape
                .properties
                .iter()
                .any(|prop| self.type_contains_string_literal(prop.type_id))
        };

        if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target)
            && has_literal_member(&shape)
        {
            return true;
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, target)
        {
            return members.into_iter().any(|member| {
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, member)
                    .is_some_and(|shape| has_literal_member(&shape))
            });
        }

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, target)
        {
            return members.into_iter().any(|member| {
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, member)
                    .is_some_and(|shape| has_literal_member(&shape))
            });
        }

        false
    }
}
