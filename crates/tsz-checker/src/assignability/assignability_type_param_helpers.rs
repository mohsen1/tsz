//! Type parameter comparability helpers for assignability checking.
//!
//! Extracted from `assignability_checker.rs` to keep module size manageable.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn nullish_cause_includes_undefined_like(&self, type_id: TypeId) -> bool {
        if matches!(type_id, TypeId::UNDEFINED | TypeId::VOID) {
            return true;
        }

        crate::query_boundaries::common::union_members(self.ctx.types, type_id).is_some_and(
            |members| {
                members
                    .iter()
                    .any(|&member| self.nullish_cause_includes_undefined_like(member))
            },
        )
    }

    /// Get the apparent type for a type parameter (constraint or `unknown` for unconstrained).
    /// This matches tsc's `getReducedApparentType` behavior for type parameters.
    pub(crate) fn get_type_param_apparent_type(&mut self, type_id: TypeId) -> TypeId {
        let Some(constraint) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, type_id)
                .and_then(|info| info.constraint)
        else {
            return TypeId::UNKNOWN;
        };

        let evaluated_constraint = self.evaluate_type_with_env(constraint);
        let (non_nullish, nullish_cause) = self.split_nullish_type(evaluated_constraint);
        if non_nullish.is_some_and(|ty| {
            ty == TypeId::OBJECT
                || crate::query_boundaries::common::is_empty_object_type(self.ctx.types, ty)
        }) && !nullish_cause
            .is_some_and(|cause| self.nullish_cause_includes_undefined_like(cause))
        {
            return TypeId::OBJECT;
        }

        evaluated_constraint
    }

    /// Check if two type parameters are comparable (one constrains to the other).
    /// In tsc, two unconstrained type parameters are NOT comparable (tsc checker.ts:23671-23684).
    pub(super) fn type_params_are_comparable(&mut self, source: TypeId, target: TypeId) -> bool {
        // Check if source's constraint chain reaches target.
        // For union constraints (e.g., U extends T | string), check if any member
        // of the union is assignable to the target. This handles cases like
        // `U extends T | string` being comparable to `T`.
        if let Some(info) = crate::query_boundaries::common::type_param_info(self.ctx.types, source)
            && let Some(constraint) = info.constraint
        {
            if self.is_assignable_to(constraint, target) {
                return true;
            }
            // Decompose union constraints: if any member is comparable/assignable to target
            if let Some(members) =
                crate::query_boundaries::dispatch::union_members(self.ctx.types, constraint)
            {
                for member in &members {
                    if *member == target || self.is_assignable_to(*member, target) {
                        return true;
                    }
                }
            }
        }
        // Check if target's constraint chain reaches source
        if let Some(info) = crate::query_boundaries::common::type_param_info(self.ctx.types, target)
            && let Some(constraint) = info.constraint
        {
            if self.is_assignable_to(source, constraint) {
                return true;
            }
            // Decompose union constraints for target
            if let Some(members) =
                crate::query_boundaries::dispatch::union_members(self.ctx.types, constraint)
            {
                for member in &members {
                    if *member == source || self.is_assignable_to(source, *member) {
                        return true;
                    }
                }
            }
        }
        false
    }
}
