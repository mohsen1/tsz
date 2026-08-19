//! Literal-surface preservation helpers for diagnostic source displays.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn target_preserves_literal_surface(&mut self, target: TypeId) -> bool {
        let target = self.evaluate_type_for_assignability(target);

        // A target preserves the source's literal surface when any of its
        // (arm) properties carries a unit literal of *any* domain — string,
        // number, boolean, or bigint. Restricting this to string literals
        // wrongly widened numeric/bigint/boolean-literal object-literal sources
        // against a same-domain union target (e.g. `{ p: 1; q: 4 }` against
        // `{ p: 1; q: 2 } | { p: 3; q: 4 }` rendered `{ p: number; q: number }`
        // instead of tsc's `{ p: 1; q: 4 }`).
        let has_literal_member = |shape: &tsz_solver::ObjectShape| {
            shape.properties.iter().any(|prop| {
                crate::query_boundaries::diagnostics::type_contains_unit_literal(
                    self.ctx.types,
                    prop.type_id,
                )
            })
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
