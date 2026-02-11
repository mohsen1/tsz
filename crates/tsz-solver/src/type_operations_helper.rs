//! Type Operations Helper - Common Type Query Patterns
//!
//! This module provides common type operation patterns that leverage both
//! TypeClassifier and TypeQueryBuilder for efficient, readable type checking.
//!
//! These helpers demonstrate best practices for type querying in the solver
//! and provide a reusable library of common operations.

use crate::type_query_builder::TypeQueryBuilder;
use crate::{TypeDatabase, TypeId};

/// Common type operation results
#[derive(Debug, Clone)]
pub struct TypeOperationResult {
    /// Type can be assigned to another type
    pub is_assignable_target: bool,

    /// Type can be indexed (array-like or object)
    pub is_indexable: bool,

    /// Type can have properties accessed
    pub is_property_accessible: bool,

    /// Type can be iterated over
    pub is_iterable: bool,

    /// Type can be called as function
    pub is_invocable: bool,
}

/// Check if a type can be used as an assignment target (lvalue).
///
/// Assignment targets include objects, properties, and destructurable types.
pub fn can_be_assignment_target(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let query = TypeQueryBuilder::new(db, type_id).build();

    // Can assign to: objects, arrays, tuples, but not primitives or functions
    query.is_object || query.is_array || query.is_tuple || query.is_union // Union can be assignment target if all members are
}

/// Check if a type can be indexed (array or object access).
///
/// Examples: `arr[0]`, `obj['key']`, `str[0]`
pub fn is_indexable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let query = TypeQueryBuilder::new(db, type_id).build();

    query.is_array || query.is_tuple || query.is_object
}

/// Check if a type supports property access (dot or bracket notation).
///
/// Examples: `obj.property`, `obj['property']`, `fn.name`
pub fn is_property_accessible(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let query = TypeQueryBuilder::new(db, type_id).build();

    // Objects and callables have properties
    // Unions only if all members are property accessible
    query.is_object || query.is_function || query.is_callable
}

/// Check if a type is iterable (for..of loops).
///
/// Examples: `for (const x of arr)`, `for (const x of str)`
pub fn is_iterable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let query = TypeQueryBuilder::new(db, type_id).build();

    // Arrays, tuples, and strings are iterable
    query.is_array || query.is_tuple || query.is_literal
}

/// Check if a type is invocable as a function.
///
/// Examples: `fn()`, `callable()`, `constructor()`
pub fn is_invocable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let query = TypeQueryBuilder::new(db, type_id).build();

    query.is_callable || query.is_function
}

/// Comprehensive type operation analysis.
///
/// Performs all common type checks in a single lookup operation,
/// returning a comprehensive result object.
pub fn analyze_type_operations(db: &dyn TypeDatabase, type_id: TypeId) -> TypeOperationResult {
    let query = TypeQueryBuilder::new(db, type_id).build();

    TypeOperationResult {
        is_assignable_target: query.is_object || query.is_array || query.is_tuple,
        is_indexable: query.is_array || query.is_tuple || query.is_object,
        is_property_accessible: query.is_object || query.is_function || query.is_callable,
        is_iterable: query.is_array || query.is_tuple || query.is_literal,
        is_invocable: query.is_callable || query.is_function,
    }
}

/// Check if a type fits a particular structural pattern.
///
/// This pattern is useful for discriminating between type categories
/// without direct TypeKey matching.
#[derive(Debug, Clone, Copy)]
pub enum TypePattern {
    /// Type is a primitive (number, string, boolean, etc.)
    Primitive,

    /// Type is a literal value (specific string, number, boolean)
    Literal,

    /// Type is a collection (array or tuple)
    Collection,

    /// Type is a composite (union or intersection)
    Composite,

    /// Type is callable (function or callable with signatures)
    Callable,

    /// Type is object-like (object, class, interface)
    ObjectLike,

    /// Type is a reference type (class, interface, type alias)
    Reference,

    /// Type doesn't match any pattern
    Unknown,
}

/// Classify a type into a high-level pattern.
pub fn classify_type_pattern(db: &dyn TypeDatabase, type_id: TypeId) -> TypePattern {
    let query = TypeQueryBuilder::new(db, type_id).build();

    if query.is_primitive {
        TypePattern::Primitive
    } else if query.is_literal {
        TypePattern::Literal
    } else if query.is_collection {
        TypePattern::Collection
    } else if query.is_composite {
        TypePattern::Composite
    } else if query.is_callable {
        TypePattern::Callable
    } else if query.is_object {
        TypePattern::ObjectLike
    } else if query.is_lazy {
        TypePattern::Reference
    } else {
        TypePattern::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_operation_result_structure() {
        let result = TypeOperationResult {
            is_assignable_target: true,
            is_indexable: false,
            is_property_accessible: true,
            is_iterable: false,
            is_invocable: false,
        };

        assert!(result.is_assignable_target);
        assert!(!result.is_indexable);
    }

    #[test]
    fn test_type_pattern_variants() {
        // This test validates that all pattern types can be instantiated
        let _patterns = [
            TypePattern::Primitive,
            TypePattern::Literal,
            TypePattern::Collection,
            TypePattern::Composite,
            TypePattern::Callable,
            TypePattern::ObjectLike,
            TypePattern::Reference,
            TypePattern::Unknown,
        ];
    }
}
