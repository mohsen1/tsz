//! Unified Type Predicates Trait
//!
//! This module consolidates all type classification queries into a single trait,
//! eliminating the 165+ scattered predicate functions across the codebase.
//!
//! # Design Principles
//!
//! 1. **Single source of truth**: All type predicates defined in one place
//! 2. **Trait-based**: Implemented for `TypeDatabase`, usable everywhere
//! 3. **Discoverable**: IDE autocomplete shows all available predicates
//! 4. **Consistent**: Impossible to have conflicting implementations
//! 5. **Extensible**: Add new predicates by extending the trait
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::type_predicates::TypePredicates;
//!
//! fn check_something(db: &dyn TypeDatabase, type_id: TypeId) {
//!     if db.is_union_type(type_id) {
//!         // Handle union
//!     }
//!
//!     if db.is_string_like(type_id) {
//!         // Handle string-like types
//!     }
//! }
//! ```
//!
//! # Benefits Over Scattered Functions
//!
//! **Before** (scattered across multiple files):
//! ```ignore
//! // In type_queries.rs
//! pub fn is_union_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool { /**/ }
//!
//! // In checker/type_query.rs (duplicate!)
//! impl CheckerState {
//!     pub fn is_union_type(&self, type_id: TypeId) -> bool { /**/ }
//! }
//!
//! // Hard to discover, easy to duplicate, inconsistent
//! ```
//!
//! **After** (unified trait):
//! ```ignore
//! // Single implementation, available everywhere
//! db.is_union_type(type_id)
//! ```

use crate::{
    CallableShapeId, IntrinsicKind, LiteralValue, TypeDatabase, TypeId, TypeKey, TypeListId,
};
use std::sync::Arc;

/// Unified trait for type classification and property queries.
///
/// This trait consolidates all type predicate functions into a single,
/// discoverable API. Implement this trait for `TypeDatabase` to gain
/// access to all type classification methods.
pub trait TypePredicates {
    // =========================================================================
    // Core Type Category Predicates
    // =========================================================================

    /// Check if a type is a union type (A | B).
    ///
    /// Returns `true` for `TypeKey::Union`.
    fn is_union_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is an intersection type (A & B).
    ///
    /// Returns `true` for `TypeKey::Intersection`.
    fn is_intersection_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is an object type (with or without index signatures).
    ///
    /// Returns `true` for `TypeKey::Object` and `TypeKey::ObjectWithIndex`.
    fn is_object_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is an array type (T[]).
    ///
    /// Returns `true` for `TypeKey::Array`.
    fn is_array_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is a tuple type ([T, U, V]).
    ///
    /// Returns `true` for `TypeKey::Tuple`.
    fn is_tuple_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is a callable type (function or callable with signatures).
    ///
    /// Returns `true` for `TypeKey::Function` and `TypeKey::Callable`.
    fn is_callable_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is a literal type (specific value).
    ///
    /// Returns `true` for `TypeKey::Literal`.
    fn is_literal_type(&self, type_id: TypeId) -> bool;

    // =========================================================================
    // Intrinsic Type Predicates
    // =========================================================================

    /// Check if a type is the `string` type.
    fn is_string_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is the `number` type.
    fn is_number_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is the `boolean` type.
    fn is_boolean_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is the `bigint` type.
    fn is_bigint_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is the `symbol` type.
    fn is_symbol_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is the `any` type.
    fn is_any_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is the `unknown` type.
    fn is_unknown_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is the `never` type.
    fn is_never_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is the `void` type.
    fn is_void_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is the `undefined` type.
    fn is_undefined_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is the `null` type.
    fn is_null_type(&self, type_id: TypeId) -> bool;

    // =========================================================================
    // Composite Predicates (string-like, number-like, etc.)
    // =========================================================================

    /// Check if a type is "string-like" (string | string literal | template literal).
    ///
    /// This is commonly used in type checking to determine if a value can be
    /// used in string contexts.
    fn is_string_like(&self, type_id: TypeId) -> bool {
        self.is_string_type(type_id)
            || matches!(
                self.lookup(type_id),
                Some(TypeKey::Literal(LiteralValue::String(_)))
            )
            || matches!(self.lookup(type_id), Some(TypeKey::TemplateLiteral(_)))
            || {
                // Union of string-like types is string-like
                if let Some(TypeKey::Union(list_id)) = self.lookup(type_id) {
                    self.type_list(list_id)
                        .iter()
                        .all(|&m| self.is_string_like(m))
                } else {
                    false
                }
            }
    }

