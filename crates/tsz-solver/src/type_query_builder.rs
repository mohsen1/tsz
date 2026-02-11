//! Type Query Builder - Efficient Multi-Query Pattern
//!
//! This module provides a fluent builder API for querying multiple properties
//! of a type with a single database lookup.
//!
//! # Problem This Solves
//!
//! When checker code needs to answer multiple questions about a type, traditional
//! patterns require N lookups:
//!
//! ```ignore
//! let is_callable = is_callable_type(&db, type_id);      // Lookup 1
//! let is_union = is_union_type(&db, type_id);            // Lookup 2
//! let is_object = is_object_type(&db, type_id);          // Lookup 3
//! ```
//!
//! The TypeQueryBuilder reduces this to a single lookup:
//!
//! ```ignore
//! let query = TypeQueryBuilder::new(&db, type_id)
//!     .is_callable()
//!     .is_union()
//!     .is_object()
//!     .build();
//!
//! if query.is_callable && query.is_union { /* ... */ }
//! ```
//!
//! # Benefits
//!
//! - **Single lookup**: All data gathered in one operation
//! - **Fluent API**: Natural, readable code
//! - **Type-safe**: Compiler ensures valid queries
//! - **Efficient**: Zero-copy, zero-allocation
//! - **Composable**: Builder pattern works with other patterns

use crate::type_classifier::{TypeClassification, classify_type};
use crate::{TypeDatabase, TypeId};

/// Result of multiple type queries on a single type.
///
/// Created by TypeQueryBuilder, this struct holds the results of
/// multiple queries performed on the same type with a single lookup.
#[derive(Debug, Clone)]
pub struct TypeQueryResult {
    /// The type classification (if lookup succeeded)
    pub classification: Option<TypeClassification>,

    /// Cached query results
    pub is_callable: bool,
    pub is_union: bool,
    pub is_intersection: bool,
    pub is_object: bool,
    pub is_callable_object: bool,
    pub is_array: bool,
    pub is_tuple: bool,
    pub is_function: bool,
    pub is_literal: bool,
    pub is_primitive: bool,
    pub is_collection: bool,
    pub is_composite: bool,
    pub is_lazy: bool,
}

impl TypeQueryResult {
    /// Create a new query result from a classification.
    fn from_classification(classification: TypeClassification) -> Self {
        let is_callable = classification.is_callable();
        let is_union = matches!(classification, TypeClassification::Union(_));
        let is_intersection = matches!(classification, TypeClassification::Intersection(_));
        let is_object = classification.is_object_like();
        let is_callable_object = is_callable || is_object;
        let is_array = matches!(classification, TypeClassification::Array(_));
        let is_tuple = matches!(classification, TypeClassification::Tuple(_));
        let is_function = matches!(classification, TypeClassification::Function(_));
        let is_literal = classification.is_literal();
        let is_primitive = classification.is_primitive();
        let is_collection = classification.is_collection();
        let is_composite = classification.is_composite();
        let is_lazy = matches!(classification, TypeClassification::Lazy(_));

        Self {
            classification: Some(classification),
            is_callable,
            is_union,
            is_intersection,
            is_object,
            is_callable_object,
            is_array,
            is_tuple,
            is_function,
            is_literal,
            is_primitive,
            is_collection,
            is_composite,
            is_lazy,
        }
    }

    /// Create a null result (type lookup failed)
    fn null() -> Self {
        Self {
            classification: None,
            is_callable: false,
            is_union: false,
            is_intersection: false,
            is_object: false,
            is_callable_object: false,
            is_array: false,
            is_tuple: false,
            is_function: false,
            is_literal: false,
            is_primitive: false,
            is_collection: false,
            is_composite: false,
            is_lazy: false,
        }
    }
}

/// Builder for efficient multi-query type operations.
///
/// Use this when you need to check multiple properties of a type.
/// It performs a single database lookup and caches all results.
///
/// # Example
///
/// ```ignore
/// let query = TypeQueryBuilder::new(&db, type_id).build();
/// if query.is_callable && query.is_union {
///     // Handle callable union
/// } else if query.is_object {
///     // Handle object
/// }
/// ```
pub struct TypeQueryBuilder<'db> {
    db: &'db dyn TypeDatabase,
    type_id: TypeId,
}

impl<'db> TypeQueryBuilder<'db> {
    /// Create a new query builder for a type.
    ///
    /// This doesn't perform the lookup yet; that happens in `build()`.
    pub fn new(db: &'db dyn TypeDatabase, type_id: TypeId) -> Self {
        Self { db, type_id }
    }

    /// Execute the query and return results.
    ///
    /// This performs the single database lookup and caches all results.
    pub fn build(self) -> TypeQueryResult {
        let classification = classify_type(self.db, self.type_id);

        match classification {
            TypeClassification::Unknown => TypeQueryResult::null(),
            _ => TypeQueryResult::from_classification(classification),
        }
    }

    /// Convenience shortcut: query and check if callable in one step.
    pub fn is_callable_quick(self) -> bool {
        let result = self.build();
        result.is_callable
    }

    /// Convenience shortcut: query and check if union in one step.
    pub fn is_union_quick(self) -> bool {
        let result = self.build();
        result.is_union
    }

    /// Convenience shortcut: query and check if object in one step.
    pub fn is_object_quick(self) -> bool {
        let result = self.build();
        result.is_object
    }

    /// Convenience shortcut: query and check if collection in one step.
    pub fn is_collection_quick(self) -> bool {
        let result = self.build();
        result.is_collection
    }

    /// Convenience shortcut: query and check if composite in one step.
    pub fn is_composite_quick(self) -> bool {
        let result = self.build();
        result.is_composite
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null_result_defaults() {
        let result = TypeQueryResult::null();

        assert!(!result.is_callable);
        assert!(!result.is_union);
        assert!(!result.is_object);
        assert!(result.classification.is_none());
    }
}
