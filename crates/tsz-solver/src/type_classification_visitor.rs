//! Type Classification Visitor - Systematic Type Traversal
//!
//! This module provides a visitor pattern implementation for systematic traversal
//! of type structures. It demonstrates how to apply the visitor pattern to
//! eliminate direct TypeKey matching and create more maintainable type handling code.
//!
//! # Problem This Solves
//!
//! Direct TypeKey matching scattered throughout the codebase:
//!
//! ```ignore
//! // ANTI-PATTERN: Direct TypeKey matching
//! match db.lookup(type_id) {
//!     TypeKey::Intrinsic(kind) => { /* handle */ }
//!     TypeKey::Literal(literal) => { /* handle */ }
//!     TypeKey::Union(members) => { /* handle */ }
//!     // ... 26 more cases ...
//! }
//! ```
//!
//! # Solution: TypeClassificationVisitor
//!
//! ```ignore
//! // PATTERN: Visitor-based approach
//! let visitor = TypeClassificationVisitor::new(db, type_id);
//! visitor.classify(|classification| {
//!     match classification {
//!         TypeClassification::Intrinsic(kind) => { /* ... */ }
//!         TypeClassification::Literal(literal) => { /* ... */ }
//!         TypeClassification::Union(members) => { /* ... */ }
//!         // ... cleaner and more maintainable ...
//!     }
//! })
//! ```
//!
//! # Key Benefits
//!
//! - **Elimination of Boilerplate**: No need to call db.lookup() and match manually
//! - **Consistency**: Single point where classification logic is defined
//! - **Extensibility**: Easy to add new visitor methods without changing all code
//! - **Type Safety**: Compiler ensures all cases are handled

use crate::db::TypeDatabase;
use crate::type_classifier::TypeClassification;
use crate::type_dispatcher::TypeDispatcher;
use crate::types::{IntrinsicKind, LiteralValue, ObjectShapeId, TupleListId, TypeId, TypeListId};

/// A visitor for classifying and handling different type structures.
///
/// This visitor provides systematic traversal of type structures using the visitor pattern.
/// It can be extended with additional visit methods for different type categories.
///
/// # Example
///
/// ```ignore
/// let visitor = TypeClassificationVisitor::new(db, type_id);
/// if visitor.is_union() {
///     visitor.visit_union(|members| {
///         println!("Union with {} members", members.len());
///     });
/// }
/// ```
pub struct TypeClassificationVisitor<'db> {
    db: &'db dyn TypeDatabase,
    type_id: TypeId,
    classification: Option<TypeClassification>,
}

impl<'db> TypeClassificationVisitor<'db> {
    /// Create a new visitor for the given type.
    pub fn new(db: &'db dyn TypeDatabase, type_id: TypeId) -> Self {
        Self {
            db,
            type_id,
            classification: None,
        }
    }

    /// Get the classification of this type (caching on first access).
    pub fn classify(&mut self) -> &TypeClassification {
        if self.classification.is_none() {
            use crate::type_classifier::classify_type;
            self.classification = Some(classify_type(self.db, self.type_id));
        }
        self.classification.as_ref().unwrap()
    }

    /// Check if this type is a union.
    pub fn is_union(&mut self) -> bool {
        use crate::type_classifier::TypeClassification;
        matches!(self.classify(), TypeClassification::Union(_))
    }

    /// Check if this type is an intersection.
    pub fn is_intersection(&mut self) -> bool {
        use crate::type_classifier::TypeClassification;
        matches!(self.classify(), TypeClassification::Intersection(_))
    }

    /// Check if this type is an object.
    pub fn is_object(&mut self) -> bool {
        use crate::type_classifier::TypeClassification;
        matches!(
            self.classify(),
            TypeClassification::Object(_) | TypeClassification::ObjectWithIndex(_)
        )
    }

    /// Check if this type is callable.
    pub fn is_callable(&mut self) -> bool {
        use crate::type_classifier::TypeClassification;
        matches!(
            self.classify(),
            TypeClassification::Function(_) | TypeClassification::Callable(_)
        )
    }

    /// Check if this type is an array.
    pub fn is_array(&mut self) -> bool {
        use crate::type_classifier::TypeClassification;
        matches!(self.classify(), TypeClassification::Array(_))
    }