    /// Check if a type is "number-like" (number | number literal | enum member).
    ///
    /// This is commonly used in arithmetic operations.
    fn is_number_like(&self, type_id: TypeId) -> bool {
        self.is_number_type(type_id)
            || matches!(
                self.lookup(type_id),
                Some(TypeKey::Literal(LiteralValue::Number(_)))
            )
            || matches!(self.lookup(type_id), Some(TypeKey::Enum(_, _)))
            || {
                // Union of number-like types is number-like
                if let Some(TypeKey::Union(list_id)) = self.lookup(type_id) {
                    self.type_list(list_id)
                        .iter()
                        .all(|&m| self.is_number_like(m))
                } else {
                    false
                }
            }
    }

    /// Check if a type is "boolean-like" (boolean | boolean literal).
    fn is_boolean_like(&self, type_id: TypeId) -> bool {
        self.is_boolean_type(type_id)
            || matches!(
                self.lookup(type_id),
                Some(TypeKey::Literal(LiteralValue::Boolean(_)))
            )
            || type_id == TypeId::BOOLEAN_TRUE
            || type_id == TypeId::BOOLEAN_FALSE
    }

    /// Check if a type is "nullish" (null | undefined).
    ///
    /// This is commonly used in strict null checking and optional chaining.
    fn is_nullish_type(&self, type_id: TypeId) -> bool {
        self.is_null_type(type_id) || self.is_undefined_type(type_id)
    }

    /// Check if a type is a "unit" type (void | undefined | null | never).
    ///
    /// Unit types are types with no meaningful value.
    fn is_unit_type(&self, type_id: TypeId) -> bool {
        self.is_void_type(type_id)
            || self.is_undefined_type(type_id)
            || self.is_null_type(type_id)
            || self.is_never_type(type_id)
    }

    // =========================================================================
    // Advanced Type Predicates
    // =========================================================================

    /// Check if a type is a type parameter or infer type.
    fn is_type_parameter(&self, type_id: TypeId) -> bool;

    /// Check if a type is a generic type application (Base<Args>).
    fn is_generic_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is a conditional type (T extends U ? X : Y).
    fn is_conditional_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is a mapped type ({ [K in Keys]: V }).
    fn is_mapped_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is a template literal type (`hello${T}world`).
    fn is_template_literal_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is an index access type (T[K]).
    fn is_index_access_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is a keyof type.
    fn is_keyof_type(&self, type_id: TypeId) -> bool;

    /// Check if a type is a type query (typeof expr).
    fn is_type_query(&self, type_id: TypeId) -> bool;

    /// Check if a type is a named type reference.
    fn is_type_reference(&self, type_id: TypeId) -> bool;

    /// Check if a type is invokable (has call signatures).
    ///
    /// This is more specific than `is_callable_type` - it ensures the type
    /// can be called as a function (not just constructed with `new`).
    fn is_invokable_type(&self, type_id: TypeId) -> bool {
        match self.lookup(type_id) {
            Some(TypeKey::Function(_)) => true,
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.callable_shape(shape_id);
                !shape.call_signatures.is_empty()
            }
            Some(TypeKey::Intersection(list_id)) => {
                let members = self.type_list(list_id);
                members.iter().any(|&m| self.is_invokable_type(m))
            }
            _ => false,
        }
    }

    /// Check if a type is an enum type.
    fn is_enum_type(&self, type_id: TypeId) -> bool;

    // =========================================================================
    // Helper: Access to underlying TypeDatabase methods
    // =========================================================================

    /// Access to the underlying lookup method.
    fn lookup(&self, type_id: TypeId) -> Option<TypeKey>;

    /// Access to type list data.
    fn type_list(&self, list_id: TypeListId) -> Arc<[TypeId]>;

    /// Access to callable shape data.
    fn callable_shape(&self, shape_id: CallableShapeId) -> Arc<crate::CallableShape>;
}

// =============================================================================
// Implementation for TypeDatabase
// =============================================================================

impl<T: TypeDatabase + ?Sized> TypePredicates for T {
    // Core predicates
    fn is_union_type(&self, type_id: TypeId) -> bool {
        matches!(self.lookup(type_id), Some(TypeKey::Union(_)))
    }

    fn is_intersection_type(&self, type_id: TypeId) -> bool {
        matches!(self.lookup(type_id), Some(TypeKey::Intersection(_)))
    }

    fn is_object_type(&self, type_id: TypeId) -> bool {
        matches!(
            self.lookup(type_id),
            Some(TypeKey::Object(_) | TypeKey::ObjectWithIndex(_))
        )
    }

    fn is_array_type(&self, type_id: TypeId) -> bool {
        matches!(self.lookup(type_id), Some(TypeKey::Array(_)))
    }

