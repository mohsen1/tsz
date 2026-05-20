use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Format an Intersection type that has an Application `display_alias`, showing the
    /// structural intersection form (not the application alias). Matches tsc's behavior
    /// for branded primitive types in assignability messages: e.g., `Brand<T>` displayed
    /// as `Number & { __brand: T }` with widened member types and capitalized primitives.
    fn format_intersection_expanding_application_alias(&mut self, type_id: TypeId) -> String {
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_skip_application_alias_for_intersections()
            .with_capitalize_primitive_intersection_members()
            .with_preserve_optional_parameter_surface_syntax(false);
        formatter.format(type_id).into_owned()
    }

    /// Returns true if the intersection type at `type_id` has at least one
    /// primitive member (number, string, or boolean). Used to distinguish
    /// branded primitive intersections (e.g. `number & { __brand: T }`) from
    /// intersections of only non-primitive types (e.g. `ClassAlias & FnAlias`).
    fn intersection_has_primitive_member(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::intersection_members(self.ctx.types, type_id).is_some_and(
            |members| {
                members
                    .iter()
                    .any(|&m| m == TypeId::NUMBER || m == TypeId::STRING || m == TypeId::BOOLEAN)
            },
        )
    }

    pub(in crate::error_reporter) fn application_backed_primitive_intersection_display(
        &mut self,
        type_id: TypeId,
        evaluated: TypeId,
    ) -> Option<String> {
        let is_primitive_intersection = |state: &Self, candidate: TypeId| {
            crate::query_boundaries::common::is_intersection_type(state.ctx.types, candidate)
                && state.intersection_has_primitive_member(candidate)
        };

        if is_primitive_intersection(self, type_id)
            && self
                .ctx
                .types
                .get_display_alias(type_id)
                .is_some_and(|alias| {
                    crate::query_boundaries::common::is_generic_application(self.ctx.types, alias)
                })
        {
            return Some(self.format_intersection_expanding_application_alias(type_id));
        }

        if crate::query_boundaries::common::is_generic_application(self.ctx.types, type_id)
            && evaluated != type_id
            && is_primitive_intersection(self, evaluated)
        {
            return Some(self.format_intersection_expanding_application_alias(evaluated));
        }

        None
    }
}