    /// Check if this type is a tuple.
    pub fn is_tuple(&mut self) -> bool {
        use crate::type_classifier::TypeClassification;
        matches!(self.classify(), TypeClassification::Tuple(_))
    }

    /// Check if this type is a literal.
    pub fn is_literal(&mut self) -> bool {
        use crate::type_classifier::TypeClassification;
        matches!(self.classify(), TypeClassification::Literal(_))
    }

    /// Check if this type is primitive.
    pub fn is_primitive(&mut self) -> bool {
        use crate::type_classifier::TypeClassification;
        matches!(self.classify(), TypeClassification::Intrinsic(_))
    }

    /// Visit a union type with the given closure.
    pub fn visit_union<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(TypeListId),
    {
        use crate::type_classifier::TypeClassification;
        if let TypeClassification::Union(members) = self.classify() {
            f(*members);
            true
        } else {
            false
        }
    }

    /// Visit an intersection type with the given closure.
    pub fn visit_intersection<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(TypeListId),
    {
        use crate::type_classifier::TypeClassification;
        if let TypeClassification::Intersection(members) = self.classify() {
            f(*members);
            true
        } else {
            false
        }
    }

    /// Visit an object type with the given closure.
    pub fn visit_object<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(ObjectShapeId),
    {
        use crate::type_classifier::TypeClassification;
        match self.classify() {
            TypeClassification::Object(shape) => {
                f(*shape);
                true
            }
            TypeClassification::ObjectWithIndex(shape) => {
                f(*shape);
                true
            }
            _ => false,
        }
    }

    /// Visit an array type with the given closure.
    pub fn visit_array<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(TypeId),
    {
        use crate::type_classifier::TypeClassification;
        if let TypeClassification::Array(elem) = self.classify() {
            f(*elem);
            true
        } else {
            false
        }
    }

    /// Visit a tuple type with the given closure.
    pub fn visit_tuple<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(TupleListId),
    {
        use crate::type_classifier::TypeClassification;
        if let TypeClassification::Tuple(elems) = self.classify() {
            f(*elems);
            true
        } else {
            false
        }
    }

    /// Visit a literal type with the given closure.
    pub fn visit_literal<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(&LiteralValue),
    {
        use crate::type_classifier::TypeClassification;
        if let TypeClassification::Literal(lit) = self.classify() {
            f(lit);
            true
        } else {
            false
        }
    }

    /// Visit an intrinsic type with the given closure.
    pub fn visit_intrinsic<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(IntrinsicKind),
    {
        use crate::type_classifier::TypeClassification;
        if let TypeClassification::Intrinsic(kind) = self.classify() {
            f(*kind);
            true
        } else {
            false
        }
    }

    /// Dispatch this type to handlers based on its category.
    ///
    /// This provides integration with TypeDispatcher for advanced handling.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut visitor = TypeClassificationVisitor::new(db, type_id);
    /// visitor.dispatch(|dispatcher| {
    ///     dispatcher
    ///         .on_union(|members| { /* handle union */ })
    ///         .on_object(|shape| { /* handle object */ })
    ///         .dispatch()
    /// });
    /// ```
    pub fn dispatch<F>(&mut self, _f: F)
    where
        F: FnOnce(TypeDispatcher) -> crate::type_dispatcher::DispatchResult,
    {
        // Implementation would integrate with TypeDispatcher
        // This is left as a placeholder for future integration
    }

    /// Get the underlying type ID.
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Get the underlying database.
    pub fn db(&self) -> &'db dyn TypeDatabase {
        self.db
    }

    /// Classify the type and return the classification.
    pub fn into_classification(mut self) -> TypeClassification {
        self.classify().clone()
    }
}

// ============================================================================
// Visitor Extension Trait for Composition
// ============================================================================

/// Trait for extending visitor behavior.
///
/// This trait allows creating composable visitor chains without modifying
/// the base TypeClassificationVisitor.
pub trait TypeVisitorExt<'db> {
    /// Create a visitor for this type.
    fn create_visitor(&self) -> TypeClassificationVisitor<'db>;
}

impl<'db> TypeVisitorExt<'db> for TypeId {
    fn create_visitor(&self) -> TypeClassificationVisitor<'db> {
        unimplemented!("Would need database context")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visitor_creation() {
        // This is a placeholder test to ensure the module compiles
        // Real tests would require a full TypeDatabase implementation
    }
}
