//! Type parameter comparability helpers for assignability checking.
//!
//! Extracted from `assignability_checker.rs` to keep module size manageable.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the apparent type for a type parameter (constraint or `unknown` for unconstrained).
    /// This matches tsc's `getReducedApparentType` behavior for type parameters.
    pub(super) fn get_type_param_apparent_type(&self, type_id: TypeId) -> TypeId {
        crate::query_boundaries::common::type_param_info(self.ctx.types, type_id)
            .and_then(|info| info.constraint)
            .unwrap_or(TypeId::UNKNOWN)
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
