use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn jsx_construct_return_has_readonly_props(&mut self, source_type: TypeId) -> bool {
        matches!(
            self.resolve_property_access_with_env(source_type, "props"),
            crate::query_boundaries::common::PropertyAccessResult::Success {
                type_id,
                ..
            } if crate::query_boundaries::common::contains_mapped_type_with_readonly_modifier(
                self.ctx.types,
                type_id,
            )
        )
    }

    pub(super) fn jsx_component_type_has_readonly_construct_props(
        &mut self,
        component_type: TypeId,
    ) -> bool {
        let mut stack = vec![component_type];
        let mut seen = rustc_hash::FxHashSet::default();

        while let Some(type_id) = stack.pop() {
            let resolved = if crate::query_boundaries::common::needs_evaluation_for_merge(
                self.ctx.types,
                type_id,
            ) {
                self.evaluate_type_with_env(type_id)
            } else {
                type_id
            };
            if !seen.insert(resolved) {
                continue;
            }
            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, resolved)
            {
                stack.extend(members);
                continue;
            }
            if let Some(sigs) = crate::query_boundaries::common::construct_signatures_for_type(
                self.ctx.types,
                resolved,
            ) && sigs.iter().any(|sig| {
                let return_type = self.evaluate_type_with_env(sig.return_type);
                self.jsx_construct_return_has_readonly_props(return_type)
            }) {
                return true;
            }
        }

        false
    }

    pub(super) fn jsx_construct_return_can_use_render_fallback(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
    ) -> bool {
        if !self.jsx_construct_return_satisfies_element_class_render(source_type, target_type) {
            return false;
        }
        if self.jsx_construct_return_has_readonly_props(source_type) {
            return true;
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
            if name.as_ref() == "props" && self.jsx_construct_return_has_readonly_props(source_type)
            {
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
