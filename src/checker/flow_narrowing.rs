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

use crate::checker::state::CheckerState;
use crate::parser::NodeIndex;
use crate::solver::TypeId;

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
        use crate::solver::TypeKey;

        // Only union types can have discriminant properties
        let Some(TypeKey::Union(list_id)) = self.ctx.types.lookup(type_id) else {
            return false;
        };

        let members = self.ctx.types.type_list(list_id);
        if members.is_empty() {
            return false;
        }

        // Get properties from the first member
        let first_member_props = match self.ctx.types.lookup(members[0]) {
            Some(TypeKey::Object(shape_id)) => {
                let shape = self.ctx.types.object_shape(shape_id);
                shape.properties.clone()
            }
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.ctx.types.object_shape(shape_id);
                shape.properties.clone()
            }
            _ => return false,
        };

        // Check each property to see if it's a discriminant
        for prop in &first_member_props {
            // Check if all members have this property with a literal type
            let is_discriminant =
                members
                    .iter()
                    .all(|&member_id| match self.ctx.types.lookup(member_id) {
                        Some(TypeKey::Object(shape_id)) => {
                            let shape = self.ctx.types.object_shape(shape_id);
                            shape.properties.iter().any(|p| {
                                p.name == prop.name
                                    && matches!(
                                        self.ctx.types.lookup(p.type_id),
                                        Some(TypeKey::Literal(_))
                                    )
                            })
                        }
                        Some(TypeKey::ObjectWithIndex(shape_id)) => {
                            let shape = self.ctx.types.object_shape(shape_id);
                            shape.properties.iter().any(|p| {
                                p.name == prop.name
                                    && matches!(
                                        self.ctx.types.lookup(p.type_id),
                                        Some(TypeKey::Literal(_))
                                    )
                            })
                        }
                        _ => false,
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
        if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
            return true;
        }

        // Check if it's a union containing null or undefined
        self.type_contains_nullish(type_id)
    }

    /// Check if a type (possibly a union) contains null or undefined.
    ///
    /// Recursively checks union members for null or undefined types.
    pub fn type_contains_nullish(&self, type_id: TypeId) -> bool {
        use crate::solver::TypeKey;

        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Union(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                members.iter().any(|&m| {
                    m == TypeId::NULL || m == TypeId::UNDEFINED || self.type_contains_nullish(m)
                })
            }
            _ => false,
        }
    }

    /// Remove null and undefined from a type (non-null assertion).
    ///
    /// For `T | null | undefined`, returns `T`.
    /// For `T` where T is not nullish, returns `T` unchanged.
    pub fn non_null_type(&self, type_id: TypeId) -> TypeId {
        use crate::solver::TypeKey;

        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Union(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                let non_nullish: Vec<TypeId> = members
                    .iter()
                    .filter(|&&m| m != TypeId::NULL && m != TypeId::UNDEFINED)
                    .copied()
                    .collect();

                if non_nullish.is_empty() {
                    TypeId::NEVER
                } else if non_nullish.len() == 1 {
                    non_nullish[0]
                } else {
                    self.ctx.types.union(non_nullish)
                }
            }
            _ => type_id,
        }
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

        // If the value type is 'any' or 'unknown', we can't be sure
        if value_type == TypeId::ANY || value_type == TypeId::UNKNOWN {
            return false;
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
