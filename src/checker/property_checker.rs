//! Property existence checking utilities.
//!
//! This module provides utilities for checking if properties exist on types,
//! consolidating property access logic to avoid duplicate TS2339 errors.

use crate::solver::{PropertyAccessResult, TypeId as SolverTypeId};
use crate::checker::types::TypeId;

/// Property checker for consolidating property existence checks.
///
/// This checker helps avoid emitting duplicate TS2339 errors by tracking
/// which property accesses have already been checked and reported.
pub struct PropertyChecker {
    /// Cache of already-checked property accesses to avoid duplicate errors
    checked_properties: std::collections::HashSet<(TypeId, String)>,
}

impl PropertyChecker {
    /// Create a new property checker.
    pub fn new() -> Self {
        Self {
            checked_properties: std::collections::HashSet::new(),
        }
    }

    /// Check if a property access has already been reported.
    ///
    /// This prevents emitting duplicate TS2339 errors for the same property access.
    pub fn has_been_checked(&self, object_type: TypeId, property_name: &str) -> bool {
        self.checked_properties.contains(&(object_type, property_name.to_string()))
    }

    /// Mark a property access as checked.
    ///
    /// Call this after emitting a TS2339 error to avoid reporting it again.
    pub fn mark_as_checked(&mut self, object_type: TypeId, property_name: &str) {
        self.checked_properties.insert((object_type, property_name.to_string()));
    }

    /// Check if we should emit a TS2339 error for a property access.
    ///
    /// Returns true if we should emit the error, false if it's already been reported.
    pub fn should_emit_error(&mut self, object_type: TypeId, property_name: &str) -> bool {
        if self.has_been_checked(object_type, property_name) {
            return false;
        }
        self.mark_as_checked(object_type, property_name);
        true
    }

    /// Evaluate a property access result and determine if an error should be emitted.
    ///
    /// This consolidates the logic for handling PropertyAccessResult and helps
    /// ensure consistent error reporting.
    pub fn evaluate_property_access(
        &mut self,
        result: &PropertyAccessResult,
        object_type: TypeId,
        property_name: &str,
        is_optional_chaining: bool,
    ) -> PropertyAccessEvaluation {
        match result {
            PropertyAccessResult::Success {
                type_id: prop_type,
                from_index_signature,
            } => PropertyAccessEvaluation::Found {
                property_type: TypeId(prop_type.0),
                from_index_signature: *from_index_signature,
            },
            PropertyAccessResult::PropertyNotFound { .. } => {
                // Check for optional chaining (?.) - suppress TS2339 error
                if is_optional_chaining {
                    return PropertyAccessEvaluation::OptionalUndefined;
                }

                // Don't emit TS2339 for private fields (starting with #) - they're handled elsewhere
                if property_name.starts_with('#') {
                    return PropertyAccessEvaluation::PrivateField;
                }

                // Check if we should emit the error (avoid duplicates)
                let should_emit = self.should_emit_error(object_type, property_name);
                PropertyAccessEvaluation::NotFound { should_emit }
            }
            PropertyAccessResult::PossiblyNullOrUndefined {
                property_type,
                cause,
            } => PropertyAccessEvaluation::PossiblyNullOrUndefined {
                property_type: property_type.map(|t| TypeId(t.0)),
                cause: TypeId(cause.0),
            },
            PropertyAccessResult::IsUnknown => PropertyAccessEvaluation::IsUnknown,
        }
    }

    /// Clear the checked properties cache.
    ///
    /// This can be useful between different checking phases or for testing.
    pub fn clear(&mut self) {
        self.checked_properties.clear();
    }
}

impl Default for PropertyChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of evaluating a property access.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyAccessEvaluation {
    /// Property was found successfully.
    Found {
        property_type: TypeId,
        from_index_signature: bool,
    },

    /// Property was not found.
    NotFound {
        /// Whether we should emit a TS2339 error (false if already reported).
        should_emit: bool,
    },

    /// Property not found, but accessed via optional chaining.
    OptionalUndefined,

    /// Private field access (handled by separate logic).
    PrivateField,

    /// Property exists but object is possibly null or undefined.
    PossiblyNullOrUndefined {
        property_type: Option<TypeId>,
        cause: TypeId,
    },

    /// Object is of type 'unknown'.
    IsUnknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_property_checker_deduplication() {
        let mut checker = PropertyChecker::new();
        let type_id = TypeId(1);

        // First check should emit
        assert!(checker.should_emit_error(type_id, "foo"));

        // Subsequent checks should not emit
        assert!(!checker.should_emit_error(type_id, "foo"));
        assert!(!checker.should_emit_error(type_id, "foo"));

        // Different property should emit
        assert!(checker.should_emit_error(type_id, "bar"));

        // Different type should emit
        let type_id2 = TypeId(2);
        assert!(checker.should_emit_error(type_id2, "foo"));
    }

    #[test]
    fn test_property_checker_clear() {
        let mut checker = PropertyChecker::new();
        let type_id = TypeId(1);

        assert!(checker.should_emit_error(type_id, "foo"));
        assert!(!checker.should_emit_error(type_id, "foo"));

        checker.clear();
        assert!(checker.should_emit_error(type_id, "foo"));
    }
}
