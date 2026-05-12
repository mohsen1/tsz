use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn jsx_construct_return_can_use_render_fallback(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
    ) -> bool {
        if !self.jsx_construct_return_satisfies_element_class_render(source_type, target_type) {
            return false;
        }

        let target_type = self.evaluate_type_with_env(target_type);
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target_type)
        else {
            return true;
        };

        shape.properties.iter().all(|prop| {
            if prop.optional {
                return true;
            }
            let name = self.ctx.types.resolve_atom_ref(prop.name);
            if name.as_ref() == "render" {
                return true;
            }
            matches!(
                self.resolve_property_access_with_env(source_type, name.as_ref()),
                crate::query_boundaries::common::PropertyAccessResult::Success {
                    type_id,
                    ..
                } if self.is_assignable_to(type_id, prop.type_id)
            )
        })
    }
}
