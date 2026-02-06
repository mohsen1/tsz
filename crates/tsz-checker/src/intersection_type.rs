//! Intersection Type Utilities Module
//!
//! This module contains intersection type utility methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Intersection member extraction and manipulation
//! - Intersection type filtering and transformation
//! - Intersection type compatibility checking
//! - Intersection type simplification
//!
//! This module extends CheckerState with utilities for intersection type
//! operations, providing cleaner APIs for intersection type checking.

use crate::state::CheckerState;
use tsz_solver::TypeId;
use tsz_solver::type_queries;

// =============================================================================
// Intersection Type Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Intersection Member Extraction
    // =========================================================================

    /// Get the members of an intersection type.
    ///
    /// Returns a vector of TypeIds representing all members of the intersection.
    /// Returns an empty vec if the type is not an intersection.
    pub fn get_intersection_members(&self, type_id: TypeId) -> Vec<TypeId> {
        type_queries::get_intersection_members(self.ctx.types, type_id).unwrap_or_default()
    }

    /// Get the number of members in an intersection type.
    ///
    /// Returns 0 if the type is not an intersection.
    pub fn intersection_member_count(&self, type_id: TypeId) -> usize {
        type_queries::get_intersection_members(self.ctx.types, type_id)
            .map(|members| members.len())
            .unwrap_or(0)
    }

    // =========================================================================
    // Intersection Type Analysis
    // =========================================================================

    /// Check if a type is a member of an intersection type.
    ///
    /// Returns true if the given member type is in the intersection.
    pub fn intersection_contains(&self, intersection_type: TypeId, member: TypeId) -> bool {
        type_queries::get_intersection_members(self.ctx.types, intersection_type)
            .map(|members| members.contains(&member))
            .unwrap_or(false)
    }

    /// Check if an intersection type contains only object types.
    ///
    /// Returns true if all intersection members are object types.
    pub fn is_object_intersection(&self, type_id: TypeId) -> bool {
        type_queries::get_intersection_members(self.ctx.types, type_id)
            .map(|members| members.iter().all(|&m| self.is_object_type(m)))
            .unwrap_or(false)
    }

    /// Check if an intersection type contains callable members.
    ///
    /// Returns true if any member has call signatures.
    pub fn intersection_has_callable(&self, type_id: TypeId) -> bool {
        type_queries::get_intersection_members(self.ctx.types, type_id)
            .map(|members| members.iter().any(|&m| self.has_call_signature(m)))
            .unwrap_or(false)
    }

    // =========================================================================
    // Intersection Type Compatibility
    // =========================================================================

    /// Check if a type is assignable to all members of an intersection.
    ///
    /// Returns true if the source type can be assigned to every member
    /// of the intersection type.
    pub fn is_assignable_to_all_intersection_members(
        &mut self,
        source: TypeId,
        intersection_type: TypeId,
    ) -> bool {
        type_queries::get_intersection_members(self.ctx.types, intersection_type)
            .map(|members| {
                members
                    .iter()
                    .all(|&member| self.is_assignable_to(source, member))
            })
            .unwrap_or(false)
    }

    /// Get the most restrictive type from an intersection.
    ///
    /// Returns the member that is most restrictive (has the most specific type).
    /// For simple cases, this might be the first non-primitive object type.
    pub fn get_most_restrictive_intersection_member(&self, type_id: TypeId) -> Option<TypeId> {
        type_queries::get_intersection_members(self.ctx.types, type_id).and_then(|members| {
            // Prefer object types over primitives
            members
                .iter()
                .find(|&&m| self.is_object_type(m))
                .copied()
                .or_else(|| members.first().copied())
        })
    }

    // =========================================================================
    // Intersection Type Filtering
    // =========================================================================

    /// Filter an intersection type to only include members satisfying a predicate.
    ///
    /// Returns a new intersection type with only the members that match the predicate.
    pub fn intersection_filter<F>(&self, intersection_type: TypeId, predicate: F) -> TypeId
    where
        F: Fn(TypeId) -> bool,
    {
        if let Some(members) =
            type_queries::get_intersection_members(self.ctx.types, intersection_type)
        {
            let filtered: Vec<TypeId> =
                members.iter().filter(|&&m| predicate(m)).copied().collect();

            if filtered.is_empty() {
                TypeId::UNKNOWN
            } else if filtered.len() == 1 {
                filtered[0]
            } else {
                self.ctx.types.intersection(filtered)
            }
        } else {
            intersection_type
        }
    }

    /// Get only the object type members from an intersection.
    ///
    /// Returns a new intersection type containing only object type members,
    /// or a single type if there's only one.
    pub fn get_object_intersection_members(&self, type_id: TypeId) -> TypeId {
        self.intersection_filter(type_id, |m| self.is_object_type(m))
    }

    // =========================================================================
    // Intersection Type Simplification
    // =========================================================================

    /// Simplify an intersection type by removing redundant members.
    ///
    /// Removes members that are supertypes of other members in the intersection.
    /// For example, `A & B` where `A extends B` simplifies to just `A`.
    pub fn simplify_intersection(&mut self, intersection_type: TypeId) -> TypeId {
        if let Some(members) =
            type_queries::get_intersection_members(self.ctx.types, intersection_type)
        {
            let mut simplified = Vec::new();

            for &member in members.iter() {
                // Check if this member is a subtype of any other member
                let is_redundant = members
                    .iter()
                    .any(|&other| member != other && self.is_assignable_to(member, other));

                if !is_redundant {
                    simplified.push(member);
                }
            }

            if simplified.is_empty() {
                TypeId::UNKNOWN
            } else if simplified.len() == 1 {
                simplified[0]
            } else {
                self.ctx.types.intersection(simplified)
            }
        } else {
            intersection_type
        }
    }

    /// Merge two types using intersection.
    ///
    /// If either type is an intersection, merges the members.
    /// Otherwise, creates a new intersection of both types.
    pub fn merge_as_intersection(&self, type1: TypeId, type2: TypeId) -> TypeId {
        let mut members = Vec::new();

        // Add members from type1 if it's an intersection
        if let Some(type1_members) = type_queries::get_intersection_members(self.ctx.types, type1) {
            members.extend(type1_members.iter().copied());
        } else {
            members.push(type1);
        }

        // Add members from type2 if it's an intersection
        if let Some(type2_members) = type_queries::get_intersection_members(self.ctx.types, type2) {
            members.extend(type2_members.iter().copied());
        } else {
            members.push(type2);
        }

        // Create intersection from all members
        if members.len() == 1 {
            members[0]
        } else {
            self.ctx.types.intersection(members)
        }
    }
}
