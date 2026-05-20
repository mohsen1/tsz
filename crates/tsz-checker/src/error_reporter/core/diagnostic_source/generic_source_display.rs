//! Generic source-display reductions for assignment diagnostics.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// When the source of an assignment is a generic intersection (e.g., `T & U`
    /// where at least one member is a type parameter with a constraint), return
    /// the reduced base-constraint form for display.
    pub(in crate::error_reporter) fn generic_intersection_source_display_substitution(
        &self,
        source: TypeId,
    ) -> Option<TypeId> {
        let members = crate::query_boundaries::common::intersection_members(
            self.ctx.types.as_type_database(),
            source,
        )?;
        let has_constrained_type_param = members.iter().any(|&m| {
            crate::query_boundaries::common::type_param_info(self.ctx.types.as_type_database(), m)
                .and_then(|info| info.constraint)
                .is_some()
        });
        if !has_constrained_type_param {
            return None;
        }
        let reduced = crate::query_boundaries::common::get_base_constraint_for_display(
            self.ctx.types.as_type_database(),
            source,
        );
        if reduced == source || self.is_literal_only_union_for_diagnostic_display(reduced) {
            return None;
        }
        Some(reduced)
    }

    fn is_literal_only_union_for_diagnostic_display(&self, ty: TypeId) -> bool {
        let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, ty)
        else {
            return false;
        };
        !members.is_empty()
            && members.iter().all(|&member| {
                crate::query_boundaries::common::literal_value(self.ctx.types, member).is_some()
                    || member == TypeId::BOOLEAN_TRUE
                    || member == TypeId::BOOLEAN_FALSE
            })
    }

    /// Whether `type_id` is a union whose every member is function-like.
    pub(in crate::error_reporter) fn union_is_all_function_like(&self, type_id: TypeId) -> bool {
        let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        else {
            return false;
        };
        members.iter().all(|&m| {
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, m).is_some()
                || crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, m)
                    .is_some()
        })
    }
}
