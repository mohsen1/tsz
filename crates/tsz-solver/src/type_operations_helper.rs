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

// ============================================================================
// Visitor-Based Type Extraction (Phase 3: Visitor Consolidation)
// ============================================================================

use crate::type_classification_visitor::TypeClassificationVisitor;

/// Extract array element type if type is an array.
///
/// Returns the element type if type is an array, otherwise returns the original type_id.
///
/// # Example
/// ```ignore
/// // Type is number[]
/// let elem = extract_array_element(db, array_type);
/// // elem == number
/// ```
///
/// This replaces the pattern:
/// ```ignore
/// match db.lookup(type_id) {
///     Some(TypeKey::Array(elem)) => elem,
///     _ => type_id,
/// }
/// ```
pub fn extract_array_element(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let mut visitor = TypeClassificationVisitor::new(db, type_id);

    // Use visitor to check if type is array and extract element
    let mut result = type_id;
    visitor.visit_array(|elem| {
        result = elem;
    });
    result
}

/// Extract tuple elements if type is a tuple.
///
/// Returns the tuple elements if type is a tuple, otherwise returns None.
///
/// This demonstrates visitor pattern for structured type extraction.
pub fn extract_tuple_elements(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::TupleListId> {
    let mut visitor = TypeClassificationVisitor::new(db, type_id);

    let mut result = None;
    visitor.visit_tuple(|elements| {
        result = Some(elements);
    });
    result
}

/// Check if a type is a union and extract its members if so.
///
/// Returns the union member list if type is a union, otherwise returns None.
///
/// # Example
/// ```ignore
/// // Type is string | number
/// let members = extract_union_members(db, union_type);
/// // members == Some(TypeListId for [string, number])
/// ```
pub fn extract_union_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::TypeListId> {
    let mut visitor = TypeClassificationVisitor::new(db, type_id);

    let mut result = None;
    visitor.visit_union(|members| {
        result = Some(members);
    });
    result
}

/// Check if a type is an intersection and extract its members if so.
///
/// Returns the intersection member list if type is an intersection, otherwise returns None.
pub fn extract_intersection_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::TypeListId> {
    let mut visitor = TypeClassificationVisitor::new(db, type_id);

    let mut result = None;
    visitor.visit_intersection(|members| {
        result = Some(members);
    });
    result
}

/// Check if a type is an object and extract its shape if so.
///
/// Returns the object shape if type is an object, otherwise returns None.
pub fn extract_object_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::ObjectShapeId> {
    let mut visitor = TypeClassificationVisitor::new(db, type_id);

    let mut result = None;
    visitor.visit_object(|shape| {
        result = Some(shape);
    });
    result
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
