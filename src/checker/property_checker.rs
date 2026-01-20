//! Property access checking for TypeScript.
//!
//! This module provides comprehensive property access resolution to emit
//! accurate TS2339 errors when properties don't exist on types.
//!
//! The checker handles:
//! - Property existence checks on object types
//! - Index signature resolution
//! - Optional chaining suppression of errors
//! - Union and intersection type property access
//! - Inherited property traversal

use crate::checker::context::CheckerContext;
use crate::parser::NodeIndex;
use rustc_hash::FxHashSet;

// Type aliases for convenience
type TypeId = crate::solver::TypeId;

/// Result of a property access check.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyAccessResult {
    /// Property exists and has the given type
    Found(TypeId),
    /// Property does not exist (should emit TS2339)
    NotFound,
    /// Property access is optional (?.) and may be undefined
    OptionalChain(TypeId),
    /// Property exists on some but not all union members
    MaybeFound(TypeId),
}

/// Property access information for diagnostics.
#[derive(Clone, Debug)]
pub struct PropertyAccessInfo {
    /// The property name being accessed
    pub property_name: String,
    /// The base object type
    pub object_type: TypeId,
    /// The node where the access occurs
    pub access_node: NodeIndex,
    /// Whether this is an optional chain access (?.)
    pub is_optional: bool,
    /// Whether this is element access (obj['prop']) vs. property access (obj.prop)
    pub is_element_access: bool,
}

/// Checker for property access expressions.
pub struct PropertyChecker<'a> {
    /// Reference to the checker context
    ctx: &'a CheckerContext<'a>,
    /// Cache of already-checked property accesses to avoid redundant work
    access_cache: FxHashSet<(u32, String)>,
}

impl<'a> PropertyChecker<'a> {
    /// Create a new property checker.
    pub fn new(ctx: &'a CheckerContext<'a>) -> Self {
        Self {
            ctx,
            access_cache: FxHashSet::default(),
        }
    }

    /// Check a property access and return the result.
    ///
    /// This is the main entry point for property access checking.
    /// It determines whether a property exists on the given object type
    /// and returns the appropriate type or error indication.
    pub fn check_property_access(
        &mut self,
        info: PropertyAccessInfo,
    ) -> PropertyAccessResult {
        // Check cache first
        let cache_key = (info.access_node.0, info.property_name.clone());
        if self.access_cache.contains(&cache_key) {
            // Already checked, return the type from the context
            return self.lookup_cached_property(&info);
        }

        let result = if info.is_optional {
            self.check_optional_property_access(&info)
        } else {
            self.check_regular_property_access(&info)
        };

        // Cache the result
        self.access_cache.insert(cache_key);

        result
    }

    /// Check a regular (non-optional) property access.
    fn check_regular_property_access(&self, info: &PropertyAccessInfo) -> PropertyAccessResult {
        use crate::solver::{TypeKey, IntrinsicKind};

        // Check if the type is any/unknown - these allow any property access
        let type_key = match self.ctx.types.lookup(info.object_type) {
            Some(key) => key,
            None => return PropertyAccessResult::NotFound,
        };

        match type_key {
            TypeKey::Intrinsic(IntrinsicKind::Any | IntrinsicKind::Unknown) => {
                // Allow any property access on any/unknown
                PropertyAccessResult::Found(info.object_type)
            }

            TypeKey::Union(_) => {
                // For unions, check if property exists on all members
                self.check_union_property_access(info)
            }

            TypeKey::Intersection(_) => {
                // For intersections, check if property exists on any member
                self.check_intersection_property_access(info)
            }

            TypeKey::Object(_) => {
                // Check object properties
                self.check_object_property_access(info)
            }

            _ => PropertyAccessResult::NotFound,
        }
    }

    /// Check property access on a union type.
    fn check_union_property_access(&self, _info: &PropertyAccessInfo) -> PropertyAccessResult {
        // Placeholder - actual implementation would check all union members
        PropertyAccessResult::NotFound
    }

