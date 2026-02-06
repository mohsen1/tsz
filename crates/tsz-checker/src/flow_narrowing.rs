//! Flow Narrowing Utilities Module
//!
//! This module contains flow narrowing and type analysis utility methods
//! for CheckerState as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Discriminant property checking (union type narrowing)
//! - Type predicate evaluation
//! - Nullish type analysis
//! - Flow-sensitive type queries
//!
//! This module extends CheckerState with utilities for type narrowing
//! operations, providing cleaner APIs for flow-sensitive type checking.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver as solver_narrowing;
use tsz_solver::TypeId;
use tsz_solver::type_queries::{
    LiteralTypeKind, UnionMembersKind, classify_for_union_members, classify_literal_type,
    get_object_shape,
};

// =============================================================================
// Flow Narrowing Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Discriminant Property Checking
    // =========================================================================

    /// Check if a type has discriminant properties for union narrowing.
    ///
    /// Discriminant properties allow TypeScript to narrow union types based on
    /// property checks. For example:
    /// ```typescript
    /// type Shape = { kind: 'circle', radius: number } | { kind: 'square', side: number };
    /// function area(shape: Shape) {
    ///   if (shape.kind === 'circle') { ... }  // narrowed to { kind: 'circle', radius: number }
    /// }
    /// ```
    ///
    /// A type has discriminant properties if:
    /// - It's a union where all members have a common property name
    /// - The common property has literal types in all members
    pub fn has_discriminant_properties(&self, type_id: TypeId) -> bool {
        // Only union types can have discriminant properties
        let UnionMembersKind::Union(members) = classify_for_union_members(self.ctx.types, type_id)
        else {
            return false;
        };

        if members.is_empty() {
            return false;
        }

        // Get properties from the first member
        let Some(first_shape) = get_object_shape(self.ctx.types, members[0]) else {
            return false;
        };
        let first_member_props = first_shape.properties.clone();

        // Check each property to see if it's a discriminant
        for prop in &first_member_props {
            // Check if all members have this property with a literal type
            let is_discriminant = members.iter().all(|&member_id| {
                if let Some(shape) = get_object_shape(self.ctx.types, member_id) {
                    shape.properties.iter().any(|p| {
                        p.name == prop.name
                            && !matches!(
                                classify_literal_type(self.ctx.types, p.type_id),
                                LiteralTypeKind::NotLiteral
                            )
                    })
                } else {
                    false
                }
            });

            if is_discriminant {
                return true;
            }
        }

        false
    }

    // =========================================================================
    // Nullish Type Analysis
    // =========================================================================

    /// Check if a type is nullish (null or undefined or union containing them).
    ///
    /// Returns true if:
    /// - Type is exactly `null`
    /// - Type is exactly `undefined`
    /// - Type is a union containing `null` or `undefined`
    pub fn is_nullish_type(&self, type_id: TypeId) -> bool {
        solver_narrowing::is_nullish_type(self.ctx.types.as_type_database(), type_id)
    }

    /// Check if a type (possibly a union) contains null or undefined.
    ///
    /// Recursively checks union members for null or undefined types.
    pub fn type_contains_nullish(&self, type_id: TypeId) -> bool {
        solver_narrowing::type_contains_nullish(self.ctx.types.as_type_database(), type_id)
    }

    /// Remove null and undefined from a type (non-null assertion).
    ///
    /// For `T | null | undefined`, returns `T`.
    /// For `T` where T is not nullish, returns `T` unchanged.
    pub fn non_null_type(&self, type_id: TypeId) -> TypeId {
        solver_narrowing::remove_nullish(self.ctx.types.as_type_database(), type_id)
    }

    // =========================================================================
    // Type Predicate Helpers
    // =========================================================================

    /// Check if a type satisfies a type predicate.
    ///
    /// Type predicates are used in user-defined type guards:
    /// ```typescript
    /// function isString(val: unknown): val is string {
    ///   return typeof val === 'string';
    /// }
    /// ```
    ///
    /// This function checks if a given type could satisfy a predicate
    /// that narrows to a specific target type.
    pub fn satisfies_type_predicate(&mut self, value_type: TypeId, predicate_type: TypeId) -> bool {
        // If the predicate type is 'any' or 'unknown', any value satisfies it
        if predicate_type == TypeId::ANY || predicate_type == TypeId::UNKNOWN {
            return true;
        }

        // For 'unknown' value type, we need to check if it could satisfy the predicate
        // Type guards like `typeof x === 'string'` should narrow unknown to the predicate type
        // This allows proper narrowing in user-defined type guards
        if value_type == TypeId::UNKNOWN {
            // unknown can be narrowed to any specific type via type guards
            return true;
        }

        // If the value type is 'any', we can't be sure - but allow it for compatibility
        if value_type == TypeId::ANY {
            return true;
        }

        // Check if value_type is assignable to predicate_type
        self.is_assignable_to(value_type, predicate_type)
    }

    /// Get the narrowed type of a variable with a fallback.
    ///
    /// Returns the type of a node, or a fallback type if the computed type is ERROR.
    pub fn get_narrowed_type_or(&mut self, idx: NodeIndex, fallback: TypeId) -> TypeId {
        let ty = self.get_type_of_node(idx);
        if ty == TypeId::ERROR { fallback } else { ty }
    }
}
