use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn target_is_normalized_object_literal_union(
        &mut self,
        target: TypeId,
    ) -> bool {
        let target = self.evaluate_type_for_assignability(target);
        let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, target)
        else {
            return false;
        };
        if members.len() < 2 {
            return false;
        }

        let mut saw_optional_undefined_surface = false;
        for member in members {
            let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, member)
            else {
                return false;
            };
            if shape.symbol.is_some() || shape.properties.is_empty() {
                return false;
            }
            saw_optional_undefined_surface |= shape.properties.iter().any(|prop| {
                crate::query_boundaries::common::type_contains_undefined(
                    self.ctx.types,
                    prop.type_id,
                )
            });
        }

        saw_optional_undefined_surface
    }
}