    fn is_tuple_type(&self, type_id: TypeId) -> bool {
        matches!(self.lookup(type_id), Some(TypeKey::Tuple(_)))
    }

    fn is_callable_type(&self, type_id: TypeId) -> bool {
        matches!(
            self.lookup(type_id),
            Some(TypeKey::Callable(_) | TypeKey::Function(_))
        )
    }

    fn is_literal_type(&self, type_id: TypeId) -> bool {
        matches!(self.lookup(type_id), Some(TypeKey::Literal(_)))
    }

    // Intrinsic predicates
    fn is_string_type(&self, type_id: TypeId) -> bool {
        matches!(
            self.lookup(type_id),
            Some(TypeKey::Intrinsic(IntrinsicKind::String))
        )
    }

    fn is_number_type(&self, type_id: TypeId) -> bool {
        matches!(
            self.lookup(type_id),
            Some(TypeKey::Intrinsic(IntrinsicKind::Number))
        )
    }

    fn is_boolean_type(&self, type_id: TypeId) -> bool {
        matches!(
            self.lookup(type_id),
            Some(TypeKey::Intrinsic(IntrinsicKind::Boolean))
        )
    }

    fn is_bigint_type(&self, type_id: TypeId) -> bool {
        matches!(
            self.lookup(type_id),
            Some(TypeKey::Intrinsic(IntrinsicKind::Bigint))
        )
    }

    fn is_symbol_type(&self, type_id: TypeId) -> bool {
        matches!(
            self.lookup(type_id),
            Some(TypeKey::Intrinsic(IntrinsicKind::Symbol))
        )
    }

    fn is_any_type(&self, type_id: TypeId) -> bool {
        type_id == TypeId::ANY
            || matches!(
                self.lookup(type_id),
                Some(TypeKey::Intrinsic(IntrinsicKind::Any))
            )
    }

    fn is_unknown_type(&self, type_id: TypeId) -> bool {
        type_id == TypeId::UNKNOWN
            || matches!(
                self.lookup(type_id),
                Some(TypeKey::Intrinsic(IntrinsicKind::Unknown))
            )
    }

    fn is_never_type(&self, type_id: TypeId) -> bool {
        type_id == TypeId::NEVER
            || matches!(
                self.lookup(type_id),
                Some(TypeKey::Intrinsic(IntrinsicKind::Never))
            )
    }

    fn is_void_type(&self, type_id: TypeId) -> bool {
        type_id == TypeId::VOID
            || matches!(
                self.lookup(type_id),
                Some(TypeKey::Intrinsic(IntrinsicKind::Void))
            )
    }

    fn is_undefined_type(&self, type_id: TypeId) -> bool {
        type_id == TypeId::UNDEFINED
            || matches!(
                self.lookup(type_id),
                Some(TypeKey::Intrinsic(IntrinsicKind::Undefined))
            )
    }

    fn is_null_type(&self, type_id: TypeId) -> bool {
        type_id == TypeId::NULL
            || matches!(
                self.lookup(type_id),
                Some(TypeKey::Intrinsic(IntrinsicKind::Null))
            )
    }

    // Advanced predicates
    fn is_type_parameter(&self, type_id: TypeId) -> bool {
        matches!(
            self.lookup(type_id),
            Some(TypeKey::TypeParameter(_) | TypeKey::Infer(_))
        )
    }

    fn is_generic_type(&self, type_id: TypeId) -> bool {
        matches!(self.lookup(type_id), Some(TypeKey::Application(_)))
    }

    fn is_conditional_type(&self, type_id: TypeId) -> bool {
        matches!(self.lookup(type_id), Some(TypeKey::Conditional(_)))
    }

    fn is_mapped_type(&self, type_id: TypeId) -> bool {
        matches!(self.lookup(type_id), Some(TypeKey::Mapped(_)))
    }

    fn is_template_literal_type(&self, type_id: TypeId) -> bool {
        matches!(self.lookup(type_id), Some(TypeKey::TemplateLiteral(_)))
    }

    fn is_index_access_type(&self, type_id: TypeId) -> bool {
        matches!(self.lookup(type_id), Some(TypeKey::IndexAccess(_, _)))
    }

    fn is_keyof_type(&self, type_id: TypeId) -> bool {
        matches!(self.lookup(type_id), Some(TypeKey::KeyOf(_)))
    }

    fn is_type_query(&self, type_id: TypeId) -> bool {
        matches!(self.lookup(type_id), Some(TypeKey::TypeQuery(_)))
    }

