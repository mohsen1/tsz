use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn should_report_nullish_assignment_through_nested_target_error(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if !self.ctx.strict_null_checks()
            || !(source == TypeId::NULL || source == TypeId::UNDEFINED)
            || matches!(target, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
        {
            return false;
        }

        // Top-level error targets keep the normal cascade-suppression behavior.
        // This override is only for structured targets whose nested members
        // contain an unresolved type, such as `() => Missing` or a class type
        // with a property of that shape.
        if crate::query_boundaries::type_predicates::is_top_level_error_or_error_union_member(
            self.ctx.types,
            target,
        ) {
            return false;
        }
        if !self.type_contains_error(target) {
            return false;
        }

        let (_, nullable_target) = self.split_nullish_type(target);
        nullable_target.is_none_or(|nullable| !self.is_assignable_to(source, nullable))
    }
}
