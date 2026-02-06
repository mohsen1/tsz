//! Union Type Utilities Module
//!
//! This module contains union type utility methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Union member extraction and manipulation
//! - Union type filtering and transformation
//! - Union type compatibility checking
//! - Union type simplification
//!
//! This module extends CheckerState with utilities for union type
//! operations, providing cleaner APIs for union type checking.

use crate::state::CheckerState;
use tsz_solver::TypeId;
use tsz_solver::type_queries;

// =============================================================================
// Union Type Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Union Member Extraction
    // =========================================================================

    /// Get the members of a union type.
    ///
    /// Returns a vector of TypeIds representing all members of the union.
    /// Returns an empty vec if the type is not a union.
    pub fn get_union_members(&self, type_id: TypeId) -> Vec<TypeId> {
        type_queries::get_union_members(self.ctx.types, type_id).unwrap_or_default()
    }

    /// Get the number of members in a union type.
    ///
    /// Returns 0 if the type is not a union.
    pub fn union_member_count(&self, type_id: TypeId) -> usize {
        type_queries::get_union_members(self.ctx.types, type_id)
            .map(|members| members.len())
            .unwrap_or(0)
    }

    // =========================================================================
    // Union Type Analysis
    // =========================================================================

    /// Check if a union type contains only primitive types.
    ///
    /// Returns true if all union members are primitive types
    /// (string, number, boolean, null, undefined, etc.).
    pub fn is_primitive_union(&self, type_id: TypeId) -> bool {
        type_queries::get_union_members(self.ctx.types, type_id)
            .map(|members| members.iter().all(|&m| self.is_primitive_type(m)))
            .unwrap_or(false)
    }

    /// Check if a union type contains a specific type.
    ///
    /// Calls the primary union_contains implementation in type_checking.rs.
    pub fn union_has_type(&self, union_type: TypeId, target: TypeId) -> bool {
        type_queries::get_union_members(self.ctx.types, union_type)
            .map(|members| members.contains(&target))
            .unwrap_or(false)
    }

    // =========================================================================
    // Union Type Manipulation
    // =========================================================================

    /// Remove a member from a union type.
    ///
    /// Returns a new union type without the specified member.
    /// If the result would be a single type, returns that type.
    pub fn union_remove_member(&self, union_type: TypeId, member_to_remove: TypeId) -> TypeId {
        if let Some(members) = type_queries::get_union_members(self.ctx.types, union_type) {
            let filtered: Vec<TypeId> = members
                .iter()
                .filter(|&&m| m != member_to_remove)
                .copied()
                .collect();

            if filtered.is_empty() {
                TypeId::NEVER
            } else if filtered.len() == 1 {
                filtered[0]
            } else {
                self.ctx.types.union(filtered)
            }
        } else {
            union_type
        }
    }

    /// Filter a union type to only include members satisfying a predicate.
    ///
    /// Returns a new union type with only the members that match the predicate.
    pub fn union_filter<F>(&self, union_type: TypeId, predicate: F) -> TypeId
    where
        F: Fn(TypeId) -> bool,
    {
        if let Some(members) = type_queries::get_union_members(self.ctx.types, union_type) {
            let filtered: Vec<TypeId> =
                members.iter().filter(|&&m| predicate(m)).copied().collect();

            if filtered.is_empty() {
                TypeId::NEVER
            } else if filtered.len() == 1 {
                filtered[0]
            } else {
                self.ctx.types.union(filtered)
            }
        } else {
            union_type
        }
    }

    // =========================================================================
    // Union Type Compatibility
    // =========================================================================

    /// Check if all members of a union are assignable to a target type.
    ///
    /// Returns true if every member of the union can be assigned to the target.
    pub fn union_all_assignable_to(&mut self, union_type: TypeId, target: TypeId) -> bool {
        if let Some(members) = type_queries::get_union_members(self.ctx.types, union_type) {
            members
                .iter()
                .all(|&member| self.is_assignable_to(member, target))
        } else {
            // Non-union types are assignable to themselves
            union_type == target
        }
    }

    /// Check if any member of a union is assignable to a target type.
    ///
    /// Returns true if at least one member of the union can be assigned to the target.
    pub fn union_any_assignable_to(&mut self, union_type: TypeId, target: TypeId) -> bool {
        if let Some(members) = type_queries::get_union_members(self.ctx.types, union_type) {
            members
                .iter()
                .any(|&member| self.is_assignable_to(member, target))
        } else {
            // Non-union types
            self.is_assignable_to(union_type, target)
        }
    }

    /// Get the most specific common type of a union's members.
    ///
    /// This returns a type that all members can be assigned to,
    /// preferring more specific types over generic ones.
    pub fn union_common_type(&mut self, union_type: TypeId) -> TypeId {
        if let Some(members) = type_queries::get_union_members(self.ctx.types, union_type) {
            if members.is_empty() {
                return TypeId::NEVER;
            }
            if members.len() == 1 {
                return members[0];
            }

            // Try to find a common type
            // Start with the first member and see if all others are assignable to it
            let first = members[0];
            if members
                .iter()
                .skip(1)
                .all(|&m| self.is_assignable_to(m, first))
            {
                return first;
            }

            // Try string as a common type for heterogeneous unions
            if members
                .iter()
                .all(|&m| self.is_assignable_to(m, TypeId::STRING))
            {
                return TypeId::STRING;
            }

            // Fall back to ANY for unions with no common type
            TypeId::ANY
        } else {
            union_type
        }
    }

    /// Simplify a union type by removing redundant members.
    ///
    /// Removes members that are assignable to other members in the union.
    /// For example, `string | "hello"` simplifies to just `string`.
    pub fn simplify_union(&mut self, union_type: TypeId) -> TypeId {
        if let Some(members) = type_queries::get_union_members(self.ctx.types, union_type) {
            let mut simplified = Vec::new();

            for &member in members.iter() {
                // Check if this member is assignable to any other member
                let is_redundant = members
                    .iter()
                    .any(|&other| member != other && self.is_assignable_to(member, other));

                if !is_redundant {
                    simplified.push(member);
                }
            }

            if simplified.is_empty() {
                TypeId::NEVER
            } else if simplified.len() == 1 {
                simplified[0]
            } else {
                self.ctx.types.union(simplified)
            }
        } else {
            union_type
        }
    }
}
