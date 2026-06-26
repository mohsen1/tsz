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

    /// When an application-backed intersection is `P & {}` — a single
    /// non-nullable primitive `P` intersected only with empty object types
    /// (`{}`) — tsc collapses it to the bare primitive `P`. This is the
    /// `NonNullable<P>` shape (the lib defines `NonNullable<T> = T & {}`) and
    /// any user helper of the form `type Helper<T> = T & {}`. The empty
    /// `{}` co-member is identity on a non-nullable primitive, so the spelling
    /// that survives is just the primitive.
    ///
    /// Returns the collapsed primitive display, or `None` when the
    /// intersection has a real property-bag co-member (the branded #5195 case
    /// `{ __brand: B }`) that must keep the expanded, capitalized spelling
    /// (`Number & { __brand: B }`). Keying on the structural shape — an
    /// empty-object co-member plus one primitive — rather than on the alias
    /// name keeps `NonNullable<T>` and arbitrary `T & {}` helpers in sync
    /// without inspecting any user-chosen identifier.
    fn collapse_application_empty_object_intersection(
        &mut self,
        type_id: TypeId,
    ) -> Option<String> {
        let members =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)?;
        let mut had_empty_object = false;
        let mut kept: Option<TypeId> = None;
        for &member in members.iter() {
            if crate::query_boundaries::common::is_empty_object_type(self.ctx.types, member) {
                had_empty_object = true;
            } else if kept.replace(member).is_some() {
                // More than one non-empty member (e.g. `P & Q & {}`): not the
                // `P & {}` identity shape, so leave the expanded form alone.
                return None;
            }
        }
        if !had_empty_object {
            return None;
        }
        let primitive = kept?;
        // Only collapse a genuinely non-nullable primitive — the wide
        // intrinsics where `& {}` is a no-op and tsc prints the bare keyword.
        if !crate::query_boundaries::common::is_widening_primitive_intrinsic(
            self.ctx.types,
            primitive,
        ) {
            return None;
        }
        Some(self.format_type_for_assignability_message(primitive))
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
            if let Some(collapsed) = self.collapse_application_empty_object_intersection(type_id) {
                return Some(collapsed);
            }
            return Some(self.format_intersection_expanding_application_alias(type_id));
        }

        if crate::query_boundaries::common::is_generic_application(self.ctx.types, type_id)
            && evaluated != type_id
            && is_primitive_intersection(self, evaluated)
        {
            if let Some(collapsed) = self.collapse_application_empty_object_intersection(evaluated)
            {
                return Some(collapsed);
            }
            return Some(self.format_intersection_expanding_application_alias(evaluated));
        }

        None
    }
}