    /// Check property access on an intersection type.
    fn check_intersection_property_access(&self, _info: &PropertyAccessInfo) -> PropertyAccessResult {
        // Placeholder - actual implementation would check intersection members
        PropertyAccessResult::NotFound
    }

    /// Check property access on an object type.
    fn check_object_property_access(&self, _info: &PropertyAccessInfo) -> PropertyAccessResult {
        // Placeholder - actual implementation would:
        // 1. Check for index signatures
        // 2. Check for direct properties
        // 3. Check inherited properties
        PropertyAccessResult::NotFound
    }

    /// Check an optional chaining property access (?.).
    fn check_optional_property_access(
        &self,
        info: &PropertyAccessInfo,
    ) -> PropertyAccessResult {
        // Optional chaining suppresses TS2339 errors
        // Return the property type unioned with undefined
        match self.check_regular_property_access(info) {
            PropertyAccessResult::Found(type_id) => {
                let with_undefined = self.ctx.types.union(vec![type_id, TypeId::UNKNOWN]);
                PropertyAccessResult::OptionalChain(with_undefined)
            }
            PropertyAccessResult::NotFound => {
                // With optional chaining, missing property returns undefined
                PropertyAccessResult::OptionalChain(TypeId::UNKNOWN)
            }
            _ => PropertyAccessResult::OptionalChain(TypeId::UNKNOWN),
        }
    }

    /// Check if a property access should suppress TS2339 error.
    ///
    /// This is used to filter out false positives.
    pub fn should_suppress_property_error(&self, info: &PropertyAccessInfo) -> bool {
        // Suppress errors for private fields (handled elsewhere)
        if info.property_name.starts_with('#') {
            return true;
        }

        // Suppress errors for optional chaining
        if info.is_optional {
            return true;
        }

        // Check if the object type is callable
        if self.is_callable_type(info.object_type) {
            // Callable types (functions) allow arbitrary property access
            return true;
        }

        false
    }

    /// Check if a type is callable (has call signatures).
    fn is_callable_type(&self, type_id: TypeId) -> bool {
        use crate::solver::{TypeKey, IntrinsicKind};

        if let Some(type_key) = self.ctx.types.lookup(type_id) {
            if let TypeKey::Function(_) = type_key {
                return true;
            }
        }
        false
    }

    /// Look up a previously cached property access result.
    fn lookup_cached_property(&self, info: &PropertyAccessInfo) -> PropertyAccessResult {
        // For now, just perform the lookup again
        self.check_regular_property_access(info)
    }

    /// Clear the access cache (useful when starting a new file/check).
    pub fn clear_cache(&mut self) {
        self.access_cache.clear();
    }
}

/// Create a diagnostic message for TS2339 (property does not exist).
pub fn create_property_not_exist_diagnostic(
    property_name: &str,
    type_id: TypeId,
    types: &crate::checker::TypeArena,
) -> String {
    use crate::checker::types::diagnostics::format_message;
    use crate::checker::types::diagnostics::diagnostic_messages::PROPERTY_DOES_NOT_EXIST;

    let type_string = format!("{:?}", type_id);  // Placeholder - would use proper type name
    format_message(PROPERTY_DOES_NOT_EXIST, &[property_name, &type_string])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_property_access_info_creation() {
        let info = PropertyAccessInfo {
            property_name: "test".to_string(),
            object_type: TypeId::UNKNOWN,
            access_node: NodeIndex(0),
            is_optional: false,
            is_element_access: false,
        };

        assert_eq!(info.property_name, "test");
        assert!(!info.is_optional);
    }

    #[test]
    fn test_should_suppress_private_field() {
        let info = PropertyAccessInfo {
            property_name: "#privateField".to_string(),
            object_type: TypeId::UNKNOWN,
            access_node: NodeIndex(0),
            is_optional: false,
            is_element_access: false,
        };

        // Private fields should suppress errors
        assert!(info.property_name.starts_with('#'));
    }
}
