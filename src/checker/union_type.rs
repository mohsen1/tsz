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

use crate::checker::state::CheckerState;
use crate::solver::TypeId;

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
        use crate::solver::TypeKey;

        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Union(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                members.to_vec()
            }
            _ => Vec::new(),
        }
    }

    /// Get the number of members in a union type.
    ///
    /// Returns 0 if the type is not a union.
    pub fn union_member_count(&self, type_id: TypeId) -> usize {
        use crate::solver::TypeKey;

        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Union(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                members.len()
            }
            _ => 0,
        }
    }

    // =========================================================================
    // Union Type Analysis
    // =========================================================================

    /// Check if a union type contains only primitive types.
    ///
    /// Returns true if all union members are primitive types
    /// (string, number, boolean, null, undefined, etc.).
    pub fn is_primitive_union(&self, type_id: TypeId) -> bool {
        use crate::solver::TypeKey;

        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Union(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                members.iter().all(|&m| self.is_primitive_type(m))
            }
            _ => false,
        }
    }

    /// Check if a union type contains a specific type.
    ///
    /// Calls the primary union_contains implementation in type_checking.rs.
    pub fn union_has_type(&self, union_type: TypeId, target: TypeId) -> bool {
        // Use the primary implementation defined in type_checking.rs
        use crate::solver::TypeKey;

        if let Some(TypeKey::Union(list_id)) = self.ctx.types.lookup(union_type) {
            let members = self.ctx.types.type_list(list_id);
            members.contains(&target)
        } else {
            false
        }
    }

    /// Check if a union type contains null or undefined.
    ///
    /// Returns true if the union includes null or undefined as a member.
    pub fn union_is_nullable(&self, type_id: TypeId) -> bool {
        use crate::solver::TypeKey;

        if let Some(TypeKey::Union(list_id)) = self.ctx.types.lookup(type_id) {
            let members = self.ctx.types.type_list(list_id);
            members
                .iter()
                .any(|&m| m == TypeId::NULL || m == TypeId::UNDEFINED)
        } else {
            false
        }
    }

    // =========================================================================
    // Union Type Manipulation
    // =========================================================================

    /// Remove a member from a union type.
    ///
    /// Returns a new union type without the specified member.
    /// If the result would be a single type, returns that type.
    pub fn union_remove_member(&self, union_type: TypeId, member_to_remove: TypeId) -> TypeId {
        use crate::solver::TypeKey;

        match self.ctx.types.lookup(union_type) {
            Some(TypeKey::Union(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
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
            }
            _ => union_type,
        }
    }

    /// Filter a union type to only include members satisfying a predicate.
    ///
    /// Returns a new union type with only the members that match the predicate.
    pub fn union_filter<F>(&self, union_type: TypeId, predicate: F) -> TypeId
    where
        F: Fn(TypeId) -> bool,
    {
        use crate::solver::TypeKey;

        match self.ctx.types.lookup(union_type) {
            Some(TypeKey::Union(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                let filtered: Vec<TypeId> =
                    members.iter().filter(|&&m| predicate(m)).copied().collect();

                if filtered.is_empty() {
                    TypeId::NEVER
                } else if filtered.len() == 1 {
                    filtered[0]
                } else {
                    self.ctx.types.union(filtered)
                }
            }
            _ => union_type,
        }
    }

    // =========================================================================
    // Union Type Compatibility
    // =========================================================================

    /// Check if all members of a union are assignable to a target type.
    ///
    /// Returns true if every member of the union can be assigned to the target.
    pub fn union_all_assignable_to(&mut self, union_type: TypeId, target: TypeId) -> bool {
        use crate::solver::TypeKey;

        if let Some(TypeKey::Union(list_id)) = self.ctx.types.lookup(union_type) {
            let members = self.ctx.types.type_list(list_id);
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
        use crate::solver::TypeKey;

        if let Some(TypeKey::Union(list_id)) = self.ctx.types.lookup(union_type) {
            let members = self.ctx.types.type_list(list_id);
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
        use crate::solver::TypeKey;

        match self.ctx.types.lookup(union_type) {
            Some(TypeKey::Union(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
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
            }
            _ => union_type,
        }
    }

    /// Simplify a union type by removing redundant members.
    ///
    /// Removes members that are assignable to other members in the union.
    /// For example, `string | "hello"` simplifies to just `string`.
    pub fn simplify_union(&mut self, union_type: TypeId) -> TypeId {
        use crate::solver::TypeKey;

        if let Some(TypeKey::Union(list_id)) = self.ctx.types.lookup(union_type) {
            let members = self.ctx.types.type_list(list_id);
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
