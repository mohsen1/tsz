use crate::state::CheckerState;
use tsz_common::{Atom, Visibility};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Returns true when `target` is a constrained type parameter `T extends C`
    /// and `source`'s required members structurally fit `C`, matching `tsc`'s
    /// deferred assertion overlap rule for `source as T`.
    pub(crate) fn assertion_source_fits_constrained_type_param(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(info) = crate::query_boundaries::common::type_param_info(self.ctx.types, target)
        else {
            return false;
        };
        let Some(raw_constraint) = info.constraint else {
            return false;
        };
        if raw_constraint == TypeId::ANY || raw_constraint == TypeId::UNKNOWN {
            return false;
        }
        if crate::query_boundaries::assignability::contains_type_parameters(
            self.ctx.types,
            raw_constraint,
        ) {
            return false;
        }

        let resolved_source = self.evaluate_type_with_resolution(source);
        let resolved_constraint = self.evaluate_type_with_resolution(raw_constraint);

        let source_props =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, resolved_source)
                .map(|shape| shape.properties.to_vec())
                .unwrap_or_default();
        if source_props.is_empty() {
            // Empty-object and non-object cases stay on the solver overlap path.
            return false;
        }
        if source_props
            .iter()
            .any(|p| p.visibility != Visibility::Public)
        {
            return false;
        }

        let mut saw_required = false;
        for prop in source_props.iter().filter(|p| !p.optional) {
            saw_required = true;
            if !self.constrained_type_param_constraint_provides_member(
                resolved_constraint,
                prop.name,
                prop.type_id,
                0,
            ) {
                return false;
            }
        }
        saw_required
    }

    fn constrained_type_param_constraint_provides_member(
        &mut self,
        constraint: TypeId,
        name: Atom,
        source_prop_type: TypeId,
        depth: u32,
    ) -> bool {
        if depth > 10 {
            return false;
        }
        if constraint == TypeId::ANY || constraint == TypeId::UNKNOWN {
            return true;
        }

        let constraint = self.evaluate_type_with_resolution(constraint);
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, constraint)
        {
            return members.iter().any(|&member| {
                self.constrained_type_param_constraint_provides_member(
                    member,
                    name,
                    source_prop_type,
                    depth + 1,
                )
            });
        }
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, constraint)
        {
            return members.iter().any(|&member| {
                self.constrained_type_param_constraint_provides_member(
                    member,
                    name,
                    source_prop_type,
                    depth + 1,
                )
            });
        }
        if let Some(prop) = crate::query_boundaries::common::find_property_in_object(
            self.ctx.types,
            constraint,
            name,
        ) {
            return crate::query_boundaries::common::types_are_comparable_for_assertion(
                self.ctx.types,
                source_prop_type,
                prop.type_id,
            );
        }
        if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, constraint)
            && let Some(idx) = shape.string_index
        {
            return crate::query_boundaries::common::types_are_comparable_for_assertion(
                self.ctx.types,
                source_prop_type,
                idx.value_type,
            );
        }
        false
    }
}