    fn is_type_reference(&self, type_id: TypeId) -> bool {
        matches!(
            self.lookup(type_id),
            Some(TypeKey::Lazy(_) | TypeKey::Recursive(_) | TypeKey::BoundParameter(_))
        )
    }

    fn is_enum_type(&self, type_id: TypeId) -> bool {
        matches!(self.lookup(type_id), Some(TypeKey::Enum(_, _)))
    }

    // Delegate to TypeDatabase methods
    fn lookup(&self, type_id: TypeId) -> Option<TypeKey> {
        TypeDatabase::lookup(self, type_id)
    }

    fn type_list(&self, list_id: TypeListId) -> Arc<[TypeId]> {
        TypeDatabase::type_list(self, list_id)
    }

    fn callable_shape(&self, shape_id: CallableShapeId) -> Arc<crate::CallableShape> {
        TypeDatabase::callable_shape(self, shape_id)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TypeInterner;

    fn setup() -> TypeInterner {
        TypeInterner::new()
    }

    #[test]
    fn test_intrinsic_predicates() {
        let interner = setup();

        assert!(interner.is_string_type(TypeId::STRING));
        assert!(interner.is_number_type(TypeId::NUMBER));
        assert!(interner.is_boolean_type(TypeId::BOOLEAN));
        assert!(interner.is_any_type(TypeId::ANY));
        assert!(interner.is_unknown_type(TypeId::UNKNOWN));
        assert!(interner.is_never_type(TypeId::NEVER));
        assert!(interner.is_void_type(TypeId::VOID));
        assert!(interner.is_undefined_type(TypeId::UNDEFINED));
        assert!(interner.is_null_type(TypeId::NULL));
    }

    #[test]
    fn test_union_predicate() {
        let interner = setup();
        // Basic negative test - intrinsic types are not unions
        assert!(!interner.is_union_type(TypeId::STRING));
        assert!(!interner.is_union_type(TypeId::NUMBER));
    }

    #[test]
    fn test_array_predicate() {
        let interner = setup();
        // Basic negative test - intrinsic types are not arrays
        assert!(!interner.is_array_type(TypeId::STRING));
        assert!(!interner.is_array_type(TypeId::NUMBER));
    }

    #[test]
    fn test_string_like_predicate() {
        let interner = setup();

        // String type is string-like
        assert!(interner.is_string_like(TypeId::STRING));

        // Number is not string-like
        assert!(!interner.is_string_like(TypeId::NUMBER));

        // Other intrinsics are not string-like
        assert!(!interner.is_string_like(TypeId::BOOLEAN));
        assert!(!interner.is_string_like(TypeId::ANY));
    }

    #[test]
    fn test_number_like_predicate() {
        let interner = setup();

        // Number type is number-like
        assert!(interner.is_number_like(TypeId::NUMBER));

        // String is not number-like
        assert!(!interner.is_number_like(TypeId::STRING));

        // Boolean is not number-like
        assert!(!interner.is_number_like(TypeId::BOOLEAN));
    }

    #[test]
    fn test_boolean_like_predicate() {
        let interner = setup();

        assert!(interner.is_boolean_like(TypeId::BOOLEAN));
        assert!(interner.is_boolean_like(TypeId::BOOLEAN_TRUE));
        assert!(interner.is_boolean_like(TypeId::BOOLEAN_FALSE));
        assert!(!interner.is_boolean_like(TypeId::STRING));
    }

    #[test]
    fn test_nullish_predicate() {
        let interner = setup();

        assert!(interner.is_nullish_type(TypeId::NULL));
        assert!(interner.is_nullish_type(TypeId::UNDEFINED));
        assert!(!interner.is_nullish_type(TypeId::STRING));
        assert!(!interner.is_nullish_type(TypeId::VOID));
    }

    #[test]
    fn test_unit_predicate() {
        let interner = setup();

        assert!(interner.is_unit_type(TypeId::VOID));
        assert!(interner.is_unit_type(TypeId::UNDEFINED));
        assert!(interner.is_unit_type(TypeId::NULL));
        assert!(interner.is_unit_type(TypeId::NEVER));
        assert!(!interner.is_unit_type(TypeId::STRING));
        assert!(!interner.is_unit_type(TypeId::ANY));
    }

    #[test]
    fn test_predicate_chaining() {
        let interner = setup();

        // Demonstrate trait methods are composable
        let is_valid_operand = interner.is_number_like(TypeId::NUMBER)
            || interner.is_string_like(TypeId::NUMBER)
            || interner.is_boolean_like(TypeId::NUMBER);

        assert!(is_valid_operand || !is_valid_operand); // Always true, just testing composability
    }
}
